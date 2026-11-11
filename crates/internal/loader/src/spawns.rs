//! Entities mods spawn from a template, on the server's game thread between
//! steps or inside a game event, where no ECS system runs.
//!
//! A spawn is the game's admin spawn: a request in the entity world's first
//! command buffer with the template GUID and the components to start with,
//! which the engine plays back after the systems of the next step. The
//! request hands out the new entity's id at once; the entity is in the
//! world from the next step on.

use std::sync::{Mutex, OnceLock};

use enshwick_core::Entity;
use enshwick_core::abi::{SpawnRequest, Status};
use rustc_hash::{FxBuildHasher, FxHashMap};

use crate::host::Loader;
use crate::pinned;
use crate::pinned::Symbol;
use crate::spawn_request::{RequestLayout, Start};
use enshwick_base::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMMAND_BUFFERS.id(),
    pinned::ecs::SPAWN_COMPONENT_BYTES.id(),
    pinned::ecs::SPAWN_COMPONENT_DATA.id(),
    pinned::ecs::SPAWN_COMPONENT_HASH.id(),
    pinned::ecs::SPAWN_COMPONENT_SIZE.id(),
    pinned::ecs::SPAWN_INITIAL.id(),
    pinned::ecs::SPAWN_INITIAL_COUNT.id(),
    pinned::ecs::SPAWN_REQUEST.id(),
    pinned::ecs::SPAWN_REQUESTER.id(),
    pinned::ecs::SPAWN_REQUEST_SIZE.id(),
    pinned::ecs::SPAWN_TEMPLATE.id(),
    pinned::world::ENTITIES.id(),
    pinned::world::TIME.id(),
];

/// The engine's spawn: the command buffer, the new entity's id out, the
/// requester, and the request [`RequestLayout`] lays out.
type SpawnFn = unsafe extern "system" fn(
    buffer: *mut u8,
    out: *mut u32,
    requester: u32,
    request: *const u64,
) -> *mut u32;
type TimeFn = unsafe extern "system" fn(world: *const u8) -> i64;

/// A component type a spawn starts an entity with: its hash and size.
#[derive(Debug, Clone, Copy)]
struct Component {
    hash: u32,
    size: usize,
}

/// The game functions, offsets and component types, from the build table
/// and reflection.
#[derive(Debug)]
struct Layout {
    spawn: SpawnFn,
    time: TimeFn,
    command_buffers: usize,
    world_entities: usize,
    request: RequestLayout,
    transform: Component,
    transform_orientation: usize,
    transform_scale: usize,
    spawn_time: Component,
    level: Option<Component>,
    life: Option<(Component, usize, usize)>,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let component = |name: &str| {
                let info = reflection.type_info(name).ok()?;
                Some(Component {
                    hash: crate::world::type_hash(info.va())?,
                    size: usize::try_from(info.size()).ok()?,
                })
            };
            let offset = |ty: &str, field: &str| {
                reflection
                    .field_offset(ty, field)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let (spawn, time) = (
                function(pinned::ecs::SPAWN_REQUEST)?,
                function(pinned::world::TIME)?,
            );
            let request = RequestLayout {
                size: loader.offset(pinned::ecs::SPAWN_REQUEST_SIZE)?,
                requester: loader.offset(pinned::ecs::SPAWN_REQUESTER)?,
                template: loader.offset(pinned::ecs::SPAWN_TEMPLATE)?,
                initial: loader.offset(pinned::ecs::SPAWN_INITIAL)?,
                initial_count: loader.offset(pinned::ecs::SPAWN_INITIAL_COUNT)?,
                component_size: loader.offset(pinned::ecs::SPAWN_COMPONENT_SIZE)?,
                component_hash: loader.offset(pinned::ecs::SPAWN_COMPONENT_HASH)?,
                component_data: loader.offset(pinned::ecs::SPAWN_COMPONENT_DATA)?,
                component_bytes: loader.offset(pinned::ecs::SPAWN_COMPONENT_BYTES)?,
            };
            if !request.fits() {
                crate::log!(
                    WARN,
                    "spawns off on {}: the spawn request's fields do not fit ({request:x?})",
                    loader.build.name
                );
                return None;
            }
            Some(Layout {
                // SAFETY: the build table's `ecs.spawn_request` is the address
                // of the game function with the `SpawnFn` signature on this
                // build.
                spawn: unsafe { core::mem::transmute::<usize, SpawnFn>(spawn) },
                // SAFETY: the build table's `world.time` is the address of the
                // game function with the `TimeFn` signature on this build.
                time: unsafe { core::mem::transmute::<usize, TimeFn>(time) },
                command_buffers: loader.offset(pinned::ecs::COMMAND_BUFFERS)?,
                world_entities: loader.offset(pinned::world::ENTITIES)?,
                request,
                transform: component("keen::ecs::CurrentTransform")?,
                transform_orientation: offset("keen::WorldTransform", "orientation")?,
                transform_scale: offset("keen::WorldTransform", "scale")?,
                spawn_time: component("keen::ecs::SpawnTime")?,
                level: component("keen::ecs::OverrideLevel"),
                life: component("keen::ecs::LifeTime").and_then(|life| {
                    Some((
                        life,
                        offset("keen::ecs::LifeTime", "lifeTime")?,
                        offset("keen::ecs::LifeTime", "endOfLife")?,
                    ))
                }),
            })
        })
        .as_ref()
}

/// Bytes of `size` with `parts` written at their offsets.
pub(crate) fn bytes(size: usize, parts: &[(usize, &[u8])]) -> Vec<u8> {
    let mut out = vec![0u8; size];
    for &(at, part) in parts {
        if let Some(slot) = out.get_mut(at..at + part.len()) {
            slot.copy_from_slice(part);
        }
    }
    out
}

/// Asks the server to spawn an entity of a template; its id.
pub(crate) fn spawn(loader: &Loader, request: &SpawnRequest) -> Result<u32, Status> {
    let mut extra = Vec::new();
    if request.flags & SpawnRequest::DO_NOT_SAVE != 0 {
        // The game's own spawns of passing creatures add DoNotSave so too.
        let ids = crate::catalogue::ids::DO_NOT_SAVE;
        let hash = crate::catalogue::component_hash(loader, ids).ok_or(Status::MISSING)?;
        let size = crate::catalogue::size(loader, ids).ok_or(Status::MISSING)?;
        extra.push((hash, vec![0u8; size]));
    }
    let entity = spawn_with(loader, request, extra, 0)?;
    note(loader, entity, request.template);
    Ok(entity)
}

/// [`spawn`], the entity starting with `extra` components as well: each a
/// reflected type's hash and its bytes, as the game's own prop spawn adds
/// them; asked for by the entity `requester`, 0 for none, which the game
/// keeps in the new entity's record as its own place does.
pub(crate) fn spawn_with(
    loader: &Loader,
    request: &SpawnRequest,
    extra: Vec<(u32, Vec<u8>)>,
    requester: u32,
) -> Result<u32, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    crate::world::server()?;
    let world = usize::try_from(crate::world::server_world())
        .ok()
        .filter(|&world| world != 0)
        .ok_or(Status::MISSING)?;
    if request.template == [0; 16] {
        return Err(Status::BAD_ARGS);
    }
    // SAFETY: `world::server` checked the caller is on the server's game
    // thread and `world` is non-null; the entity world lies inside the World
    // at the table's `world.entities`, so the subtraction gives the World the
    // game's own time function takes.
    let now = unsafe {
        (layout.time)(core::ptr::with_exposed_provenance(
            world - layout.world_entities,
        ))
    };
    let position: Vec<u8> = request
        .position
        .iter()
        .flat_map(|axis| axis.to_le_bytes())
        .collect();
    let orientation: Vec<u8> = request
        .orientation
        .iter()
        .flat_map(|part| part.to_le_bytes())
        .collect();
    let scale: Vec<u8> = [1.0f32; 3].iter().flat_map(|s| s.to_le_bytes()).collect();
    let transform = bytes(
        layout.transform.size,
        &[
            (0, &position),
            (layout.transform_orientation, &orientation),
            (layout.transform_scale, &scale),
        ],
    );
    let spawn_time = bytes(layout.spawn_time.size, &[(0, &now.to_le_bytes())]);
    let mut initial = vec![
        (layout.transform.hash, transform),
        (layout.spawn_time.hash, spawn_time),
    ];
    if request.level != 0 {
        let level = layout.level.ok_or(Status::MISSING)?;
        initial.push((
            level.hash,
            bytes(level.size, &[(0, &request.level.to_le_bytes())]),
        ));
    }
    if request.lifetime_ns != 0 {
        let (life, life_at, end_at) = layout.life.ok_or(Status::MISSING)?;
        let lifetime = i64::try_from(request.lifetime_ns).map_err(|_| Status::BAD_ARGS)?;
        let end = now.saturating_add(lifetime);
        initial.push((
            life.hash,
            bytes(
                life.size,
                &[
                    (life_at, &lifetime.to_le_bytes()),
                    (end_at, &end.to_le_bytes()),
                ],
            ),
        ));
    }
    initial.extend(extra);
    let starts: Vec<Start> = initial
        .iter()
        .map(|(hash, data)| Start {
            hash: *hash,
            data: data.as_ptr().expose_provenance() as u64,
            bytes: data.len() as u64,
        })
        .collect();
    let entries = layout.request.components(&starts).ok_or(Status::MISSING)?;
    let engine_request = layout
        .request
        .request(
            requester,
            request.template,
            entries.as_ptr().expose_provenance() as u64,
            starts.len() as u64,
        )
        .ok_or(Status::MISSING)?;
    // SAFETY: `world::server` checked the caller is on the server's game
    // thread and `world` is non-null; the entity world holds the pointer to
    // its command buffers at the table's `ecs.command_buffers`.
    let buffer = unsafe { crate::memory::read_ptr::<u8>(world, layout.command_buffers) };
    if buffer.is_null() {
        return Err(Status::MISSING);
    }
    let mut entity = 0u32;
    // SAFETY: `buffer` is the world's first command buffer, non-null by the
    // earlier check, used on the game thread between steps, where no system
    // writes it; the request and its components stay alive for the call,
    // which copies them.
    unsafe { (layout.spawn)(buffer, &raw mut entity, requester, engine_request.as_ptr()) };
    if entity == 0 {
        return Err(Status::REFUSED);
    }
    crate::log!(DEBUG, "spawn of entity {entity} asked");
    Ok(entity)
}

/// The fewest spawns kept before the ones no longer in the world go.
const MOST_SPAWNED: usize = 1 << 16;

/// A spawn a mod asked for: its template, the step it was asked in, and
/// the last [`set_saved`](crate::entities::set_saved) asked for it with its
/// step.
#[derive(Debug, Clone, Copy)]
struct Kept {
    template: [u8; 16],
    at: u64,
    asked: Option<(bool, u64)>,
}

/// The entities mods spawned or placed in the running world: the entities a
/// mod may keep out of the save, which a vanilla entity given the id of a
/// gone one does not pass for.
#[derive(Debug)]
struct Spawned {
    kept: FxHashMap<u32, Kept>,
    /// The size at which the next note first drops the entities gone from
    /// the world: twice what stayed the last time, so the walk costs each
    /// spawn a constant share.
    limit: usize,
}

static SPAWNED: Mutex<Spawned> = Mutex::new(Spawned {
    kept: FxHashMap::with_hasher(FxBuildHasher),
    limit: MOST_SPAWNED,
});

/// Whether a command asked in step `asked` may not have reached the world
/// at step `now`: the engine plays it back after the systems of the next
/// step.
const fn waiting(asked: u64, now: u64) -> bool {
    now <= asked.saturating_add(1)
}

impl Spawned {
    /// Keeps `entity` of `template`, asked for in step `now`; at the limit
    /// first drops the entities for which `in_world` is false, keeping those
    /// asked for too recently to be in the world yet.
    fn note(
        &mut self,
        entity: u32,
        template: [u8; 16],
        now: u64,
        in_world: impl Fn(u32, [u8; 16]) -> bool,
    ) {
        if self.kept.len() >= self.limit {
            self.kept
                .retain(|&kept, was| waiting(was.at, now) || in_world(kept, was.template));
            self.limit = MOST_SPAWNED.max(self.kept.len() * 2);
        }
        let asked = None;
        self.kept.insert(
            entity,
            Kept {
                template,
                at: now,
                asked,
            },
        );
    }

    /// Whether `entity` needs a command to end up `saved`, the world showing
    /// `live` at step `now`: a command still waiting counts over the world.
    fn needs(&self, entity: u32, saved: bool, live: bool, now: u64) -> bool {
        let shown = self
            .kept
            .get(&entity)
            .and_then(|kept| kept.asked)
            .filter(|&(_, at)| waiting(at, now))
            .map_or(live, |(asked, _)| asked);
        shown != saved
    }
}

/// Keeps `entity`, which a mod spawned or placed from `template`, as one a
/// mod may keep out of the save.
pub(crate) fn note(loader: &Loader, entity: u32, template: [u8; 16]) {
    let now = crate::events::steps();
    SPAWNED.relock().note(entity, template, now, |kept, was| {
        crate::world::template(loader, Entity(kept)) == Ok(was)
    });
}

/// The template a mod spawned or placed `entity` from in the running world,
/// or `None` for an entity no mod spawned.
pub(crate) fn spawned(entity: Entity) -> Option<[u8; 16]> {
    SPAWNED
        .relock()
        .kept
        .get(&entity.0)
        .map(|kept| kept.template)
}

/// Whether `entity` needs a `DoNotSave` command to end up `saved`, the world
/// showing `live` now.
pub(crate) fn needs_saved(entity: Entity, saved: bool, live: bool) -> bool {
    SPAWNED
        .relock()
        .needs(entity.0, saved, live, crate::events::steps())
}

/// Notes that a command to make `entity` `saved` went to the world now.
pub(crate) fn asked_saved(entity: Entity, saved: bool) {
    let now = crate::events::steps();
    if let Some(kept) = SPAWNED.relock().kept.get_mut(&entity.0) {
        kept.asked = Some((saved, now));
    }
}

/// Forgets `entity`, which a mod asked to destroy, so a vanilla entity
/// given its id later does not pass for it.
pub(crate) fn destroyed(entity: Entity) {
    SPAWNED.relock().kept.remove(&entity.0);
}

/// Forgets the entities mods spawned, when a hosted game ends.
pub(crate) fn forget() {
    let mut spawned = SPAWNED.relock();
    spawned.kept.clear();
    spawned.limit = MOST_SPAWNED;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Spawned {
        Spawned {
            kept: FxHashMap::default(),
            limit: MOST_SPAWNED,
        }
    }

    #[test]
    fn a_spawn_is_kept_with_its_template_until_destroyed_or_the_hosted_game_ends() {
        SPAWNED.relock().note(7, [1; 16], 0, |_, _| false);
        SPAWNED.relock().note(9, [1; 16], 0, |_, _| false);
        assert_eq!(spawned(Entity(7)), Some([1; 16]));
        assert_eq!(spawned(Entity(8)), None, "no mod spawned it");
        destroyed(Entity(9));
        assert_eq!(
            spawned(Entity(9)),
            None,
            "its id may go to a vanilla entity"
        );
        forget();
        assert_eq!(spawned(Entity(7)), None);
    }

    #[test]
    fn a_full_record_drops_only_the_entities_gone_from_the_world_then_waits_twice_as_long() {
        let mut record = empty();
        for entity in 0..u32::try_from(MOST_SPAWNED - 1).unwrap() {
            record.note(entity, [2; 16], 0, |_, _| unreachable!("not full yet"));
        }
        record.note(u32::MAX - 1, [4; 16], 9, |_, _| {
            unreachable!("not full yet")
        });
        record.note(u32::MAX, [3; 16], 9, |kept, was| {
            kept % 2 == 0 && was == [2; 16]
        });
        assert_eq!(record.kept.len(), MOST_SPAWNED / 2 + 2);
        assert_eq!(record.kept.get(&4).map(|k| k.template), Some([2; 16]));
        assert!(!record.kept.contains_key(&5), "gone from the world");
        assert!(
            record.kept.contains_key(&(u32::MAX - 1)),
            "asked for in this step, so not in the world yet"
        );
        assert!(record.kept.contains_key(&u32::MAX));
        assert_eq!(
            record.limit,
            (MOST_SPAWNED / 2 + 1) * 2,
            "twice what stayed"
        );
        record.note(u32::MAX - 2, [3; 16], 10, |_, _| {
            unreachable!("below the limit")
        });
    }

    #[test]
    fn a_record_of_live_entities_is_walked_again_only_after_it_doubles() {
        let mut record = empty();
        for entity in 0..u32::try_from(MOST_SPAWNED).unwrap() {
            record.note(entity, [2; 16], 0, |_, _| true);
        }
        record.note(u32::MAX, [2; 16], 5, |_, _| true);
        assert_eq!(record.limit, MOST_SPAWNED * 2);
        let more =
            u32::try_from(MOST_SPAWNED).unwrap()..u32::try_from(MOST_SPAWNED * 2 - 2).unwrap();
        for entity in more {
            record.note(entity, [2; 16], 6, |_, _| {
                unreachable!("below the doubled limit")
            });
        }
    }

    #[test]
    fn a_change_of_mind_in_one_step_queues_both_commands_and_a_repeat_queues_none() {
        let mut record = empty();
        record.note(7, [1; 16], 0, |_, _| false);
        assert!(record.needs(7, false, true, 3), "saved now, asked out");
        record.kept.get_mut(&7).unwrap().asked = Some((false, 3));
        assert!(!record.needs(7, false, true, 3), "the same ask again");
        assert!(
            record.needs(7, true, true, 3),
            "put back before the world shows it out"
        );
        record.kept.get_mut(&7).unwrap().asked = Some((true, 3));
        assert!(!record.needs(7, true, true, 4), "still waiting a step on");
        assert!(
            record.needs(7, true, false, 5),
            "after playback the world counts again"
        );
        assert!(!record.needs(7, true, true, 5));
    }
}
