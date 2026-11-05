//! Entity queries and destroy requests on the server's game thread.
//!
//! The query is the engine's own walk over every archetype that holds a
//! component type and a transform, the one its game code uses to find
//! placed things. A destroy is a request in the entity world's first command
//! buffer, which the engine plays back after the systems of the next step,
//! as it does for its own game-thread code.

use std::sync::{Mutex, OnceLock};

use enshwick::Entity;
use enshwick::abi::{EntityPlace, Status};
use rustc_hash::FxHashSet;

use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;
use crate::pinned::{Constant, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::COMMAND_BUFFERS.id(),
    pinned::ecs::DESTROY_REQUEST.id(),
    pinned::ecs::ENTITIES_ENTRY_SIZE.id(),
    pinned::ecs::ENTITIES_ENTRY_TRANSFORM.id(),
    pinned::ecs::ENTITIES_WITH.id(),
];

/// The engine's growable array the query fills; the loader gives it no grow
/// function, so a full array ends the walk with 0.
#[repr(C)]
struct Found {
    entries: *mut u8,
    count: u64,
    capacity: u64,
    grow: usize,
}

type QueryFn = unsafe extern "system" fn(out: *mut Found, entity_world: *mut u8, ty: u32) -> u8;
type DestroyFn = unsafe extern "system" fn(buffer: *mut u8, requester: u32, entity: u32) -> u8;

/// The game functions and offsets, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    query: QueryFn,
    destroy: DestroyFn,
    entry_size: usize,
    entry_transform: usize,
    command_buffers: usize,
}

/// Entries the first walk makes room for.
const FIRST_CAPACITY: usize = 1024;
/// The most entries a query returns.
const MAX_CAPACITY: usize = 1 << 19;

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let function = |name: Symbol| {
                loader
                    .build
                    .symbol(name.id())
                    .and_then(|rva| usize::try_from(loader.base + rva).ok())
            };
            let constant = |name: Constant| {
                loader
                    .build
                    .constant(name.id())
                    .and_then(|v| usize::try_from(v).ok())
            };
            let (query, destroy) = (
                function(pinned::ecs::ENTITIES_WITH)?,
                function(pinned::ecs::DESTROY_REQUEST)?,
            );
            Some(Layout {
                // SAFETY: the build table's `ecs.entities_with` is the
                // address of the game function with the `QueryFn` signature
                // on this build.
                query: unsafe { core::mem::transmute::<usize, QueryFn>(query) },
                // SAFETY: the build table's `ecs.destroy_request` is the
                // address of the game function with the `DestroyFn` signature
                // on this build.
                destroy: unsafe { core::mem::transmute::<usize, DestroyFn>(destroy) },
                entry_size: constant(pinned::ecs::ENTITIES_ENTRY_SIZE)?,
                entry_transform: constant(pinned::ecs::ENTITIES_ENTRY_TRANSFORM)?,
                command_buffers: constant(pinned::ecs::COMMAND_BUFFERS)?,
            })
        })
        .as_ref()
}

/// The server's entity world, when the caller is on its game thread.
fn entity_world() -> Result<usize, Status> {
    crate::world::server()?;
    usize::try_from(crate::world::server_world())
        .ok()
        .filter(|&world| world != 0)
        .ok_or(Status::MISSING)
}

/// Every entity with a component of type `ty` and a transform, with its
/// position in 32.32 fixed-point metres.
pub(crate) fn with_component(loader: &Loader, ty: u32) -> Result<Vec<EntityPlace>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world()?;
    let mut capacity = FIRST_CAPACITY;
    loop {
        let mut entries = vec![0u8; capacity * layout.entry_size];
        let mut found = Found {
            entries: entries.as_mut_ptr(),
            count: 0,
            capacity: capacity as u64,
            grow: 0,
        };
        // SAFETY: `entity_world` checked that the caller is on the server's
        // game thread and the world is non-null; `found` points at `entries`,
        // which has room for `capacity` entries of the table's
        // `ecs.entities_entry_size`, and no grow function, so the engine
        // writes no further.
        let whole = unsafe {
            (layout.query)(
                &raw mut found,
                core::ptr::with_exposed_provenance_mut(world),
                ty,
            )
        };
        if whole != 0 || capacity >= MAX_CAPACITY {
            let count = usize::try_from(found.count).unwrap_or(0).min(capacity);
            return Ok(entries
                .chunks_exact(layout.entry_size)
                .take(count)
                .map(|entry| place(layout, entry))
                .collect());
        }
        capacity *= 4;
    }
}

/// One entry of the engine's array: the entity, then its transform, which
/// starts with the position.
fn place(layout: &Layout, entry: &[u8]) -> EntityPlace {
    let word = |at: usize| {
        entry
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map_or(0, i64::from_le_bytes)
    };
    let at = layout.entry_transform;
    EntityPlace {
        entity: entry
            .get(..4)
            .and_then(|b| b.try_into().ok())
            .map_or(Entity(0), |b| Entity(u32::from_le_bytes(b))),
        reserved: 0,
        position: [word(at), word(at + 8), word(at + 16)],
    }
}

/// Asks the engine to destroy `entity` after the systems of the next step;
/// players' characters and entities not in the world are refused.
pub(crate) fn destroy(loader: &Loader, entity: Entity) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world()?;
    if entity == Entity(0) {
        return Err(Status::BAD_ARGS);
    }
    crate::world::template(loader, entity)?;
    if crate::world::players(loader)?
        .iter()
        .any(|p| p.entity == entity)
    {
        return Err(Status::REFUSED);
    }
    // SAFETY: `entity_world` checked the caller is on the server's game
    // thread and the world is non-null; the world holds the pointer to its
    // command buffers at the table's `ecs.command_buffers`.
    let buffer = unsafe { crate::memory::read_ptr::<u8>(world, layout.command_buffers) };
    if buffer.is_null() {
        return Err(Status::MISSING);
    }
    // SAFETY: `buffer` is the world's first command buffer, non-null by the
    // earlier check, used on the game thread between steps, where no system
    // writes it.
    let queued = unsafe { (layout.destroy)(buffer, 0, entity.0) };
    if queued == 0 {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// The engine's command append: (buffer, its stream at buffer +
/// [`COMMAND_STREAM`], kind, size, entity), the command or null.
type AppendFn = unsafe extern "system" fn(
    buffer: *mut u8,
    stream: *mut u8,
    kind: u8,
    size: u32,
    entity: u32,
) -> *mut u8;

/// The command kinds `AddComponents` and `RemoveComponents` of the engine's
/// command enum (servers 399 and 26430253).
const ADD_COMPONENTS: u8 = 3;
const REMOVE_COMPONENTS: u8 = 4;
/// Where a command buffer keeps the stream its commands go into.
const COMMAND_STREAM: usize = 0x10; // bound, not a table entry: the append's second argument on 399 and 26430253
/// Where a components command keeps its 16 hashes, its 16 sizes and its
/// data; the append zeroes the command, so unused hashes stay 0.
const COMMAND_HASHES: usize = 0x0c;
const COMMAND_SIZES: usize = 0x4c;
const COMMAND_DATA: usize = 0x6c;
/// `mov r8b, 1; call rel32` in `ecs.destroy_request`: its call of the
/// command append with kind 1, `DestroyEntity`.
const APPEND_CALL: [u8; 4] = [0x41, 0xb0, 0x01, 0xe8];
/// The bytes of `ecs.destroy_request` searched for the call.
const APPEND_SEARCH: usize = 0x40;
/// The first bytes of the command append on servers 399 and 26430253.
const APPEND_PROLOGUE: [u8; 14] = [
    0x48, 0x89, 0x74, 0x24, 0x10, 0x57, 0x48, 0x83, 0xec, 0x20, 0x44, 0x8b, 0x52, 0x10,
];

static APPEND: OnceLock<Option<AppendFn>> = OnceLock::new();

/// The command append, from the call inside `ecs.destroy_request`, its
/// prologue checked; `None`, once logged, when either differs on this build.
fn append(layout: &Layout) -> Option<AppendFn> {
    *APPEND.get_or_init(|| {
        let destroy = layout.destroy as usize;
        let mut code = [0u8; APPEND_SEARCH];
        // SAFETY: `destroy` is the build table's `ecs.destroy_request`, mapped
        // code of the loaded executable, at least `APPEND_SEARCH` bytes long
        // (0x5a on server 399).
        unsafe { enshwick::raw::read_bytes_at(core::ptr::with_exposed_provenance(destroy), 0, &mut code) };
        let found = code
            .windows(APPEND_CALL.len())
            .position(|w| w == APPEND_CALL)
            .filter(|&at| at + 8 <= APPEND_SEARCH)
            .and_then(|at| {
                let rel = i32::from_le_bytes(code[at + 4..at + 8].try_into().ok()?);
                destroy.checked_add(at + 8)?.checked_add_signed(isize::try_from(rel).ok()?)
            });
        let checked = found.filter(|&target| {
            let mut head = [0u8; APPEND_PROLOGUE.len()];
            // SAFETY: `target` is the target of a call in mapped game code,
            // itself mapped code of the executable.
            unsafe { enshwick::raw::read_bytes_at(core::ptr::with_exposed_provenance(target), 0, &mut head) };
            head == APPEND_PROLOGUE
        });
        if checked.is_none() {
            crate::log!(
                WARN,
                "entities left out of the save off: the engine's command append is not where this build's destroy request calls it, or has other bytes"
            );
        }
        // SAFETY: `target` is the engine's command append, the call target in
        // `ecs.destroy_request` whose prologue matched servers 399 and
        // 26430253, with the `AppendFn` signature.
        checked.map(|target| unsafe { core::mem::transmute::<usize, AppendFn>(target) })
    })
}

/// The entities mods spawned in this world, which only may be left out of
/// its save.
// shortcut: the engine reuses ids, so a mod's destroyed entity's id may come back as another; track by spawn record when a mod needs more.
static SPAWNED: Mutex<Option<FxHashSet<u32>>> = Mutex::new(None);

/// Notes `entity` as one a mod spawned.
pub(crate) fn spawned(entity: u32) {
    SPAWNED.relock().get_or_insert_default().insert(entity);
}

/// Forgets the entities mods spawned, when a hosted game stops.
pub(crate) fn forget() {
    *SPAWNED.relock() = None;
}

/// Keeps `entity`, one a mod spawned, out of the world's save (`saved`
/// false) or back in it: the engine's own `AddComponents` or
/// `RemoveComponents` of `DoNotSave`, played back after the systems of the
/// next step. Vanilla entities are refused.
pub(crate) fn set_saved(loader: &Loader, entity: Entity, saved: bool) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let world = entity_world()?;
    if entity == Entity(0) {
        return Err(Status::BAD_ARGS);
    }
    crate::world::template(loader, entity)?;
    if !SPAWNED
        .relock()
        .as_ref()
        .is_some_and(|s| s.contains(&entity.0))
    {
        return Err(Status::REFUSED);
    }
    let append = append(layout).ok_or(Status::MISSING)?;
    let hash = crate::catalogue::component_hash(loader, crate::catalogue::ids::DO_NOT_SAVE)
        .ok_or(Status::MISSING)?;
    let size = crate::catalogue::size(loader, crate::catalogue::ids::DO_NOT_SAVE)
        .ok_or(Status::MISSING)?;
    // SAFETY: `entity_world` checked the caller is on the server's game
    // thread and the world is non-null; the world holds the pointer to its
    // command buffers at the table's `ecs.command_buffers`.
    let buffer = unsafe { crate::memory::read_ptr::<u8>(world, layout.command_buffers) };
    if buffer.is_null() {
        return Err(Status::MISSING);
    }
    let (kind, bytes) = if saved {
        (REMOVE_COMPONENTS, COMMAND_SIZES)
    } else {
        (ADD_COMPONENTS, COMMAND_DATA + size)
    };
    // SAFETY: `buffer` is the world's first command buffer, non-null by the
    // earlier check, used on the game thread between steps, as `destroy`
    // uses it; its stream sits at `COMMAND_STREAM` inside it.
    let command = unsafe {
        append(
            buffer,
            buffer.add(COMMAND_STREAM),
            kind,
            u32::try_from(bytes).map_err(|_| Status::BAD_ARGS)?,
            entity.0,
        )
    };
    if command.is_null() {
        return Err(Status::REFUSED);
    }
    let command = command.expose_provenance();
    // SAFETY: `command` is the command just appended, `bytes` long and zeroed
    // by the append; its first hash sits at `COMMAND_HASHES`.
    unsafe { crate::memory::write(command, COMMAND_HASHES, hash) };
    if !saved {
        // SAFETY: the same command, `COMMAND_DATA + size` bytes long; its first
        // size sits at `COMMAND_SIZES` and its data, zero for `DoNotSave`'s
        // one byte, from `COMMAND_DATA`, left as the append zeroed it.
        unsafe {
            crate::memory::write(
                command,
                COMMAND_SIZES,
                u16::try_from(size).unwrap_or(u16::MAX),
            )
        };
    }
    Ok(())
}
