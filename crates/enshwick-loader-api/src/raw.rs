//! Reads and writes of game memory at byte offsets, for structs the API does
//! not wrap: a field at an offset from [`Load::field_offset`] or
//! [`Load::constant`], inside a context or component a hook was given.
//!
//! Both work unaligned, as game structs often are, and copy the value: no
//! reference into game memory is made, so nothing outlives the read.
//!
//! [`Load::field_offset`]: crate::Load::field_offset
//! [`Load::constant`]: crate::Load::constant

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// `base + offset` is readable for `size_of::<T>()` bytes, which hold a valid
/// `T`, and no other thread writes them during the read.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::raw::read_at;
///
/// let record = [0u8, 0, 0, 0, 0x2a, 0, 0, 0];
/// // SAFETY: eight readable bytes; a u32 lies at offset 4.
/// let value: u32 = unsafe { read_at(record.as_ptr(), 4) };
/// assert_eq!(value, 42);
/// ```
#[inline]
#[must_use]
pub unsafe fn read_at<T: Copy>(base: *const u8, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().read_unaligned() }
}

/// Writes `value` at `offset` bytes past `base`.
///
/// # Safety
///
/// `base + offset` is writable for `size_of::<T>()` bytes, which nothing
/// else reads or writes during the write, and a `T` is what belongs there.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::raw::{read_at, write_at};
///
/// let mut record = [0u8; 8];
/// // SAFETY: eight writable bytes; a u16 belongs at offset 3.
/// unsafe { write_at(record.as_mut_ptr(), 3, 0x0102_u16) };
/// // SAFETY: as above.
/// assert_eq!(unsafe { read_at::<u16>(record.as_ptr(), 3) }, 0x0102);
/// ```
#[inline]
pub unsafe fn write_at<T: Copy>(base: *mut u8, offset: usize, value: T) {
    // SAFETY: forwarded from the caller.
    unsafe { base.wrapping_add(offset).cast::<T>().write_unaligned(value) };
}
