//! The fake's item filters.

use core::ffi::c_void;

use crate::abi::{ItemActionFn, ModHandle, Status};
use crate::test::table::{Registered, Registration, with};

pub(crate) extern "system" fn item_filter(
    me: ModHandle,
    callback: ItemActionFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.item_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}
