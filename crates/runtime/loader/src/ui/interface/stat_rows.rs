//! Rows of the stat kinds with a group on the game's Character screen.
//!
//! The screen's attribute submenu fills one row list per group, each with a
//! builder of its own, and draws every row of a list under the group's
//! header with the game's row widget (client 26430625, decompiler: the
//! submenu `doAttributesSubmenu`, nine builders, the row push they share).
//! A detour on a group's builder lets it fill its list, then pushes a row
//! per kind of that group: a copy of the list's last game row, so every
//! field the loader does not set keeps the game's own layout, with the
//! kind's name, description and the local player's value as the server
//! last sent it. The game formats the value as it formats its own rows, an
//! integer with a sign or a percent sign.
//!
//! A detour links only for a group some kind shows in, under a fuse of its
//! own, so a builder this build lacks or changed turns off only its group.
//! Per hit it pays, for each kind of its group, a load of the player's
//! value and a lookup of texts kept with their hashes; a group without kinds
//! never links.

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
/// builders, the row widget and its value text): two texts, the value, the
/// flags that pick how it reads, and the breakdown the cursor shows. Its
/// size is the table's `ui.attribute_row_size`, checked at the link.
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
    entries_rest: [u8; 8],
    /// The value reads as seconds, or as milliseconds.
    seconds: u8,
    milliseconds: u8,
    layout: [u8; 21],
    /// The value reads `%d%%`, and a positive one gets a `+`.
    percent: u8,
    plus: u8,
    rest: [u8; 6],
}

/// The game's text as three words: its bytes, their length and their hash.
type UiTextWords = [u64; 3];

/// A row list's head: its rows and how many (client 26430625, decompiler).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct List {
    rows: u64,
    filled: u64,
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
    let mut linked = 0;
    for ((kinds, &(detour, next, fuse)), builder) in
        groups.iter().zip(&DETOURS).zip(pinned::ui::ATTRIBUTE_ROWS)
    {
        if kinds.is_empty() {
            continue;
        }
        if loader.address(builder).is_none() {
            let reason = loader
                .build
                .reason(builder.id(), crate::engine::builds::ABSENT);
            loader.fuse_off(fuse, builder.id(), reason);
            continue;
        }
        if loader.link_fused(fuse, Target::Pinned(builder), next, detour, HookFlags::NONE) {
            linked += 1;
        }
    }
    crate::log!(
        INFO,
        "interface: stat rows in {linked} groups of the Character screen"
    );
}

/// The row push of this build, once its row size is the one the loader
/// lays rows out in, or the first table entry it lacks, with why.
fn linked_push(loader: &Loader) -> Result<PushFn, (&'static str, &'static str)> {
    use crate::engine::builds::ABSENT;
    let size = pinned::ui::ATTRIBUTE_ROW_SIZE;
    match loader.offset(size) {
        Some(bytes) if bytes == size_of::<Row>() => {}
        Some(_) => return Err((size.id(), "is not the row size the loader lays rows out in")),
        None => return Err((size.id(), ABSENT)),
    }
    let push = pinned::ui::ATTRIBUTE_ROW_PUSH;
    let address = loader
        .address(push)
        .filter(|_| loader.code_holds(push))
        .and_then(|a| usize::try_from(a).ok())
        .ok_or((push.id(), ABSENT))?;
    // SAFETY: `code_holds` checked the code at `ui.attribute_row_push`
    // against its route's check, so it is the game's push of an attribute
    // row, typed `PushFn` (client 26430625, decompiler).
    Ok(unsafe { core::mem::transmute::<usize, PushFn>(address) })
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
    use crate::engine::memory::read;
    let Some(linked) = LINKED.get() else {
        return;
    };
    let kinds = &linked.groups[group as usize];
    if kinds.is_empty() || list == 0 {
        return;
    }
    // SAFETY: the caller's contract: a live row list, whose head is a `List`.
    let head = unsafe { read::<List>(list, 0) };
    let (Ok(rows), Some(newest)) = (
        usize::try_from(head.rows),
        usize::try_from(head.filled)
            .ok()
            .and_then(|c| c.checked_sub(1)),
    ) else {
        return;
    };
    if rows == 0 {
        return;
    }
    let game_row = |at: usize| {
        // SAFETY: row `at` of the list's `filled` rows of `Row`s from
        // `rows`; the table's row size is `Row`'s, checked at the link.
        unsafe { read::<Row>(rows + at * size_of::<Row>(), 0) }
    };
    let Some(template) = (0..=newest).rev().map(game_row).find(is_template) else {
        return;
    };
    let language = crate::content::served::language().0;
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
            let row = row(&template, shown, value);
            // SAFETY: the game's push on its own live list, from this thread,
            // with a row laid out as its builders lay theirs (client
            // 26430625); it copies the row, whose texts live for the process.
            let _pushed = unsafe {
                (linked.push)(core::ptr::with_exposed_provenance_mut(list), &raw const row)
            };
        });
    }
}

/// Kind `kind`'s texts in `language`; `None` when it shows no row.
#[cold]
fn texts_of(kind: u32, language: u32) -> Option<Texts> {
    let loader = crate::engine::loader::loader()?;
    let shown =
        crate::inventory::stats::shown_kind(loader, kind, enshwick_core::i18n::Language(language))?;
    Some(Texts::new(&shown.name, &shown.description, shown.format))
}

/// Whether a game row can stand as the template of a kind's: a row with a
/// label, not one of the blank rows the builders put between parts of a
/// group (client 26430625, decompiler).
fn is_template(row: &Row) -> bool {
    row.label[1] != 0
}

/// A row for `texts` with `value`, from the game's row `template`.
fn row(template: &Row, texts: &Texts, value: f32) -> Row {
    let (value, percent, plus) = shown_value(value, texts.format);
    Row {
        label: texts.label,
        tooltip: texts.tooltip,
        value,
        second: 0,
        pair: 0,
        trend: 0,
        breakdown: 0,
        seconds: 0,
        milliseconds: 0,
        percent: u8::from(percent),
        plus: u8::from(plus),
        ..*template
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
            offset_of!(Row, seconds),
            offset_of!(Row, milliseconds),
            offset_of!(Row, percent),
            offset_of!(Row, plus),
        ];
        assert_eq!(
            fields,
            [
                0x18, 0x30, 0x34, 0x38, 0x3c, 0x278, 0x281, 0x282, 0x298, 0x299
            ]
        );
        assert_eq!(offset_of!(List, filled), 8);
    }

    #[test]
    fn a_row_keeps_the_game_s_layout_and_sets_only_its_own_fields() {
        let mut template: Row = bytemuck::cast([0xab_u8; 0x2a0]);
        template.layout[5] = 7;
        let texts = Texts::new("Luck", "", StatFormat::Percent);
        let row = row(&template, &texts, 0.1);
        assert_eq!(row.layout[5], 7, "the game's layout");
        assert_eq!(row.entries, template.entries);
        assert_eq!(row.value, 10);
        assert_eq!((row.label[1], row.tooltip[1]), (4, 0));
        assert_eq!(
            (row.percent, row.plus, row.breakdown, row.pair, row.trend),
            (1, 0, 0, 0, 0)
        );
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
    fn a_blank_separator_row_is_no_template_and_a_labelled_row_is() {
        let mut row: Row = bytemuck::Zeroable::zeroed();
        assert!(!is_template(&row), "a separator's label is empty");
        row.label = Texts::new("Strength", "", StatFormat::Number).label;
        assert!(is_template(&row));
    }
}
