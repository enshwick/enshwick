//! The fake's mod messages.

use core::ffi::c_void;

use crate::abi::{ModHandle, NetMessageFn, Status, Str};
use crate::test::table::{put, text, with};

pub(crate) extern "system" fn net_channel(
    _me: ModHandle,
    name: Str,
    callback: NetMessageFn,
    user: *mut c_void,
    out: *mut u32,
) -> Status {
    let name = text(name);
    let id = with(None, |s| {
        if s.channels.iter().any(|c| c.0 == name) {
            return None;
        }
        let id = u32::try_from(s.channels.len() + 1).unwrap_or(u32::MAX);
        s.channels
            .push((name, id, callback, user.expose_provenance()));
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
    peer: u64,
    data: *const u8,
    size: u32,
) -> Status {
    let bytes = if size == 0 || data.is_null() {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `size` bytes at `data` for the call.
        unsafe { core::slice::from_raw_parts(data, size as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        if peer != 0 && !s.host.net_peers.contains(&peer) {
            return Status::MISSING;
        }
        s.net_sent.push((channel, peer, bytes));
        Status::OK
    })
}

pub(crate) extern "system" fn net_ready(_me: ModHandle, peer: u64) -> Status {
    with(Status::MISSING, |s| {
        if s.host.net_peers.contains(&peer) {
            Status::OK
        } else {
            Status::MISSING
        }
    })
}
