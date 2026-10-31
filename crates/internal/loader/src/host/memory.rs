//! Reading and writing the ABI's headed structs and lists across the boundary.

use enshwick::abi::{Header, Status, Str};

/// A slice from a pointer and a count, empty for a null pointer.
///
/// # Safety
///
/// - A non-null `ptr` with a nonzero `count` points at `count` initialised,
///   aligned values.
/// - Those values stay alive and unchanged for the lifetime `'a` the caller
///   picks.
pub(crate) unsafe fn slice<'a, T>(ptr: *const T, count: usize) -> &'a [T] {
    if ptr.is_null() || count == 0 {
        return &[];
    }
    // SAFETY: `ptr` is non-null and `count` nonzero, checked just before; the
    // caller guarantees `count` aligned values there, alive and unchanged for
    // `'a`.
    unsafe { core::slice::from_raw_parts(ptr, count) }
}

/// The value of the symbol named `name`.
pub(crate) fn find(symbols: &[enshwick::abi::Symbol], name: &str) -> Option<u64> {
    symbols
        .iter()
        .find_map(|symbol| (text(symbol.name) == Some(name)).then_some(symbol.rva))
}

/// Writes `value`, a struct that starts with a [`Header`], through `out`, as
/// far as the mod's header says its struct reaches.
///
/// # Safety
///
/// - `out` is null or points at the mod's version of `T`, starting with its
///   header and writable for the size that header states; no alignment is
///   needed.
/// - No other thread reads or writes that struct during the call.
pub(crate) unsafe fn put_headed<T: enshwick::abi::Headed>(out: *mut T, value: T) -> Status {
    // SAFETY: `write_headed` needs `out` null or writable for its header's
    // size with no concurrent access, which this function's `# Safety`
    // bullets guarantee.
    if unsafe { enshwick::abi::write_headed(out, value) } {
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
pub(crate) unsafe fn get_headed<T: enshwick::abi::Headed>(
    raw: *const T,
    required: usize,
) -> Option<T> {
    // SAFETY: the caller guarantees `raw` null or readable for the size its
    // header gives, with no concurrent writer, as `read_headed` requires.
    unsafe { enshwick::abi::read_headed(raw, required) }
}

/// The size of each entry of a mod's list at `out`, from the header of its
/// first entry: `None` when that is shorter than a [`Header`].
///
/// # Safety
///
/// - `out` is readable for the eight bytes of a [`Header`]; no alignment is
///   needed.
/// - No other thread writes those bytes during the read.
pub(crate) unsafe fn stride_of<T>(out: *const T) -> Option<usize> {
    // SAFETY: the caller guarantees `out` readable for a `Header`'s eight
    // bytes with no concurrent writer; the read is unaligned.
    // ast-grep-ignore: game-memory-through-raw
    let size = unsafe { out.cast::<Header>().read_unaligned() }.size as usize;
    (size >= size_of::<Header>()).then_some(size)
}

/// Writes `items` into a mod's list of `capacity` entries, each as large as
/// the header of its first entry says, as far as both sides know an entry,
/// and their total through `count`.
///
/// # Safety
///
/// - Unless `out` is null or `capacity` zero, `out` points at `capacity`
///   consecutive entries of the mod's version of `T`, each starting with its
///   header and writable for the size the first entry's header states; no
///   alignment is needed.
/// - `count` is null or points at a writable, aligned `u32`.
/// - No other thread reads or writes the list or `count` during the call.
pub(crate) unsafe fn put_headed_list<T: enshwick::abi::Headed>(
    items: &[T],
    out: *mut T,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let Ok(total) = u32::try_from(items.len()) else {
        return Status::BAD_ARGS;
    };
    if !out.is_null() && capacity > 0 {
        // SAFETY: `out` is non-null and `capacity` non-zero, checked just
        // above, so the first bullet of this function's `# Safety` gives a
        // first entry starting with its header the mod filled in, and the
        // last bullet no concurrent writer.
        let Some(stride) = (unsafe { stride_of(out) }) else {
            return Status::BAD_ARGS;
        };
        let known = stride.min(size_of::<T>());
        let size = u32::try_from(known).unwrap_or(u32::MAX);
        for (index, item) in items.iter().take(capacity as usize).enumerate() {
            // SAFETY: `index` is below `capacity`, and this function's
            // `# Safety` gives `capacity` entries of `stride` bytes at `out`,
            // so entry `index` starts `index * stride` bytes into the same
            // allocation.
            let entry = unsafe { out.cast::<u8>().add(index * stride) };
            // SAFETY: `entry` is writable for `stride` bytes by this
            // function's `# Safety`, `item` is readable for `size_of::<T>()`,
            // `known` is at most both, and the loader's `items` do not overlap
            // the mod's list.
            unsafe {
                core::ptr::copy_nonoverlapping((&raw const *item).cast::<u8>(), entry, known);
            }
            // SAFETY: the header's `size` is the entry's first field, inside the
            // `stride` bytes, at least a `Header`, that `stride_of` checked.
            unsafe { entry.cast::<u32>().write_unaligned(size) };
        }
    }
    // SAFETY: `count` is null or a writable, aligned `u32` that no other thread
    // touches during the call, by this function's `# Safety`.
    unsafe { put(count, total) }
}

/// Copies a mod's list of `count` entries at `raw`, each as large as the
/// header of its first entry says; `None` when an entry is shorter than
/// `required`.
///
/// # Safety
///
/// - `count` is zero, or `raw` is null or points at `count` consecutive
///   entries of the mod's version of the struct.
/// - Each entry starts with its header and is readable for the size the
///   first entry's header gives; no alignment is needed.
/// - No other thread writes the list during the copy.
pub(crate) unsafe fn get_headed_list<T: enshwick::abi::Headed>(
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
    let stride = unsafe { stride_of(raw) }?;
    (0..count as usize)
        .map(|index| {
            // SAFETY: `index` is below `count`, and the caller guarantees
            // `count` entries, each as large as the first entry's header says,
            // so entry `index` starts `index * stride` bytes into the list.
            let entry = unsafe { raw.cast::<u8>().add(index * stride) }.cast::<T>();
            // SAFETY: `entry` starts one of the caller's `count` entries, each
            // starting with its header.
            if unsafe { stride_of(entry) } != Some(stride) {
                return None;
            }
            // SAFETY: `entry` starts one of the caller's entries, readable for
            // `stride` bytes, the size its own header gives as just checked.
            unsafe { get_headed(entry, required) }
        })
        .collect()
}

/// Reads a borrowed string, or `None` when it is not UTF-8.
pub(crate) fn text<'a>(value: Str) -> Option<&'a str> {
    // SAFETY: every `Str` a mod passes is borrowed for the call, which outlives
    // this use.
    unsafe { value.as_str() }
}

/// Writes `value` through `out` when `out` is not null.
///
/// # Safety
///
/// - `out` is null or points at a `T` that is writable and aligned.
/// - No other thread reads or writes that `T` during the call.
pub(crate) unsafe fn put<T>(out: *mut T, value: T) -> Status {
    if out.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `out` is non-null, checked just before, so this function's
    // `# Safety` makes it a writable, aligned `T` with no concurrent access.
    unsafe { out.write(value) };
    Status::OK
}

/// Writes up to `capacity` of `items` to `out` and their number to `count`.
///
/// # Safety
///
/// - `out` is null or points at `capacity` consecutive writable, aligned
///   `T`s.
/// - `count` is null or points at a writable, aligned `u32`.
/// - No other thread reads or writes those entries or `count` during the
///   call.
pub(crate) unsafe fn put_list<T: Copy>(
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
