//! How a host reads a mod's values and writes its answers across the
//! boundary, once for the loader and the fake host, so both refuse and
//! write the same way.

use crate::abi::{Headed, Header, Status, Str};

/// A slice from a pointer and a count, empty for a null pointer.
///
/// # Safety
///
/// - A non-null `ptr` with a nonzero `count` points at `count` initialised,
///   aligned values.
/// - Those values stay alive and unchanged for the lifetime `'a` the caller
///   picks.
pub unsafe fn slice<'a, T>(ptr: *const T, count: usize) -> &'a [T] {
    if ptr.is_null() || count == 0 {
        return &[];
    }
    // SAFETY: `ptr` is non-null and `count` nonzero, checked just before; the
    // caller guarantees `count` aligned values there, alive and unchanged for
    // `'a`.
    unsafe { core::slice::from_raw_parts(ptr, count) }
}

/// A mod's list of `count` values at `ptr`: `None` for a null pointer with
/// a nonzero count, which no list can be.
///
/// # Safety
///
/// As [`slice`].
pub unsafe fn list_in<'a, T>(ptr: *const T, count: usize) -> Option<&'a [T]> {
    if ptr.is_null() && count != 0 {
        return None;
    }
    // SAFETY: the caller's guarantees are `slice`'s.
    Some(unsafe { slice(ptr, count) })
}

/// Reads a borrowed string, or `None` when it is not UTF-8.
#[must_use]
pub fn text<'a>(value: Str) -> Option<&'a str> {
    // SAFETY: every `Str` a mod passes is borrowed for the call, which outlives
    // this use.
    unsafe { value.as_str() }
}

/// Writes `value` through `out`: [`Status::BAD_ARGS`] for a null `out`.
///
/// # Safety
///
/// - `out` is null or points at `size_of::<T>()` writable bytes; no
///   alignment is needed.
/// - No other thread reads or writes those bytes during the call.
pub unsafe fn put<T>(out: *mut T, value: T) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null, checked just before, so this function's
    // `# Safety` makes it writable for a `T` with no concurrent access; an
    // unaligned write needs no alignment.
    unsafe { out.write_unaligned(value) };
    Status::OK
}

/// Writes `value`, a struct that starts with a [`Header`], through `out`, as
/// far as the mod's header says its struct reaches: [`Status::BAD_ARGS`] for
/// a null `out` or a header shorter than a [`Header`].
///
/// # Safety
///
/// - `out` is null or points at the mod's version of `T`, starting with its
///   header and writable for the size that header states; no alignment is
///   needed.
/// - No other thread reads or writes that struct during the call.
pub unsafe fn put_headed<T: Headed>(out: *mut T, value: T) -> Status {
    // SAFETY: `write_headed` needs `out` null or writable for its header's
    // size with no concurrent access, which this function's `# Safety`
    // bullets guarantee.
    if unsafe { crate::abi::write_headed(out, value) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}

/// Copies a struct that starts with a [`Header`] from a mod, which may know
/// an older version of it: `None` for null or for a struct shorter than
/// `required`, the end of the fields every version of it has.
///
/// # Safety
///
/// - `raw` is null or points at the mod's version of the struct, readable
///   for the size its header gives; no alignment is needed.
/// - No other thread writes that struct during the copy.
pub unsafe fn headed_in<T: Headed>(raw: *const T, required: usize) -> Option<T> {
    // SAFETY: the caller guarantees `raw` null or readable for the size its
    // header gives, with no concurrent writer, as `read_headed` requires.
    unsafe { crate::abi::read_headed(raw, required) }
}

/// The size the header at `raw` states: `None` when that is shorter than a
/// [`Header`].
///
/// # Safety
///
/// - `raw` is readable for the eight bytes of a [`Header`]; no alignment is
///   needed.
/// - No other thread writes those bytes during the read.
pub unsafe fn room<T>(raw: *const T) -> Option<usize> {
    // SAFETY: the caller guarantees `raw` readable for a `Header`'s eight
    // bytes with no concurrent writer; the read is unaligned.
    // An entry of a mod's headed struct or list, not game memory.
    // ast-grep-ignore: game-memory-through-raw
    let size = unsafe { raw.cast::<Header>().read_unaligned() }.size as usize;
    (size >= size_of::<Header>()).then_some(size)
}

/// Copies a mod's list of `count` entries at `raw`, each as large as the
/// header of its first entry says.
///
/// `None` for a null list with entries, an entry whose header states another
/// size, or one shorter than `required`.
///
/// # Safety
///
/// - `count` is zero, or `raw` is null or points at `count` consecutive
///   entries of the mod's version of the struct.
/// - Each entry starts with its header and is readable for the size the
///   first entry's header gives; no alignment is needed.
/// - No other thread writes the list during the copy.
pub unsafe fn headed_list_in<T: Headed>(
    raw: *const T,
    count: u32,
    required: usize,
) -> Option<Vec<T>> {
    if count == 0 {
        return Some(Vec::new());
    }
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null, checked just before, and `count` is nonzero,
    // so the caller guarantees a first entry that starts with its header.
    let stride = unsafe { room(raw) }?;
    (0..count as usize)
        .map(|index| {
            // SAFETY: `index` is below `count`, and the caller guarantees
            // `count` entries, each as large as the first entry's header says,
            // so entry `index` starts `index * stride` bytes into the list.
            let entry = unsafe { raw.cast::<u8>().add(index * stride) }.cast::<T>();
            // SAFETY: `entry` starts one of the caller's `count` entries, each
            // starting with its header.
            if unsafe { room(entry) } != Some(stride) {
                return None;
            }
            // SAFETY: `entry` starts one of the caller's entries, readable for
            // `stride` bytes, the size its own header gives as just checked.
            unsafe { headed_in(entry, required) }
        })
        .collect()
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`:
/// [`Status::BAD_ARGS`] for a null `count`, the items written all the same.
///
/// # Safety
///
/// - `out` is null or points at `capacity` consecutive writable, aligned
///   `T`s.
/// - `count` is null or points at a writable, aligned `u32`.
/// - No other thread reads or writes those entries or `count` during the
///   call.
pub unsafe fn put_list<T: Copy>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Ok(total) = u32::try_from(items.len()) else {
        return Status::BAD_ARGS;
    };
    if !out.is_null() {
        for (i, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `out` is non-null, checked just before, and `i` is below
            // `capacity`; this function's `# Safety` gives `capacity` entries
            // at `out`, so the offset stays inside that array.
            let slot = unsafe { out.add(i) };
            // SAFETY: `slot` is inside the `capacity` writable, aligned entries
            // at `out` that this function's `# Safety` gives, with no
            // concurrent access.
            unsafe { slot.write(*item) };
        }
    }
    // SAFETY: `count` is null or a writable, aligned `u32` that no other thread
    // touches during the call, by this function's `# Safety`.
    unsafe { put(count, total) }
}

/// Writes up to `capacity` of `items` to `out`, `stride` bytes apart, and
/// their number to `count`.
///
/// Each entry gets as much as both sides know, with its header saying so;
/// [`Status::BAD_ARGS`] for a stride shorter than a [`Header`] or a null
/// `count`.
///
/// # Safety
///
/// - A non-null `out` is writable for `capacity` entries of `stride` bytes
///   each; no alignment is needed.
/// - `count` is null or points at a writable, aligned `u32`.
/// - No other thread reads or writes those entries or `count` during the
///   call.
pub unsafe fn put_sized_list<T: Copy>(
    items: &[T],
    out: *mut T,
    stride: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Ok(total) = u32::try_from(items.len()) else {
        return Status::BAD_ARGS;
    };
    if !out.is_null() {
        let stride = stride as usize;
        if stride < size_of::<Header>() {
            return Status::BAD_ARGS;
        }
        let known = stride.min(size_of::<T>());
        let size = u32::try_from(known).unwrap_or(u32::MAX);
        for (index, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `index` is below `capacity`, and the caller guarantees
            // `capacity` entries of `stride` bytes at the non-null `out`, so
            // entry `index` starts inside that list.
            let entry = unsafe { out.cast::<u8>().add(index * stride) };
            // SAFETY: the caller guarantees `entry` writable for `stride`
            // bytes, `item` is readable for `size_of::<T>()`, `known` is at
            // most both, and the host's `items` do not overlap the mod's list.
            unsafe {
                core::ptr::copy_nonoverlapping((&raw const *item).cast::<u8>(), entry, known);
            }
            // SAFETY: the header's `size` is the entry's first field, inside
            // the `stride` bytes, at least a `Header` as checked before the
            // loop.
            unsafe { entry.cast::<u32>().write_unaligned(size) };
        }
    }
    // SAFETY: `count` is null or a writable, aligned `u32` that no other
    // thread touches during the call, by this function's `# Safety`.
    unsafe { put(count, total) }
}

/// [`put_sized_list`] for a mod's list of headed entries, each as large as
/// the header of its first entry says.
///
/// # Safety
///
/// - Unless `out` is null or `capacity` zero, `out` points at `capacity`
///   consecutive entries of the mod's version of `T`, each starting with its
///   header and writable for the size the first entry's header states; no
///   alignment is needed.
/// - `count` is null or points at a writable, aligned `u32`.
/// - No other thread reads or writes the list or `count` during the call.
pub unsafe fn put_headed_list<T: Headed>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    if out.is_null() || capacity == 0 {
        // SAFETY: a null list writes no entry; `count` is as this function's
        // `# Safety` gives it.
        return unsafe { put_sized_list(items, core::ptr::null_mut(), 0, 0, count) };
    }
    // SAFETY: `out` is non-null and `capacity` non-zero, checked just above,
    // so this function's `# Safety` gives a first entry starting with the
    // header the mod filled in, with no concurrent writer.
    let Some(stride) = (unsafe { room(out) }) else {
        return Status::BAD_ARGS;
    };
    let stride = u32::try_from(stride).unwrap_or(u32::MAX);
    // SAFETY: this function's `# Safety` gives `capacity` entries of the
    // first entry's size at `out` and `count` as `put_sized_list` needs them.
    unsafe { put_sized_list(items, out, stride, capacity, count) }
}

/// Copies `data` to `out` and writes its length to `len`.
///
/// [`Status::BAD_ARGS`] when `capacity` is short or `out` is null, with
/// `len` written, so the caller grows its buffer and asks again;
/// [`Status::REFUSED`] for data past `u32::MAX` bytes, which no length
/// the ABI carries can name.
///
/// # Safety
///
/// - `len` is null or points at a writable, aligned `u32`.
/// - `out` is null or writable for `capacity` bytes, apart from `data`.
pub unsafe fn copy_out(data: &[u8], out: *mut u8, capacity: u32, len: *mut u32) -> Status {
    let Ok(size) = u32::try_from(data.len()) else {
        return Status::REFUSED;
    };
    if !len.is_null() {
        // SAFETY: `len` is non-null, checked just before, and the caller
        // guarantees it writable and aligned.
        unsafe { len.write(size) };
    }
    if size > capacity || out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null and `data` is at most `capacity` bytes, both
    // checked just before; the caller guarantees `capacity` writable bytes
    // at `out`, apart from `data`.
    unsafe { out.copy_from_nonoverlapping(data.as_ptr(), data.len()) };
    Status::OK
}

#[cfg(test)]
mod tests;
