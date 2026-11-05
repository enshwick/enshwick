//! Host functions for players' actions with items.

use enshwick::abi::{ItemActionFn, ModHandle, Status};

use crate::host::{caller_index, entry};

pub(crate) extern "system" fn item_filter(
    me: ModHandle,
    callback: ItemActionFn,
    user: *mut core::ffi::c_void,
) -> Status {
    entry("item_filter", || match caller_index(me) {
        Ok((loader, index)) => crate::conflicts::note(
            "item filter",
            index,
            crate::conflicts::Mode::Chained,
            crate::item_actions::register(loader, index, callback, user),
        ),
        Err(status) => status,
    })
}
