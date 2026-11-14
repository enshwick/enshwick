use std::cell::Cell;
use std::sync::atomic::Ordering;

use enshwick_core::Entity;
use enshwick_core::abi::{BedCheck, Header};
use enshwick_core::raw::{read_at, read_ptr_at};

use enshwick_base::Relock;
use enshwick_core::abi::HookFlags;

use super::lookup::{home, position_of};
use super::{BEDS_FUSE, LOOKUP_FUSE, MOVE_WINDOW, Moving, SKIP_UPDATE, STATE, layout};
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// Links the survivor lookup's moves and record updates, after its lookups;
/// false once it is off.
pub(super) fn link_moves(loader: &Loader) -> bool {
    loader.link_fused(
        &LOOKUP_FUSE,
        Target::Pinned(pinned::npc::MOVE),
        &MOVE_NEXT,
        move_link,
        HookFlags::NONE,
    ) && loader.link_fused(
        &LOOKUP_FUSE,
        Target::Pinned(pinned::npc::UPDATE),
        &UPDATE_NEXT,
        update_link,
        HookFlags::NONE,
    )
}

/// Links the bed checks; false once they are off.
pub(super) fn link_bed_checks(loader: &Loader) -> bool {
    loader.link_fused(
        &BEDS_FUSE,
        Target::Pinned(pinned::npc::BED_CHECK),
        &BED_CHECK_NEXT,
        bed_check_link,
        HookFlags::NONE,
    )
}

crate::engine::chains::chain_detour! {
    /// The game's `npc.update(bases, npc, state, flag)`, a survivor's record
    /// update: skipped once for the survivor the uniqueness pass kept
    /// another entity of. Its work reads one thread-local and the id.
    fn update_link(bases: *mut u8, npc: *const u32, state: usize, flag: usize) -> usize;
    next UPDATE_NEXT;
    |onward| {
        let skip = SKIP_UPDATE.with(Cell::take);
        // SAFETY: the game passes the update a non-null (checked first)
        // pointer to the `u32` survivor id it updates, readable for the call.
        if skip.is_some() && !npc.is_null() && skip == Some(unsafe { read_at(npc.cast(), 0) }) {
            0
        } else {
            onward.call()
        }
    }
}

crate::engine::chains::chain_detour! {
    /// The game's `npc.move(ctx, ..)`, a survivor's move: skipped for another
    /// entity of the survivor a staff moves.
    fn move_link(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize;
    next MOVE_NEXT;
    fuse &LOOKUP_FUSE, "a survivor move";
    |onward| {
        // SAFETY: the game passes the move the non-null (checked first)
        // behaviour context of the survivor it moves, on the system's thread
        // while it runs.
        if STATE.move_active.load(Ordering::Acquire) && !ctx.is_null() && unsafe { skip_move(ctx) } {
            0
        } else {
            onward.call()
        }
    }
}

/// Whether this entity is another entity of the survivor a staff moves.
///
/// # Safety
///
/// - `ctx` is a survivor's live behaviour context: its first word is the ECS
///   query context, and it keeps the `ActiveNpcState` pointer at the table's
///   `survivors.move_survivor`.
/// - The call runs on the behaviour system's thread while it runs.
pub(super) unsafe fn skip_move(ctx: *mut *mut u64) -> bool {
    let (Some(loader), Some(engine)) = (
        crate::engine::loader::loader(),
        crate::engine::ecs::resolved(),
    ) else {
        return false;
    };
    let Some(layout) = layout(loader) else {
        return false;
    };
    let mut moving = STATE.moving.relock();
    let Some(Moving {
        survivor,
        entity: target,
        since,
    }) = *moving
    else {
        return false;
    };
    if since.elapsed() > MOVE_WINDOW {
        *moving = None;
        STATE.move_active.store(false, Ordering::Release);
        return false;
    }
    let ctx = ctx.cast_const().cast::<u8>();
    // SAFETY: the caller's contract makes `ctx` a live behaviour context
    // whose first word is the ECS query context.
    let query = unsafe { read_ptr_at::<u64>(ctx, 0) };
    // SAFETY: `query` is null or the behaviour's live query context, used on
    // its system's thread by the caller's contract.
    let me = unsafe { engine.entity(query) };
    if me == target {
        *moving = None;
        STATE.move_active.store(false, Ordering::Release);
        return false;
    }
    // SAFETY: the caller's contract makes `ctx` a live behaviour context
    // that keeps the survivor's `ActiveNpcState` pointer at the table's
    // `survivors.move_survivor`.
    let state = unsafe { read_ptr_at::<u8>(ctx, layout.offsets.move_survivor) }.cast_const();
    if state.is_null() {
        return false;
    }
    // SAFETY: `state` is the survivor's non-null live `ActiveNpcState`
    // (checked earlier), which starts with its `u32` survivor id.
    let id: u32 = unsafe { read_at(state, 0) };
    id == survivor
}

crate::engine::chains::chain_detour! {
    /// The game's `npc.bed_check(ctx)`: a bed the game kept is asked of the
    /// mods' bed checks.
    fn bed_check_link(ctx: *mut u8) -> u8;
    next BED_CHECK_NEXT;
    fuse &BEDS_FUSE, "a bed check";
    |onward| {
        let valid = onward.call();
        // SAFETY: the game passes the bed check the non-null (checked first)
        // behaviour context of the survivor it checks, on the system's
        // thread while it runs.
        if valid == 0 || ctx.is_null() || unsafe { bed_kept(ctx.cast_const()) } {
            valid
        } else {
            0
        }
    }
}

/// Whether the mods keep a bed the game kept.
///
/// # Safety
///
/// - `ctx` is a survivor's live behaviour context: its first word is the ECS
///   query context, and it keeps its component pointers at the table's
///   `survivors.npc_*` offsets.
/// - The call runs on the behaviour system's thread while it runs.
pub(super) unsafe fn bed_kept(ctx: *const u8) -> bool {
    let (Some(loader), Some(engine), Some(bed_checks)) = (
        crate::engine::loader::loader(),
        crate::engine::ecs::resolved(),
        STATE.registrations.bed_checks.sealed(),
    ) else {
        return true;
    };
    let Some(layout) = layout(loader) else {
        return true;
    };
    let o = &layout.offsets;
    // SAFETY: the caller's contract makes `ctx` a live behaviour context that
    // keeps the `NpcCommon` pointer at the table's `survivors.npc_common`.
    let common = unsafe { read_ptr_at::<u8>(ctx, o.npc_common) }.cast_const();
    // SAFETY: the caller's contract makes `ctx` a live behaviour context that
    // keeps the `ActiveNpcState` pointer at the table's `survivors.npc_state`.
    let state = unsafe { read_ptr_at::<u8>(ctx, o.npc_state) }.cast_const();
    // SAFETY: the caller's contract makes `ctx` a live behaviour context that
    // keeps the saved state pointer at the table's
    // `survivors.npc_saved_state`.
    let saved = unsafe { read_ptr_at::<u8>(ctx, o.npc_saved_state) }.cast_const();
    // SAFETY: the caller's contract makes `ctx` a live behaviour context that
    // keeps the transform pointer at the table's `survivors.npc_transform`.
    let transform = unsafe { read_ptr_at::<u8>(ctx, o.npc_transform) }.cast_const();
    if common.is_null() || state.is_null() {
        return true;
    }
    // SAFETY: `common` is the survivor's non-null live `NpcCommon` (checked
    // earlier), which keeps its `u32` bed id at the table's
    // `survivors.npc_common_bed`.
    let bed: u32 = unsafe { read_at(common, o.npc_common_bed) };
    // SAFETY: `state` is the survivor's non-null live `ActiveNpcState`
    // (checked earlier), which starts with its `u32` survivor id.
    let survivor: u32 = unsafe { read_at(state, 0) };
    // SAFETY: the caller's contract makes `ctx` a live behaviour context
    // whose first word is the ECS query context.
    let ecs = unsafe { read_ptr_at::<u64>(ctx, 0) };
    // SAFETY: `ecs` is null or the behaviour's live query context, used on
    // its system's thread by the caller's contract.
    let Some(bed_at) = (unsafe { position_of(engine, ecs, Entity(bed)) }) else {
        return true;
    };
    // SAFETY: `ecs` is null or the behaviour's live query context, used on
    // its system's thread by the caller's contract.
    let entity = unsafe { engine.entity(ecs) };
    let check = BedCheck {
        header: Header::of::<BedCheck>(),
        npc_id: enshwick_core::abi::NpcId::new(survivor),
        entity: Entity(entity),
        bed_id: Entity(bed),
        // SAFETY: `saved` and `transform` are null or the survivor's live
        // components, which the behaviour context holds while it runs.
        home: unsafe { home(saved, transform) },
        bed_at,
    };
    bed_checks
        .iter()
        .filter(|r| !crate::engine::loader::is_off(r.owner))
        .all(|r| {
            let user = core::ptr::with_exposed_provenance_mut(r.user);
            // SAFETY: the registered `BedCheckFn` lives in the mod's DLL,
            // which stays loaded for the process, and takes the `user` word
            // it registered; `check` outlives the call.
            crate::engine::profile::call(
                r.owner,
                crate::engine::profile::Feature::Survivors,
                || unsafe { (r.callback)(&raw const check, user) != 0 },
            )
        })
}
