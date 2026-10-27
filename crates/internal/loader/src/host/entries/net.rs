//! Host functions for mod messages between server and client.

use enshwick::abi as api_frame;
use enshwick::abi::{ModHandle, Status, Str};

use crate::host::memory::put;
use crate::host::{caller, caller_index, entry};

pub(crate) extern "system" fn net_channel(
    me: ModHandle,
    name: Str,
    callback: api_frame::NetMessageFn,
    user: *mut core::ffi::c_void,
    out: *mut u32,
) -> Status {
    entry("net_channel", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        // SAFETY: the name is borrowed for the call.
        let Some(name) = (unsafe { name.as_str() }) else {
            return Status::BAD_ARGS;
        };
        match crate::net::open(loader, index, name, callback, user) {
            // SAFETY: `HostApi::net_channel` has the mod pass `out` null or
            // pointing at a writable, aligned `u32` that no other thread
            // touches during the call.
            Ok(id) => unsafe { put(out, id) },
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn net_send(
    me: ModHandle,
    channel: u32,
    peer: enshwick::PlayerKey,
    data: *const u8,
    size: u32,
) -> Status {
    entry("net_send", || {
        let (loader, index) = match caller_index(me) {
            Ok(found) => found,
            Err(status) => return status,
        };
        let Ok(len) = usize::try_from(size) else {
            return Status::BAD_ARGS;
        };
        if data.is_null() && len != 0 {
            return Status::BAD_ARGS;
        }
        let bytes = if len == 0 {
            &[][..]
        } else {
            // SAFETY: the caller passes `size` bytes at `data` for the call.
            unsafe { core::slice::from_raw_parts(data, len) }
        };
        match crate::net::send(loader, index, channel, peer.get(), bytes) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}

pub(crate) extern "system" fn net_ready(me: ModHandle, peer: enshwick::PlayerKey) -> Status {
    entry("net_ready", || match caller(me) {
        Ok(_) if crate::net::ready(peer.get()) => Status::OK,
        Ok(_) => Status::MISSING,
        Err(status) => status,
    })
}
