//! Entity queries and destroy requests on the server's game thread.
//!
//! The query is the engine's own walk over every archetype that holds a
//! component type and a transform, the one its game code uses to find
//! placed things. A destroy is a request in the entity world's first command
//! buffer, which the engine plays back after the systems of the next step,
//! as it does for its own game-thread code. Keeping an entity out of the
//! save, or putting it back, is the same: an `AddComponents` or
//! `RemoveComponents` command for `DoNotSave` in that buffer, as the game's
//! own animal behaviour adds it to a live animal (server 26430253).

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::abi::{EntityPlace, Status};

use crate::host::Loader;
use crate::pinned;
use crate::pinned::{Constant, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::ADD_COMPONENTS.id(),
    pinned::ecs::COMMAND_APPEND.id(),
    pinned::ecs::COMMAND_BUFFERS.id(),
    pinned::ecs::COMMAND_RECORDS.id(),
    pinned::ecs::COMMAND_SET_DATA.id(),
    pinned::ecs::COMMAND_SET_HASH.id(),
    pinned::ecs::COMMAND_SET_SIZE.id(),
    pinned::ecs::DESTROY_REQUEST.id(),
    pinned::ecs::ENTITIES_ENTRY_SIZE.id(),
    pinned::ecs::ENTITIES_ENTRY_TRANSFORM.id(),
    pinned::ecs::ENTITIES_WITH.id(),
    pinned::ecs::REMOVE_COMPONENTS.id(),
    pinned::ecs::REMOVE_COMPONENTS_HASHES.id(),
    pinned::ecs::REMOVE_COMPONENTS_SIZE.id(),
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
/// The engine's command append: the buffer, its record list, the command
/// kind, the command's whole size and its entity; the command, zeroed after
/// its header, or null when the buffer is full.
type AppendFn = unsafe extern "system" fn(
    buffer: *mut u8,
    records: *mut u8,
    kind: u8,
    size: u32,
    entity: u32,
) -> *mut u8;

/// The game functions and offsets, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    query: QueryFn,
    destroy: DestroyFn,
    entry_size: usize,
    entry_transform: usize,
    command_buffers: usize,
}

/// The commands that add and remove `DoNotSave`, from the build table and
/// the catalogue, apart from the entity reads, so a missing piece turns
/// off only what needs it.
#[derive(Debug, Clone, Copy)]
struct Saving {
    append: AppendFn,
    command_buffers: usize,
    records: usize,
    hash: u32,
    /// `None` on a build where a piece of the `AddComponents` command is
    /// not found: entities stay in the save.
    add: Option<AddCommand>,
    /// `None` on a build where a piece of the `RemoveComponents` command is
    /// not found: entities left out stay out.
    remove: Option<RemoveCommand>,
}

/// The `AddComponents` command for `DoNotSave`: its kind, where its first
/// type hash and size sit, its whole size and the component's.
#[derive(Debug, Clone, Copy)]
struct AddCommand {
    kind: u8,
    hash_at: usize,
    size_at: usize,
    size: u32,
    component: u16,
}

/// The `RemoveComponents` command: its kind, where its first type hash sits
/// and its whole size.
#[derive(Debug, Clone, Copy)]
struct RemoveCommand {
    kind: u8,
    hash_at: usize,
    size: u32,
}

impl Saving {
    fn new(loader: &Loader) -> Option<Self> {
        let kind = |name: Constant| u8::try_from(loader.offset(name)?).ok();
        let ids = crate::catalogue::ids::DO_NOT_SAVE;
        let append = usize::try_from(loader.address(pinned::ecs::COMMAND_APPEND)?).ok()?;
        let component = crate::catalogue::size(loader, ids)?;
        let add = (|| {
            let data = loader.offset(pinned::ecs::COMMAND_SET_DATA)?;
            Some(AddCommand {
                kind: kind(pinned::ecs::ADD_COMPONENTS)?,
                hash_at: loader.offset(pinned::ecs::COMMAND_SET_HASH)?,
                size_at: loader.offset(pinned::ecs::COMMAND_SET_SIZE)?,
                size: u32::try_from(data + component).ok()?,
                component: u16::try_from(component).ok()?,
            })
        })();
        let remove = (|| {
            Some(RemoveCommand {
                kind: kind(pinned::ecs::REMOVE_COMPONENTS)?,
                hash_at: loader.offset(pinned::ecs::REMOVE_COMPONENTS_HASHES)?,
                size: u32::try_from(loader.offset(pinned::ecs::REMOVE_COMPONENTS_SIZE)?).ok()?,
            })
        })();
        Some(Self {
            // SAFETY: `append` is the build table's `ecs.command_append`,
            // the game function `AppendFn` types, resolved by
            // `loader.address`.
            append: unsafe { core::mem::transmute::<usize, AppendFn>(append) },
            command_buffers: loader.offset(pinned::ecs::COMMAND_BUFFERS)?,
            records: loader.offset(pinned::ecs::COMMAND_RECORDS)?,
            hash: crate::catalogue::component_hash(loader, ids)?,
            add,
            remove,
        })
    }
}

static SAVING: OnceLock<Option<Saving>> = OnceLock::new();

fn saving(loader: &Loader) -> Option<&'static Saving> {
    SAVING.get_or_init(|| Saving::new(loader)).as_ref()
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
    crate::spawns::destroyed(entity);
    Ok(())
}

/// Keeps `entity`, which a mod spawned or placed, out of the world's saves
/// (`saved` false) or puts it back in them, after the systems of the next
/// step; vanilla entities are refused, so no mod drops the game's own data
/// from a save.
pub(crate) fn set_saved(loader: &Loader, entity: Entity, saved: bool) -> Result<(), Status> {
    if entity == Entity(0) {
        return Err(Status::BAD_ARGS);
    }
    let saving = saving(loader).ok_or(Status::MISSING)?;
    let world = entity_world()?;
    let template = crate::world::template(loader, entity)?;
    if crate::spawns::spawned(entity) != Some(template) {
        return Err(Status::REFUSED);
    }
    let live = crate::world::component(loader, entity, saving.hash).is_err();
    if !crate::spawns::needs_saved(entity, saved, live) {
        return Ok(());
    }
    if saved {
        let remove = saving.remove.ok_or(Status::MISSING)?;
        let command = append(saving, world, remove.kind, remove.size, entity)?;
        // SAFETY: `append` handed out `remove.size` writable bytes, the
        // table's `ecs.remove_components_size`, which nothing else touches
        // before the next step; its first type hash is a `u32` at the
        // table's `ecs.remove_components_hashes`, inside them.
        unsafe { enshwick::raw::write_at(command, remove.hash_at, saving.hash) };
    } else {
        let add = saving.add.ok_or(Status::MISSING)?;
        let command = append(saving, world, add.kind, add.size, entity)?;
        // SAFETY: `append` handed out `add.size` writable bytes,
        // `ecs.command_set_data` plus the component's size, zeroed after the
        // header, which nothing else touches before the next step; the first
        // type hash is a `u32` at the table's `ecs.command_set_hash`, in the
        // header before the data.
        unsafe { enshwick::raw::write_at(command, add.hash_at, saving.hash) };
        // SAFETY: the same command of `add.size` writable bytes; the first
        // component's size is a `u16` at the table's `ecs.command_set_size`,
        // in the header before the data.
        unsafe { enshwick::raw::write_at(command, add.size_at, add.component) };
    }
    crate::spawns::asked_saved(entity, saved);
    Ok(())
}

/// Appends a command of `kind` and `size` bytes for `entity` to the
/// world's first command buffer; the command, zeroed after its header.
fn append(
    saving: &Saving,
    world: usize,
    kind: u8,
    size: u32,
    entity: Entity,
) -> Result<*mut u8, Status> {
    // SAFETY: `entity_world` checked the caller is on the server's game
    // thread and the world is non-null; the world holds the pointer to its
    // command buffers at the table's `ecs.command_buffers`.
    let buffer = unsafe { crate::memory::read_ptr::<u8>(world, saving.command_buffers) };
    if buffer.is_null() {
        return Err(Status::MISSING);
    }
    // SAFETY: `saving.append` is the table's `ecs.command_append`, on the
    // world's first command buffer, non-null by the guard before it, and its
    // record list at the table's `ecs.command_records`, on the game thread
    // between steps (`entity_world`), where no system writes it.
    let command = unsafe {
        (saving.append)(
            buffer,
            buffer.wrapping_add(saving.records),
            kind,
            size,
            entity.0,
        )
    };
    if command.is_null() {
        return Err(Status::REFUSED);
    }
    Ok(command)
}
