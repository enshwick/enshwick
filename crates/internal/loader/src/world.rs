//! Reads of the server's world for mods: components, templates and players.
//! Every read happens on the game thread, between steps or inside a game
//! event, where no ECS system runs; elsewhere it is refused.
//!
//! The game thread is the dedicated server's, or on a client that hosts a
//! game the built-in server's `GameServer` thread; each hosted game has its
//! own server and thread, captured by the post-step link.

use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use enshwick_loader_api::abi::{Header, PlayerInfo, Status};

use crate::host::Loader;

/// The server object, seen by the post-step hook.
pub(crate) static SERVER: AtomicUsize = AtomicUsize::new(0);
/// The game thread's id, seen by the post-step hook.
pub(crate) static GAME_THREAD: AtomicU32 = AtomicU32::new(0);
/// The server's entity world, seen by the post-step hook.
static SERVER_WORLD: AtomicUsize = AtomicUsize::new(0);

/// Records the server seen after a step on `thread`, and its entity world
/// from `world_offset` and `entities`.
pub(crate) fn capture(server: usize, thread: u32, world_offset: usize, entities: usize) {
    SERVER.store(server, Ordering::Release);
    GAME_THREAD.store(thread, Ordering::Release);
    let world = if server == 0 {
        0
    } else {
        // SAFETY: the server holds its world pointer, read on its own thread.
        unsafe { read::<usize>(server, world_offset) }
    };
    let entity_world = if world == 0 { 0 } else { world + entities };
    SERVER_WORLD.store(entity_world, Ordering::Release);
}

/// Forgets the server of a hosted game that stopped.
pub(crate) fn release() {
    SERVER_WORLD.store(0, Ordering::Release);
    SERVER.store(0, Ordering::Release);
    GAME_THREAD.store(0, Ordering::Release);
}

/// Whether a server has been seen and not released.
pub(crate) fn hosting() -> bool {
    SERVER.load(Ordering::Acquire) != 0
}

/// The running server's entity world, 0 for none.
pub(crate) fn server_world() -> u64 {
    SERVER_WORLD.load(Ordering::Acquire) as u64
}

type TypeIndex = unsafe extern "system" fn(map: *const u8, hash: *const u32) -> *const u16;
type ReadComponent = unsafe extern "system" fn(
    out: *mut [usize; 2],
    world: *const u8,
    index: *const u16,
    entity: u32,
);
type EntitySlot = unsafe extern "system" fn(map: *const u8, entity: *const u32) -> u32;

/// The game functions and offsets the reads use, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    type_index: TypeIndex,
    read_component: ReadComponent,
    entity_slot: EntitySlot,
    server_world: usize,
    entity_world: usize,
    registry_maps: [usize; 2],
    entity_map: usize,
    entity_capacity: usize,
    entity_records: usize,
    record_template: usize,
    server_players: usize,
    permission_table: usize,
    permission_entry: usize,
    permission_mask: usize,
    slots: usize,
    slot_size: usize,
    slot_count: usize,
    slot_state: usize,
    slot_entity: usize,
    slot_key: usize,
    slot_name: usize,
    slot_machine: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

/// Player slots in game, for the joined-player reads.
const SLOT_IN_GAME: u8 = 3;
/// Names longer than this are not a client's name.
const MAX_NAME: usize = 0x100;

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let symbol = |name: &str| loader.build.symbol(name).map(|rva| loader.base + rva);
            let constant = |name: &str| {
                loader
                    .build
                    .constant(name)
                    .and_then(|v| usize::try_from(v).ok())
            };
            let function = |name: &str| symbol(name).and_then(|a| usize::try_from(a).ok());
            let type_index = function("ecs.type_index")?;
            let read_component = function("ecs.component_read_raw")?;
            let entity_slot = function("ecs.entity_slot")?;
            // SAFETY: each symbol is the game function with this signature on
            // this build.
            let type_index = unsafe { core::mem::transmute::<usize, TypeIndex>(type_index) };
            // SAFETY: as above.
            let read_component =
                unsafe { core::mem::transmute::<usize, ReadComponent>(read_component) };
            // SAFETY: as above.
            let entity_slot = unsafe { core::mem::transmute::<usize, EntitySlot>(entity_slot) };
            Some(Layout {
                type_index,
                read_component,
                entity_slot,
                server_world: constant("server.world")?,
                entity_world: constant("world.entities")?,
                registry_maps: [
                    constant("ecs.registry_type_map")?,
                    constant("ecs.registry_type_map_fallback")?,
                ],
                entity_map: constant("ecs.entity_map")?,
                entity_capacity: constant("ecs.entity_map_capacity")?,
                entity_records: constant("ecs.entity_records")?,
                record_template: constant("ecs.record_template")?,
                server_players: constant("server.player_infos")?,
                permission_table: constant("server.permission_table")?,
                permission_entry: constant("server.permission_entry_size")?,
                permission_mask: constant("server.permission_mask")?,
                slots: constant("server.player_slots")?,
                slot_size: constant("server.player_slot_size")?,
                slot_count: constant("server.player_slot_count")?,
                slot_state: constant("server.player_slot_state")?,
                slot_entity: constant("server.player_slot_entity")?,
                slot_key: constant("server.player_slot_key")?,
                slot_name: constant("server.player_slot_name")?,
                slot_machine: constant("server.player_slot_machine")?,
            })
        })
        .as_ref()
}

/// Reads a `T` at `base + offset`.
///
/// # Safety
///
/// The address holds a readable `T`.
pub(crate) unsafe fn read<T: Copy>(base: usize, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { core::ptr::with_exposed_provenance::<T>(base.wrapping_add(offset)).read_unaligned() }
}

/// The server, when the caller is on the game thread.
pub(crate) fn server() -> Result<usize, Status> {
    let server = SERVER.load(Ordering::Acquire);
    if server == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: no preconditions.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    if me != GAME_THREAD.load(Ordering::Acquire) {
        return Err(Status::WRONG_THREAD);
    }
    Ok(server)
}

/// The entity world: `*(server + world) + entities`.
fn entity_world(layout: &Layout, server: usize) -> Option<usize> {
    // SAFETY: the server holds its world pointer.
    let world: usize = unsafe { read(server, layout.server_world) };
    (world != 0).then(|| world + layout.entity_world)
}

/// A component's data pointer, or why not.
pub(crate) fn component(loader: &Loader, entity: u32, hash: u32) -> Result<*const c_void, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world(layout, server()?).ok_or(Status::MISSING)?;
    // SAFETY: the entity world starts with its component registry.
    let registry: usize = unsafe { read(world, 0) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    let index = layout
        .registry_maps
        .iter()
        .map(|&map| {
            // SAFETY: the registry's type maps, on the game thread between steps.
            unsafe {
                (layout.type_index)(
                    core::ptr::with_exposed_provenance(registry + map),
                    &raw const hash,
                )
            }
        })
        .find(|index| !index.is_null())
        .ok_or(Status::MISSING)?;
    let mut out = [0usize; 2];
    // SAFETY: as above; `index` points into the registry.
    unsafe {
        (layout.read_component)(
            &raw mut out,
            core::ptr::with_exposed_provenance(world),
            index,
            entity,
        );
    };
    match out[0] {
        0 => Err(Status::MISSING),
        data => Ok(core::ptr::with_exposed_provenance(data)),
    }
}

/// An entity's template GUID.
pub(crate) fn template(loader: &Loader, entity: u32) -> Result<[u8; 16], Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world(layout, server()?).ok_or(Status::MISSING)?;
    // SAFETY: the entity world's id-to-slot map, on the game thread.
    let slot = unsafe {
        (layout.entity_slot)(
            core::ptr::with_exposed_provenance(world + layout.entity_map),
            &raw const entity,
        )
    };
    // SAFETY: the map's capacity, which a missing entity returns.
    let capacity: u32 = unsafe { read(world, layout.entity_capacity) };
    if slot >= capacity {
        return Err(Status::MISSING);
    }
    // SAFETY: the record table holds `capacity` pointers.
    let records: usize = unsafe { read(world, layout.entity_records) };
    // SAFETY: as above.
    let record: usize = unsafe { read(records, slot as usize * size_of::<usize>()) };
    if record == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: a live record points at its template GUID.
    let guid: usize = unsafe { read(record, layout.record_template) };
    if guid == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: a GUID is 16 bytes.
    Ok(unsafe { read::<[u8; 16]>(guid, 0) })
}

/// Every player in game, in slot order.
pub(crate) fn players(loader: &Loader) -> Result<Vec<PlayerInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = server()?;
    // SAFETY: the server's player infos object, for permissions.
    let infos: usize = unsafe { read(server, layout.server_players) };
    Ok((0..layout.slot_count)
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the server holds `slot_count` player slots.
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: as above.
            let state: u8 = unsafe { read(slot, layout.slot_state) };
            // SAFETY: as above.
            let entity: u32 = unsafe { read(slot, layout.slot_entity) };
            (handle != 0 && state == SLOT_IN_GAME && entity != 0)
                .then(|| player_at(layout, slot, index, entity, infos))
        })
        .collect())
}

/// The machine handle of each player in game, by account key: the handle
/// the server's network code names a connection by.
pub(crate) fn machines(loader: &Loader) -> Result<Vec<(u64, u32)>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = server()?;
    Ok((0..layout.slot_count)
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the server holds `slot_count` player slots.
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: as above.
            let machine: u32 = unsafe { read(slot, layout.slot_machine) };
            // SAFETY: as above.
            let key: u64 = unsafe { read(slot, layout.slot_key) };
            (handle != 0 && machine != 0).then_some((key, machine))
        })
        .collect())
}

fn player_at(layout: &Layout, slot: usize, index: usize, entity: u32, infos: usize) -> PlayerInfo {
    let mut name = [0u8; 64];
    // SAFETY: the slot's name is a pointer and a byte length.
    let data: usize = unsafe { read(slot, layout.slot_name) };
    // SAFETY: as above.
    let len: u64 = unsafe { read(slot, layout.slot_name + size_of::<usize>()) };
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    let mut name_len = 0;
    if data != 0 && len <= MAX_NAME {
        let kept = len.min(name.len());
        // SAFETY: the name buffer holds `len` bytes.
        let bytes = unsafe {
            core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(data), kept)
        };
        name[..kept].copy_from_slice(bytes);
        name_len = kept;
    }
    let permissions = if infos == 0 {
        0
    } else {
        let table = infos + layout.permission_table;
        let index = (entity as usize).wrapping_sub(1);
        if index < 0x3ff {
            let entry = table + index * layout.permission_entry;
            // SAFETY: the table holds 0x3ff entries; each starts with its entity.
            let owner: u32 = unsafe { read(entry, 0) };
            if owner == entity {
                // SAFETY: as above.
                unsafe { read::<u8>(entry, layout.permission_mask) }
            } else {
                0
            }
        } else {
            0
        }
    };
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::try_from(index).unwrap_or(u32::MAX),
        entity,
        // SAFETY: as for the other slot fields.
        key: unsafe { read(slot, layout.slot_key) },
        permissions,
        name_len: u8::try_from(name_len).unwrap_or(0),
        name,
    }
}

/// The ECS type hash of a reflected type: the low 32 bits of its descriptor's
/// hash field.
pub(crate) fn type_hash(loader: &Loader, descriptor: u64) -> Option<u32> {
    let offset = loader.build.constant("reflect.type_hash")?;
    let address = usize::try_from(descriptor.checked_add(offset)?).ok()?;
    // SAFETY: a type descriptor in the game's image, which stays mapped.
    let value: u64 = unsafe { read(address, 0) };
    #[expect(clippy::cast_possible_truncation, reason = "the hash is the low half")]
    Some(value as u32)
}
