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

/// The client tables, parsed.
fn client_tables() -> Vec<enshwick_gamedata::Table> {
    enshwick_gamedata::embedded()
        .iter()
        .map(|(_, text)| enshwick_gamedata::Table::parse(text).unwrap())
        .filter(|t| t.side() == enshwick_gamedata::Side::Client)
        .collect()
}

/// A table's constant by id.
fn constant(table: &enshwick_gamedata::Table, id: &str) -> Option<u64> {
    table
        .constants()
        .iter()
        .find(|c| c.id() == id)
        .map(enshwick_gamedata::Constant::value)
}

/// Which optional parts of the interface read on a build whose constants
/// `lookup` gives: the dialogs, the scroll area's highlight and extents,
/// and the text field.
fn optional_parts(lookup: Lookup<'_>) -> [bool; 4] {
    [
        DialogLayout::read(lookup).is_ok(),
        scroll::nav_highlight(lookup).is_ok(),
        scroll::extents(lookup).is_ok(),
        text_field::TextLayout::read(lookup).is_ok(),
    ]
}

/// Each optional part whose layout does not read turns off alone: the
/// interface's own layout and every other optional part still read.
#[test]
fn each_optional_part_turns_off_alone() {
    let parts = [
        pinned::ui::POPUP_TITLE.id(),
        pinned::ui::NAV_HIGHLIGHT.id(),
        pinned::ui::SCROLL_EXTENT.id(),
        pinned::ui::TEXT_FIELD_EXTENT.id(),
    ];
    let mut checked = 0;
    for table in client_tables() {
        let lookup = |id: &str| constant(&table, id);
        if optional_parts(&lookup) != [true; 4] {
            continue;
        }
        for (off, part) in parts.into_iter().enumerate() {
            let without = |id: &str| if id == part { None } else { lookup(id) };
            Layout::read(&without).unwrap();
            let read = optional_parts(&without);
            for (index, on) in read.into_iter().enumerate() {
                assert_eq!(on, index != off, "{} without {part}", table.name());
            }
        }
        checked += 1;
    }
    assert!(checked > 0, "no client table pins every optional part");
}

/// Every client table that pins the text field gives its sizes, they fit
/// the loader's buffer, and a build whose field or kept state moved turns
/// the text field off instead of letting the game write past the buffer.
#[test]
fn the_text_field_s_sizes_come_from_the_table_and_fit_its_buffer() {
    let mut checked = 0;
    for table in client_tables() {
        if !table
            .symbols()
            .iter()
            .any(|s| s.id() == pinned::ui::TEXT_FIELD.id())
        {
            continue;
        }
        let lookup = |id: &str| constant(&table, id);
        text_field::TextLayout::read(&lookup)
            .unwrap_or_else(|why| panic!("{}: {why}", table.name()));
        let changed = |changed: &'static str, value: Option<u64>| {
            move |id: &str| if id == changed { value } else { lookup(id) }
        };
        let extent = pinned::ui::TEXT_FIELD_EXTENT.id();
        let text = pinned::ui::TEXT_FIELD_TEXT.id();
        let state = pinned::ui::TEXT_FIELD_STATE_SIZE.id();
        let editing = pinned::ui::TEXT_FIELD_EDITING.id();
        for (id, value, room, blamed) in [
            (extent, Some(0x1000), true, extent),
            (text, Some(0x1000), true, text),
            (state, Some(0x10), true, editing),
            (extent, None, false, extent),
        ] {
            let read = text_field::TextLayout::read(&changed(id, value));
            let got = match read {
                Err(Missing::Room(id)) => Some((true, id)),
                Err(Missing::Entry(id)) => Some((false, id)),
                _ => None,
            };
            assert_eq!(
                got,
                Some((room, blamed)),
                "{} with {id} at {value:?}",
                table.name()
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no client table pins the text field");
}

/// The scroll area's highlight sound sits inside the navigation config on
/// every client table that has it, and one past it keeps the scroll area
/// off.
#[test]
fn the_scroll_areas_highlight_fits_the_navigation_config() {
    let highlight = pinned::ui::NAV_HIGHLIGHT.id();
    for table in client_tables() {
        let lookup = |id: &str| constant(&table, id);
        if lookup(highlight).is_none() {
            assert!(matches!(
                scroll::nav_highlight(&lookup),
                Err(Missing::Entry(id)) if id == highlight
            ));
            continue;
        }
        let at = scroll::nav_highlight(&lookup).unwrap();
        assert!(at + 16 <= NAV_CONFIG, "{}", table.name());
        let config = lookup(pinned::ui::NAV_CONFIG.id()).unwrap();
        for value in [config + NAV_CONFIG as u64, config.wrapping_sub(1), u64::MAX] {
            let moved = |id: &str| {
                if id == highlight {
                    Some(value)
                } else {
                    lookup(id)
                }
            };
            assert!(
                matches!(scroll::nav_highlight(&moved), Err(Missing::Room(id)) if id == highlight),
                "{} at {value:#x}",
                table.name()
            );
        }
    }
    let far = |id: &str| (id == highlight).then_some(u64::MAX).or(Some(0));
    assert!(matches!(
        scroll::nav_highlight(&far),
        Err(Missing::Room(id)) if id == highlight
    ));
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
    let highlight = newest
        .constants()
        .iter()
        .find(|c| c.id() == pinned::ui::NAV_HIGHLIGHT.id())
        .map(enshwick_gamedata::Constant::value);
    assert_eq!(highlight, Some(0x60), "the config's 0x28 plus 0x38");
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

/// Every client table that pins the scroll area gives its extents, they fit
/// the loader's buffers, and a grown or missing one turns the scroll area
/// off.
#[test]
fn the_scroll_area_s_extents_come_from_the_table_and_fit_its_buffers() {
    let mut checked = 0;
    for table in client_tables() {
        if !table
            .symbols()
            .iter()
            .any(|s| s.id() == pinned::ui::SCROLL_BEGIN.id())
        {
            continue;
        }
        let lookup = |id: &str| constant(&table, id);
        let state =
            scroll::extents(&lookup).unwrap_or_else(|why| panic!("{}: {why}", table.name()));
        assert_eq!(
            Some(state as u64),
            lookup(pinned::ui::SCROLL_STATE_SIZE.id()),
            "{}: the kept state the loader asks for is the build's",
            table.name()
        );
        for (id, room) in [
            (pinned::ui::SCROLL_EXTENT.id(), scroll::SCROLL_OBJECT),
            (pinned::ui::SCROLL_CONFIG_EXTENT.id(), scroll::SCROLL_CONFIG),
            (pinned::ui::SCROLL_STATE_SIZE.id(), scroll::STATE_SIZE),
        ] {
            let grown = |asked: &str| {
                if asked == id {
                    Some(room as u64 + 1)
                } else {
                    lookup(asked)
                }
            };
            assert!(
                matches!(scroll::extents(&grown), Err(Missing::Room(at)) if at == id),
                "{} with {id} grown",
                table.name()
            );
            let missing = |asked: &str| if asked == id { None } else { lookup(asked) };
            assert!(
                matches!(scroll::extents(&missing), Err(Missing::Entry(at)) if at == id),
                "{} without {id}",
                table.name()
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no client table pins the scroll area");
    let reflected = Missing::Reflected("keen::GameUiScrollbarParameterData", "sliderTexture");
    assert_eq!(
        reflected.to_string(),
        "sliderTexture of keen::GameUiScrollbarParameterData is not reflected on this build"
    );
}

/// The scroll config and its look as client 426 reflects them.
const FIELDS_426: scroll::Fields = scroll::Fields {
    look_size: 0xc0,
    tint: 0x04,
    range: 0x24,
    slider: 0x44,
    shown: 0x7c,
    widths: [0x80, 0x84, 0x88, 0x8c],
    filled: 0x90,
    config_size: 0x104,
    flags: 0x40,
    scissor: 0x2e,
    center: 0x2f,
    bar: 0x32,
    look: 0x44,
};

/// The reflected scroll config and look fit the loader's buffers, and a
/// build whose look grew, whose field left its type or whose look runs past
/// the config turns the scroll area off.
#[test]
fn the_scroll_area_s_reflected_fields_fit_its_buffers() {
    assert_eq!(FIELDS_426.checked().ok(), Some(FIELDS_426));
    let refused = |fields: scroll::Fields| match fields.checked() {
        Err(Missing::Room(name)) => name,
        other => panic!("{other:?}"),
    };
    let grown = scroll::Fields {
        look_size: scroll::SCROLLBAR_LOOK + 8,
        ..FIELDS_426
    };
    assert_eq!(refused(grown), "keen::GameUiScrollbarParameterData");
    let config = scroll::Fields {
        config_size: scroll::SCROLL_CONFIG + 4,
        ..FIELDS_426
    };
    assert_eq!(refused(config), "keen::GameUiScrollingLayoutParameterData");
    let moved = scroll::Fields {
        widths: [0xbe, 0x84, 0x88, 0x8c],
        ..FIELDS_426
    };
    assert_eq!(refused(moved), "sliderIdleWidth");
    let late = scroll::Fields {
        look: 0x50,
        ..FIELDS_426
    };
    assert_eq!(refused(late), "scrollBar");
    let far = scroll::Fields {
        tint: usize::MAX,
        ..FIELDS_426
    };
    assert_eq!(refused(far), "rangeRectColor");
}
