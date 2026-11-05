//! Entities mods spawn from a template, on the server's game thread between
//! steps or inside a game event, where no ECS system runs.
//!
//! A spawn is the game's admin spawn: a request in the entity world's first
//! command buffer with the template GUID and the components to start with,
//! which the engine plays back after the systems of the next step. The
//! request hands out the new entity's id at once; the entity is in the
//! world from the next step on.

use std::sync::OnceLock;

use enshwick::abi::{SpawnRequest, Status};

use crate::host::Loader;
use crate::pinned;
use crate::pinned::Symbol;
use crate::spawn_request::{RequestLayout, Start};

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
fn bytes(size: usize, parts: &[(usize, &[u8])]) -> Vec<u8> {
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
    spawn_with(loader, request, Vec::new(), 0)
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
