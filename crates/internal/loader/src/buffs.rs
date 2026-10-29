//! Buffs and debuffs: the types the game knows, the buffs an entity has,
//! giving one and removing one, in the server's world.
//!
//! An entity holds its buffs in `BuffContainer`, slots in groups by kind
//! (food, buffs, debuffs, rested, terrain); each slot names a buff entity
//! that carries the effect, its type and whether it was cleansed. The types
//! live in the game's buff registry.
//!
//! The game applies a buff only from inside its systems, from what an
//! actor's animation does. The loader gives one as the game restores a
//! character's saved buffs when the character spawns: the restore takes the
//! type, a duration and a level, spawns the buff entity at once and fills
//! the slot, outside any system, on the game thread. The restore would
//! overwrite a slot of the same type without removing the old buff, so the
//! loader renews a buff the entity has instead, and refuses a type cleansed
//! in this step, whose slot the game frees only in the next. A removal is
//! the game's cleanse: the slot is marked, and `update_buffcontainer`
//! destroys the buff in the next step.

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::abi::{ActiveBuff, BuffRequest, BuffTypeInfo, Header, Status};

use crate::host::Loader;
use crate::memory::{BlobArray, read, write};
use crate::pinned::{self, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::buffs::CLEANSE.id(),
    pinned::buffs::RECORD_HAS_LEVEL.id(),
    pinned::buffs::RECORD_HAS_STRENGTH.id(),
    pinned::buffs::RECORD_LEVEL.id(),
    pinned::buffs::RECORD_LIFETIME.id(),
    pinned::buffs::RECORD_REMAINING.id(),
    pinned::buffs::RECORD_SIZE.id(),
    pinned::buffs::RECORD_STRENGTH.id(),
    pinned::buffs::RECORD_TYPE.id(),
    pinned::buffs::REGISTRY.id(),
    pinned::buffs::REGISTRY_BLOB.id(),
    pinned::buffs::RESTORE.id(),
    pinned::buffs::TYPE_BY_ID.id(),
    pinned::game::BALANCING.id(),
    pinned::server::PLAYER_INFOS.id(),
    pinned::server::WORLD.id(),
    pinned::world::TIME.id(),
];

/// The room the loader keeps for the saved-buff record the restore reads;
/// the build's record must fit.
const RECORD: usize = 0x100; // bound, not a game offset: the loader's room
/// More buff types than any build has: a count past it is not a count.
const MAX_TYPES: usize = 1 << 16;
/// The value of the record's strength the game's own apply gives a buff.
const STRENGTH: f32 = 1.0;
/// The slot groups of `BuffContainer` in order, with their kinds.
const GROUPS: [(&str, u16); 5] = [
    ("foodBuffs", BuffTypeInfo::FOOD),
    ("genericBuffs", BuffTypeInfo::BUFF),
    ("genericDebuffs", BuffTypeInfo::DEBUFF),
    ("restedBuff", BuffTypeInfo::RESTED),
    ("terrainDebuff", BuffTypeInfo::TERRAIN),
];

type TypeByIdFn = unsafe extern "system" fn(id: u32, registry: *const u8) -> *const u8;
type RestoreFn = unsafe extern "system" fn(
    container: *mut u8,
    target: u32,
    record: *const u8,
    server: *mut u8,
    balancing: *mut u8,
    food_stays: u8,
);
type CleanseFn = unsafe extern "system" fn(container: *mut u8, id: u32) -> u8;
type NowFn = unsafe extern "system" fn(world: *const u8) -> i64;

/// One slot group of `BuffContainer`: where it starts, how many slots.
#[derive(Debug, Clone, Copy)]
struct Group {
    start: usize,
    count: usize,
    kind: u16,
}

/// Where everything lies, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    type_by_id: TypeByIdFn,
    restore: RestoreFn,
    cleanse: CleanseFn,
    now: NowFn,
    world: usize,
    game: usize,
    registry: usize,
    registry_blob: usize,
    balancing: usize,
    container: u32,
    groups: [Group; 5],
    instance_size: usize,
    instance_entity: usize,
    instance_type: usize,
    instance_cleansed: usize,
    network: u32,
    network_lifetime: usize,
    network_end: usize,
    network_level: usize,
    lifetime: u32,
    lifetime_total: usize,
    lifetime_end: usize,
    types: usize,
    type_size: usize,
    type_id: usize,
    type_slot: usize,
    type_duration: usize,
    type_name: usize,
    type_hidden: usize,
    type_saved: usize,
    record_type: usize,
    record_lifetime: usize,
    record_remaining: usize,
    record_level: usize,
    record_has_level: usize,
    record_strength: usize,
    record_has_strength: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let field = |ty: &str, name: &str| {
                reflection
                    .field_offset(ty, name)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let size = |ty: &str| {
                reflection
                    .type_info(ty)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())
            };
            let hash = |ty: &str| {
                let info = reflection.type_info(ty).ok()?;
                crate::world::type_hash(info.va())
            };
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let container = "keen::ecs::BuffContainer";
            let instance = "keen::ecs::BuffInstance";
            let instance_size = size(instance)?;
            let mut starts = [0usize; 6];
            for (i, (name, _)) in GROUPS.iter().enumerate() {
                starts[i] = field(container, name)?;
            }
            starts[5] = field(container, "hungerStartTime")?;
            let mut groups = [Group {
                start: 0,
                count: 0,
                kind: 0,
            }; 5];
            for (i, group) in groups.iter_mut().enumerate() {
                *group = Group {
                    start: starts[i],
                    count: starts[i + 1].checked_sub(starts[i])? / instance_size,
                    kind: GROUPS[i].1,
                };
            }
            let network = "keen::ecs::BuffNetworkData";
            let lifetime = "keen::ecs::LifeTime";
            let ty = "keen::BuffType";
            let (type_by_id, restore, cleanse, now) = (
                function(pinned::buffs::TYPE_BY_ID)?,
                function(pinned::buffs::RESTORE)?,
                function(pinned::buffs::CLEANSE)?,
                function(pinned::world::TIME)?,
            );
            if loader
                .offset(pinned::buffs::RECORD_SIZE)
                .is_none_or(|size| size > RECORD)
            {
                return None;
            }
            Some(Layout {
                // SAFETY: the table's `buffs.type_by_id` is the address of the
                // game function with the `TypeByIdFn` signature on this
                // build, in the image, which stays mapped.
                type_by_id: unsafe { core::mem::transmute::<usize, TypeByIdFn>(type_by_id) },
                // SAFETY: the table's `buffs.restore` is the address of the
                // game function with the `RestoreFn` signature on this build,
                // in the image, which stays mapped.
                restore: unsafe { core::mem::transmute::<usize, RestoreFn>(restore) },
                // SAFETY: the table's `buffs.cleanse` is the address of the
                // game function with the `CleanseFn` signature on this build,
                // in the image, which stays mapped.
                cleanse: unsafe { core::mem::transmute::<usize, CleanseFn>(cleanse) },
                // SAFETY: the table's `world.time` is the address of the game
                // function with the `NowFn` signature on this build, in the
                // image, which stays mapped.
                now: unsafe { core::mem::transmute::<usize, NowFn>(now) },
                world: loader.offset(pinned::server::WORLD)?,
                game: loader.offset(pinned::server::PLAYER_INFOS)?,
                registry: loader.offset(pinned::buffs::REGISTRY)?,
                registry_blob: loader.offset(pinned::buffs::REGISTRY_BLOB)?,
                balancing: loader.offset(pinned::game::BALANCING)?,
                container: hash(container)?,
                groups,
                instance_size,
                instance_entity: field(instance, "buffEntityId")?,
                instance_type: field(instance, "typeId")?,
                instance_cleansed: field(instance, "cleansed")?,
                network: hash(network)?,
                network_lifetime: field(network, "lifeTime")?,
                network_end: field(network, "endOfLife")?,
                network_level: field(network, "level")?,
                lifetime: hash(lifetime)?,
                lifetime_total: field(lifetime, "lifeTime")?,
                lifetime_end: field(lifetime, "endOfLife")?,
                types: field("keen::BuffTypeCollectionResource", "buffs")?,
                type_size: size(ty)?,
                type_id: field(ty, "buffTypeId")?,
                type_slot: field(ty, "slot")?,
                type_duration: field(ty, "defaultLifeTime")?,
                type_name: field(ty, "name")?,
                type_hidden: field(ty, "hideInUi")?,
                type_saved: field(ty, "isSaved")?,
                record_type: loader.offset(pinned::buffs::RECORD_TYPE)?,
                record_lifetime: loader.offset(pinned::buffs::RECORD_LIFETIME)?,
                record_remaining: loader.offset(pinned::buffs::RECORD_REMAINING)?,
                record_level: loader.offset(pinned::buffs::RECORD_LEVEL)?,
                record_has_level: loader.offset(pinned::buffs::RECORD_HAS_LEVEL)?,
                record_strength: loader.offset(pinned::buffs::RECORD_STRENGTH)?,
                record_has_strength: loader.offset(pinned::buffs::RECORD_HAS_STRENGTH)?,
            })
        })
        .as_ref()
}

/// The server, its world, its game object and the game time, on the game
/// thread.
fn server(layout: &Layout) -> Result<(usize, usize, usize, i64), Status> {
    let server = crate::world::server()?;
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `server.world` offset holds its world
    // pointer.
    let world: usize = unsafe { read(server, layout.world) };
    // SAFETY: `server` comes from `crate::world::server`, on the game thread
    // that alone writes it; the table's `server.player_infos` offset holds
    // its game object pointer.
    let game: usize = unsafe { read(server, layout.game) };
    if world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `now` is the table's `world.time`, called with the server's live,
    // non-zero (`world == 0` returns first) world on the game thread (checked
    // by `crate::world::server`).
    let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
    Ok((server, world, game, now))
}

/// The buff registry, on the game thread.
fn registry(layout: &Layout, game: usize) -> Result<usize, Status> {
    // SAFETY: callers pass the non-zero game object `server` read on the game
    // thread; the table's `buffs.registry` offset holds its buff registry
    // pointer, set from its start.
    let registry: usize = unsafe { read(game, layout.registry) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    Ok(registry)
}

/// The type `id`'s data, or `None` for an unknown one.
fn find_type(layout: &Layout, registry: usize, id: u32) -> Option<usize> {
    // SAFETY: `type_by_id` is the table's `buffs.type_by_id`; callers pass
    // the non-zero live registry `registry` returned, on the game thread.
    let found = unsafe { (layout.type_by_id)(id, core::ptr::with_exposed_provenance(registry)) };
    (!found.is_null()).then(|| found.expose_provenance())
}

fn type_info(layout: &Layout, ty: usize) -> BuffTypeInfo {
    // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type` or
    // the registry's collection on the game thread; the reflected
    // `keen::BuffType` places its 16-byte `name` at `type_name`.
    let name: [u8; 16] = unsafe { read(ty, layout.type_name) };
    // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type` or
    // the registry's collection on the game thread; the reflected
    // `keen::BuffType` places its `isSaved` byte at `type_saved`.
    let saved: u8 = unsafe { read(ty, layout.type_saved) };
    // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type` or
    // the registry's collection on the game thread; the reflected
    // `keen::BuffType` places its `hideInUi` byte at `type_hidden`.
    let hidden: u8 = unsafe { read(ty, layout.type_hidden) };
    let mut flags = 0;
    if saved != 0 {
        flags |= BuffTypeInfo::SAVED;
    }
    if hidden != 0 {
        flags |= BuffTypeInfo::HIDDEN;
    }
    BuffTypeInfo {
        header: Header::of::<BuffTypeInfo>(),
        // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type`
        // or the registry's collection on the game thread; the reflected
        // `keen::BuffType` places its `u32` `buffTypeId` at `type_id`.
        id: unsafe { read(ty, layout.type_id) },
        // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type`
        // or the registry's collection on the game thread; the reflected
        // `keen::BuffType` places its `slot` byte at `type_slot`.
        slot: u16::from(unsafe { read::<u8>(ty, layout.type_slot) }),
        flags,
        name: enshwick_gamedata::reflect::fnv1a32(&name),
        // SAFETY: `ty` is a live `BuffType` of the registry, from `find_type`
        // or the registry's collection on the game thread; the reflected
        // `keen::BuffType` places its `i64` `defaultLifeTime` at
        // `type_duration`.
        duration: unsafe { read(ty, layout.type_duration) },
    }
}

/// Every buff type of the running game.
pub(crate) fn types(loader: &Loader) -> Result<Vec<BuffTypeInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, _, game, _) = server(layout)?;
    let registry = registry(layout, game)?;
    // SAFETY: `registry` is the non-zero live registry (checked by
    // `registry`), on the game thread (checked by `server`); the table's
    // `buffs.registry_blob` offset holds its collection's blob pointer.
    let blob: usize = unsafe { read(registry, layout.registry_blob) };
    if blob == 0 {
        return Err(Status::MISSING);
    }
    let header = blob + layout.types;
    // SAFETY: `blob` is non-zero (`blob == 0` returns first) and the reflected
    // `buffs` field of `keen::BuffTypeCollectionResource` is a blob array
    // header at `header`.
    let Some(BlobArray { start, count }) = (unsafe { BlobArray::read(header) }) else {
        return Ok(Vec::new());
    };
    if count > MAX_TYPES {
        return Err(Status::MISSING);
    }
    Ok((0..count)
        .map(|i| type_info(layout, start + i * layout.type_size))
        .collect())
}

/// The slots of `container`, each with its group's kind.
fn slots(layout: &Layout, container: usize) -> impl Iterator<Item = (usize, u16)> + '_ {
    layout.groups.iter().flat_map(move |group| {
        (0..group.count).map(move |i| {
            (
                container + group.start + i * layout.instance_size,
                group.kind,
            )
        })
    })
}

/// The buffs `entity` has.
pub(crate) fn of(loader: &Loader, entity: Entity) -> Result<Vec<ActiveBuff>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, _, _, now) = server(layout)?;
    let container = crate::world::component(loader, entity, layout.container)?.expose_provenance();
    let mut out = Vec::new();
    for (slot, kind) in slots(layout, container) {
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `u32` `buffEntityId` at
        // `instance_entity`.
        let buff: u32 = unsafe { read(slot, layout.instance_entity) };
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `cleansed` byte at
        // `instance_cleansed`.
        let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
        if buff == 0 || cleansed != 0 {
            continue;
        }
        let Ok(network) = crate::world::component(loader, Entity(buff), layout.network) else {
            continue;
        };
        let network = network.expose_provenance();
        // SAFETY: `network` is the buff entity's live `BuffNetworkData`,
        // which `crate::world::component` found on the game thread; the
        // reflected type places its `i64` `lifeTime` at `network_lifetime`.
        let total: i64 = unsafe { read(network, layout.network_lifetime) };
        // SAFETY: `network` is the buff entity's live `BuffNetworkData`,
        // which `crate::world::component` found on the game thread; the
        // reflected type places its `i64` `endOfLife` at `network_end`.
        let end: i64 = unsafe { read(network, layout.network_end) };
        out.push(ActiveBuff {
            header: Header::of::<ActiveBuff>(),
            // SAFETY: `slot` is a slot of the entity's live `BuffContainer`,
            // which `crate::world::component` found on the game thread; the
            // reflected `keen::ecs::BuffInstance` places its `u32` `typeId` at
            // `instance_type`.
            id: unsafe { read(slot, layout.instance_type) },
            entity: Entity(buff),
            slot: u32::from(kind),
            // SAFETY: `network` is the buff entity's live `BuffNetworkData`,
            // which `crate::world::component` found on the game thread; the
            // reflected type places its `level` at `network_level`, read as
            // the `u32` the ABI hands on.
            level: unsafe { read(network, layout.network_level) },
            duration: total,
            remaining: end.saturating_sub(now).max(0),
        });
    }
    Ok(out)
}

/// The type ids of the buffs `entity` has; empty for an entity without
/// buffs. On the game thread.
pub(crate) fn type_ids(loader: &Loader, entity: Entity) -> Vec<u32> {
    let Some(layout) = layout(loader) else {
        return Vec::new();
    };
    let Ok(container) = crate::world::component(loader, entity, layout.container) else {
        return Vec::new();
    };
    let container = container.expose_provenance();
    slots(layout, container)
        .filter_map(|(slot, _)| {
            // SAFETY: `slot` is a slot of the entity's live `BuffContainer`,
            // which `crate::world::component` found on the game thread; the
            // reflected `keen::ecs::BuffInstance` places its `u32`
            // `buffEntityId` at `instance_entity`.
            let buff: u32 = unsafe { read(slot, layout.instance_entity) };
            // SAFETY: `slot` is a slot of the entity's live `BuffContainer`,
            // which `crate::world::component` found on the game thread; the
            // reflected `keen::ecs::BuffInstance` places its `cleansed` byte at
            // `instance_cleansed`.
            let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
            // SAFETY: `slot` is a slot of the entity's live `BuffContainer`,
            // which `crate::world::component` found on the game thread; the
            // reflected `keen::ecs::BuffInstance` places its `u32` `typeId` at
            // `instance_type`.
            (buff != 0 && cleansed == 0).then(|| unsafe { read(slot, layout.instance_type) })
        })
        .collect()
}

/// Gives `request.entity` the buff `request.id`, or renews the one it has.
pub(crate) fn apply(loader: &Loader, request: &BuffRequest) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (server, _, game, now) = server(layout)?;
    if request.entity == Entity(0) || request.level > u32::from(u8::MAX) || request.duration < 0 {
        return Err(Status::BAD_ARGS);
    }
    let ty =
        find_type(layout, registry(layout, game)?, request.id.get()).ok_or(Status::BAD_ARGS)?;
    let duration = if request.duration == 0 {
        // SAFETY: `ty` is the non-null live `BuffType` `find_type` returned
        // on the game thread; the reflected `keen::BuffType` places its `i64`
        // `defaultLifeTime` at `type_duration`.
        unsafe { read::<i64>(ty, layout.type_duration) }
    } else {
        request.duration
    };
    let container =
        crate::world::component(loader, request.entity, layout.container)?.expose_provenance();
    for (slot, _) in slots(layout, container) {
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `u32` `typeId` at
        // `instance_type`.
        let id: u32 = unsafe { read(slot, layout.instance_type) };
        if id != request.id.get() {
            continue;
        }
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `cleansed` byte at
        // `instance_cleansed`.
        let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
        if cleansed != 0 {
            return Err(Status::REFUSED);
        }
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `u32` `buffEntityId` at
        // `instance_entity`.
        let buff: u32 = unsafe { read(slot, layout.instance_entity) };
        if buff != 0 {
            return renew(loader, layout, buff, duration, now);
        }
    }
    let container = crate::writes::component_mut(loader, request.entity.0, layout.container)?
        .expose_provenance();
    let mut record = [0u8; RECORD];
    let mut put = |at: usize, bytes: &[u8]| {
        if let Some(out) = record.get_mut(at..at + bytes.len()) {
            out.copy_from_slice(bytes);
        }
    };
    put(layout.record_type, &request.id.get().to_le_bytes());
    put(layout.record_lifetime, &duration.to_le_bytes());
    put(layout.record_remaining, &duration.to_le_bytes());
    if let Ok(level @ 1..) = u8::try_from(request.level) {
        put(layout.record_level, &[level]);
        put(layout.record_has_level, &[1]);
    }
    put(layout.record_strength, &STRENGTH.to_le_bytes());
    put(layout.record_has_strength, &[1]);
    // SAFETY: `game` is the non-zero game object `server` read on the game
    // thread; the table's `game.balancing` offset holds its balancing
    // registry pointer.
    let balancing: usize = unsafe { read(game, layout.balancing) };
    // SAFETY: `restore` is the table's `buffs.restore`, the game's restore of
    // a saved buff, called as the game calls it when a character spawns: on
    // the game thread at a quiet moment (checked by `server`), with the
    // entity's live container `component_mut` marked changed, a record of
    // `RECORD` bytes the build's `buffs.record_size` fits (checked in
    // `layout`), the server and its balancing registry.
    unsafe {
        (layout.restore)(
            core::ptr::with_exposed_provenance_mut(container),
            request.entity.0,
            record.as_ptr(),
            core::ptr::with_exposed_provenance_mut(server),
            core::ptr::with_exposed_provenance_mut(balancing),
            0,
        );
    };
    Ok(())
}

/// Renews the buff entity `buff` to last `duration` from `now`.
fn renew(
    loader: &Loader,
    layout: &Layout,
    buff: u32,
    duration: i64,
    now: i64,
) -> Result<(), Status> {
    let lifetime = crate::writes::component_mut(loader, buff, layout.lifetime)?.expose_provenance();
    // SAFETY: `lifetime` is the buff entity's live `LifeTime`, which
    // `component_mut` handed out for writing at a quiet moment on the game
    // thread; the reflected type places its `i64` `lifeTime` at
    // `lifetime_total`.
    unsafe { write(lifetime, layout.lifetime_total, duration) };
    // SAFETY: `lifetime` is the buff entity's live `LifeTime`, which
    // `component_mut` handed out for writing at a quiet moment on the game
    // thread; the reflected type places its `i64` `endOfLife` at
    // `lifetime_end`.
    unsafe { write(lifetime, layout.lifetime_end, now.saturating_add(duration)) };
    Ok(())
}

/// Cleanses every buff of type `id` from `entity`; whether it had one.
pub(crate) fn remove(loader: &Loader, entity: Entity, id: u32) -> Result<bool, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    server(layout)?;
    let container = crate::world::component(loader, entity, layout.container)?.expose_provenance();
    let had = slots(layout, container).any(|(slot, _)| {
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `u32` `typeId` at
        // `instance_type`.
        let ty = unsafe { read::<u32>(slot, layout.instance_type) };
        // SAFETY: `slot` is a slot of the entity's live `BuffContainer`, which
        // `crate::world::component` found on the game thread; the reflected
        // `keen::ecs::BuffInstance` places its `cleansed` byte at
        // `instance_cleansed`.
        let cleansed = unsafe { read::<u8>(slot, layout.instance_cleansed) };
        ty == id && cleansed == 0
    });
    if !had {
        return Ok(false);
    }
    let container =
        crate::writes::component_mut(loader, entity.0, layout.container)?.expose_provenance();
    // SAFETY: `cleanse` is the table's `buffs.cleanse`, called on the
    // entity's live container, which `component_mut` marked changed, on the
    // game thread at a quiet moment (checked by `server`).
    unsafe { (layout.cleanse)(core::ptr::with_exposed_provenance_mut(container), id) };
    Ok(true)
}
