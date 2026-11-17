use super::*;

#[test]
fn a_guid_parses_to_memory_order_and_its_game_id() {
    let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    assert_eq!(&chest.0[..4], &[0x3a, 0x05, 0x43, 0x69]);
    assert_eq!(chest.fnv1a(), 1_302_084_846);
}

#[test]
fn growing_an_array_of_zero_sized_elements_fails_without_a_panic() {
    let mut blob = Blob::new(&[8, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(blob.grow_array(0, 0, 4, &[], &[(Vec::new(), None)]), None);
    assert_eq!(blob.bytes(), [8, 0, 0, 0, 0, 0, 0, 0]);
}

/// A root with one array field at 0 of two `u32` right after it.
fn two_u32() -> Vec<u8> {
    [8u32, 2, 1, 2]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

#[test]
fn a_sized_build_reserves_the_length_its_edits_reach_even_growing_one_array_twice() {
    let base = two_u32();
    let one = [(3u32.to_le_bytes().to_vec(), None)];
    let built = built(|out| {
        sized(out, &base, |blob| {
            blob.append(&[9; 3], 1);
            blob.grow_array(0, 4, 4, &[], &one)?;
            blob.grow_array(0, 4, 8, &[], &one)?;
            let (start, count) = blob.array(0)?;
            blob.set_array(0, start, count).then_some(())
        })
    })
    .unwrap();
    let blob = Blob::new(&built);
    assert_eq!(blob.array(0), Some((32, 4)));
    assert_eq!(blob.read_u32(32 + 12), Some(3));
}

#[test]
fn a_build_its_measure_cannot_follow_still_comes_out_whole() {
    let base = two_u32();
    let mut bytes = Vec::new();
    let answer = sized(&mut bytes, &base, |blob| {
        let at = blob.append(&5u32.to_le_bytes(), 4);
        // Past the base: a measure holds no appended byte, so it fails here.
        let five = blob.read_u32(at)?;
        blob.write(8, &five.to_le_bytes()).then_some(())
    });
    assert_eq!(answer, Some(()));
    assert_eq!(Blob::new(&bytes).read_u32(8), Some(5));
    assert_eq!(bytes.len(), 20);
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
