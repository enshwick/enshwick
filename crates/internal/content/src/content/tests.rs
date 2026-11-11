use super::*;

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
