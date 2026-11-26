//! The fake's host entries: components and templates, whether a server
//! runs, the mod's features and status, and game events.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, found, list, text, with};
use enshwick_core::abi::{EventTypeInfo, GameEventFn, ModHandle, Side, Status, StatusFn, Str};

entries! {
    pub(crate) fn component(
        me as Caller,
        entity: enshwick_core::Entity,
        type_hash: u32,
        out: *mut *const c_void as args::Out,
    ) -> Status {
        if !crate::test::table::on_game_thread() {
            return Status::WRONG_THREAD;
        }
        let address = with(None, |s| {
            s.host
                .base
                .components
                .get(&(entity.get(), type_hash))
                .map(|c| c.0)
        });
        found(address.map(core::ptr::with_exposed_provenance), out)
    }

    pub(crate) fn hosting(me as Caller, out: *mut u8 as args::Out) -> Status {
        let hosting = with(0, |s| u8::from(s.host.base.serving()));
        crate::test::table::put(out, hosting)
    }

    pub(crate) fn server_world(me as Caller, out: *mut u64 as args::Out) -> Status {
        crate::test::table::put(out, with(0, |s| s.host.base.server_world))
    }

    pub(crate) fn feature(me as Mod, name: Str as args::Text, off: Str as args::Text) -> Status {
        let name = text(name);
        if name.is_empty() {
            return Status::BAD_ARGS;
        }
        let off = Some(text(off)).filter(|off| !off.is_empty());
        with(Status::MISSING, |s| {
            match s.base.features.iter_mut().find(|(have, _)| *have == name) {
                Some(kept) => kept.1 = off,
                None => s.base.features.push((name, off)),
            }
            Status::OK
        })
    }

    pub(crate) fn status(
        me as Mod,
        callback: Option<StatusFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            s.base
                .status_sources
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn subscribe_game_event(
        me as Mod,
        type_hash: u32,
        callback: Option<GameEventFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        keep_game_event(me, type_hash, callback, user, false)
    }

    pub(crate) fn subscribe_client_game_event(
        me as Mod,
        type_hash: u32,
        callback: Option<GameEventFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        keep_game_event(me, type_hash, callback, user, true)
    }
}

/// Keeps a game event handler as the loader does: only during init, 0 for
/// every type, and a client handler only where a client world steps, a
/// dedicated server answering it without keeping it.
fn keep_game_event(
    me: ModHandle,
    type_hash: u32,
    callback: GameEventFn,
    user: *mut c_void,
    client: bool,
) -> Status {
    crate::test::table::with_open(|s| {
        let kept = if !client {
            &mut s.base.game_events
        } else if s.host.base.side == Side::CLIENT {
            &mut s.base.client_game_events
        } else {
            return Status::OK;
        };
        kept.push((type_hash, Registered::new(callback, user, me)));
        Status::OK
    })
}

entries! {
    pub(crate) fn event_types(
        me as Caller,
        [out: *mut EventTypeInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let found: Vec<EventTypeInfo> = with(Vec::new(), |s| {
            s.host
                .base
                .event_types
                .iter()
                .map(|&(name, hash, size)| enshwick_core::headed!(EventTypeInfo {
                    name: Str::new(name),
                    hash,
                    size,
                }))
                .collect()
        });
        list(&found, out, entry_size, capacity, count)
    }
}
