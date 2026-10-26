//! Reads of the server's world for mods: components, templates and players.
//! Reads happen on the game thread; elsewhere they are refused. Calls that
//! change the world also wait for a quiet moment: between steps, or inside
//! a game event, where no ECS system runs. The world-step link tells the two
//! apart, since the game thread runs systems itself while it steps.
//!
//! The game thread is the dedicated server's, or on a client that hosts a
//! game the built-in server's `GameServer` thread; each hosted game has its
//! own server and thread, captured by the post-step link.

use core::cell::Cell;
use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use arrayvec::ArrayVec;
use enshwick::abi::{Header, HookFlags, PlayerInfo, Status};

use crate::chains::Next;
use crate::host::Loader;
use crate::memory::read;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "ecs.component_read_raw",
    "ecs.entity_map",
    "ecs.entity_map_capacity",
    "ecs.entity_records",
    "ecs.entity_slot",
    "ecs.record_template",
    "ecs.registry_type_map",
    "ecs.registry_type_map_fallback",
    "ecs.type_index",
    "server.permission_entry_size",
    "server.permission_mask",
    "server.permission_table",
    "server.player_infos",
    "server.player_slot_count",
    "server.player_slot_entity",
    "server.player_slot_key",
    "server.player_slot_machine",
    "server.player_slot_name",
    "server.player_slot_size",
    "server.player_slot_state",
    "server.player_slots",
    "server.world",
    "world.entities",
    "world.step",
];

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
        // SAFETY: `server` is non-zero (`server == 0` gives 0 first) and is the
        // server the post-step link was handed, on the game thread that runs
        // this link; `world_offset` is the table's `server.world` offset of its
        // world pointer, which only that thread writes.
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
    crate::stats::forget();
}

thread_local! {
    /// World steps this thread is inside.
    static STEPPING: Cell<u32> = const { Cell::new(0) };
    /// Loader hand-overs to mods inside a step where no system runs.
    static QUIET: Cell<u32> = const { Cell::new(0) };
}

/// Whether the world-step link is made; without it, writes are gated by
/// the thread alone.
static STEP_LINKED: AtomicBool = AtomicBool::new(false);

type WorldStepFn = unsafe extern "system" fn(world: *mut u8) -> usize;
// SAFETY: `WorldStepFn` is the signature of the table's `world.step`, and
// this slot is the `next` of the one link `link` makes on it, whose detour
// `world_step` alone calls `get`.
static WORLD_STEP_NEXT: Next<WorldStepFn> = unsafe { Next::new() };

/// Links the world step, which runs every ECS system of a world, the
/// server's on its game thread and on a client its own as well.
pub(crate) fn link(loader: &'static Loader) {
    let target = loader.address("world.step");
    let linked = loader.link_feature(
        "world-step gate",
        target,
        &WORLD_STEP_NEXT,
        world_step,
        HookFlags::NONE,
    );
    STEP_LINKED.store(linked, Ordering::Release);
    if !linked {
        crate::log!(WARN, "world writes are gated by the game thread only");
    }
}

unsafe extern "system" fn world_step(world: *mut u8) -> usize {
    STEPPING.set(STEPPING.get().wrapping_add(1));
    // SAFETY: `world` is the game's argument to `world.step`, passed on
    // unchanged; the chain is wired before the target jumps here, so the slot
    // holds the rest of that chain with the same signature.
    let result = unsafe { WORLD_STEP_NEXT.get()(world) };
    STEPPING.set(STEPPING.get().wrapping_sub(1));
    result
}

/// Marks a hand-over to mods inside a step where no system runs, such as
/// the game events at its start, until dropped.
#[derive(Debug)]
pub(crate) struct Quiet(());

impl Quiet {
    pub(crate) fn enter() -> Self {
        QUIET.set(QUIET.get().wrapping_add(1));
        Self(())
    }
}

impl Drop for Quiet {
    fn drop(&mut self) {
        QUIET.set(QUIET.get().wrapping_sub(1));
    }
}

/// Whether this thread may change the world now: outside a world step, or
/// in a quiet hand-over inside one.
fn quiet() -> bool {
    !STEP_LINKED.load(Ordering::Relaxed) || STEPPING.get() == 0 || QUIET.get() != 0
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
            // SAFETY: the table's `ecs.type_index` is the address of the game
            // function with the `TypeIndex` signature on this build, in the
            // image, which stays mapped.
            let type_index = unsafe { core::mem::transmute::<usize, TypeIndex>(type_index) };
            // SAFETY: the table's `ecs.component_read_raw` is the address of
            // the game function with the `ReadComponent` signature on this
            // build, in the image, which stays mapped.
            let read_component =
                unsafe { core::mem::transmute::<usize, ReadComponent>(read_component) };
            // SAFETY: the table's `ecs.entity_slot` is the address of the game
            // function with the `EntitySlot` signature on this build, in the
            // image, which stays mapped.
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

/// The server, when the caller is on the game thread.
pub(crate) fn reader() -> Result<usize, Status> {
    let server = SERVER.load(Ordering::Acquire);
    if server == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `GetCurrentThreadId` has no preconditions.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    if me != GAME_THREAD.load(Ordering::Acquire) {
        return Err(Status::WRONG_THREAD);
    }
    Ok(server)
}

/// The server, when the caller is on the game thread at a quiet moment,
/// where the world may change: [`Status::WRONG_THREAD`] inside a step.
pub(crate) fn server() -> Result<usize, Status> {
    let server = reader()?;
    if !quiet() {
        return Err(Status::WRONG_THREAD);
    }
    Ok(server)
}

/// The entity world: `*(server + world) + entities`.
fn entity_world(layout: &Layout, server: usize) -> Option<usize> {
    // SAFETY: callers pass the server `reader` returned, so this runs on the
    // game thread that alone writes it; the table's `server.world` offset
    // holds its world pointer.
    let world: usize = unsafe { read(server, layout.server_world) };
    (world != 0).then(|| world + layout.entity_world)
}

/// The server's entity world and the registry's index of the component
/// type `hash`, when the caller is on the game thread.
pub(crate) fn type_index(loader: &Loader, hash: u32) -> Result<(usize, *const u16), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world(layout, reader()?).ok_or(Status::MISSING)?;
    // SAFETY: `world` is the entity world, non-zero (checked by
    // `entity_world`), which starts with its component registry pointer at
    // the table's `world.entities` offset; read on the game thread (checked
    // by `reader`), which alone writes it.
    let registry: usize = unsafe { read(world, 0) };
    if registry == 0 {
        return Err(Status::MISSING);
    }
    let index = layout
        .registry_maps
        .iter()
        .map(|&map| {
            // SAFETY: `type_index` is the table's `ecs.type_index`; `registry`
            // is non-zero (`registry == 0` returns first) and `map` one of the
            // table's `ecs.registry_type_map` offsets of its type maps; `hash`
            // lives across the call, made on the game thread (checked by
            // `reader`).
            unsafe {
                (layout.type_index)(
                    core::ptr::with_exposed_provenance(registry + map),
                    &raw const hash,
                )
            }
        })
        .find(|index| !index.is_null())
        .ok_or(Status::MISSING)?;
    Ok((world, index))
}

/// A component's data pointer, or why not.
pub(crate) fn component(loader: &Loader, entity: u32, hash: u32) -> Result<*const c_void, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (world, index) = type_index(loader, hash)?;
    let mut out = [0usize; 2];
    // SAFETY: `read_component` is the table's `ecs.component_read_raw`;
    // `out` is two writable words on this stack, `world` the non-zero entity
    // world and `index` the non-null registry index `type_index` returned,
    // on the game thread (checked by `reader` inside `type_index`).
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
    let world = entity_world(layout, reader()?).ok_or(Status::MISSING)?;
    // SAFETY: `entity_slot` is the table's `ecs.entity_slot`; `world` is the
    // non-zero entity world and the table's `ecs.entity_map` offset its
    // id-to-slot map; `entity` lives across the call, made on the game
    // thread (checked by `reader`).
    let slot = unsafe {
        (layout.entity_slot)(
            core::ptr::with_exposed_provenance(world + layout.entity_map),
            &raw const entity,
        )
    };
    // SAFETY: `world` is the non-zero entity world; the table's
    // `ecs.entity_map_capacity` offset holds the map's capacity, which the
    // lookup returns for a missing entity; read on the game thread (checked
    // by `reader`), which alone writes it.
    let capacity: u32 = unsafe { read(world, layout.entity_capacity) };
    if slot >= capacity {
        return Err(Status::MISSING);
    }
    // SAFETY: `world` is the non-zero entity world; the table's
    // `ecs.entity_records` offset holds the pointer to its record table; read
    // on the game thread (checked by `reader`), which alone writes it.
    let records: usize = unsafe { read(world, layout.entity_records) };
    // SAFETY: the record table holds `capacity` record pointers and `slot <
    // capacity` (`slot >= capacity` returns first); read on the game thread
    // (checked by `reader`), which alone writes it.
    let record: usize = unsafe { read(records, slot as usize * size_of::<usize>()) };
    if record == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `record` is a non-zero (`record == 0` returns first) live record;
    // the table's `ecs.record_template` offset holds its template GUID pointer;
    // read on the game thread (checked by `reader`).
    let guid: usize = unsafe { read(record, layout.record_template) };
    if guid == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `guid` is non-zero (`guid == 0` returns first) and points at the
    // template's 16-byte GUID, which the template keeps while it is loaded.
    Ok(unsafe { read::<[u8; 16]>(guid, 0) })
}

/// Where `entity` stands, in 32.32 fixed-point metres, from its
/// `CurrentTransform`; on the server's game thread.
pub(crate) fn fixed_position(loader: &Loader, entity: u32) -> Option<[i64; 3]> {
    let info = loader
        .reflection
        .type_info("keen::ecs::CurrentTransform")
        .ok()?;
    let hash = type_hash(info.va())?;
    let data = component(loader, entity, hash).ok()?;
    // SAFETY: `data` is the non-null data of the entity's live
    // `CurrentTransform` (checked by `component`), which starts with its
    // position, three fixed-point `i64`; read on the game thread (checked by
    // `reader`), which alone writes it.
    Some(unsafe { enshwick::raw::read_at::<[i64; 3]>(data.cast(), 0) })
}

/// Every player in game, in slot order.
pub(crate) fn players(loader: &Loader) -> Result<Vec<PlayerInfo>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = reader()?;
    // SAFETY: `server` is the server `reader` returned on the game thread,
    // which alone writes it; the table's `server.player_infos` offset holds
    // its player infos pointer.
    let infos: usize = unsafe { read(server, layout.server_players) };
    Ok((0..layout.slot_count)
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the table's `server.player_slots`, `player_slot_size`
            // and `player_slot_count` place `slot_count` slots in the server,
            // `index < slot_count`, and each slot starts with its `u32`
            // handle; read on the game thread (checked by `reader`).
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_state` offset holds its state byte; read on
            // the game thread (checked by `reader`).
            let state: u8 = unsafe { read(slot, layout.slot_state) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_entity` offset holds its `u32` entity; read
            // on the game thread (checked by `reader`).
            let entity: u32 = unsafe { read(slot, layout.slot_entity) };
            (handle != 0 && state == SLOT_IN_GAME && entity != 0)
                .then(|| player_at(layout, slot, index, entity, infos))
        })
        .collect())
}

/// Player slots [`player_entities`] reads; the game has 16.
pub(crate) const MAX_PLAYERS: usize = 64;

/// The character entity of each player in game, as [`players`] lists them,
/// without their names and permissions.
pub(crate) fn player_entities(loader: &Loader) -> Result<ArrayVec<u32, MAX_PLAYERS>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = reader()?;
    Ok((0..layout.slot_count.min(MAX_PLAYERS))
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the table's `server.player_slots`, `player_slot_size`
            // and `player_slot_count` place `slot_count` slots in the server,
            // `index < slot_count`, and each slot starts with its `u32`
            // handle; read on the game thread (checked by `reader`).
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_state` offset holds its state byte; read on
            // the game thread (checked by `reader`).
            let state: u8 = unsafe { read(slot, layout.slot_state) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_entity` offset holds its `u32` entity; read
            // on the game thread (checked by `reader`).
            let entity: u32 = unsafe { read(slot, layout.slot_entity) };
            (handle != 0 && state == SLOT_IN_GAME && entity != 0).then_some(entity)
        })
        .collect())
}

/// The machine handle of each player in game, by account key: the handle
/// the server's network code names a connection by.
pub(crate) fn machines(loader: &Loader) -> Result<Vec<(u64, u32)>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = reader()?;
    Ok((0..layout.slot_count)
        .filter_map(|index| {
            let slot = server + layout.slots + index * layout.slot_size;
            // SAFETY: the table's `server.player_slots`, `player_slot_size`
            // and `player_slot_count` place `slot_count` slots in the server,
            // `index < slot_count`, and each slot starts with its `u32`
            // handle; read on the game thread (checked by `reader`).
            let handle: u32 = unsafe { read(slot, 0) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_machine` offset holds its `u32` machine
            // handle; read on the game thread (checked by `reader`).
            let machine: u32 = unsafe { read(slot, layout.slot_machine) };
            // SAFETY: `slot` is a player slot of the server (`index <
            // slot_count`, slots from the table); the table's
            // `server.player_slot_key` offset holds its `u64` account key;
            // read on the game thread (checked by `reader`).
            let key: u64 = unsafe { read(slot, layout.slot_key) };
            (handle != 0 && machine != 0).then_some((key, machine))
        })
        .collect())
}

/// The account keys of the live player slots, as [`machines`] reads them;
/// `None` off the game thread or when the slots do not read.
pub(crate) fn live_keys() -> Option<Vec<u64>> {
    let machines = machines(crate::host::loader()?).ok()?;
    Some(machines.iter().map(|&(key, _)| key).collect())
}

/// Entries in the player infos' permission table, one per entity id from 1.
const PERMISSION_ENTRIES: usize = 0x3ff;

fn player_at(layout: &Layout, slot: usize, index: usize, entity: u32, infos: usize) -> PlayerInfo {
    let mut name = [0u8; 64];
    // SAFETY: `slot` is a player slot `players` found in the server on the
    // game thread; the table's `server.player_slot_name` offset holds the
    // name's data pointer.
    let data: usize = unsafe { read(slot, layout.slot_name) };
    // SAFETY: `slot` is a player slot `players` found in the server on the
    // game thread; the name's `u64` byte length follows its data pointer at
    // the table's `server.player_slot_name` offset.
    let len: u64 = unsafe { read(slot, layout.slot_name + size_of::<usize>()) };
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    let mut name_len = 0;
    if data != 0 && len <= MAX_NAME {
        let kept = len.min(name.len());
        for (at, byte) in name[..kept].iter_mut().enumerate() {
            // SAFETY: `data` is non-zero (the `data != 0` condition) and the
            // name buffer holds `len` bytes, `at < kept <= len`; read on the
            // game thread.
            *byte = unsafe { read(data, at) };
        }
        name_len = kept;
    }
    let permissions = if infos == 0 {
        0
    } else {
        let table = infos + layout.permission_table;
        let index = (entity as usize).wrapping_sub(1);
        if index < PERMISSION_ENTRIES {
            let entry = table + index * layout.permission_entry;
            // SAFETY: `infos` is non-zero (`infos == 0` gives 0 first); the
            // table's `server.permission_table` and `permission_entry_size`
            // place `PERMISSION_ENTRIES` entries there, `index` is below that
            // (the enclosing condition), and each starts with its `u32` entity;
            // read on the game thread.
            let owner: u32 = unsafe { read(entry, 0) };
            if owner == entity {
                // SAFETY: `entry` is a permission entry inside the table
                // (`index < PERMISSION_ENTRIES`, the enclosing condition); the
                // table's `server.permission_mask` offset holds its mask byte;
                // read on the game thread.
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
        // SAFETY: `slot` is a player slot `players` found in the server on
        // the game thread; the table's `server.player_slot_key` offset holds
        // its `u64` account key.
        key: unsafe { read(slot, layout.slot_key) },
        permissions,
        name_len: u8::try_from(name_len).unwrap_or(0),
        name,
    }
}

/// The ECS type hash of a reflected type: the low 32 bits of its descriptor's
/// hash field.
pub(crate) fn type_hash(descriptor: u64) -> Option<u32> {
    let address =
        usize::try_from(descriptor.checked_add(enshwick_reflect::DESCRIPTOR_HASH)?).ok()?;
    // SAFETY: callers pass the address of a reflected type descriptor in the
    // game's image, which stays mapped and is not written after load;
    // `DESCRIPTOR_HASH` is the offset of its eight-byte hash field.
    let value: u64 = unsafe { read(address, 0) };
    #[expect(clippy::cast_possible_truncation, reason = "the hash is the low half")]
    Some(value as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_wait_for_a_quiet_moment_inside_a_step() {
        STEP_LINKED.store(true, Ordering::Relaxed);
        assert!(quiet());
        STEPPING.set(1);
        assert!(!quiet());
        {
            let _quiet = Quiet::enter();
            assert!(quiet());
        }
        assert!(!quiet());
        STEPPING.set(0);
        assert!(quiet());
    }
}
