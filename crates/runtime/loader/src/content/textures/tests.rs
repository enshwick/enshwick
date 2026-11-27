use super::*;
use enshwick_base::Relock;

/// The reason the status summary lists `feature` off for.
fn off_reason(feature: &str) -> Option<String> {
    crate::engine::diag::parts_off()
        .into_iter()
        .find(|p| p.part == feature)
        .map(|p| p.reason)
}

#[test]
fn a_ring_record_fits_only_inside_the_loaders_room_with_its_context_after_the_code() {
    assert!(record_fits(RECORD_ROOM, RECORD_ROOM - 16));
    assert!(record_fits(0x20, 1));
    assert!(!record_fits(RECORD_ROOM + 1, 1), "past the loader's room");
    assert!(!record_fits(0x20, 0), "the code byte comes first");
    assert!(!record_fits(0x20, 0x11), "the context runs past the record");
    assert!(!record_fits(0x20, usize::MAX), "an offset that overflows");
}

/// One test for both fuses, which a failed link turns off for the
/// process: the templates first, while the textures are still on.
#[test]
fn textures_and_templates_whose_code_does_not_hold_are_off_by_their_own_fuses() {
    let loader = crate::start::tests::bare_loader();
    assert!(!link_keys(&loader));
    assert!(TEMPLATES_FUSE.is_off());
    assert!(
        !TEXTURES_FUSE.is_off(),
        "the textures keep their own switch"
    );
    assert!(
        off_reason("mod templates").is_some_and(|r| r.starts_with(pinned::textures::KEYS.id())),
        "{:?}",
        off_reason("mod templates")
    );
    assert!(!link_textures(&loader));
    assert!(TEXTURES_FUSE.is_off());
    assert!(
        off_reason("mod textures").is_some_and(|r| r.starts_with(CHECKED[0].id())),
        "{:?}",
        off_reason("mod textures")
    );
    let mut chains = loader.chains.relock();
    chains.seal();
    assert!(chains.is_empty());
}

#[test]
fn the_key_listing_lists_templates_whether_or_not_the_textures_are_on() {
    assert!(lists(TEMPLATE, false), "templates list without the images");
    assert!(lists(TEMPLATE, true));
    assert!(!lists(UI_IMAGE, false), "images list only while on");
    assert!(lists(UI_IMAGE, true));
}

#[test]
fn every_client_table_offers_a_check_for_the_code_the_hooks_rely_on() {
    let clients = crate::engine::builds::load()
        .into_iter()
        .filter(|table| table.side() == enshwick_gamedata::Side::Client);
    for table in clients {
        let resolved = table.resolve();
        for symbol in CHECKED {
            let bytes = format!("{symbol}.bytes");
            assert!(
                resolved.head(symbol.id()).is_some() || resolved.constant(&bytes).is_some(),
                "{}: {symbol}",
                table.name()
            );
        }
    }
}

#[test]
fn served_keys_follow_the_game_s_listing_once_each_as_it_lays_keys_out() {
    let key = |guid: u8| enshwick_core::abi::ResourceKey {
        guid: Guid([guid; 16]),
        type_hash: TEMPLATE,
        part: 2,
    };
    let mut listed = vec![[7u8; KEY]];
    with_served(&mut listed, &[key(1), key(1), key(3)]);
    assert_eq!(listed.len(), 3, "the game's key, then each served one once");
    assert_eq!(listed[0], [7u8; KEY]);
    assert_eq!(listed[1][..16], [1; 16]);
    assert_eq!(listed[1][16..20], TEMPLATE.to_le_bytes());
    assert_eq!(listed[1][20..24], 2u32.to_le_bytes());
    assert_eq!(listed[2][..16], [3; 16]);
}

#[test]
fn templates_are_listed_even_when_the_ui_image_hooks_are_not_armed() {
    assert!(lists(TEMPLATE, false));
    assert!(lists(TEMPLATE, true));
    assert!(!lists(UI_IMAGE, false), "an image the request cannot load");
    assert!(lists(UI_IMAGE, true));
    assert!(!lists(0x1234, true));
}

#[test]
fn a_removed_mod_s_clone_is_listed_as_its_vanilla_template_s_entry() {
    let mut vanilla = [0u8; KEY];
    vanilla[..16].copy_from_slice(&[4; 16]);
    vanilla[16..20].copy_from_slice(&TEMPLATE.to_le_bytes());
    vanilla[20..24].copy_from_slice(&3u32.to_le_bytes());
    let mut listed = vec![vanilla];
    with_aliases(
        &mut listed,
        &[
            (Guid([8; 16]), Guid([4; 16])),
            (Guid([8; 16]), Guid([4; 16])),
            (Guid([9; 16]), Guid([5; 16])),
        ],
    );
    assert_eq!(
        listed.len(),
        2,
        "once, and never for an unlisted vanilla template"
    );
    assert_eq!(listed[1][..16], [8; 16]);
    assert_eq!(
        listed[1][16..],
        vanilla[16..],
        "the vanilla entry's type and part"
    );
}

#[test]
fn a_served_hash_carries_its_size_and_the_mark() {
    let hash = hash_of(b"pixels");
    assert_eq!((hash[0], hash[3]), (6, MARK));
    assert_eq!(hash, hash_of(b"pixels"));
    assert_ne!(hash, hash_of(b"pixelz"));
}

#[test]
fn a_served_hash_keeps_its_value_for_the_same_bytes() {
    assert_eq!(hash_of(b"pixels"), [6, 0x2272_692c, 0xa60d_bbbe, MARK]);
}

#[test]
fn requests_are_read_only_when_inside_one_readable_region() {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_PRIVATE, PAGE_NOACCESS, PAGE_READWRITE,
    };
    let region = |protect| crate::engine::pages::Region {
        base: 0x1000,
        end: 0x3000,
        allocation: 0x1000,
        state: MEM_COMMIT,
        kind: MEM_PRIVATE,
        protect,
    };
    let readable = |_| Some(region(PAGE_READWRITE));
    assert!(requests_read(0x1000, 2, 0x1000, readable));
    assert!(
        !requests_read(0x1000, 3, 0x1000, readable),
        "past the region"
    );
    assert!(!requests_read(0x1000, 1, 0x40, |_| Some(region(
        PAGE_NOACCESS
    ))));
    assert!(!requests_read(0x1000, 1, 0x40, |_| None));
    assert!(
        requests_read(0x1000, 5000, 1, readable),
        "any count the region holds"
    );
    assert!(!requests_read(0x1000, 0, 0x40, readable), "none to read");
    assert!(!requests_read(0, 1, 0x40, readable), "no array");
    assert!(
        !requests_read(usize::MAX - 8, 2, 0x40, readable),
        "an end that overflows"
    );
}

#[test]
fn served_content_stops_at_its_limit_and_the_same_bytes_serve_again() {
    let store = Box::new(store::Store::new());
    let hash = |n: u32| [4, n, n.wrapping_mul(7), MARK];
    assert!(store.is_empty());
    assert_eq!(store.insert(hash(1), b"abcd"), Ok(()));
    assert_eq!(store.get(&hash(1)), Some(&b"abcd"[..]));
    assert_eq!(store.get(&hash(2)), None);
    assert_eq!(store.insert(hash(1), b"wxyz"), Ok(()), "served once");
    assert_eq!(store.get(&hash(1)), Some(&b"abcd"[..]));
    let big = vec![0u8; store::MAX_BYTES];
    assert_eq!(
        store.insert(hash(2), &big),
        Err(store::Full),
        "past the bytes"
    );
    for n in 2..=u32::try_from(store::MAX_ENTRIES).unwrap() {
        assert_eq!(store.insert(hash(n), &[1]), Ok(()), "entry {n}");
    }
    assert_eq!(
        store.insert(hash(0), &[1]),
        Err(store::Full),
        "past the entries"
    );
    for n in 1..=u32::try_from(store::MAX_ENTRIES).unwrap() {
        assert!(store.get(&hash(n)).is_some(), "entry {n} still found");
    }
}

#[test]
fn the_images_showing_is_announced_once_and_only_after_every_link_ran() {
    let textures = Textures::new();
    textures.relisted.store(true, Ordering::Release);
    textures.listed.store(true, Ordering::Release);
    assert!(!textures.announces(), "the content request has not run");
    textures.linked.store(true, Ordering::Release);
    assert!(textures.announces());
    assert!(!textures.announces(), "a later fill announces nothing");
}
