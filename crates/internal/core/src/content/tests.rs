use super::*;

#[test]
fn a_builder_reads_a_null_base_as_empty() {
    let builder: Builder = Box::new(|base: &[u8]| Some(vec![u8::from(base.is_empty())]));
    let user = core::ptr::from_ref(&builder).cast_mut().cast();
    let (mut out, mut out_len) = (core::ptr::null::<u8>(), 0usize);
    // SAFETY: a null base, `user` points at the `Builder` alive in this
    // frame, and both out pointers are writable locals.
    let status = unsafe { build(core::ptr::null(), 0, user, &raw mut out, &raw mut out_len) };
    assert_eq!(status, Status::OK);
    // SAFETY: `build` returned OK and set `out` to the `out_len` bytes it
    // leaked, which are never freed.
    assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, [1]);
    // SAFETY: `user` points at the `Builder` alive in this frame, and
    // `build` refuses a null `out` before it writes anything.
    let refused = unsafe {
        build(
            core::ptr::null(),
            0,
            user,
            core::ptr::null_mut(),
            &raw mut out_len,
        )
    };
    assert_eq!(refused, Status::BAD_ARGS);
}

#[test]
fn a_guid_parses_to_memory_order_and_its_game_id() {
    let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    assert_eq!(&chest.0[..4], &[0x3a, 0x05, 0x43, 0x69]);
    assert_eq!(chest.fnv1a(), 1_302_084_846);
}

#[test]
fn moved_elements_keep_pointing_at_their_data() {
    // Root: array at 0 of one 8-byte element at 8; the element is a string
    // header pointing at "ab" at 16.
    let mut base = vec![0u8; 18];
    base[0..4].copy_from_slice(&8u32.to_le_bytes());
    base[4..8].copy_from_slice(&1u32.to_le_bytes());
    base[8..12].copy_from_slice(&8u32.to_le_bytes());
    base[12..16].copy_from_slice(&2u32.to_le_bytes());
    base[16..18].copy_from_slice(b"ab");
    let mut blob = Blob::new(&base);
    let copy = blob.bytes()[8..16].to_vec();
    assert_eq!(blob.grow_array(0, 8, 4, &[0], &[(copy, Some(8))]), Some(1));
    let (start, count) = blob.array(0).unwrap();
    assert_eq!(count, 2);
    for element in [start, start + 8] {
        let (text, len) = blob.array(element).unwrap();
        assert_eq!(&blob.bytes()[text..text + len as usize], b"ab");
    }
}
