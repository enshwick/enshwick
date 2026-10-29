//! The sort of stock entities into bases, and the list of base ids.

use enshwick::raw::{read_at, write_at};

use super::{Game, Layout, ObjWorldFn, append, point, rec, window_of, windowed};
use crate::base_groups::lookup::{self, Bases};
use crate::base_groups::window;
use crate::chains::Next;

/// The sort's answers.
pub(super) const SORTED: u8 = 0x00;
pub(super) const NOT_IN_A_BASE: u8 = 0x1f;
pub(super) const GROW_FAILED: u8 = 0x27;

pub(super) type SortFn = unsafe extern "system" fn(
    out: *mut u8,
    obj: *mut u8,
    entity: *const u8,
    filtered: u64,
) -> *mut u8;
pub(super) type BaseIdsFn = unsafe extern "system" fn(vec: *mut u8, obj: *mut u8) -> u64;

// SAFETY: `SortFn` is the signature of the sort the table's `bases.sort`
// pins, and `link` uses this slot only as that function's next.
pub(super) static SORT_NEXT: Next<SortFn> = unsafe { Next::new() };
// SAFETY: `ObjWorldFn` is the signature of the sort of every entity the
// table's `bases.sort_all` pins, and `link` uses this slot only as its next.
pub(super) static SORT_ALL_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };
// SAFETY: `BaseIdsFn` is the signature of the base id list the table's
// `bases.base_ids` pins, and `link` uses this slot only as its next.
pub(super) static BASE_IDS_NEXT: Next<BaseIdsFn> = unsafe { Next::new() };

pub(super) unsafe extern "system" fn sort(
    out: *mut u8,
    obj: *mut u8,
    entity: *const u8,
    filtered: u64,
) -> *mut u8 {
    // SAFETY: the sort's own object, in the serial bases update.
    let game = unsafe { windowed(obj) };
    if let (Some((game, layout)), false) = (game, out.is_null() || entity.is_null()) {
        // SAFETY: the sort's own answer, entity and object.
        let done = crate::bases::BASES_FUSE.run("the sort into bases", || unsafe {
            sort_into(out, obj, &game, layout, entity, filtered & 0xff != 0);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the sort's link, so the slot holds the
    // next function of that signature; the game's arguments pass unchanged.
    unsafe { SORT_NEXT.get()(out, obj, entity, filtered) }
}

/// The sort over every base of `game`.
///
/// # Safety
///
/// - `out` is writable for 8 bytes, `entity` readable for 0x20, `obj` the
///   live object `game` views, in the serial bases update.
pub(super) unsafe fn sort_into(
    out: *mut u8,
    obj: *mut u8,
    game: &Game,
    layout: &Layout,
    entity: *const u8,
    filtered: bool,
) {
    let container = &game.fields().container;
    // SAFETY: the caller's writable answer; its u32 the game zeroes first
    // sits at the build table's `bases.sort_answer_word`.
    unsafe { write_at(out, layout.sort.answer_word, 0u32) };
    // SAFETY: the caller's entity; its fixed-point position sits at the
    // build table's `bases.sort_entity_position`.
    let p = unsafe { point(entity.wrapping_add(layout.sort.entity_position)) };
    let hit = window::indices(game.count()).find(|&i| lookup::in_base(game, i, p));
    let answer = match hit {
        Some(i) => {
            let list = if filtered {
                layout.record.list_kept
            } else {
                layout.record.list_all
            };
            // SAFETY: the caller's entity; its u32 id comes first.
            let id = unsafe { read_at::<u32>(entity, 0) };
            // SAFETY: record `i` holds a base the game's new base code
            // started, whose lists are the game's with its grower.
            if unsafe { append(rec(obj, i).wrapping_add(list), container, id) } {
                SORTED
            } else {
                GROW_FAILED
            }
        }
        None => NOT_IN_A_BASE,
    };
    // SAFETY: the caller's writable answer; its error byte first.
    unsafe { write_at(out, 0, answer) };
}

pub(super) unsafe extern "system" fn sort_all(obj: *mut u8, world: *mut u8) {
    // SAFETY: the object of the serial bases update.
    if let Some((game, layout)) = unsafe { windowed(obj) } {
        // SAFETY: the same live object, viewed by `game`.
        let _done = crate::bases::BASES_FUSE.run("the sort of every entity", || unsafe {
            empty_lists(obj, &game, layout);
        });
    }
    // SAFETY: this detour runs only as the sort of every entity's link, so
    // the slot holds the next function of that signature.
    unsafe { SORT_ALL_NEXT.get()(obj, world) };
}

/// Empties both stock lists of every window base of `game`, as the sort of
/// every entity empties the array's before it sorts.
///
/// # Safety
///
/// - `obj` is the live object `game` views, in the serial bases update.
pub(super) unsafe fn empty_lists(obj: *mut u8, game: &Game, layout: &Layout) {
    let len = game.fields().container.len;
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; the u64 length of its first list.
        unsafe { write_at(rec(obj, i), layout.record.list_all + len, 0u64) };
        // SAFETY: record `i` holds a base; the u64 length of its second
        // list.
        unsafe { write_at(rec(obj, i), layout.record.list_kept + len, 0u64) };
    }
}

pub(super) unsafe extern "system" fn base_ids(vec: *mut u8, obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the base id list's link, so the slot
    // holds the next function of that signature.
    let vanilla = unsafe { BASE_IDS_NEXT.get()(vec, obj) };
    // SAFETY: the list's own object.
    if let (Some((game, layout)), false) = (unsafe { windowed(obj) }, vec.is_null()) {
        // SAFETY: the list's own container and object.
        let _done = crate::bases::BASES_FUSE.run("the base ids", || unsafe {
            append_ids(vec, obj, &game, layout);
        });
    }
    vanilla
}

/// Appends the id of every window base of `game` that has one, in index
/// order, until the container cannot grow.
///
/// # Safety
///
/// - `vec` is the game's live container of 16-byte ids; `obj` the live
///   object `game` views.
pub(super) unsafe fn append_ids(vec: *mut u8, obj: *mut u8, game: &Game, layout: &Layout) {
    let container = &game.fields().container;
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its 16-byte id.
        let id = unsafe { read_at::<[u64; 2]>(rec(obj, i), layout.record.base_id) };
        // SAFETY: the caller's live container of 16-byte ids.
        if id != [0, 0] && !unsafe { append(vec, container, id) } {
            break;
        }
    }
}
