//! Game events mods raise and teleports they ask for, on the server's game
//! thread between steps or inside a game event, where no ECS system runs.
//!
//! An event goes into the entity world's first event stream, the one the
//! game's own game-thread code pushes its login events into, so the engine
//! moves it into its queues at the start of the next step and the systems
//! that read the type see it. Only types on an allow list are raised: types
//! the server's own systems raise, whose size the loader checks against
//! reflection.
//!
//! A teleport is the game's admin teleport: a request in the entity world's
//! first command buffer that gives the entity a `Teleport` component after
//! the next step; the game's `teleport` system then moves the entity, counts
//! the teleport for the clients and removes the component. That system moves
//! only entities that count their teleports, actors and characters, so the
//! loader refuses any other.

use std::sync::OnceLock;

use enshwick::abi::{Status, TeleportRequest};
use rustc_hash::FxHashMap;

use crate::host::Loader;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "ecs.command_append",
    "ecs.command_buffers",
    "ecs.command_records",
    "ecs.command_set_data",
    "ecs.command_set_hash",
    "ecs.command_set_size",
    "ecs.event_push",
    "ecs.event_streams",
    "world.entities",
    "world.time",
];

/// The event types mods may raise: types the server's own systems raise on
/// the 1.0 build 1269579, each with a consumer on the server or the clients.
const ALLOWED: [&str; 4] = [
    "keen::ecs::NpcCommandEvent",
    "keen::ecs::SetToggleStateEvent",
    "keen::ecs::AddThreatTargetEvent",
    "keen::ecs::UiPingEvent",
];

/// The source id the game's own game-thread pushes give their events.
const GAME_THREAD_SOURCE: u16 = 0;
/// The largest event the game's push takes, exclusive.
const MAX_EVENT: u32 = 256;
/// The command kind that sets a component on an entity.
const SET_COMPONENT: u8 = 3;

type PushFn =
    unsafe extern "system" fn(stream: *mut u8, hash: u32, size: u32, source: u16) -> *mut u8;
type TimeFn = unsafe extern "system" fn(world: *const u8) -> i64;
type AppendFn = unsafe extern "system" fn(
    buffer: *mut u8,
    records: *mut u8,
    kind: u8,
    size: u32,
    entity: u32,
) -> *mut u8;

/// An allowed event type: its reflected size.
#[derive(Debug, Clone, Copy)]
struct Allowed {
    size: u32,
}

/// The `Teleport` component's layout, from reflection.
#[derive(Debug, Clone, Copy)]
struct TeleportLayout {
    hash: u32,
    size: usize,
    position: usize,
    orientation: usize,
    scale: usize,
    best_spawn: usize,
    current_orientation: usize,
}

/// The game functions and offsets, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    push: PushFn,
    time: TimeFn,
    append: AppendFn,
    allowed: FxHashMap<u32, Allowed>,
    time_offset: usize,
    streams: usize,
    command_buffers: usize,
    records: usize,
    record_hash: usize,
    record_size: usize,
    record_data: usize,
    world_entities: usize,
    teleport: Option<TeleportLayout>,
    transform: u32,
    /// `TeleportCount`, which the game's teleport counts up on what it moves.
    teleport_count: u32,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let reflection = &loader.reflection;
            let hash_of = |name: &str| {
                let info = reflection.type_info(name).ok()?;
                crate::world::type_hash(info.va())
            };
            let offset = |ty: &str, field: &str| {
                reflection
                    .field_offset(ty, field)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let allowed = ALLOWED
                .iter()
                .filter_map(|name| {
                    let size = reflection.type_info(name).ok()?.size();
                    Some((hash_of(name)?, Allowed { size }))
                })
                .collect();
            let teleport = (|| {
                let transform = offset("keen::ecs::Teleport", "targetTransform")?;
                Some(TeleportLayout {
                    hash: hash_of("keen::ecs::Teleport")?,
                    size: usize::try_from(reflection.type_info("keen::ecs::Teleport").ok()?.size())
                        .ok()?,
                    position: transform + offset("keen::WorldTransform", "position")?,
                    orientation: transform + offset("keen::WorldTransform", "orientation")?,
                    scale: transform + offset("keen::WorldTransform", "scale")?,
                    best_spawn: offset("keen::ecs::Teleport", "searchBestSpawnPosition")?,
                    current_orientation: offset("keen::WorldTransform", "orientation")?,
                })
            })();
            let (push, time, append) = (
                function("ecs.event_push")?,
                function("world.time")?,
                function("ecs.command_append")?,
            );
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                push: unsafe { core::mem::transmute::<usize, PushFn>(push) },
                // SAFETY: as above.
                time: unsafe { core::mem::transmute::<usize, TimeFn>(time) },
                // SAFETY: as above.
                append: unsafe { core::mem::transmute::<usize, AppendFn>(append) },
                allowed,
                time_offset: offset("keen::ecs::GameEvent", "timeStamp")?,
                streams: loader.offset("ecs.event_streams")?,
                command_buffers: loader.offset("ecs.command_buffers")?,
                records: loader.offset("ecs.command_records")?,
                record_hash: loader.offset("ecs.command_set_hash")?,
                record_size: loader.offset("ecs.command_set_size")?,
                record_data: loader.offset("ecs.command_set_data")?,
                world_entities: loader.offset("world.entities")?,
                teleport,
                transform: hash_of("keen::ecs::CurrentTransform")?,
                teleport_count: hash_of("keen::ecs::TeleportCount")?,
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

/// Raises the event `bytes` of the allowed type `hash`, its whole reflected
/// size; the loader stamps it with the game time.
pub(crate) fn emit(loader: &Loader, hash: u32, bytes: &[u8]) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let allowed = layout.allowed.get(&hash).ok_or(Status::REFUSED)?;
    if usize::try_from(allowed.size).ok() != Some(bytes.len()) {
        return Err(Status::BAD_ARGS);
    }
    push(loader, hash, bytes)
}

/// Raises the event `bytes` of type `hash` for the loader's own features,
/// which know the type's consumers; stamped with the game time.
pub(crate) fn push(loader: &Loader, hash: u32, bytes: &[u8]) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    if bytes.len() < layout.time_offset + size_of::<i64>() {
        return Err(Status::BAD_ARGS);
    }
    let size = u32::try_from(bytes.len())
        .ok()
        .filter(|&size| size < MAX_EVENT)
        .ok_or(Status::BAD_ARGS)?;
    let world = entity_world()?;
    // SAFETY: the entity world holds its stream array; the first stream is
    // the one the game's game-thread code pushes into.
    let stream: usize = unsafe { crate::world::read(world, layout.streams) };
    if stream == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the World holds its time where the game's own function reads
    // it; the entity world lies inside the World.
    let now = unsafe {
        (layout.time)(core::ptr::with_exposed_provenance(
            world - layout.world_entities,
        ))
    };
    // SAFETY: the server's first stream on its game thread between steps,
    // where no system pushes; the size is under the stream's limit.
    let payload = unsafe {
        (layout.push)(
            core::ptr::with_exposed_provenance_mut(stream),
            hash,
            size,
            GAME_THREAD_SOURCE,
        )
    };
    if payload.is_null() {
        return Err(Status::REFUSED);
    }
    // SAFETY: the game handed out `size` zeroed bytes for the event.
    let out = unsafe { core::slice::from_raw_parts_mut(payload, bytes.len()) };
    out.copy_from_slice(bytes);
    out[layout.time_offset..layout.time_offset + 8].copy_from_slice(&now.to_le_bytes());
    Ok(())
}

/// Asks the server to teleport an entity after the next step, as its admin
/// teleport does.
pub(crate) fn teleport(loader: &Loader, request: &TeleportRequest) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let teleport = layout.teleport.ok_or(Status::MISSING)?;
    let world = entity_world()?;
    if request.entity == 0 {
        return Err(Status::BAD_ARGS);
    }
    let current = crate::world::component(loader, request.entity, layout.transform)?;
    // The game's teleport moves only what counts its teleports, actors and
    // characters; a prop would keep the component and stay.
    if crate::world::component(loader, request.entity, layout.teleport_count).is_err() {
        return Err(Status::REFUSED);
    }
    let orientation: [f32; 4] = if request.flags & TeleportRequest::ORIENTED == 0 {
        // SAFETY: a live CurrentTransform is a WorldTransform.
        unsafe { crate::world::read(current.addr(), teleport.current_orientation) }
    } else {
        request.orientation
    };
    let mut data = vec![0u8; teleport.size];
    let mut put = |at: usize, bytes: &[u8]| {
        if let Some(out) = data.get_mut(at..at + bytes.len()) {
            out.copy_from_slice(bytes);
        }
    };
    for (i, axis) in request.position.iter().enumerate() {
        put(teleport.position + i * 8, &axis.to_le_bytes());
    }
    for (i, part) in orientation.iter().enumerate() {
        put(teleport.orientation + i * 4, &part.to_le_bytes());
    }
    for i in 0..3 {
        put(teleport.scale + i * 4, &1.0f32.to_le_bytes());
    }
    put(
        teleport.best_spawn,
        &[u8::from(request.flags & TeleportRequest::BEST_SPAWN != 0)],
    );
    // SAFETY: the entity world holds a pointer to its command buffers.
    let buffer: usize = unsafe { crate::world::read(world, layout.command_buffers) };
    if buffer == 0 {
        return Err(Status::MISSING);
    }
    let size = u32::try_from(layout.record_data + data.len()).map_err(|_| Status::BAD_ARGS)?;
    // SAFETY: the world's first command buffer and its record list, on the
    // game thread between steps, where no system writes it.
    let record = unsafe {
        (layout.append)(
            core::ptr::with_exposed_provenance_mut(buffer),
            core::ptr::with_exposed_provenance_mut(buffer + layout.records),
            SET_COMPONENT,
            size,
            request.entity,
        )
    };
    if record.is_null() {
        return Err(Status::REFUSED);
    }
    let record = record.addr();
    let component_size = u16::try_from(data.len()).map_err(|_| Status::BAD_ARGS)?;
    // SAFETY: the game handed out `size` zeroed bytes for the record: the
    // component's hash, its size and its bytes at the build's offsets.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u32>(record + layout.record_hash)
            .write_unaligned(teleport.hash);
    };
    // SAFETY: as above.
    unsafe {
        core::ptr::with_exposed_provenance_mut::<u16>(record + layout.record_size)
            .write_unaligned(component_size);
    };
    // SAFETY: as above.
    unsafe {
        core::ptr::copy_nonoverlapping(
            data.as_ptr(),
            core::ptr::with_exposed_provenance_mut(record + layout.record_data),
            data.len(),
        );
    };
    Ok(())
}
