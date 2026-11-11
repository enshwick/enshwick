//! How the fake's entries read a mod's values and write their answers, as
//! the loader's table does.

use enshwick_core::abi::{Header, Status, Str};

use super::mods::list;

/// Writes `value` through `out`, which the safe layer passes valid or null.
pub(crate) fn put<T>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes a valid out pointer; `as_mut` turns away null.
    match unsafe { out.as_mut() } {
        Some(slot) => {
            *slot = value;
            Status::OK
        }
        None => Status::BAD_ARGS,
    }
}

pub(crate) fn found<T>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put(out, value))
}

pub(crate) fn text(s: Str) -> String {
    // SAFETY: strings are borrowed for the call.
    unsafe { s.as_str() }.map_or_else(|| "<not UTF-8>".to_owned(), str::to_owned)
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
pub(crate) fn put_list<T: Copy>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if count.is_null() {
        return Status::BAD_ARGS;
    }
    if !out.is_null() {
        for (i, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `i` is below `capacity` and the safe layer passes a
            // non-null `out` with room for `capacity` entries, so the offset
            // stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned
            // entries the safe layer passes at `out`.
            unsafe { slot.write(*item) };
        }
    }
    // SAFETY: `count` is non-null, checked at the top; the safe layer passes
    // it pointing at a writable, aligned `u32` for the call.
    unsafe { count.write(u32::try_from(items.len()).unwrap_or(u32::MAX)) };
    Status::OK
}

/// A mod's struct that starts with a header, read as the loader reads it: it
/// needs only [`Headed::BORN`](enshwick_core::abi::Headed::BORN), and the fields past
/// what the mod wrote read zero, so a mod of an older minor works here as in
/// the game.
pub(crate) fn get_headed<T: enshwick_core::abi::Headed>(raw: *const T) -> Option<T> {
    // SAFETY: the safe layer passes null or its version of `T`, readable for
    // the size its header states, with no other writer during the call.
    unsafe { enshwick_core::abi::read_headed(raw, T::BORN) }
}

/// Writes `value` to a mod's struct that starts with a header, as the loader
/// does: only as far as the mod's header says, with that size written back.
pub(crate) fn put_headed<T: enshwick_core::abi::Headed>(out: *mut T, value: T) -> Status {
    // SAFETY: the safe layer passes null or its version of `T`, its header
    // stating the size it may be written for, untouched by others during the
    // call.
    if unsafe { enshwick_core::abi::write_headed(out, value) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

/// [`found`] for a struct that starts with a header, written as
/// [`put_headed`] writes it.
pub(crate) fn found_headed<T: enshwick_core::abi::Headed>(value: Option<T>, out: *mut T) -> Status {
    value.map_or(Status::MISSING, |value| put_headed(out, value))
}

/// A mod's list of `count` headed entries, read as the loader reads it: the
/// entries are as far apart as the first one's header says, every one states
/// that size, and each is read as [`get_headed`] reads one. `None` for a
/// null list with entries, or entries of different sizes.
pub(crate) fn get_headed_list<T: enshwick_core::abi::Headed>(
    raw: *const T,
    count: u32,
) -> Option<Vec<T>> {
    if count == 0 {
        return Some(Vec::new());
    }
    if raw.is_null() {
        return None;
    }
    // SAFETY: a non-null list from the safe layer starts with an entry's
    // header, readable for its eight bytes.
    // The mod's headed list, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let stride = unsafe { raw.cast::<Header>().read_unaligned() }.size as usize;
    if stride < size_of::<Header>() {
        return None;
    }
    (0..count as usize)
        .map(|index| {
            let entry = raw.cast::<u8>().wrapping_add(index * stride).cast::<T>();
            // SAFETY: the safe layer passes `count` entries of `stride` bytes,
            // so `entry` starts one of them, its header readable.
            // The mod's headed list, not game memory.
            // ast-grep-ignore: game-memory-through-raw
            let size = unsafe { entry.cast::<Header>().read_unaligned() }.size as usize;
            (size == stride).then(|| get_headed(entry))?
        })
        .collect()
}

/// Writes up to `capacity` of `items` to a mod's list of headed entries, as
/// the loader does: entries are as far apart as the first one's header says,
/// and each gets only what both sides know.
pub(crate) fn put_headed_list<T: enshwick_core::abi::Headed>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if out.is_null() || capacity == 0 {
        return list(items, core::ptr::null_mut(), 0, 0, count);
    }
    let stride = {
        // SAFETY: a non-null list with room for an entry, checked above,
        // starts with that entry's header, readable for its eight bytes.
        // The mod's headed list, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        unsafe { out.cast::<Header>().read_unaligned() }.size
    };
    list(items, out, stride, capacity, count)
}
