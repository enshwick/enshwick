//! The sort of stock entities into bases, and the list of base ids.

use enshwick_core::abi::HookFlags;
use enshwick_core::raw::{read_at, write_at};

use super::{Game, Layout, PAST16, append, point, rec, window_of, windowed};
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned::bases;
use crate::world::base_groups::lookup::{self, Bases};
use crate::world::base_groups::window;

/// The sort's answers.
pub(super) const SORTED: u8 = 0x00;
pub(super) const NOT_IN_A_BASE: u8 = 0x1f;
pub(super) const GROW_FAILED: u8 = 0x27;

/// Links the sorts and, with `base_ids`, the client's base id list, all
/// under [`PAST16`]; true when every link is made.
pub(super) fn link(loader: &Loader, base_ids: bool) -> bool {
    let at = Target::Pinned;
    loader.link_fused(
        &PAST16,
        at(bases::SORT),
        &SORT_NEXT,
        sort,
        HookFlags::EXCLUSIVE,
    ) && loader.link_fused(
        &PAST16,
        at(bases::SORT_ALL),
        &SORT_ALL_NEXT,
        sort_all,
        HookFlags::NONE,
    ) && (!base_ids
        || loader.link_fused(
            &PAST16,
            at(bases::BASE_IDS),
            &BASE_IDS_NEXT,
            self::base_ids,
            HookFlags::NONE,
        ))
}

crate::engine::chains::chain_detour! {
    /// The table's `bases.sort`: one stock entity sorted into the bases,
    /// over the window while the object holds more than 16.
    fn sort(out: *mut u8, obj: *mut u8, entity: *const u8, filtered: u64) -> *mut u8;
    next SORT_NEXT;
    fuse &PAST16, "the sort into bases";
    |onward| {
        // SAFETY: the sort's own object, in the serial bases update.
        let game = unsafe { windowed(obj) };
        if let (Some((game, layout)), false) = (game, out.is_null() || entity.is_null()) {
            // SAFETY: the sort's own answer, entity and object.
            unsafe { sort_into(out, obj, &game, layout, entity, filtered & 0xff != 0) };
            return out;
        }
        onward.call()
    }
}

/// The sort over every base of `game`.
///
/// # Safety
///
/// - `out` is writable for [`super::SORT_ANSWER_BYTES`], `entity` readable
///   for [`super::SORT_ENTITY_BYTES`], `obj` the live object `game` views, in the
///   serial bases update.
/// - `layout` passed [`Layout::misfit`], so its sort fields lie inside
///   both.
pub(super) unsafe fn sort_into(
    out: *mut u8,
    obj: *mut u8,
    game: &Game,
    layout: &Layout,
    entity: *const u8,
    filtered: bool,
) {
    let container = &game.fields().container;
    // SAFETY: the caller's writable answer; the u32 the game zeroes first,
    // at the table's `bases.sort_answer_word`, which `misfit` kept inside
    // its writable bytes.
    unsafe { write_at(out, layout.sort.answer_word, 0u32) };
    // SAFETY: the caller's entity; its fixed-point position, at the table's
    // `bases.sort_entity_position`, which `misfit` kept inside its readable
    // bytes.
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
            if unsafe {
                append(
                    rec(obj, i, &layout.window).wrapping_add(list),
                    container,
                    id,
                )
            } {
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

crate::engine::chains::chain_detour! {
    /// The table's `bases.sort_all`, the sort of every entity, which empties
    /// the window's stock lists as well while the object holds more than 16.
    fn sort_all(obj: *mut u8, world: *mut u8) -> ();
    next SORT_ALL_NEXT;
    fuse &PAST16, "the sort of every entity";
    |onward| {
        // SAFETY: the object of the serial bases update.
        if let Some((game, layout)) = unsafe { windowed(obj) } {
            // SAFETY: the same live object, viewed by `game`.
            unsafe { empty_lists(obj, &game, layout) };
        }
        onward.call();
    }
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
        let record = rec(obj, i, &layout.window);
        // SAFETY: record `i` holds a base; the u64 length of its first list.
        unsafe { write_at(record, layout.record.list_all + len, 0u64) };
        // SAFETY: record `i` holds a base; the u64 length of its second
        // list.
        unsafe { write_at(record, layout.record.list_kept + len, 0u64) };
    }
}

crate::engine::chains::chain_detour! {
    /// The client's table `bases.base_ids`, the list of base ids, with the
    /// window's appended while the object holds more than 16.
    fn base_ids(vec: *mut u8, obj: *mut u8) -> u64;
    next BASE_IDS_NEXT;
    fuse &PAST16, "the base ids";
    |onward| {
        let vanilla = onward.call();
        // SAFETY: the list's own object.
        if let (Some((game, layout)), false) = (unsafe { windowed(obj) }, vec.is_null()) {
            // SAFETY: the list's own container and object.
            unsafe { append_ids(vec, obj, &game, layout) };
        }
        vanilla
    }
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
    let Some(base_id) = layout.record.base_id else {
        return;
    };
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its 16-byte id.
        let id = unsafe { read_at::<[u64; 2]>(rec(obj, i, &layout.window), base_id) };
        // SAFETY: the caller's live container of 16-byte ids.
        if id != [0, 0] && !unsafe { append(vec, container, id) } {
            break;
        }
    }
}
