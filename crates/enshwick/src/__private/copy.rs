//! The copy-out tail every host call that hands a mod bytes shares, so the
//! loader and the fake host check the length the same way.

use crate::abi::Status;

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
mod tests {
    use super::*;

    #[test]
    fn a_short_buffer_gets_the_length_and_a_long_enough_one_the_bytes() {
        let data = [1u8, 2, 3];
        let (mut out, mut len) = ([0u8; 3], 0u32);
        // SAFETY: `out` holds 3 writable bytes and `len` is a local `u32`.
        let short = unsafe { copy_out(&data, out.as_mut_ptr(), 2, &raw mut len) };
        assert_eq!((short, len, out), (Status::BAD_ARGS, 3, [0; 3]));
        // SAFETY: `out` holds 3 writable bytes and `len` is a local `u32`.
        let copied = unsafe { copy_out(&data, out.as_mut_ptr(), 3, &raw mut len) };
        assert_eq!((copied, len, out), (Status::OK, 3, data));
    }

    #[test]
    fn a_null_out_only_asks_the_length() {
        let mut len = 0u32;
        // SAFETY: a null `out` is allowed and `len` is a local `u32`.
        let status = unsafe { copy_out(&[9; 5], core::ptr::null_mut(), 5, &raw mut len) };
        assert_eq!((status, len), (Status::BAD_ARGS, 5));
    }
}
