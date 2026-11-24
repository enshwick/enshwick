//! Rows of the stat kinds with a group on the game's Character screen.
//!
//! The screen's attribute submenu fills one row list per group, each with a
//! builder of its own, and draws every row of a list under the group's
//! header with the game's row widget (client 26430625, decompiler: the
//! submenu `doAttributesSubmenu`, nine builders, the row push they share).
//! A detour on a group's builder lets it fill its list, then pushes a row
//! per kind of that group, built from zeroed bytes as the submenu builds its
//! own empty row: the kind's name, description and the local player's value
//! as the server last sent it, the byte every game row holds 1 in, and no
//! grid cell, so the row follows the group's game rows; its breakdown stays
//! off. The game formats the value as it formats its own rows, an integer
//! with a sign or a percent sign.
//!
//! A detour links only for a group some kind shows in, under a fuse of its
//! own, so a builder this build lacks or changed turns off only its group.
//! Per hit it pays, for each kind of its group, a load of the player's value
//! and a lookup of texts kept with their hashes.

use core::cell::RefCell;
use std::sync::OnceLock;

use enshwick_core::abi::HookFlags;
use enshwick_core::content::items::StatFormat;
use enshwick_core::stats::StatGroup;
use rustc_hash::FxHashMap;

use super::engine::UiText;
use crate::engine::chains::Next;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// How many groups the Character screen has.
const GROUPS: usize = StatGroup::ALL.len();

/// What every group's rows need: the push and the row layout.
static FUSE: Fuse = Fuse::new("stat rows");
static LINKED: OnceLock<Linked> = OnceLock::new();

type BuilderFn = unsafe extern "system" fn(list: *mut u8);
type PushFn = unsafe extern "system" fn(list: *mut u8, row: *const Row) -> *mut u8;

/// A row of the game's attribute lists (client 26430625, decompiler of the
/// builders, the submenu's empty row, the row widget and its value text):
/// two texts, the value, the flags that pick how it reads, the breakdown the
/// cursor shows and the row's grid cell. Its size and the offsets of the
/// fields the loader sets are the table's, checked at the link
/// ([`fields_hold`]).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Row {
    label: UiTextWords,
    /// The description the cursor shows.
    tooltip: UiTextWords,
    value: i32,
    /// The second value of a row that reads `first / second`, and its flag.
    second: i32,
    pair: u8,
    pair_rest: [u8; 3],
    /// An arrow beside the value: 0 for none.
    trend: u8,
    /// The breakdown's entries, which show only with `breakdown`.
    entries: [u8; 571],
    breakdown: u8,
    breakdown_rest: [u8; 7],
    /// A byte every row of the game holds 1 in: its builders' rows and the
    /// submenu's empty row; no code of the screen reads it.
    flag: u8,
    /// The value reads as seconds, or as milliseconds.
    seconds: u8,
    milliseconds: u8,
    cell_rest: [u8; 5],
    /// The grid cell the widget places the row in; [`NO_CELL`] places it
    /// after the rows before it.
    cell: [i64; 2],
    /// The value reads `%d%%`, and a positive one gets a `+`.
    percent: u8,
    plus: u8,
    rest: [u8; 6],
}

/// The grid cell of a row the widget lays out in order, as every game row
/// is: either word -1 (client 26430625, decompiler of the row widget).
const NO_CELL: [i64; 2] = [-1, -1];

/// The value of [`Row::flag`] in every game row.
const FLAG: u8 = 1;

/// The game's text as three words: its bytes, their length and their hash.
type UiTextWords = [u64; 3];

/// A row list's head, as far as the loader reads it: its rows' storage
/// (client 26430625, decompiler).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct List {
    rows: u64,
}

/// What the rows need of this build.
#[derive(Clone, Copy, Debug)]
struct Linked {
    push: PushFn,
    /// The kinds of each group and their stat keys, in the order of
    /// [`StatGroup::ALL`].
    groups: &'static [Box<[(u32, u32)]>; GROUPS],
}

/// A kind as its rows show it in one language: its texts escaped for the
/// game's markup, as the words a row holds, worked out once; the bytes they
/// point at live as long as the process, since a row the game keeps for the
/// frame points at them.
#[derive(Debug)]
struct Texts {
    label: UiTextWords,
    tooltip: UiTextWords,
    format: StatFormat,
    _bytes: [Box<[u8]>; 2],
}

impl Texts {
    /// The texts of `name` and `description`, escaped.
    fn new(name: &str, description: &str, format: StatFormat) -> Self {
        let escaped = |text: &str| {
            let mut out = Vec::with_capacity(text.len());
            super::arena::escape(&mut out, text.as_bytes());
            out.into_boxed_slice()
        };
        let (name, description) = (escaped(name), escaped(description));
        let words = |bytes: &[u8]| {
            let text = UiText::new(bytes);
            [text.ptr.expose_provenance() as u64, text.len, text.hash]
        };
        Self {
            label: words(&name),
            tooltip: words(&description),
            format,
            _bytes: [name, description],
        }
    }
}

thread_local! {
    /// The kinds' texts by kind and language, filled the first time a row
    /// shows; never emptied, so a row's text stays valid.
    static TEXTS: RefCell<FxHashMap<(u32, u32), Option<Texts>>> = RefCell::default();
}

macro_rules! row_detours {
    ($($name:ident, $next:ident, $fuse:ident, $group:ident;)*) => {
        $(
            static $fuse: Fuse = Fuse::new(concat!("stat rows of ", stringify!($group)));

            crate::engine::chains::chain_detour! {
                /// The loader's link on one of the Character screen's row
                /// builders, `ui.attribute_rows_*`: the game's rows, then a
                /// row per kind of its group.
                fn $name(list: *mut u8) -> ();
                next $next;
                fuse &$fuse, "the Character screen's stat rows";
                |onward| {
                    onward.call();
                    // SAFETY: the builder returned and filled its row list,
                    // the one the submenu passed it, live on the interface
                    // thread (client 26430625, decompiler).
                    unsafe { push_rows(list.expose_provenance(), StatGroup::$group) };
                }
            }
        )*

        /// The builders' detours, their slots and their fuses, in the order
        /// of [`StatGroup::ALL`].
        static DETOURS: [(BuilderFn, &Next<BuilderFn>, &Fuse); GROUPS] =
            [$(($name as BuilderFn, &$next, &$fuse)),*];
    };
}

row_detours! {
    main_rows, MAIN_NEXT, MAIN_FUSE, Main;
    fog_survival_rows, FOG_SURVIVAL_NEXT, FOG_SURVIVAL_FUSE, FogSurvival;
    primary_rows, PRIMARY_NEXT, PRIMARY_FUSE, Primary;
    regeneration_rows, REGENERATION_NEXT, REGENERATION_FUSE, Regeneration;
    misc_rows, MISC_NEXT, MISC_FUSE, Misc;
    damage_rows, DAMAGE_NEXT, DAMAGE_FUSE, Damage;
    weapon_damage_rows, WEAPON_DAMAGE_NEXT, WEAPON_DAMAGE_FUSE, WeaponDamage;
    enemy_damage_rows, ENEMY_DAMAGE_NEXT, ENEMY_DAMAGE_FUSE, EnemyDamage;
    protection_rows, PROTECTION_NEXT, PROTECTION_FUSE, Protection;
}

/// Links the builder of each group some kind shows in, on a client whose
/// build has the push and the row layout; a group whose builder the build
/// lacks shows no rows, with the reason, the others do.
pub(super) fn link(loader: &Loader) {
    let groups: [Box<[(u32, u32)]>; GROUPS] =
        core::array::from_fn(|at| crate::inventory::stats::group_kinds(StatGroup::ALL[at]).into());
    if groups.iter().all(|kinds| kinds.is_empty()) {
        return;
    }
    let push = match linked_push(loader) {
        Ok(push) => push,
        Err((id, reason)) => {
            loader.fuse_off(&FUSE, id, reason);
            return;
        }
    };
    let groups = Box::leak(Box::new(groups));
    let _first = LINKED.set(Linked { push, groups });
    let builders = pinned::ui::ATTRIBUTE_ROWS;
    let shared = shared(&builders.map(|builder| loader.address(builder)));
    let mut linked = 0;
    for (at, (kinds, &(detour, next, fuse))) in groups.iter().zip(&DETOURS).enumerate() {
        if kinds.is_empty() {
            continue;
        }
        if shared[at] {
            loader.fuse_off(fuse, builders[at].id(), SHARED);
        } else if loader.link_feature(
            fuse,
            Target::Pinned(builders[at]),
            next,
            detour,
            HookFlags::NONE,
        ) {
            linked += 1;
        }
    }
    crate::log!(
        INFO,
        "interface: stat rows in {linked} groups of the Character screen"
    );
}

/// Why a group whose builder is another group's too shows no rows.
const SHARED: &str = "is the builder of another group too, so its rows could go under either";

/// Which groups' builders lie at one address with another group's: a
/// function two builder routes find cannot tell whose rows it builds, so
/// neither group links, whatever kinds the groups show.
fn shared(addresses: &[Option<u64>; GROUPS]) -> [bool; GROUPS] {
    core::array::from_fn(|at| {
        addresses[at].is_some_and(|a| addresses.iter().filter(|&&b| b == Some(a)).count() > 1)
    })
}

/// What a route lookup failed on: the entry, and why.
type Failed = (&'static str, &'static str);

/// The build's constant `id`, or why not.
fn offset(
    lookup: &dyn Fn(pinned::Constant) -> Option<usize>,
    id: pinned::Constant,
) -> Result<usize, Failed> {
    lookup(id).ok_or((id.id(), crate::engine::builds::ABSENT))
}

/// The row push of this build, once its row size is the one the loader lays
/// rows out in and [`fields_hold`], or the first table entry it lacks, with
/// why.
fn linked_push(loader: &Loader) -> Result<PushFn, Failed> {
    use crate::engine::builds::ABSENT;
    let push = pinned::ui::ATTRIBUTE_ROW_PUSH;
    let address = loader
        .address(push)
        .filter(|_| loader.code_holds(push))
        .and_then(|a| usize::try_from(a).ok())
        .ok_or((push.id(), ABSENT))?;
    let size = pinned::ui::ATTRIBUTE_ROW_SIZE;
    match loader.offset(size) {
        Some(bytes) if bytes == size_of::<Row>() => {}
        Some(_) => return Err((size.id(), "is not the row size the loader lays rows out in")),
        None => return Err((size.id(), ABSENT)),
    }
    fields_hold(&|id| loader.offset(id))?;
    // SAFETY: `code_holds` checked the code at `ui.attribute_row_push`
    // against its route's check, so it is the game's push of an attribute
    // row, typed `PushFn` (client 26430625, decompiler).
    Ok(unsafe { core::mem::transmute::<usize, PushFn>(address) })
}

/// Whether every field of [`Row`] and [`List`] the loader reads or writes
/// lies where the game's code reads it on this build, else the first entry
/// that disagrees or is absent, with why. Most fields are one read through
/// a row pointer; the tooltip is a read of the submenu's copy of the hovered
/// row and the flag a store into its empty row there, and the pair flag a
/// read beside the second value (client 26430625, decompiler).
fn fields_hold(lookup: &dyn Fn(pinned::Constant) -> Option<usize>) -> Result<(), Failed> {
    use core::mem::offset_of;
    use pinned::ui as ids;
    let offset = |id| offset(lookup, id);
    let moved = "is not where the loader's row or list keeps the field";
    let read = [
        (
            ids::ATTRIBUTE_ROW_LABEL_LENGTH,
            offset_of!(Row, label) + size_of::<u64>(),
        ),
        (ids::ATTRIBUTE_ROW_VALUE, offset_of!(Row, value)),
        (ids::ATTRIBUTE_ROW_SECOND, offset_of!(Row, second)),
        (ids::ATTRIBUTE_ROW_TREND, offset_of!(Row, trend)),
        (ids::ATTRIBUTE_ROW_SECONDS, offset_of!(Row, seconds)),
        (
            ids::ATTRIBUTE_ROW_MILLISECONDS,
            offset_of!(Row, milliseconds),
        ),
        (ids::ATTRIBUTE_ROW_CELL, offset_of!(Row, cell)),
        (
            ids::ATTRIBUTE_ROW_CELL_SECOND,
            offset_of!(Row, cell) + size_of::<i64>(),
        ),
        (ids::ATTRIBUTE_ROW_PERCENT, offset_of!(Row, percent)),
        (ids::ATTRIBUTE_ROW_PLUS, offset_of!(Row, plus)),
        (ids::ATTRIBUTE_LIST_ROWS, offset_of!(List, rows)),
    ];
    for (id, field) in read {
        if offset(id)? != field {
            return Err((id.id(), moved));
        }
    }
    let hovered = offset(ids::ATTRIBUTE_ROW_HOVERED)?;
    let composed = [
        (
            ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP,
            offset(ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP)?.checked_sub(hovered),
            offset_of!(Row, tooltip),
        ),
        (
            ids::ATTRIBUTE_ROW_HOVERED_FLAG,
            offset(ids::ATTRIBUTE_ROW_HOVERED_FLAG)?.checked_sub(hovered),
            offset_of!(Row, flag),
        ),
        (
            ids::ATTRIBUTE_ROW_PAIR_AFTER_SECOND,
            offset(ids::ATTRIBUTE_ROW_SECOND)?
                .checked_add(offset(ids::ATTRIBUTE_ROW_PAIR_AFTER_SECOND)?),
            offset_of!(Row, pair),
        ),
    ];
    for (id, found, field) in composed {
        if found != Some(field) {
            return Err((id.id(), moved));
        }
    }
    Ok(())
}

/// How a value shows in a row: the integer, and whether it reads as a
/// percentage and gets a `+` when positive.
fn shown_value(value: f32, format: StatFormat) -> (i32, bool, bool) {
    let (number, percent, plus) = match format {
        StatFormat::Percent => (value * 100.0, true, false),
        StatFormat::Signed => (value, false, true),
        _ => (value, false, false),
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a saturating cast of the rounded value is the row's integer"
    )]
    let number = number.round() as i32;
    (number, percent, plus)
}

/// Pushes a row per kind of `group` onto the row `list` its builder filled.
///
/// # Safety
///
/// - `list` is a row list of the game's attribute submenu that its builder
///   just filled, live for the call on the interface thread.
unsafe fn push_rows(list: usize, group: StatGroup) {
    let Some(linked) = LINKED.get() else {
        return;
    };
    let kinds = &linked.groups[group as usize];
    // SAFETY: the caller's contract: a live row list, whose head is a `List`.
    if list == 0 || kinds.is_empty() || !stored(unsafe { crate::engine::memory::read(list, 0) }) {
        return;
    }
    let language = crate::engine::texts::game_language().0;
    for &(kind, key) in kinds {
        let Some(value) = crate::inventory::stats::own_value(kind, key) else {
            continue;
        };
        TEXTS.with_borrow_mut(|texts| {
            let Some(shown) = texts
                .entry((kind, language))
                .or_insert_with(|| texts_of(kind, language))
            else {
                return;
            };
            let row = row(shown, value);
            // SAFETY: the game's push on its own live list, from this thread,
            // with a row laid out as its builders lay theirs (client
            // 26430625); it copies the row, whose texts live for the process.
            let _pushed = unsafe {
                (linked.push)(core::ptr::with_exposed_provenance_mut(list), &raw const row)
            };
        });
    }
}

/// Whether a list `head` has its storage; one without, which the game's push
/// would grow from nothing, takes no row.
fn stored(head: List) -> bool {
    head.rows != 0
}

/// Kind `kind`'s texts in `language`; `None` when it shows no row.
#[cold]
fn texts_of(kind: u32, language: u32) -> Option<Texts> {
    let loader = crate::engine::loader::loader()?;
    let shown =
        crate::inventory::stats::shown_kind(loader, kind, enshwick_core::i18n::Language(language))?;
    Some(Texts::new(&shown.name, &shown.description, shown.format))
}

/// A row for `texts` with `value`, from zeroed bytes as the submenu's empty
/// row: no breakdown, no second value, no trend.
fn row(texts: &Texts, value: f32) -> Row {
    let (value, percent, plus) = shown_value(value, texts.format);
    Row {
        label: texts.label,
        tooltip: texts.tooltip,
        value,
        flag: FLAG,
        cell: NO_CELL,
        percent: u8::from(percent),
        plus: u8::from(plus),
        ..bytemuck::Zeroable::zeroed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_value_shows_as_the_integer_its_format_reads() {
        assert_eq!(shown_value(3.4, StatFormat::Number), (3, false, false));
        assert_eq!(shown_value(-2.6, StatFormat::Signed), (-3, false, true));
        assert_eq!(shown_value(0.125, StatFormat::Percent), (13, true, false));
        assert_eq!(shown_value(f32::MAX, StatFormat::Number).0, i32::MAX);
    }

    #[test]
    fn a_row_lies_where_the_game_s_builders_write_it() {
        use core::mem::offset_of;
        assert_eq!(size_of::<Row>(), 0x2a0);
        let fields = [
            offset_of!(Row, tooltip),
            offset_of!(Row, value),
            offset_of!(Row, second),
            offset_of!(Row, pair),
            offset_of!(Row, trend),
            offset_of!(Row, breakdown),
            offset_of!(Row, flag),
            offset_of!(Row, seconds),
            offset_of!(Row, milliseconds),
            offset_of!(Row, cell),
            offset_of!(Row, percent),
            offset_of!(Row, plus),
        ];
        assert_eq!(
            fields,
            [
                0x18, 0x30, 0x34, 0x38, 0x3c, 0x278, 0x280, 0x281, 0x282, 0x288, 0x298, 0x299
            ]
        );
    }

    #[test]
    fn a_row_is_zero_but_its_texts_value_format_flag_and_cell() {
        let texts = Texts::new("Luck", "Fortune", StatFormat::Percent);
        let built = row(&texts, 0.1);
        let mut want: Row = bytemuck::Zeroable::zeroed();
        want.label = texts.label;
        want.tooltip = texts.tooltip;
        want.value = 10;
        want.percent = 1;
        want.flag = 1;
        want.cell = [-1, -1];
        assert_eq!(bytemuck::bytes_of(&built), bytemuck::bytes_of(&want));
        assert_eq!((built.label[1], built.tooltip[1]), (4, 7));
        assert_eq!(
            (built.breakdown, built.pair, built.trend, built.seconds),
            (0, 0, 0, 0),
            "no breakdown, pair, trend or time"
        );
        assert!(built.entries.iter().all(|&b| b == 0));
    }

    #[test]
    fn a_signed_row_gets_its_plus_and_no_percent() {
        let texts = Texts::new("Grit", "", StatFormat::Signed);
        let built = row(&texts, 2.0);
        assert_eq!((built.value, built.percent, built.plus), (2, 0, 1));
    }

    #[test]
    fn a_row_s_texts_keep_their_backslashes_in_the_game_s_markup() {
        let texts = Texts::new(r"Crit\Luck", r"a\b", StatFormat::Number);
        assert_eq!(
            (texts.label[1], texts.tooltip[1]),
            (10, 4),
            "each backslash doubled"
        );
    }

    #[test]
    fn a_list_without_storage_takes_no_row() {
        assert!(!stored(List { rows: 0 }));
        assert!(
            stored(List { rows: 0x1000 }),
            "an empty list with its inline rows takes the kinds' rows"
        );
    }

    #[test]
    fn two_groups_whose_builders_are_one_function_both_stay_unlinked() {
        let mut addresses = [None; GROUPS];
        assert_eq!(shared(&addresses), [false; GROUPS], "absent is not shared");
        for (at, address) in addresses.iter_mut().enumerate() {
            *address = Some(0x1000 + at as u64 * 0x10);
        }
        assert_eq!(shared(&addresses), [false; GROUPS]);
        addresses[7] = addresses[6];
        let mut want = [false; GROUPS];
        want[6] = true;
        want[7] = true;
        assert_eq!(shared(&addresses), want);
    }

    /// The offsets client 26430625's routes give, by entry.
    fn routed() -> Vec<(pinned::Constant, usize)> {
        use pinned::ui as ids;
        vec![
            (ids::ATTRIBUTE_ROW_LABEL_LENGTH, 0x8),
            (ids::ATTRIBUTE_ROW_VALUE, 0x30),
            (ids::ATTRIBUTE_ROW_SECOND, 0x34),
            (ids::ATTRIBUTE_ROW_TREND, 0x3c),
            (ids::ATTRIBUTE_ROW_SECONDS, 0x281),
            (ids::ATTRIBUTE_ROW_MILLISECONDS, 0x282),
            (ids::ATTRIBUTE_ROW_CELL, 0x288),
            (ids::ATTRIBUTE_ROW_CELL_SECOND, 0x290),
            (ids::ATTRIBUTE_ROW_PERCENT, 0x298),
            (ids::ATTRIBUTE_ROW_PLUS, 0x299),
            (ids::ATTRIBUTE_LIST_ROWS, 0),
            (ids::ATTRIBUTE_ROW_HOVERED, 0x3a8),
            (ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP, 0x3c0),
            (ids::ATTRIBUTE_ROW_HOVERED_FLAG, 0x628),
            (ids::ATTRIBUTE_ROW_PAIR_AFTER_SECOND, 4),
        ]
    }

    fn holds(table: &[(pinned::Constant, usize)]) -> Result<(), Failed> {
        fields_hold(&|id| {
            table
                .iter()
                .find(|(entry, _)| entry.id() == id.id())
                .map(|&(_, at)| at)
        })
    }

    #[test]
    fn the_routed_fields_hold_and_a_moved_one_names_its_entry() {
        use pinned::ui as ids;
        assert_eq!(holds(&routed()), Ok(()));
        for (at, moved_to, entry) in [
            (1, 0x2c, ids::ATTRIBUTE_ROW_VALUE),
            (7, 0x298, ids::ATTRIBUTE_ROW_CELL_SECOND),
            (12, 0x3b0, ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP),
            (13, 0x630, ids::ATTRIBUTE_ROW_HOVERED_FLAG),
        ] {
            let mut moved = routed();
            moved[at].1 = moved_to;
            assert_eq!(holds(&moved).map_err(|(id, _)| id), Err(entry.id()));
        }
    }

    #[test]
    fn a_build_without_a_routed_field_names_it_as_absent() {
        use pinned::ui as ids;
        let mut table = routed();
        table.retain(|(id, _)| id.id() != ids::ATTRIBUTE_ROW_CELL.id());
        assert_eq!(
            holds(&table),
            Err((ids::ATTRIBUTE_ROW_CELL.id(), crate::engine::builds::ABSENT))
        );
    }
}
