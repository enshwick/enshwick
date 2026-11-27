use super::*;

#[test]
fn groups_differ_by_pack_and_are_never_a_mod_s() {
    assert_ne!(group("a"), group("b"));
    assert_ne!(group("a"), 0);
}

#[test]
fn a_pack_s_group_keeps_its_value() {
    assert_eq!(group("lanterns"), 0x912e_fac2_9df0_a0f5);
}

#[test]
fn the_files_hash_keeps_its_value_for_the_same_files() {
    let files = PackFiles::new()
        .with("texts.toml", "[en]\n")
        .with("icons/lantern.png", "png");
    assert_eq!(files.hash(), 0x741d_d7f1);
}
