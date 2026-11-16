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
//! last sent it. A group whose list is empty this frame copies a spare
//! row kept without the fields that point into its frame. The game formats
//! the value as it formats its own rows, an integer with a sign or a
//! percent sign.
//!
//! A detour links only for a group some kind shows in, and for the Main
//! group, whose rows give the spare, under a fuse of its own, so a builder
//! this build lacks or changed turns off only its group; Main's builder,
//! when no kind shows there, is never listed off. A build whose breakdown
//! flag no route finds keeps every group's rows but the spare.
//! Per hit it pays, for each kind of its group, a load of the player's
//! value and a lookup of texts kept with their hashes; Main's detour without
//! kinds returns at once once the spare is kept.

use core::cell::{OnceCell, RefCell};
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

/// The group whose builder links whenever a kind shows anywhere, as the
/// source of the spare template: its list always holds rows (health, mana,
/// stamina; client 26430625).
const SPARE_SOURCE: StatGroup = StatGroup::Main;

/// What every group's rows need: the push and the row layout.
static FUSE: Fuse = Fuse::new("stat rows");
static LINKED: OnceLock<Linked> = OnceLock::new();

type BuilderFn = unsafe extern "system" fn(list: *mut u8);
type PushFn = unsafe extern "system" fn(list: *mut u8, row: *const Row) -> *mut u8;

/// A row of the game's attribute lists (client 26430625, decompiler of the
/// builders, the row widget and its value text): two texts, the value, the
/// flags that pick how it reads, and the breakdown the cursor shows. Its
/// size and the offsets of the fields the loader sets are the table's,
/// checked at the link ([`fields_hold`]).
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
    /// The routes put the breakdown flag where [`Row`] keeps it, so a row
    /// can turn it off and a spare can stand without its entries.
    breakdown: bool,
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
    /// The first labelled row of [`SPARE_SOURCE`]'s list, without the
    /// fields that point into its frame ([`spare`]): the template of a
    /// group whose own list has no row this frame.
    static SPARE: OnceCell<Row> = const { OnceCell::new() };
}

/// `row` without the fields that point into the frame that built it: its
/// two texts and the breakdown's entries, the only pointers a builder
/// writes into a row (the 0x288 and 0x290 words are a grid position, -1 for
/// none; client 26430625, decompiler of the builders and the row widget).
/// Its breakdown is off, so the game reads no entry of it.
fn spare(row: &Row) -> Row {
    Row {
        label: [0; 3],
        tooltip: [0; 3],
        entries: [0; 571],
        breakdown: 0,
        ..*row
    }
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
    let (push, breakdown) = match linked_push(loader) {
        Ok(linked) => linked,
        Err((id, reason)) => {
            loader.fuse_off(&FUSE, id, reason);
            return;
        }
    };
    let groups = Box::leak(Box::new(groups));
    let _first = LINKED.set(Linked {
        push,
        breakdown,
        groups,
    });
    let lookup = |id: pinned::Constant| loader.offset(id);
    let mut linked = 0;
    for (at, ((kinds, &(detour, next, fuse)), builder)) in groups
        .iter()
        .zip(&DETOURS)
        .zip(pinned::ui::ATTRIBUTE_ROWS)
        .enumerate()
    {
        let asked = !kinds.is_empty();
        if !asked && (StatGroup::ALL[at] != SPARE_SOURCE || !breakdown) {
            continue;
        }
        let found = if loader.address(builder).is_none() {
            Err((builder.id(), crate::engine::builds::ABSENT))
        } else {
            opens_its_group(&lookup, at)
        };
        match found {
            Err((id, reason)) if asked => {
                let _off = loader.fuse_off(fuse, id, reason);
            }
            Err((id, reason)) => crate::log!(
                INFO,
                "interface: no spare stat row, no kind shows in Main: {id} {}",
                loader.build.reason(id, reason)
            ),
            Ok(()) => {
                if loader.link_fused(fuse, Target::Pinned(builder), next, detour, HookFlags::NONE)
                    && asked
                {
                    linked += 1;
                }
            }
        }
    }
    crate::log!(
        INFO,
        "interface: stat rows in {linked} groups of the Character screen"
    );
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

/// Whether the builder of group `at` (in the order of [`StatGroup::ALL`])
/// first loads the text that opens its group, as reflection names it: a
/// builder that loads another group's text fills another group's list, as
/// when a build reorders the submenu's calls, and its group shows no rows.
fn opens_its_group(
    lookup: &dyn Fn(pinned::Constant) -> Option<usize>,
    at: usize,
) -> Result<(), Failed> {
    let read = pinned::ui::ATTRIBUTE_ROWS_TEXT[at];
    let named = pinned::ui::ATTRIBUTE_TEXTS[at];
    (offset(lookup, read)? == offset(lookup, named)?)
        .then_some(())
        .ok_or((
            read.id(),
            "is not the text that opens the builder's group, so the build calls another group's builder there",
        ))
}

/// The row push of this build and whether [`fields_hold`] found the
/// breakdown flag, once its row size is the one the loader lays rows out
/// in, or the first table entry it lacks, with why.
fn linked_push(loader: &Loader) -> Result<(PushFn, bool), Failed> {
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
    let breakdown = fields_hold(&|id| loader.offset(id))?;
    // SAFETY: `code_holds` checked the code at `ui.attribute_row_push`
    // against its route's check, so it is the game's push of an attribute
    // row, typed `PushFn` (client 26430625, decompiler).
    Ok((
        unsafe { core::mem::transmute::<usize, PushFn>(address) },
        breakdown,
    ))
}

/// Whether every field of [`Row`] and [`List`] the loader reads or writes
/// lies where the game's code reads it on this build, else the first entry
/// that disagrees or is absent, with why. Most fields are one read through
/// a row pointer; the tooltip is a read of the submenu's copy of the hovered
/// row, the pair flag a read beside the second value, and the breakdown flag
/// a builder's store below the row it pushes (client 26430625, decompiler).
/// The breakdown flag's routes lie in one builder, Fog Survival's: when they
/// are absent, `Ok(false)`, and rows leave the flag as their template has it.
fn fields_hold(lookup: &dyn Fn(pinned::Constant) -> Option<usize>) -> Result<bool, Failed> {
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
        (ids::ATTRIBUTE_ROW_PERCENT, offset_of!(Row, percent)),
        (ids::ATTRIBUTE_ROW_PLUS, offset_of!(Row, plus)),
        (ids::ATTRIBUTE_LIST_ROWS, offset_of!(List, rows)),
        (ids::ATTRIBUTE_LIST_FILLED, offset_of!(List, filled)),
    ];
    for (id, field) in read {
        if offset(id)? != field {
            return Err((id.id(), moved));
        }
    }
    let composed = [
        (
            ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP,
            offset(ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP)?
                .checked_sub(offset(ids::ATTRIBUTE_ROW_HOVERED)?),
            offset_of!(Row, tooltip),
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
    let (Some(set), Some(row)) = (
        lookup(ids::ATTRIBUTE_ROW_BREAKDOWN_SET),
        lookup(ids::ATTRIBUTE_ROW_BREAKDOWN_ROW),
    ) else {
        return Ok(false);
    };
    if set.checked_add(row) != Some(offset_of!(Row, breakdown)) {
        return Err((ids::ATTRIBUTE_ROW_BREAKDOWN_SET.id(), moved));
    }
    Ok(true)
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
    let keeps_spare = linked.breakdown && group == SPARE_SOURCE;
    if list == 0 || kinds.is_empty() && (!keeps_spare || SPARE.with(|s| s.get().is_some())) {
        return;
    }
    // SAFETY: the caller's contract: a live row list, whose head is a `List`.
    let Some((rows, filled)) = stored(unsafe { read::<List>(list, 0) }) else {
        return;
    };
    let game_row = |at: usize| {
        // SAFETY: row `at` of the list's `filled` rows of `Row`s from
        // `rows`, non-null by `stored`; the table's row size is `Row`'s,
        // checked at the link.
        unsafe { read::<Row>(rows + at * size_of::<Row>(), 0) }
    };
    let own = (0..filled).rev().map(game_row).find(is_template);
    if keeps_spare && let Some(row) = &own {
        SPARE.with(|s| {
            let _kept = s.get_or_init(|| spare(row));
        });
    }
    if kinds.is_empty() {
        return;
    }
    // A group whose list is empty this frame, such as Fog Survival outside
    // the shroud, takes the spare Main's list left.
    let Some(template) = own.or_else(|| {
        linked
            .breakdown
            .then(|| SPARE.with(|s| s.get().copied()))
            .flatten()
    }) else {
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
            let row = row(&template, shown, value, linked.breakdown);
            // SAFETY: the game's push on its own live list, from this thread,
            // with a row laid out as its builders lay theirs (client
            // 26430625); it copies the row, whose texts live for the process.
            let _pushed = unsafe {
                (linked.push)(core::ptr::with_exposed_provenance_mut(list), &raw const row)
            };
        });
    }
}

/// Where a list `head` keeps its rows and how many it holds; `None` for a
/// list with no storage, which the game's push would grow from nothing.
fn stored(head: List) -> Option<(usize, usize)> {
    let rows = usize::try_from(head.rows).ok().filter(|&rows| rows != 0)?;
    Some((rows, usize::try_from(head.filled).ok()?))
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

/// A row for `texts` with `value`, from the game's row `template`; its
/// breakdown off when the routes found where the flag lies (`breakdown`),
/// else as the template has it.
fn row(template: &Row, texts: &Texts, value: f32, breakdown: bool) -> Row {
    let (value, percent, plus) = shown_value(value, texts.format);
    Row {
        label: texts.label,
        tooltip: texts.tooltip,
        value,
        second: 0,
        pair: 0,
        trend: 0,
        breakdown: if breakdown { 0 } else { template.breakdown },
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
        let row = row(&template, &texts, 0.1, true);
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
    fn a_spare_row_keeps_the_layout_and_drops_what_points_into_its_frame() {
        let mut row: Row = bytemuck::cast([0xab_u8; 0x2a0]);
        row.layout[5] = 7;
        let spare = spare(&row);
        assert_eq!((spare.label, spare.tooltip), ([0; 3], [0; 3]));
        assert!(spare.entries.iter().all(|&b| b == 0));
        assert_eq!(spare.breakdown, 0, "no entry is read");
        assert_eq!(spare.layout, row.layout, "the grid position stays");
        assert_eq!(spare.entries_rest, row.entries_rest);
    }

    #[test]
    fn a_row_keeps_its_template_s_breakdown_flag_where_no_route_found_it() {
        let mut template: Row = bytemuck::Zeroable::zeroed();
        template.breakdown = 1;
        let texts = Texts::new("Luck", "", StatFormat::Number);
        assert_eq!(row(&template, &texts, 1.0, false).breakdown, 1);
        assert_eq!(row(&template, &texts, 1.0, true).breakdown, 0);
    }

    #[test]
    fn a_list_without_storage_takes_no_row() {
        assert_eq!(stored(List { rows: 0, filled: 0 }), None);
        assert_eq!(stored(List { rows: 0, filled: 3 }), None);
        assert_eq!(
            stored(List {
                rows: 0x1000,
                filled: 0
            }),
            Some((0x1000, 0)),
            "an empty list with its inline rows takes the spare"
        );
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
            (ids::ATTRIBUTE_ROW_PERCENT, 0x298),
            (ids::ATTRIBUTE_ROW_PLUS, 0x299),
            (ids::ATTRIBUTE_LIST_ROWS, 0),
            (ids::ATTRIBUTE_LIST_FILLED, 8),
            (ids::ATTRIBUTE_ROW_HOVERED, 0x3a8),
            (ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP, 0x3c0),
            (ids::ATTRIBUTE_ROW_PAIR_AFTER_SECOND, 4),
            (ids::ATTRIBUTE_ROW_BREAKDOWN_SET, 0x238),
            (ids::ATTRIBUTE_ROW_BREAKDOWN_ROW, 0x40),
        ]
    }

    fn holds(table: &[(pinned::Constant, usize)]) -> Result<bool, Failed> {
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
        assert_eq!(holds(&routed()), Ok(true));
        let mut moved = routed();
        moved[1].1 = 0x2c;
        assert_eq!(
            holds(&moved).map_err(|(id, _)| id),
            Err(ids::ATTRIBUTE_ROW_VALUE.id())
        );
        let mut tooltip = routed();
        tooltip[11].1 = 0x3b0;
        assert_eq!(
            holds(&tooltip).map_err(|(id, _)| id),
            Err(ids::ATTRIBUTE_ROW_HOVERED_TOOLTIP.id())
        );
        let mut breakdown = routed();
        breakdown[14].1 = 0x48;
        assert_eq!(
            holds(&breakdown).map_err(|(id, _)| id),
            Err(ids::ATTRIBUTE_ROW_BREAKDOWN_SET.id())
        );
    }

    #[test]
    fn a_build_without_the_breakdown_routes_keeps_the_rows_without_the_flag() {
        let mut table = routed();
        table.truncate(14);
        assert_eq!(
            holds(&table),
            Ok(false),
            "only Fog Survival's builder routes it"
        );
    }

    #[test]
    fn a_builder_that_opens_another_group_s_text_is_refused() {
        use pinned::ui as ids;
        let (read, named) = (ids::ATTRIBUTE_ROWS_TEXT[2], ids::ATTRIBUTE_TEXTS[2]);
        let lookup = |text: usize| {
            move |id: pinned::Constant| {
                if id.id() == read.id() {
                    Some(text)
                } else {
                    (id.id() == named.id()).then_some(0x820)
                }
            }
        };
        assert_eq!(opens_its_group(&lookup(0x820), 2), Ok(()));
        assert_eq!(
            opens_its_group(&lookup(0x10e0), 2).map_err(|(id, _)| id),
            Err(read.id())
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
