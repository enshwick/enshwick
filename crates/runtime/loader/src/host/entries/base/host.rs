//! Host functions for the running game: components, templates, hosting, the
//! server's world, features, the status report and game events.

use enshwick_base::Relock;
use enshwick_core::abi::{EventTypeInfo, Header, Level, ModHandle, Side, Status, Str};
use enshwick_core::{Entity, abi as api_frame};

use crate::engine::loader::loader;
use crate::host::memory::{put, put_headed_list, text};
use crate::host::{caller, calling, entry};

pub(crate) extern "system" fn event_types(
    me: ModHandle,
    out: *mut EventTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("event_types", || {
        let loader = match caller(me) {
            Ok(loader) => loader,
            Err(status) => return status,
        };
        let types: Vec<EventTypeInfo> = crate::engine::catalogue::event_types(&loader.reflection)
            .filter_map(|ty| {
                Some(EventTypeInfo {
                    header: Header::of::<EventTypeInfo>(),
                    name: Str::new(ty.name()),
                    hash: crate::engine::world::type_hash(ty.va())?,
                    size: ty.size(),
                })
            })
            .collect();
        // SAFETY: `HostApi::event_types` has the mod pass `out` null or
        // pointing at `capacity` entries of its version of `EventTypeInfo`,
        // each writable for the size the first entry's header states, and
        // `count` null or pointing at a writable, aligned `u32`, neither
        // touched by another thread during the call.
        unsafe { put_headed_list(&types, out, capacity, count) }
    })
}

pub(crate) extern "system" fn component(
    me: ModHandle,
    entity: Entity,
    type_hash: u32,
    out: *mut *const core::ffi::c_void,
) -> Status {
    entry("component", || {
        match caller(me)
            .and_then(|loader| crate::engine::world::component(loader, entity, type_hash))
        {
            // SAFETY: `HostApi::component` has the mod pass `out` null or
            // pointing at a writable, aligned `c_void` that no other thread
            // touches during the call.
            Ok(data) => unsafe { put(out, data) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn entity_template(
    me: ModHandle,
    entity: Entity,
    out: *mut [u8; 16],
) -> Status {
    entry("entity_template", || {
        match caller(me).and_then(|loader| crate::engine::world::template(loader, entity)) {
            // SAFETY: `HostApi::entity_template` has the mod pass `out` null or
            // pointing at a writable, aligned `[u8; 16]` that no other thread
            // touches during the call.
            Ok(guid) => unsafe { put(out, guid) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn hosting() -> u8 {
    entry("hosting", || {
        loader().map_or(0, |loader| {
            u8::from(loader.side == Side::SERVER || crate::engine::world::hosting())
        })
    })
}

pub(crate) extern "system" fn server_world() -> u64 {
    entry("server_world", crate::engine::world::server_world)
}

pub(crate) extern "system" fn feature(me: ModHandle, name: Str, off: Str) -> Status {
    entry("feature", || {
        let (Some(name), Some(off)) = (text(name), text(off)) else {
            return Status::BAD_ARGS;
        };
        if name.is_empty() {
            return Status::BAD_ARGS;
        }
        let record = match calling(me) {
            Ok((_, _, record)) => record,
            Err(status) => return status,
        };
        let texts = record.texts.relock();
        let changed = crate::engine::features::report(
            &mut record.features.relock(),
            name,
            Some(off).filter(|off| !off.is_empty()),
            texts.as_ref(),
        );
        drop(texts);
        if let Some(line) = changed {
            let level = if off.is_empty() {
                Level::INFO
            } else {
                Level::WARN
            };
            crate::engine::log::write(level, &record.id, &line);
        }
        Status::OK
    })
}

pub(crate) extern "system" fn status(
    me: ModHandle,
    callback: api_frame::StatusFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("status", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::engine::loader::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        crate::base::status::register(index, callback, user)
    })
}

pub(crate) extern "system" fn client_game_event(
    me: ModHandle,
    type_hash: u32,
    callback: api_frame::GameEventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("client_game_event", || {
        let (loader, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        if !crate::engine::loader::in_init(index.get()) {
            return Status::TOO_LATE;
        }
        crate::base::game_events::register_client(loader.side, index, type_hash, callback, user)
    })
}

pub(crate) extern "system" fn game_event(
    me: ModHandle,
    type_hash: u32,
    callback: api_frame::GameEventFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("game_event", || {
        let (_, index, _) = match calling(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        crate::base::game_events::register(index, type_hash, callback, user)
    })
}
