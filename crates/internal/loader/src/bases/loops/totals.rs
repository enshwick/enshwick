//! The per-base totals and cleanups the game keeps beside its array, done
//! for the window too, and the count the client's base list sizes itself
//! by.

use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::{
    ANIMAL_COUNTS, BEDS, Game, Layout, ObjFn, ObjWorldFn, RecordFields, add_u16, found_layout,
    occupied, rec, window_of, windowed,
};
use crate::base_groups::lookup::Bases;
use crate::base_groups::window;
use crate::chains::Next;

/// An animal's kind for which the game counts it apart, and the kinds it
/// counts together: 2 to 9.
const KIND_1: u8 = 1;
const KINDS_FROM: u8 = 2;
const KINDS_SPAN: u8 = 7;
/// The animal flags the game counts.
const FLAG_4: u8 = 4;
const FLAG_2: u8 = 2;
/// The kind of an NPC at home.
const NPC_AT_HOME: u8 = 7;
/// A cleanup's work on one record, whose fields it gets.
type Clear = unsafe fn(*mut u8, &RecordFields);

// SAFETY: `ObjFn` is the signature of the NPC totals the table's
// `bases.npc_totals` pins, and `link` uses this slot only as its next.
pub(super) static NPC_TOTALS_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjWorldFn` is the signature of the server update the table's
// `bases.server_update` pins, and `link` uses this slot only as its next.
pub(super) static SERVER_UPDATE_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup the table's
// `bases.bed_cleanup` pins, and `link` uses this slot only as its next.
pub(super) static BED_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup the table's
// `bases.comfort_cleanup` pins, and `link` uses this slot only as its next.
pub(super) static COMFORT_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup the table's
// `bases.props_cleanup` pins, and `link` uses this slot only as its next.
pub(super) static PROPS_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };

pub(super) unsafe extern "system" fn npc_totals(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the NPC totals' link, so the slot
    // holds the next function of that signature.
    let vanilla = unsafe { NPC_TOTALS_NEXT.get()(obj) };
    // SAFETY: the object of the serial bases update.
    if let Some((game, layout)) = unsafe { windowed(obj) } {
        // SAFETY: the same live object, viewed by `game`.
        let _done = crate::bases::BASES_FUSE.run("the NPC totals", || unsafe {
            count_npcs(obj, &game, layout);
        });
    }
    vanilla
}

/// The NPC totals' work for the window of `game`.
///
/// # Safety
///
/// - `obj` is the live object `game` views, in the serial bases update.
pub(super) unsafe fn count_npcs(obj: *mut u8, game: &Game, layout: &Layout) {
    let object = &layout.object;
    let mut home = [0u16; window::INDICES];
    let mut placed = [0u16; window::INDICES];
    let map = obj.wrapping_add(object.npc_map);
    // SAFETY: the object's NPC map, live and unchanged in the update; its
    // values pointer.
    let values = unsafe { read_ptr_at::<u8>(map, layout.map.values) };
    // SAFETY: the object's NPC map, live and unchanged in the update.
    for b in unsafe { occupied(map, &layout.map) } {
        let value = values.wrapping_add(b * object.npc_value);
        // SAFETY: an occupied bucket's value, `npc_value` bytes; its u8
        // base index first.
        let base = usize::from(unsafe { read_at::<u8>(value, 0) });
        // SAFETY: an occupied bucket's value, `npc_value` bytes; its u8
        // kind.
        let kind = unsafe { read_at::<u8>(value, object.npc_kind) };
        placed[base] = placed[base].wrapping_add(1);
        if kind == NPC_AT_HOME {
            home[base] = home[base].wrapping_add(1);
        }
    }
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its u16 count of NPCs at home.
        unsafe {
            write_at(
                rec(obj, i, &layout.window),
                layout.record.npcs_home,
                home[i],
            )
        };
        // SAFETY: the live object's u16 total of placed NPCs.
        unsafe { add_u16(obj, object.npcs, placed[i]) };
        // SAFETY: the live object's u16 total of NPCs at home.
        unsafe { add_u16(obj, object.npcs_home, home[i]) };
    }
}

pub(super) unsafe extern "system" fn server_update(obj: *mut u8, world: *mut u8) {
    // SAFETY: this detour runs only as the server update's link, so the
    // slot holds the next function of that signature.
    unsafe { SERVER_UPDATE_NEXT.get()(obj, world) };
    // SAFETY: the server's object, after its serial update.
    if let Some((game, layout)) = unsafe { windowed(obj) } {
        // SAFETY: the same live object, viewed by `game`.
        let _done = crate::bases::BASES_FUSE.run("the animal totals", || unsafe {
            count_animals(obj, &game, layout);
        });
    }
}

/// The animal counts' work for the window of `game`.
///
/// # Safety
///
/// - `obj` is the live server object `game` views, in its serial update.
pub(super) unsafe fn count_animals(obj: *mut u8, game: &Game, layout: &Layout) {
    let object = &layout.object;
    let mut counts = [[0u16; ANIMAL_COUNTS]; window::INDICES];
    let map = obj.wrapping_add(object.animal_map);
    // SAFETY: the object's animal map, live and unchanged in the update; its
    // values pointer.
    let values = unsafe { read_ptr_at::<u8>(map, layout.map.values) };
    // SAFETY: the object's animal map, live and unchanged in the update.
    for b in unsafe { occupied(map, &layout.map) } {
        // SAFETY: an occupied bucket's value, `{base, flags, kind}`.
        let [base, flags, kind] = unsafe { read_at::<[u8; 3]>(values, b * object.animal_value) };
        let count = &mut counts[usize::from(base)];
        count[0] = count[0].wrapping_add(1);
        if kind == KIND_1 {
            count[1] = count[1].wrapping_add(1);
        } else if kind.wrapping_sub(KINDS_FROM) <= KINDS_SPAN {
            count[2] = count[2].wrapping_add(1);
            count[3] = count[3].wrapping_add(u16::from(flags & FLAG_4 != 0));
            count[4] = count[4].wrapping_add(u16::from(flags & FLAG_2 != 0));
        }
    }
    for i in window_of(game) {
        for (k, count) in counts[i].into_iter().enumerate() {
            let at = layout.record.animals + k * 2;
            // SAFETY: record `i` holds a base; its u16 animal counts, which
            // `Layout::read` holds inside the record.
            unsafe { write_at(rec(obj, i, &layout.window), at, count) };
        }
        // SAFETY: the live object's u32 animal total.
        let total = unsafe { read_at::<u32>(obj, object.animals) };
        let total = total.wrapping_add(u32::from(counts[i][0]));
        // SAFETY: the live object's u32 animal total.
        unsafe { write_at(obj, object.animals, total) };
    }
}

/// The cleanups' post-hook: `clear` for every window record of `obj`.
///
/// # Safety
///
/// - `obj` is null or a live bases object.
unsafe fn clean(obj: *mut u8, site: &str, clear: Clear) {
    // SAFETY: the caller's guarantee.
    if let Some((game, layout)) = unsafe { windowed(obj) } {
        // SAFETY: the same live object, viewed by `game`.
        let _done = crate::bases::BASES_FUSE.run(site, || unsafe {
            clean_window(obj, &game, layout, clear);
        });
    }
}

/// `clear` for every window record of the object `game` views, whose fields
/// are `record`.
///
/// # Safety
///
/// - `obj` is the live object `game` views; `clear` writes only inside the
///   record it gets.
pub(super) unsafe fn clean_window(obj: *mut u8, game: &Game, layout: &Layout, clear: Clear) {
    for i in window_of(game) {
        // SAFETY: record `i` holds a base of the caller's live object.
        unsafe { clear(rec(obj, i, &layout.window), &layout.record) };
    }
}

/// The bed count cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base, whose fields are `record`.
pub(super) unsafe fn clear_beds(base: *mut u8, record: &RecordFields) {
    for k in 0..BEDS {
        // SAFETY: the caller's live record; its u32 bed counts, which
        // `Layout::read` holds inside the record.
        unsafe { write_at(base, record.beds + k * 4, 0u32) };
    }
}

/// The comfort cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base, whose fields are `record`.
pub(super) unsafe fn clear_comfort(base: *mut u8, record: &RecordFields) {
    let comfort = base.wrapping_add(record.comfort);
    // SAFETY: the caller's live record; its comfort array, which
    // `Layout::read` holds inside the record.
    unsafe { comfort.write_bytes(0, record.comfort_len) };
}

/// The crafting prop cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base, whose fields are `record`.
pub(super) unsafe fn clear_props(base: *mut u8, record: &RecordFields) {
    // SAFETY: the caller's live record; its u64 prop mask.
    unsafe { write_at(base, record.props, 0u64) };
}

pub(super) unsafe extern "system" fn bed_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link, so the slot holds
    // the next function of that signature.
    let vanilla = unsafe { BED_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the bed count cleanup", clear_beds) };
    vanilla
}

pub(super) unsafe extern "system" fn comfort_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link, so the slot holds
    // the next function of that signature.
    let vanilla = unsafe { COMFORT_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the comfort cleanup", clear_comfort) };
    vanilla
}

pub(super) unsafe extern "system" fn props_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link, so the slot holds
    // the next function of that signature.
    let vanilla = unsafe { PROPS_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the crafting prop cleanup", clear_props) };
    vanilla
}

/// The client's count getter, which the base list sizes itself by: the
/// game's count, or one past the highest index in use, so the list has an
/// entry at every window index a base holds. Patched in place of the
/// game's, which it never calls.
pub(super) unsafe extern "system" fn shown_bases(obj: *const u8) -> u32 {
    // The patch is registered only after the window's sites, its layout
    // among them, are found.
    let Some(layout) = found_layout() else {
        return 0;
    };
    // SAFETY: the base list's own live object, which the game's getter reads
    // the same way; its u32 count sits at the layout's `count`, which `find`
    // checked in the getter's bytes.
    let count = unsafe { read_at::<u32>(obj, layout.count) };
    let shown = crate::bases::BASES_FUSE.run("the base list's count", || {
        // SAFETY: the same object, read in the client's UI system.
        unsafe { windowed(obj) }.map(|(game, _)| {
            let count = usize::try_from(count).unwrap_or(0);
            u32::try_from(window::shown(count, game.count())).unwrap_or(u32::MAX)
        })
    });
    shown.flatten().unwrap_or(count)
}
