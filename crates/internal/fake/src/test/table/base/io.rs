//! How the fake's entries read a mod's values and write their answers: the
//! loader's own helpers, behind the safe layer's promise that every pointer
//! it passes is valid for the call.

use enshwick_core::__fake::copy;
use enshwick_core::abi::{Headed, Status, Str};

/// Writes `value` through `out`, which the safe layer passes valid or null.
pub(crate) fn put<T>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes `out` null or a writable, aligned `T`
    // that nothing else touches during the call.
    unsafe { copy::put(out, value) }
}

pub(crate) fn found<T>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put(out, value))
}

pub(crate) fn text(s: Str) -> String {
    copy::text(s).map_or_else(|| "<not UTF-8>".to_owned(), str::to_owned)
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
pub(crate) fn put_list<T: Copy>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes `out` null or room for `capacity`
    // aligned entries and `count` null or a writable, aligned `u32`, which
    // nothing else touches during the call.
    unsafe { copy::put_list(items, out, capacity, count) }
}

/// A mod's struct that starts with a header, read as the loader reads it: it
/// needs only [`Headed::BORN`], and the fields past what the mod wrote read
/// zero, so a mod of an older minor works here as in the game.
pub(crate) fn get_headed<T: Headed>(raw: *const T) -> Option<T> {
    // SAFETY: the safe layer passes null or its version of `T`, readable for
    // the size its header states, with no other writer during the call.
    unsafe { copy::headed_in(raw, T::BORN) }
}

/// Writes `value` to a mod's struct that starts with a header, as the loader
/// does: only as far as the mod's header says, with that size written back.
pub(crate) fn put_headed<T: Headed>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes null or its version of `T`, its header
    // stating the size it may be written for, untouched by others during the
    // call.
    unsafe { copy::put_headed(out, value) }
}

/// [`found`] for a struct that starts with a header, written as
/// [`put_headed`] writes it.
pub(crate) fn found_headed<T: Headed>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put_headed(out, value))
}

/// A mod's list of `count` headed entries, read as the loader reads it.
pub(crate) fn get_headed_list<T: Headed>(raw: *const T, count: u32) -> Option<Vec<T>> {
    // SAFETY: the safe layer passes `count` entries at `raw`, each starting
    // with its header and readable for the size it states, with no other
    // writer during the call.
    unsafe { copy::headed_list_in(raw, count, T::BORN) }
}

/// Writes up to `capacity` of `items` to a mod's list of headed entries, as
/// the loader does.
pub(crate) fn put_headed_list<T: Headed>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes `out` null or room for `capacity`
    // entries, each as large as the first one's header states, and `count`
    // null or a writable, aligned `u32`, untouched by others during the call.
    unsafe { copy::put_headed_list(items, out, capacity, count) }
}

/// Copies up to `capacity` of `items` to `out`, `entry_size` bytes apart, and
/// writes how many there are, as the loader does.
pub(crate) fn list<T: Copy>(
    items: &[T],
    out: *mut T,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    // SAFETY: the safe layer passes `out` null or room for `capacity`
    // entries of `entry_size` bytes and `count` null or a writable, aligned
    // `u32`, untouched by others during the call.
    unsafe { copy::put_sized_list(items, out, entry_size, capacity, count) }
}
