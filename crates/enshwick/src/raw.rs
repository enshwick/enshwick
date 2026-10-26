//! Reads and writes of game memory at byte offsets, for structs the API does
//! not wrap: a field at an offset from [`Load::field_offset`] or
//! [`Load::constant`], inside a context or component a hook was given.
//!
//! Every read of game memory goes through [`read_at`], [`read_ptr_at`] or
//! [`read_bytes_at`]. A
//! read copies the value unaligned and makes no reference into game memory.
//! Its type must be valid for any bit pattern ([`AnyBitPattern`]), so a
//! `bool` or an enum, where most bit patterns are undefined behaviour, cannot
//! be read: read the integer and compare it.
//!
//! ```
//! use enshwick::raw::read_at;
//!
//! let state = [0u8, 1];
//! // SAFETY: `state` holds two readable bytes, and offset 1 is the second.
//! let open = unsafe { read_at::<u8>(state.as_ptr(), 1) } != 0;
//! assert!(open);
//! ```
//!
//! [`Load::field_offset`]: crate::Load::field_offset
//! [`Load::constant`]: crate::Load::constant

pub use bytemuck::{AnyBitPattern, NoUninit};

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// - `base + offset` is readable for `size_of::<T>()` bytes; no alignment is
///   needed.
/// - No other thread writes those bytes during the read.
///
/// # Examples
///
/// ```
/// use enshwick::raw::read_at;
///
/// let record = [0u8, 0, 0, 0, 0x2a, 0, 0, 0];
/// // SAFETY: the eight-byte array holds a u32 at offset 4.
/// let value: u32 = unsafe { read_at(record.as_ptr(), 4) };
/// assert_eq!(value, 42);
/// ```
///
/// A `bool` is refused at compile time, since a byte of 2 is no `bool`:
///
/// ```compile_fail
/// use enshwick::raw::read_at;
///
/// let state = [2u8];
/// // SAFETY: one readable byte.
/// let open: bool = unsafe { read_at(state.as_ptr(), 0) };
/// ```
#[inline]
#[must_use]
pub unsafe fn read_at<T: AnyBitPattern>(base: *const u8, offset: usize) -> T {
    // SAFETY: the caller guarantees `size_of::<T>()` readable bytes at
    // `base + offset` with no concurrent writer; `read_unaligned` needs no
    // alignment, and `T: AnyBitPattern` makes every bit pattern a valid `T`.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Reads a pointer to `T` at `offset` bytes past `base`, keeping the
/// provenance the game stored with it.
///
/// # Safety
///
/// - `base + offset` is readable for `size_of::<usize>()` bytes; no
///   alignment is needed.
/// - No other thread writes those bytes during the read.
///
/// # Examples
///
/// ```
/// use enshwick::raw::read_ptr_at;
///
/// let value = 7u32;
/// let slot = [&raw const value];
/// // SAFETY: `slot` holds one pointer at offset 0.
/// let found = unsafe { read_ptr_at::<u32>(slot.as_ptr().cast(), 0) };
/// assert_eq!(found.cast_const(), &raw const value);
/// ```
#[inline]
#[must_use]
pub unsafe fn read_ptr_at<T>(base: *const u8, offset: usize) -> *mut T {
    // SAFETY: the caller guarantees pointer-sized readable bytes at `base + offset`
    // with no concurrent writer; `read_unaligned` needs no alignment, and
    // every bit pattern is a valid raw pointer.
    unsafe { base.wrapping_add(offset).cast::<*mut T>().read_unaligned() }
}

/// Writes `value` at `offset` bytes past `base`.
///
/// # Safety
///
/// - `base + offset` is writable for `size_of::<T>()` bytes; no alignment is
///   needed.
/// - Nothing else reads or writes those bytes during the write.
/// - The game expects a `T` there, so its own readers see a valid value.
///
/// # Examples
///
/// ```
/// use enshwick::raw::{read_at, write_at};
///
/// let mut record = [0u8; 8];
/// // SAFETY: the eight-byte array takes a u16 at offset 3.
/// unsafe { write_at(record.as_mut_ptr(), 3, 0x0102_u16) };
/// // SAFETY: the same array, now holding that u16 at offset 3.
/// assert_eq!(unsafe { read_at::<u16>(record.as_ptr(), 3) }, 0x0102);
/// ```
#[inline]
pub unsafe fn write_at<T: NoUninit>(base: *mut u8, offset: usize, value: T) {
    // SAFETY: the caller guarantees `size_of::<T>()` writable bytes at
    // `base + offset` that nothing else touches; `write_unaligned` needs no
    // alignment, and `T: NoUninit` writes no uninitialised padding.
    unsafe { base.wrapping_add(offset).cast::<T>().write_unaligned(value) };
}

/// Writes the pointer `value` at `offset` bytes past `base`.
///
/// # Safety
///
/// - `base + offset` is writable for `size_of::<usize>()` bytes; no
///   alignment is needed.
/// - Nothing else reads or writes those bytes during the write.
///
/// # Examples
///
/// ```
/// use enshwick::raw::{read_ptr_at, write_ptr_at};
///
/// let mut value = 7u32;
/// let mut slot = [core::ptr::null_mut::<u32>()];
/// // SAFETY: `slot` takes one pointer at offset 0.
/// unsafe { write_ptr_at(slot.as_mut_ptr().cast(), 0, &raw mut value) };
/// // SAFETY: `slot` holds that pointer at offset 0.
/// assert_eq!(unsafe { read_ptr_at::<u32>(slot.as_ptr().cast(), 0) }, &raw mut value);
/// ```
#[inline]
pub unsafe fn write_ptr_at<T>(base: *mut u8, offset: usize, value: *mut T) {
    // SAFETY: the caller guarantees pointer-sized writable bytes at `base + offset`
    // that nothing else touches; `write_unaligned` needs no alignment.
    unsafe {
        base.wrapping_add(offset)
            .cast::<*mut T>()
            .write_unaligned(value);
    }
}

/// Copies `out.len()` bytes at `offset` bytes past `base` into `out`, for a
/// span of game memory such as a string or a payload; no reference into game
/// memory is made.
///
/// # Safety
///
/// - `base + offset` is readable for `out.len()` bytes.
/// - No other thread writes those bytes during the copy.
///
/// # Examples
///
/// ```
/// use enshwick::raw::read_bytes_at;
///
/// let record = *b"..name";
/// let mut name = [0u8; 4];
/// // SAFETY: the six-byte array holds four bytes at offset 2.
/// unsafe { read_bytes_at(record.as_ptr(), 2, &mut name) };
/// assert_eq!(&name, b"name");
/// ```
#[inline]
pub unsafe fn read_bytes_at(base: *const u8, offset: usize, out: &mut [u8]) {
    // SAFETY: the caller guarantees `out.len()` readable bytes at
    // `base + offset` with no concurrent writer; `out` is a distinct, writable
    // buffer of that length, and bytes need no alignment.
    unsafe {
        core::ptr::copy_nonoverlapping(base.wrapping_add(offset), out.as_mut_ptr(), out.len());
    }
}

/// Copies `bytes` to `offset` bytes past `base`, for a span of game memory
/// such as a string or a payload.
///
/// # Safety
///
/// - `base + offset` is writable for `bytes.len()` bytes.
/// - Nothing else reads or writes those bytes during the copy.
///
/// # Examples
///
/// ```
/// use enshwick::raw::write_bytes_at;
///
/// let mut record = [0u8; 6];
/// // SAFETY: the six-byte array takes four bytes at offset 2.
/// unsafe { write_bytes_at(record.as_mut_ptr(), 2, b"name") };
/// assert_eq!(&record, b"\0\0name");
/// ```
#[inline]
pub unsafe fn write_bytes_at(base: *mut u8, offset: usize, bytes: &[u8]) {
    // SAFETY: the caller guarantees `bytes.len()` writable bytes at
    // `base + offset` that nothing else touches; `bytes` is a distinct buffer
    // of that length, and bytes need no alignment.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), base.wrapping_add(offset), bytes.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_at_an_odd_offset_needs_no_alignment() {
        let bytes = [0u8, 0x78, 0x56, 0x34, 0x12, 0];
        // SAFETY: four readable bytes of `bytes` start at offset 1.
        let value: u32 = unsafe { read_at(bytes.as_ptr(), 1) };
        assert_eq!(value, 0x1234_5678);
    }

    #[test]
    fn an_array_reads_whole() {
        let floats = [1.0f32, 2.0, 3.0];
        // SAFETY: twelve readable bytes of `floats` start at offset 0.
        let value: [f32; 3] = unsafe { read_at(floats.as_ptr().cast(), 0) };
        assert_eq!(value, floats);
    }

    #[test]
    fn a_written_value_reads_back() {
        let mut bytes = [0u8; 9];
        // SAFETY: eight writable bytes of `bytes` start at offset 1.
        unsafe { write_at(bytes.as_mut_ptr(), 1, -5i64) };
        // SAFETY: the same eight bytes, now holding an i64.
        assert_eq!(unsafe { read_at::<i64>(bytes.as_ptr(), 1) }, -5);
    }

    #[test]
    fn a_byte_span_reads_and_writes_at_an_offset() {
        let mut bytes = [0u8; 8];
        // SAFETY: three writable bytes of `bytes` start at offset 5.
        unsafe { write_bytes_at(bytes.as_mut_ptr(), 5, &[1, 2, 3]) };
        let mut out = [0u8; 4];
        // SAFETY: four readable bytes of `bytes` start at offset 4.
        unsafe { read_bytes_at(bytes.as_ptr(), 4, &mut out) };
        assert_eq!(out, [0, 1, 2, 3]);
    }

    #[test]
    fn a_pointer_reads_back_unaligned() {
        let target = 3u16;
        let mut bytes = [0u8; 9];
        // SAFETY: eight writable bytes of `bytes` start at offset 1.
        unsafe { write_ptr_at(bytes.as_mut_ptr(), 1, (&raw const target).cast_mut()) };
        // SAFETY: the same eight bytes, now holding a pointer to `target`.
        let found = unsafe { read_ptr_at::<u16>(bytes.as_ptr(), 1) };
        // SAFETY: `found` is the pointer to `target`, live in this frame.
        assert_eq!(unsafe { *found }, 3);
    }
}
