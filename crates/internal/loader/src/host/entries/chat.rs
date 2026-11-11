//! Host functions for the server's chat lines.

use enshwick_core::abi::{ChatFilterFn, ModHandle, Status};

use crate::host::{caller_index, entry};

pub(crate) extern "system" fn chat_filter(
    me: ModHandle,
    callback: ChatFilterFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("chat_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "chat filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::chat::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}
