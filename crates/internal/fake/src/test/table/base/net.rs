//! The fake's mod messages.

use core::ffi::c_void;

use crate::test::table::{Registered, put, text, with};
use enshwick_core::abi::{ModHandle, NetMessageFn, Status, Str};

pub(crate) extern "system" fn net_channel(
    me: ModHandle,
    name: Str,
    callback: NetMessageFn,
    user: *mut c_void,
    out: *mut u32,
) -> Status {
    let name = text(name);
    let id = with(None, |s| {
        // The loader opens channels to clients for mods of side both alone.
        let joins = s.manifest.as_ref().is_none_or(|manifest| manifest.joins);
        if !joins || s.channels.iter().any(|c| c.0 == name) {
            return None;
        }
        let id = u32::try_from(s.channels.len() + 1).unwrap_or(u32::MAX);
        s.channels
            .push((name, id, Registered::new(callback, user, me)));
        Some(id)
    });
    match id {
        Some(id) => put(out, id),
        None => Status::REFUSED,
    }
}

pub(crate) extern "system" fn net_send(
    _me: ModHandle,
    channel: u32,
    peer: enshwick_core::PlayerKey,
    data: *const u8,
    size: u32,
) -> Status {
    if size as usize > enshwick_core::net::MAX_MESSAGE {
        return Status::BAD_ARGS;
    }
    let bytes = if size == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `size` bytes at `data` for the call.
        unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if peer.get() != 0 && !s.host.net_peers.contains(&peer.get()) {
            return Status::MISSING;
        }
        s.net_sent.push((channel, peer, bytes));
        Status::OK
    })
}

pub(crate) extern "system" fn net_ready(_me: ModHandle, peer: enshwick_core::PlayerKey) -> Status {
    with(Status::MISSING, |s| {
        if s.host.net_peers.contains(&peer.get()) {
            Status::OK
        } else {
            Status::MISSING
        }
    })
}
