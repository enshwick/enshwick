use std::sync::Mutex;

use enshwick::abi::AltarLimit;

use super::detours::chain_limit;
use super::*;
use crate::locks::Relock;

/// The level each run of `plus_vanilla` saw, in order.
static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

unsafe extern "system" fn plus_vanilla(limit: *mut AltarLimit, _user: *mut c_void) {
    // SAFETY: the loader's limit, writable for the call.
    let limit = unsafe { &mut *limit };
    SEEN.relock().push(limit.flame_level);
    limit.allowed = limit.allowed.saturating_add(limit.vanilla);
    limit.vanilla = 0;
    limit.flame_level = 99;
}

#[test]
fn every_site_is_the_table_s_symbol_and_the_first_one_missing_is_named() {
    let address =
        |s: pinned::Symbol| Some(0x1000 + SITE_SYMBOLS.iter().position(|&t| t == s)? as u64);
    let sites = sites_of(fake::FIELDS, address).unwrap();
    assert_eq!(sites.targets().len(), SITE_SYMBOLS.len());
    assert_eq!(sites.cursor, address(pinned::bases::CURSOR).unwrap());
    let without_zones =
        |s: pinned::Symbol| (s != pinned::bases::ZONES_AT).then(|| address(s)).flatten();
    assert_eq!(
        sites_of(fake::FIELDS, without_zones).map(|_| ()),
        Err(pinned::bases::ZONES_AT)
    );
}

/// The newest table of each side reads as the fields and the loops'
/// layout the tests' fake object is laid out by; the client's placement
/// cursor holds more before its balancing.
#[test]
fn the_newest_tables_read_as_the_fake_object_s_layout() {
    use enshwick_gamedata::{Side as TableSide, newest};

    let tables = crate::builds::load();
    for (side, balancing) in [(TableSide::Server, 0x170), (TableSide::Client, 0x180)] {
        let resolved = newest(&tables, side).unwrap().resolve();
        let offset = |id: Constant| {
            resolved
                .constant(id.id())
                .and_then(|value| usize::try_from(value).ok())
        };
        let fields = Fields {
            balancing,
            ..fake::FIELDS
        };
        assert_eq!(Fields::read(&offset), Ok(fields), "{side:?}");
        assert_eq!(loops::Layout::read(&offset), Ok(fake::LAYOUT), "{side:?}");
    }
}

/// A table whose stock slots are too narrow to put index 93 behind the
/// server's fields keeps 16 bases, naming the stock slots.
#[test]
fn a_table_that_puts_the_window_on_a_vanilla_field_keeps_16_bases() {
    use enshwick_gamedata::{Side as TableSide, newest};

    let tables = crate::builds::load();
    let resolved = newest(&tables, TableSide::Server).unwrap().resolve();
    let offset = |id: Constant| {
        if id == pinned::bases::SLOT {
            return Some(0x10);
        }
        resolved
            .constant(id.id())
            .and_then(|value| usize::try_from(value).ok())
    };
    let why = loops::Layout::read(&offset).unwrap_err();
    assert_eq!(
        why,
        past16::SiteError::Window(window::GeometryError::Overlap)
    );
    assert_eq!(why.entry(), pinned::bases::SLOTS.id());
}

/// Fields whose answer cannot hold a zone box are refused, with the
/// fields named.
#[test]
fn an_answer_too_small_for_a_zone_box_is_refused() {
    let offset = |id: Constant| {
        if id == pinned::bases::ANSWER_SIZE {
            Some(0x2c)
        } else {
            fake_offset(id)
        }
    };
    let refused = Fields::read(&offset).unwrap_err();
    assert_eq!(refused, FieldsError::Fit(pinned::bases::ANSWER_BOX.id()));
    assert_eq!(refused.entry(), pinned::bases::ANSWER_BOX.id());
    assert_eq!(Fields::read(&fake_offset), Ok(fake::FIELDS));
}

/// A field the table lacks is refused with that entry named, for the
/// feature to turn off on it.
#[test]
fn a_missing_field_is_refused_with_its_entry() {
    let offset = |id: Constant| {
        (id != pinned::bases::ZONE_HIGH)
            .then(|| fake_offset(id))
            .flatten()
    };
    let refused = Fields::read(&offset).unwrap_err();
    assert_eq!(refused.entry(), pinned::bases::ZONE_HIGH.id());
    assert_eq!(refused.to_string(), "is not in this build");
}

/// A zone list whose allocator lies before it, or offsets so large they
/// overflow, are refused on the entry that breaks the fit.
#[test]
fn fields_that_overflow_or_run_backwards_are_refused() {
    let backwards = |id: Constant| {
        if id == pinned::bases::ZONE_LIST_ALLOCATOR {
            Some(0)
        } else {
            fake_offset(id)
        }
    };
    assert_eq!(
        Fields::read(&backwards).unwrap_err().entry(),
        pinned::bases::ZONE_LIST_ALLOCATOR.id()
    );
    let huge = |id: Constant| {
        if id == pinned::bases::ZONE_HIGH {
            Some(usize::MAX)
        } else {
            fake_offset(id)
        }
    };
    assert_eq!(
        Fields::read(&huge).unwrap_err().entry(),
        pinned::bases::ZONE_HIGH.id()
    );
}

/// The fake object's fields as the build table would give them.
fn fake_offset(id: Constant) -> Option<usize> {
    let f = fake::FIELDS;
    let pairs = [
        (pinned::bases::COUNT, f.count),
        (pinned::bases::STRIDE, f.stride),
        (pinned::bases::MEMBERS, f.members),
        (pinned::bases::ZONE_LIST, f.zone_list),
        (pinned::bases::ZONE_LIST_LEN, f.zone_list_len),
        (pinned::bases::ZONE_SIZE, f.zone_size),
        (pinned::bases::ZONE_HIGH, f.zone_high),
        (pinned::bases::ITEM_ALTAR, f.item_altar),
        (
            pinned::bases::MAX_ALTARS_REACHED,
            f.max_altars_reached as usize,
        ),
        (pinned::bases::CURSOR_BALANCING, f.balancing),
        (pinned::bases::KIND, f.kind),
        (pinned::bases::ALLOCATOR, f.allocator),
        (pinned::bases::ANSWER_ERROR, f.answer.error),
        (pinned::bases::ANSWER_ZERO, f.answer.zero),
        (pinned::bases::ANSWER_INDEX, f.answer.index),
        (pinned::bases::ANSWER_BOX, f.answer.boxed),
        (pinned::bases::ANSWER_SIZE, f.answer.size),
        (pinned::bases::CONTAINER_DATA, f.container.data),
        (pinned::bases::CONTAINER_LEN, f.container.len),
        (pinned::bases::CONTAINER_CAP, f.container.cap),
        (pinned::bases::CONTAINER_GROW, f.container.grow),
        (pinned::bases::CONTAINER_BOX, f.container.boxed),
        (pinned::bases::HEAP_ALLOCATE, f.heap.allocate),
        (pinned::bases::HEAP_FREE, f.heap.free),
        (
            pinned::bases::ZONE_LIST_ALLOCATOR,
            f.zone_list + f.container.allocator,
        ),
    ];
    pairs
        .into_iter()
        .find(|&(c, _)| c == id)
        .map(|(_, value)| value)
}

/// The cursor's code check covers the offsets the detour reads: on each
/// game executable `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name, the
/// cursor's body in its table holds, and its hash covers the altar flag
/// test and the balancing load at the table's values, outside every
/// open field; skipped without them.
#[test]
fn the_cursor_s_body_fixes_the_offsets_its_detour_reads() {
    use enshwick_gamedata::reflect::{Image, Layout};
    use enshwick_gamedata::{Side, exact};

    let tables = crate::builds::load();
    for (variable, side) in [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ] {
        let Some(path) = std::env::var_os(variable) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let resolved = exact(&tables, side, image.fingerprint().unwrap())
            .unwrap()
            .resolve();
        let id = pinned::bases::CURSOR.id();
        let body = resolved.body(id).unwrap();
        let start = image.base() + resolved.symbol(id).unwrap();
        let code = image.read(start, body.len() as u64).unwrap();
        assert!(body.holds(code), "{variable}");
        let constant = |id: Constant| resolved.constant(id.id()).unwrap();
        let flag = u8::try_from(constant(pinned::bases::ITEM_ALTAR)).unwrap();
        let balancing = u32::try_from(constant(pinned::bases::CURSOR_BALANCING)).unwrap();
        let mut load = vec![0x48, 0x8B, 0x8E];
        load.extend_from_slice(&balancing.to_le_bytes());
        let fixed = |want: &[u8]| {
            code.windows(want.len()).enumerate().any(|(at, bytes)| {
                bytes == want
                    && body.open().iter().all(|&open| {
                        let open = open as usize;
                        open + 4 <= at || at + want.len() <= open
                    })
            })
        };
        assert!(
            fixed(&[0x80, 0x7F, flag, 0x00]),
            "{variable}: the flag test"
        );
        assert!(fixed(&load), "{variable}: the balancing load");
    }
}

#[test]
fn limit_filters_chain_on_the_count_so_far_within_the_zones() {
    let filter = LimitFilter::new(ModIndex::new(0), plus_vanilla, core::ptr::null_mut());
    assert_eq!(chain_limit(3, 6, 16, core::iter::empty()), 6);
    assert_eq!(chain_limit(3, 6, 32, [&filter].into_iter()), 12);
    SEEN.relock().clear();
    assert_eq!(
        chain_limit(3, 6, 32, [&filter, &filter].into_iter()),
        18,
        "each filter sees the game's own count"
    );
    assert_eq!(*SEEN.relock(), [3, 3], "each filter sees the level");
    assert_eq!(chain_limit(3, 6, 16, [&filter, &filter].into_iter()), 16);
    assert_eq!(
        chain_limit(11, 16, 255, [&filter, &filter, &filter].into_iter()),
        64
    );
}
