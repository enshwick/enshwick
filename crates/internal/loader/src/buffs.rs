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

use enshwick::abi::{ActiveBuff, BuffRequest, BuffTypeInfo, Header, Status};

use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "buffs.cleanse",
    "buffs.record_has_level",
    "buffs.record_has_strength",
    "buffs.record_level",
    "buffs.record_lifetime",
    "buffs.record_remaining",
    "buffs.record_size",
    "buffs.record_strength",
    "buffs.record_type",
    "buffs.registry",
    "buffs.registry_blob",
    "buffs.restore",
    "buffs.type_by_id",
    "game.balancing",
    "server.player_infos",
    "server.world",
    "world.time",
];

/// The room the loader keeps for the saved-buff record the restore reads;
/// the build's record must fit.
const RECORD: usize = 0x100;
/// More buff types than any build has: a count past it is not a count.
const MAX_TYPES: u32 = 1 << 16;
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
            let function = |name: &str| {
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
                function("buffs.type_by_id")?,
                function("buffs.restore")?,
                function("buffs.cleanse")?,
                function("world.time")?,
            );
            if loader
                .offset("buffs.record_size")
                .is_none_or(|size| size > RECORD)
            {
                return None;
            }
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                type_by_id: unsafe { core::mem::transmute::<usize, TypeByIdFn>(type_by_id) },
                // SAFETY: as above.
                restore: unsafe { core::mem::transmute::<usize, RestoreFn>(restore) },
                // SAFETY: as above.
                cleanse: unsafe { core::mem::transmute::<usize, CleanseFn>(cleanse) },
                // SAFETY: as above.
                now: unsafe { core::mem::transmute::<usize, NowFn>(now) },
                world: loader.offset("server.world")?,
                game: loader.offset("server.player_infos")?,
                registry: loader.offset("buffs.registry")?,
                registry_blob: loader.offset("buffs.registry_blob")?,
                balancing: loader.offset("game.balancing")?,
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
                record_type: loader.offset("buffs.record_type")?,
                record_lifetime: loader.offset("buffs.record_lifetime")?,
                record_remaining: loader.offset("buffs.record_remaining")?,
                record_level: loader.offset("buffs.record_level")?,
                record_has_level: loader.offset("buffs.record_has_level")?,
                record_strength: loader.offset("buffs.record_strength")?,
                record_has_strength: loader.offset("buffs.record_has_strength")?,
            })
        })
        .as_ref()
}

/// Reads a `T` at `base + at`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: usize, at: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { crate::world::read(base, at) }
}

/// Writes a `T` at `base + at`.
///
/// # Safety
///
/// The address holds a writable `T`.
unsafe fn write<T: Copy>(base: usize, at: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<T>(base.wrapping_add(at)).write_unaligned(value);
    };
}

/// The server, its world, its game object and the game time, on the game
/// thread.
fn server(layout: &Layout) -> Result<(usize, usize, usize, i64), Status> {
    let server = crate::world::server()?;
    // SAFETY: the server holds its world and its game object.
    let world: usize = unsafe { read(server, layout.world) };
    // SAFETY: as above.
    let game: usize = unsafe { read(server, layout.game) };
    if world == 0 || game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game's time of its live world, on the game thread.
    let now = unsafe { (layout.now)(core::ptr::with_exposed_provenance(world)) };
    Ok((server, world, game, now))
}

/// The buff registry, on the game thread.
fn registry(layout: &Layout, game: usize) -> Result<usize, Status> {
    // SAFETY: the game object holds the buff registry from its start.
    let registry: usize = unsafe { read(game, layout.registry) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    Ok(registry)
}

/// The type `id`'s data, or `None` for an unknown one.
fn find_type(layout: &Layout, registry: usize, id: u32) -> Option<usize> {
    // SAFETY: the game's lookup on its live registry.
    let found = unsafe { (layout.type_by_id)(id, core::ptr::with_exposed_provenance(registry)) };
    (!found.is_null()).then(|| found.addr())
}

fn type_info(layout: &Layout, ty: usize) -> BuffTypeInfo {
    // SAFETY: a live `BuffType` of the registry; its fields as reflected.
    let name: [u8; 16] = unsafe { read(ty, layout.type_name) };
    // SAFETY: as above.
    let saved: u8 = unsafe { read(ty, layout.type_saved) };
    // SAFETY: as above.
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
        // SAFETY: as above.
        id: unsafe { read(ty, layout.type_id) },
        // SAFETY: as above.
        slot: u16::from(unsafe { read::<u8>(ty, layout.type_slot) }),
        flags,
        name: enshwick_reflect::fnv1a32(&name),
        // SAFETY: as above.
        duration: unsafe { read(ty, layout.type_duration) },
    }
}

/// Every buff type of the running game.
pub(crate) fn types(loader: &Loader) -> Result<Vec<BuffTypeInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, _, game, _) = server(layout)?;
    let registry = registry(layout, game)?;
    // SAFETY: the registry holds its collection's blob.
    let blob: usize = unsafe { read(registry, layout.registry_blob) };
    if blob == 0 {
        return Err(Status::MISSING);
    }
    let header = blob + layout.types;
    // SAFETY: the blob's array header: an offset from itself and a count.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: as above.
    let count: u32 = unsafe { read(header, 4) };
    if count > MAX_TYPES {
        return Err(Status::MISSING);
    }
    let start = header.wrapping_add_signed(offset as isize);
    Ok((0..count as usize)
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
pub(crate) fn of(loader: &Loader, entity: u32) -> Result<Vec<ActiveBuff>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, _, _, now) = server(layout)?;
    let container = crate::world::component(loader, entity, layout.container)?.addr();
    let mut out = Vec::new();
    for (slot, kind) in slots(layout, container) {
        // SAFETY: a slot of the live container, as reflected.
        let buff: u32 = unsafe { read(slot, layout.instance_entity) };
        // SAFETY: as above.
        let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
        if buff == 0 || cleansed != 0 {
            continue;
        }
        let Ok(network) = crate::world::component(loader, buff, layout.network) else {
            continue;
        };
        let network = network.addr();
        // SAFETY: the buff entity's live `BuffNetworkData`, as reflected.
        let total: i64 = unsafe { read(network, layout.network_lifetime) };
        // SAFETY: as above.
        let end: i64 = unsafe { read(network, layout.network_end) };
        out.push(ActiveBuff {
            header: Header::of::<ActiveBuff>(),
            // SAFETY: as for the slot above.
            id: unsafe { read(slot, layout.instance_type) },
            entity: buff,
            slot: u32::from(kind),
            // SAFETY: as for the network data above.
            level: unsafe { read(network, layout.network_level) },
            duration: total,
            remaining: end.saturating_sub(now).max(0),
        });
    }
    Ok(out)
}

/// The type ids of the buffs `entity` has; empty for an entity without
/// buffs. On the game thread.
pub(crate) fn type_ids(loader: &Loader, entity: u32) -> Vec<u32> {
    let Some(layout) = layout(loader) else {
        return Vec::new();
    };
    let Ok(container) = crate::world::component(loader, entity, layout.container) else {
        return Vec::new();
    };
    let container = container.addr();
    slots(layout, container)
        .filter_map(|(slot, _)| {
            // SAFETY: a slot of the live container, as reflected, read on
            // the game thread.
            let buff: u32 = unsafe { read(slot, layout.instance_entity) };
            // SAFETY: as above.
            let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
            // SAFETY: as above.
            (buff != 0 && cleansed == 0).then(|| unsafe { read(slot, layout.instance_type) })
        })
        .collect()
}

/// Gives `request.entity` the buff `request.id`, or renews the one it has.
pub(crate) fn apply(loader: &Loader, request: &BuffRequest) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (server, _, game, now) = server(layout)?;
    if request.entity == 0 || request.level > u32::from(u8::MAX) || request.duration < 0 {
        return Err(Status::BAD_ARGS);
    }
    let ty = find_type(layout, registry(layout, game)?, request.id).ok_or(Status::BAD_ARGS)?;
    let duration = if request.duration == 0 {
        // SAFETY: a live `BuffType`, as reflected.
        unsafe { read::<i64>(ty, layout.type_duration) }
    } else {
        request.duration
    };
    let container = crate::world::component(loader, request.entity, layout.container)?.addr();
    for (slot, _) in slots(layout, container) {
        // SAFETY: a slot of the live container, as reflected.
        let id: u32 = unsafe { read(slot, layout.instance_type) };
        if id != request.id {
            continue;
        }
        // SAFETY: as above.
        let cleansed: u8 = unsafe { read(slot, layout.instance_cleansed) };
        if cleansed != 0 {
            return Err(Status::REFUSED);
        }
        // SAFETY: as above.
        let buff: u32 = unsafe { read(slot, layout.instance_entity) };
        if buff != 0 {
            return renew(loader, layout, buff, duration, now);
        }
    }
    let container = crate::writes::component_mut(loader, request.entity, layout.container)?.addr();
    let mut record = [0u8; RECORD];
    let mut put = |at: usize, bytes: &[u8]| {
        if let Some(out) = record.get_mut(at..at + bytes.len()) {
            out.copy_from_slice(bytes);
        }
    };
    put(layout.record_type, &request.id.to_le_bytes());
    put(layout.record_lifetime, &duration.to_le_bytes());
    put(layout.record_remaining, &duration.to_le_bytes());
    if let Ok(level @ 1..) = u8::try_from(request.level) {
        put(layout.record_level, &[level]);
        put(layout.record_has_level, &[1]);
    }
    put(layout.record_strength, &STRENGTH.to_le_bytes());
    put(layout.record_has_strength, &[1]);
    // SAFETY: the game object holds its balancing registry.
    let balancing: usize = unsafe { read(game, layout.balancing) };
    // SAFETY: the game's restore of a saved buff, with the entity's live
    // container marked changed, a full record, its server and balancing
    // registry, on the game thread between steps, where the game calls it
    // when a character spawns.
    unsafe {
        (layout.restore)(
            core::ptr::with_exposed_provenance_mut(container),
            request.entity,
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
    let lifetime = crate::writes::component_mut(loader, buff, layout.lifetime)?.addr();
    // SAFETY: the buff entity's live `LifeTime`, handed out for writing.
    unsafe { write(lifetime, layout.lifetime_total, duration) };
    // SAFETY: as above.
    unsafe { write(lifetime, layout.lifetime_end, now.saturating_add(duration)) };
    Ok(())
}

/// Cleanses every buff of type `id` from `entity`; whether it had one.
pub(crate) fn remove(loader: &Loader, entity: u32, id: u32) -> Result<bool, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    server(layout)?;
    let container = crate::world::component(loader, entity, layout.container)?.addr();
    let had = slots(layout, container).any(|(slot, _)| {
        // SAFETY: a slot of the live container, as reflected.
        let ty = unsafe { read::<u32>(slot, layout.instance_type) };
        // SAFETY: as above.
        let cleansed = unsafe { read::<u8>(slot, layout.instance_cleansed) };
        ty == id && cleansed == 0
    });
    if !had {
        return Ok(false);
    }
    let container = crate::writes::component_mut(loader, entity, layout.container)?.addr();
    // SAFETY: the game's cleanse on the entity's live container, marked
    // changed, on the game thread between steps.
    unsafe { (layout.cleanse)(core::ptr::with_exposed_provenance_mut(container), id) };
    Ok(true)
}
