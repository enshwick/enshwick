//! Entity queries and destroy requests on the server's game thread.
//!
//! The query is the engine's own walk over every archetype that holds a
//! component type and a transform, the one its game code uses to find
//! placed things. A destroy is a request in the entity world's first command
//! buffer, which the engine plays back after the systems of the next step,
//! as it does for its own game-thread code.

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::abi::{EntityPlace, Status};

use crate::host::Loader;
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
