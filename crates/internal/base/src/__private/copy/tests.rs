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

#[test]
fn a_list_in_refuses_only_a_null_pointer_with_entries() {
    let values = [4u32, 5];
    // SAFETY: a null pointer with no entries reads nothing.
    let none = unsafe { list_in::<u32>(core::ptr::null(), 0) };
    // SAFETY: a null pointer with entries is refused before any read.
    let refused = unsafe { list_in::<u32>(core::ptr::null(), 2) };
    // SAFETY: `values` holds two aligned `u32`s alive for the test.
    let read = unsafe { list_in(values.as_ptr(), 2) };
    assert_eq!(none, Some(&[][..]));
    assert_eq!(refused, None);
    assert_eq!(read, Some(&values[..]));
}

#[test]
fn a_list_writes_what_fits_and_counts_every_item() {
    let items = [1u16, 2, 3];
    let (mut out, mut count) = ([0u16; 2], 0u32);
    // SAFETY: `out` holds two writable `u16`s and `count` is a local `u32`.
    let status = unsafe { put_list(&items, out.as_mut_ptr(), 2, &raw mut count) };
    assert_eq!((status, out, count), (Status::OK, [1, 2], 3));
}

#[test]
fn a_put_refuses_a_null_out() {
    // SAFETY: a null `out` is refused before any write.
    let status = unsafe { put(core::ptr::null_mut::<u8>(), 1) };
    assert_eq!(status, Status::BAD_ARGS);
}

#[test]
#[expect(clippy::cast_ptr_alignment, reason = "the out is unaligned on purpose")]
fn a_put_writes_through_an_out_that_is_not_aligned() {
    let mut bytes = [0u8; 8];
    let out = bytes.as_mut_ptr().wrapping_add(1).cast::<u32>();
    // SAFETY: `out` is bytes 1 to 4 of the local array, writable; it is odd,
    // so not aligned for a `u32`.
    let status = unsafe { put(out, 0x0403_0201) };
    assert_eq!((status, bytes), (Status::OK, [0, 1, 2, 3, 4, 0, 0, 0]));
}
