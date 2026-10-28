use super::*;

#[test]
fn texts_hash_as_the_game_does() {
    assert_eq!(hash(b""), FNV_BASIS);
    assert_eq!(hash(b"doBasicProgressBar()"), 0xb37b_692b_58e0_8ab5);
    assert_eq!(hash(b"contentLayer"), 0xffd8_6fcd_3b62_1d0f);
    // Non-ASCII bytes are sign-extended before the xor.
    let plain = "ж"
        .bytes()
        .fold(FNV_BASIS, |h, b| (h ^ u64::from(b)).wrapping_mul(FNV_PRIME));
    assert_ne!(hash("ж".as_bytes()), plain);
}

/// The layouts of every client table fit the loader's buffers, and a
/// build whose label grew past them keeps the interface off.
#[test]
fn the_tables_layouts_fit_the_loader_s_buffers() {
    let tables = enshwick_gamedata::embedded()
        .iter()
        .map(|(_, text)| enshwick_gamedata::Table::parse(text).unwrap());
    for table in tables.filter(|t| t.side() == enshwick_gamedata::Side::Client) {
        let lookup = |id: &str| {
            table
                .constants()
                .iter()
                .find(|c| c.id() == id)
                .map(enshwick_gamedata::Constant::value)
        };
        let layout = Layout::read(&lookup).unwrap();
        assert_eq!(layout.nav, [0, 0x20, 0x24, 0x28, 0x30, 0x34, 0x48]);
        DialogLayout::read(&lookup).unwrap();
        ViewLayout::read(&lookup).unwrap();
        let grown = |id: &str| {
            if id == pinned::ui::LABEL_EXTENT.id() {
                Some(0x200)
            } else {
                lookup(id)
            }
        };
        assert!(matches!(
            Layout::read(&grown),
            Err(Missing::Room(id)) if id == pinned::ui::LABEL_EXTENT.id()
        ));
    }
}

/// A client table pins the scroll area's and the text field's functions
/// whole, each with its expected bytes, or none of them, and the newest
/// pins both.
#[test]
fn client_tables_pin_the_scroll_area_and_the_text_field_whole() {
    let scroll = [
        pinned::ui::PERSIST,
        pinned::ui::FIRST_USE,
        pinned::ui::SCROLL_CONFIG,
        pinned::ui::SCROLLBAR_LOOK,
        pinned::ui::SCROLL_BEGIN,
        pinned::ui::SCROLL_FOCUS,
        pinned::ui::SCROLL_SET_FOCUS,
        pinned::ui::SCROLL_NAV,
        pinned::ui::SCROLL_INPUT,
        pinned::ui::SCROLL_END,
        pinned::ui::COLUMN,
        pinned::ui::NO_CONSTRUCT,
    ];
    let text_field = [
        pinned::ui::TEXT_FIELD,
        pinned::ui::TEXT_FIELD_STATE,
        pinned::ui::TEXT_FIELD_INPUT,
        pinned::ui::FOCUS_REQUEST,
        pinned::ui::FRAME_HIT,
        pinned::ui::NODE_MIN_HEIGHT,
    ];
    let tables: Vec<_> = enshwick_gamedata::embedded()
        .iter()
        .map(|(_, text)| enshwick_gamedata::Table::parse(text).unwrap())
        .filter(|t| t.side() == enshwick_gamedata::Side::Client)
        .collect();
    let pins = |table: &enshwick_gamedata::Table, group: &[Symbol]| {
        let pinned = group.iter().filter(|name| {
            let bytes = format!("{name}.bytes");
            table.symbols().iter().any(|s| s.id() == name.id())
                && table.constants().iter().any(|c| c.id() == bytes)
        });
        pinned.count()
    };
    for table in &tables {
        for group in [&scroll[..], &text_field[..]] {
            let count = pins(table, group);
            assert!(count == 0 || count == group.len(), "{}", table.name());
        }
    }
    let newest = tables.iter().max_by_key(|t| t.game_version()).unwrap();
    assert_eq!(pins(newest, &scroll), scroll.len());
    assert_eq!(pins(newest, &text_field), text_field.len());
}

#[test]
fn whole_stops_show_whole_numbers() {
    assert_eq!(slider_format(1.0, 20.0, 20), 2);
    assert_eq!(slider_format(0.0, 1.0, 11), 4);
    assert_eq!(slider_format(0.0, 10.0, 0), 4);
}

#[test]
fn buffers_write_little_endian_in_bounds() {
    let mut buf = Buf::<16>::zeroed();
    buf.u32(0, 0x0102_0304);
    buf.u64(8, 7);
    buf.u64(12, 9);
    assert_eq!(buf.read_u64(0), 0x0102_0304);
    assert_eq!(buf.read_usize(8), 7);
    assert_eq!(buf.read_u64(12), 0);
}
