//! The fake's chat filters.

use core::ffi::c_void;

use crate::abi::{ChatFilterFn, ModHandle, Status};
use crate::test::table::{Registered, Registration, with};

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
