//! The placement cursor's refusal of an altar that would start a base past
//! the asked limit, with the game's own "too many altars" answer.

use enshwick::raw::{read_at, write_at};

use super::{ASKED, SITES, known};
use crate::base_groups::limit::{self, MAX_BASES};
use crate::base_groups::window::Kind;

/// Sets the cursor's "too many altars" bit for an altar whose zone would
/// stand apart from every base once the game holds as many as it may.
///
/// # Safety
///
/// - `flags`, `ctx`, `transform` and `item` are the placement cursor's
///   own, non-null arguments, live for the call.
/// - Called on the cursor's thread after the original ran, which alone
///   writes `flags` meanwhile.
pub(super) unsafe fn refuse_past_limit(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
) {
    let (Some(sites), Some((_, zones)), Some(engine)) =
        (SITES.get(), known(), crate::ecs::resolved())
    else {
        return;
    };
    let fields = sites.fields;
    // SAFETY: the caller guarantees the cursor's live item definition; its
    // u8 altar flag sits at the build table's `bases.item_altar`.
    let altar = unsafe { read_at::<u8>(item, fields.item_altar) } != 0;
    // SAFETY: the caller guarantees the cursor's live u32 flags word.
    let reached =
        unsafe { read_at::<u32>(flags.cast_const().cast(), 0) } & fields.max_altars_reached;
    if !altar || reached != 0 {
        return;
    }
    let most = u32::try_from(ASKED.bases(Kind::Server)).unwrap_or(MAX_BASES);
    if zones.bases < most {
        return;
    }
    // SAFETY: the caller guarantees the cursor's live context; its balancing
    // registry word sits at the offset the `CURSOR_ALTAR` pattern read from
    // the cursor's own code.
    let balancing = unsafe { read_at::<u64>(ctx, sites.balancing) };
    if balancing == 0 {
        return;
    }
    // SAFETY: `balancing` is the running cursor's non-zero balancing
    // registry word, used on the cursor's thread.
    let Some(size) = (unsafe { engine.zone_size(balancing, 1) }) else {
        return;
    };
    // SAFETY: the caller guarantees the cursor's live transform, which
    // starts with the position, three i64.
    let fixed = unsafe { read_at::<[i64; 3]>(transform.cast(), 0) };
    let candidate = limit::candidate(enshwick::ecs::metres(fixed), size);
    if zones.refuses(&candidate, most) {
        // SAFETY: the caller guarantees the cursor's live u32 flags word.
        let refused =
            unsafe { read_at::<u32>(flags.cast_const().cast(), 0) } | fields.max_altars_reached;
        // SAFETY: the caller guarantees the cursor's live u32 flags word,
        // which only the cursor's thread writes meanwhile.
        unsafe { write_at(flags.cast(), 0, refused) };
    }
}
