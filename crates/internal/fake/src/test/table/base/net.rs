//! The fake's mod messages.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with};
use enshwick_core::abi::{NetMessageFn, Status, Str};

entries! {
    pub(crate) fn net_channel(
        me as Mod,
        name: Str as args::Text,
        callback: Option<NetMessageFn> as args::Callback,
        user: *mut c_void,
        out: *mut u32 as args::Out,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        let name = text(name);
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let id = with(None, |s| {
            // The loader opens channels to clients for mods of side both alone.
            if !super::mods::joins(s, me) || s.base.channels.iter().any(|c| c.0 == name) {
                return None;
            }
            let id = u32::try_from(s.base.channels.len() + 1).unwrap_or(u32::MAX);
            s.base
                .channels
                .push((name, id, Registered::new(callback, user, me)));
            Some(id)
        });
        match id {
            Some(id) => put(out, id),
            None => Status::REFUSED,
        }
    }

    pub(crate) fn net_send(
        me as Mod,
        channel: u32,
        peer: enshwick_core::PlayerKey,
        [data: *const u8, size: u32] as args::List,
    ) -> Status {
        // As the loader: the channels open once every init returned, and a
        // mod sends only on its own, before the length counts.
        let own = with(false, |s| {
            s.base.closed()
                && s.base
                    .channels
                    .iter()
                    .any(|c| c.1 == channel && c.2.owner() == Some(me))
        });
        if !own || size as usize > enshwick_core::net::MAX_MESSAGE {
            return Status::BAD_ARGS;
        }
        let bytes = if size == 0 || data.is_null() {
            Vec::new()
        } else {
            // SAFETY: the safe layer passes `size` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec()
        };
        with(Status::MISSING, |s| {
            if peer.get() != 0 && !s.host.base.net_peers.contains(&peer.get()) {
                return Status::MISSING;
            }
            s.base.net_sent.push((channel, peer, bytes));
            Status::OK
        })
    }

    pub(crate) fn net_ready(me as Caller, peer: enshwick_core::PlayerKey) -> Status {
        with(Status::MISSING, |s| {
            if s.host.base.net_peers.contains(&peer.get()) {
                Status::OK
            } else {
                Status::MISSING
            }
        })
    }
}
