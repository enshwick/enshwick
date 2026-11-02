use super::*;

fn patch(owner: ModIndex, address: usize, expected: &[u8], replacement: &[u8]) -> CodePatch {
    CodePatch {
        owner,
        feature: WHOLE_MOD,
        address,
        expected: expected.into(),
        replacement: replacement.into(),
        needs: Box::default(),
    }
}

/// A recipe list lookup the table lacks turns off like every other
/// loader part: its line names the build, and the status lists it.
#[test]
fn a_missing_recipe_list_lookup_is_off_on_its_build_and_in_the_status() {
    let build = crate::builds::embedded(enshwick::abi::Side::SERVER);
    let off = LiftLimitError::NotInBuild {
        entry: pinned::recipe::LIST_LOOKUP.id().to_owned(),
    };
    let (line, entry) = lookup_off(&build, &off);
    assert_eq!(
        line,
        format!(
            "recipe list lookup off on {}: recipe.list_lookup is not in this build",
            build.name
        )
    );
    assert_eq!(entry, "recipe.list_lookup is not in this build");
}

#[test]
fn ranges_overlap_only_when_they_share_a_byte() {
    assert!(overlaps((10, 5), (14, 1)));
    assert!(overlaps((14, 1), (10, 5)));
    assert!(!overlaps((10, 5), (15, 3)));
    assert!(!overlaps((15, 3), (10, 5)));
    assert!(!overlaps((10, 0), (10, 5)));
}

#[test]
fn a_patch_that_reads_as_expected_is_written_and_a_changed_one_skipped() {
    let code = [0x83_u8, 0xf8, 0x08, 0x72, 0x05, 0xc3];
    let base = code.as_ptr().addr();
    let current = |address: usize, len: usize| {
        let from = address.checked_sub(base)?;
        code.get(from..from + len).map(<[u8]>::to_vec)
    };
    let patches = [
        patch(ModIndex::new(0), base + 2, &[0x08], &[0x10]),
        patch(ModIndex::new(1), base + 3, &[0x74], &[0xeb]),
        patch(ModIndex::new(2), base + 5, &[0xc3, 0x90], &[0x90, 0x90]),
    ];
    let verdicts = check(&patches, &[], |_| false, current);
    assert_eq!(verdicts[0], Verdict::Write);
    assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("72")));
    assert!(matches!(&verdicts[2], Verdict::Skip(why) if why.contains("cannot be read")));
}

#[test]
fn an_owner_s_patches_go_in_together_or_not_at_all() {
    let code = [0x83_u8, 0xf8, 0x08, 0x72, 0x05, 0xc3];
    let base = code.as_ptr().addr();
    let current = |address: usize, len: usize| {
        let from = address.checked_sub(base)?;
        code.get(from..from + len).map(<[u8]>::to_vec)
    };
    let patches = [
        patch(ModIndex::new(0), base + 2, &[0x08], &[0x10]),
        patch(ModIndex::new(0), base + 3, &[0x74], &[0xeb]),
        patch(ModIndex::new(1), base + 5, &[0xc3], &[0x90]),
    ];
    let verdicts = check(&patches, &[], |_| false, current);
    assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("another patch")));
    assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("72")));
    assert_eq!(verdicts[2], Verdict::Write);
}

#[test]
fn a_skipped_loader_patch_leaves_out_only_its_own_feature() {
    let feature = |name, at| CodePatch {
        feature: name,
        ..patch(ModIndex::LOADER, at, &[0x90], &[0xcc])
    };
    let patches = [
        feature("one", 0x10),
        feature("one", 0x20),
        feature("two", 0x30),
    ];
    let mut verdicts = [
        Verdict::Skip("stuck".into()),
        Verdict::Write,
        Verdict::Write,
    ];
    skip_with_feature(&patches, &mut verdicts);
    assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("another patch")));
    assert_eq!(verdicts[2], Verdict::Write);
}

#[test]
fn a_patch_over_hooked_bytes_is_skipped_and_one_of_a_mod_that_is_off_left_out() {
    let code = [0x90_u8; 16];
    let base = code.as_ptr().addr();
    let current = |address: usize, len: usize| {
        let from = address.checked_sub(base)?;
        code.get(from..from + len).map(<[u8]>::to_vec)
    };
    let patches = [
        patch(ModIndex::new(0), base + 4, &[0x90, 0x90], &[0xcc, 0xcc]),
        patch(ModIndex::new(1), base + 12, &[0x90], &[0xcc]),
    ];
    let verdicts = check(
        &patches,
        &[(base, 5)],
        |owner| owner == ModIndex::new(1),
        current,
    );
    assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("hook")));
    assert_eq!(verdicts[1], Verdict::Off);
}

#[test]
fn registering_refuses_overlaps_bad_lengths_and_bytes_outside_the_code() {
    let text = (0x1000, 0x100);
    // Each test owns its addresses: the registry is shared.
    assert_eq!(
        register(patch(ModIndex::new(0), 0x1010, &[1, 2], &[3, 4]), text),
        Status::OK
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x1011, &[2], &[5]), text),
        Status::REFUSED
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x1012, &[9], &[5]), text),
        Status::OK
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x1020, &[1], &[]), text),
        Status::BAD_ARGS
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x1020, &[], &[]), text),
        Status::BAD_ARGS
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x10ff, &[1, 2], &[3, 4]), text),
        Status::BAD_ARGS
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x0fff, &[1, 2], &[3, 4]), text),
        Status::BAD_ARGS
    );
}

#[test]
fn a_patch_whose_hook_is_not_armed_stays_out_with_its_owner_s_others() {
    let code = [0x90_u8; 16];
    let base = code.as_ptr().addr();
    let current = |address: usize, len: usize| {
        let from = address.checked_sub(base)?;
        code.get(from..from + len).map(<[u8]>::to_vec)
    };
    let hook = u64::try_from(base + 8).unwrap();
    let needing = |owner, at| CodePatch {
        needs: Box::new([hook]),
        ..patch(owner, base + at, &[0x90], &[0xcc])
    };
    let patches = [
        needing(ModIndex::new(0), 0),
        patch(ModIndex::new(0), base + 1, &[0x90], &[0xcc]),
    ];
    let verdicts = check(&patches, &[], |_| false, current);
    assert!(matches!(&verdicts[0], Verdict::Skip(why) if why.contains("needs")));
    assert!(matches!(&verdicts[1], Verdict::Skip(why) if why.contains("another patch")));
    let verdicts = check(&patches, &[(base + 8, 5)], |_| false, current);
    assert_eq!(verdicts, [Verdict::Write, Verdict::Write]);
}

#[test]
fn a_set_of_patches_registers_whole_or_not_at_all() {
    let text = (0x2000, 0x100);
    assert_eq!(
        register_all(
            vec![
                patch(ModIndex::new(0), 0x2010, &[1], &[2]),
                patch(ModIndex::new(0), 0x2020, &[1, 2], &[3]),
            ],
            text
        ),
        Status::BAD_ARGS
    );
    assert_eq!(
        register_all(
            vec![
                patch(ModIndex::new(0), 0x2010, &[1], &[2]),
                patch(ModIndex::new(0), 0x2010, &[1], &[3])
            ],
            text
        ),
        Status::REFUSED
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x2010, &[1], &[2]), text),
        Status::OK
    );
    assert_eq!(
        register(patch(ModIndex::new(1), 0x2020, &[1], &[2]), text),
        Status::OK
    );
}

#[test]
fn the_stop_block_becomes_inc_r13_and_a_jump_to_the_next_recipe() {
    let next = 0x1_4096_3cdb;
    for stop in [0x1_4096_3ddb, 0x1_4096_3c00] {
        let made = stop_replacement(stop, next, 27).unwrap();
        assert_eq!(made.len(), 27);
        assert_eq!(&made[..4], &[0x49, 0xff, 0xc5, 0xe9]);
        let rel = i32::from_le_bytes(made[4..8].try_into().unwrap());
        let end = i64::try_from(stop + 8).unwrap();
        assert_eq!(end + i64::from(rel), i64::try_from(next).unwrap());
        assert!(made[8..].iter().all(|&b| b == 0xcc));
    }
}

#[test]
fn a_stop_block_too_short_or_too_far_from_the_next_recipe_is_refused() {
    assert_eq!(stop_replacement(0x1000, 0x2000, 7), None);
    assert_eq!(stop_replacement(0x1_0000_1000, 0x1000, 27), None);
}

#[test]
fn the_text_section_is_found_in_the_headers() {
    let mut headers = vec![0_u8; 0x400];
    headers[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    headers[0x80..0x84].copy_from_slice(b"PE\0\0");
    headers[0x86..0x88].copy_from_slice(&2_u16.to_le_bytes());
    headers[0x94..0x96].copy_from_slice(&0xf0_u16.to_le_bytes());
    let first = 0x80 + 24 + 0xf0;
    headers[first..first + 8].copy_from_slice(b".rdata\0\0");
    let second = first + 40;
    headers[second..second + 8].copy_from_slice(b".text\0\0\0");
    headers[second + 8..second + 12].copy_from_slice(&0x5000_u32.to_le_bytes());
    headers[second + 12..second + 16].copy_from_slice(&0x1000_u32.to_le_bytes());
    assert_eq!(text_section(&headers), Some((0x1000, 0x5000)));
    headers[second] = b'x';
    assert_eq!(text_section(&headers), None);
}

#[test]
fn only_workshops_past_the_limit_get_full_lists() {
    let layout = Layout {
        recipes: 8,
        recipe: 0x10,
        workshop: 4,
    };
    let mut registry = vec![0_u8; 0x10 + 5 * 0x10];
    registry[8..12].copy_from_slice(&8_i32.to_le_bytes());
    registry[12..16].copy_from_slice(&5_u32.to_le_bytes());
    for (i, workshop) in [7_u32, 9, 7, 7, 9].into_iter().enumerate() {
        let at = 0x10 + i * 0x10 + 4;
        registry[at..at + 4].copy_from_slice(&workshop.to_le_bytes());
    }
    let lists = full_lists(&registry, layout, 2).unwrap();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[&7], [0x10, 0x30, 0x40]);
    assert!(full_lists(&registry[..0x30], layout, 2).is_none());
}
