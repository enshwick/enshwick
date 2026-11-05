//! The fake's item filters.

use core::ffi::c_void;

use crate::abi::{ItemActionFn, ModHandle, Status};
use crate::test::table::{Registration, with};

pub(crate) extern "system" fn item_filter(
    _me: ModHandle,
    callback: ItemActionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.item_filters.push((callback, user.expose_provenance()));
        Status::OK
    })
}
