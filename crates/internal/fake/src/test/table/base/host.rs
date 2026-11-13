//! The fake's host entries: components and templates, whether a server
//! runs, the mod's features and status, and game events.

use core::ffi::c_void;

use crate::test::table::{Registered, found, put_headed_list, text, with};
use enshwick_core::abi::{
    EventTypeInfo, GameEventFn, Header, ModHandle, Side, Status, StatusFn, Str,
};

pub(crate) extern "system" fn component(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    type_hash: u32,
    out: *mut *const c_void,
) -> Status {
    let address = with(None, |s| {
        s.host.components.get(&(entity.0, type_hash)).map(|c| c.0)
    });
    found(address.map(core::ptr::with_exposed_provenance), out)
}

pub(crate) extern "system" fn entity_template(
    _me: ModHandle,
    entity: enshwick_core::Entity,
    out: *mut [u8; 16],
) -> Status {
    found(
        with(None, |s| s.host.templates.get(&entity.0).copied()),
        out,
    )
}

pub(crate) extern "system" fn hosting() -> u8 {
    with(0, |s| {
        u8::from(s.host.side == Side::SERVER || s.host.hosting)
    })
}

pub(crate) extern "system" fn server_world() -> u64 {
    with(0, |s| s.host.server_world)
}

pub(crate) extern "system" fn feature(_me: ModHandle, name: Str, off: Str) -> Status {
    let name = text(name);
    if name.is_empty() {
        return Status::BAD_ARGS;
    }
    let off = Some(text(off)).filter(|off| !off.is_empty());
    with(Status::MISSING, |s| {
        match s.features.iter_mut().find(|(have, _)| *have == name) {
            Some(kept) => kept.1 = off,
            None => s.features.push((name, off)),
        }
        Status::OK
    })
}

pub(crate) extern "system" fn status(
    me: ModHandle,
    callback: StatusFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.status_sources.push(Registered::new(callback, user, me));
        Status::OK
    })
}

pub(crate) extern "system" fn game_event(
    me: ModHandle,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        s.game_events
            .push((type_hash, Registered::new(callback, user, me)));
        Status::OK
    })
}

pub(crate) extern "system" fn client_game_event(
    me: ModHandle,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.host.side != Side::CLIENT {
            return Status::UNAVAILABLE;
        }
        if s.registration == crate::test::table::Registration::Closed {
            return Status::TOO_LATE;
        }
        if type_hash == 0
            || s.client_game_events
                .iter()
                .filter(|(_, handler)| handler.owner() == Some(me))
                .count()
                >= 32
        {
            return Status::BAD_ARGS;
        }
        s.client_game_events
            .push((type_hash, Registered::new(callback, user, me)));
        Status::OK
    })
}

pub(crate) extern "system" fn event_types(
    _me: ModHandle,
    out: *mut EventTypeInfo,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let found: Vec<EventTypeInfo> = with(Vec::new(), |s| {
        s.host
            .event_types
            .iter()
            .map(|&(name, hash, size)| EventTypeInfo {
                header: Header::of::<EventTypeInfo>(),
                name: Str::new(name),
                hash,
                size,
            })
            .collect()
    });
    put_headed_list(&found, out, capacity, count)
}
