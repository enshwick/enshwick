//! Host functions for the game's memory pages: how far a read-only run reaches.

use enshwick::abi::{ModHandle, Status};

use crate::host::entry;
use crate::host::memory::put;

pub(crate) extern "system" fn read_only(
    _me: ModHandle,
    address: usize,
    max: usize,
    out: *mut usize,
) -> Status {
    entry("read_only", || {
        // SAFETY: `HostApi::read_only` has the mod pass `out` null or pointing
        // at a writable, aligned `usize` that no other thread touches during
        // the call.
        unsafe { put(out, crate::pages::read_only(address, max)) }
    })
}
