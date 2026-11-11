//! The fake's chat filters.

use core::ffi::c_void;

use crate::test::table::{Registered, Registration, with};
use enshwick_core::abi::{ChatFilterFn, ModHandle, Status};

pub(crate) extern "system" fn chat_filter(
    me: ModHandle,
    callback: ChatFilterFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        s.chat_filters.push(Registered::new(callback, user, me));
        Status::OK
    })
}
