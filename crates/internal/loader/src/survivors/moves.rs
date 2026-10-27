use std::cell::Cell;
use std::sync::atomic::Ordering;

use enshwick::Entity;
use enshwick::abi::{BedCheck, Header};
use enshwick::raw::{read_at, read_ptr_at};

use crate::locks::Relock;

use super::lookup::{home, position_of};
use super::{
    BED_CHECK_NEXT, LOADED, MOVE_ACTIVE, MOVE_NEXT, MOVE_WINDOW, MOVING, SEALED, SKIP_UPDATE,
    SURVIVORS_FUSE, UPDATE_NEXT, layout,
};

pub(super) unsafe extern "system" fn update_link(
    bases: *mut u8,
    npc: *const u32,
    state: usize,
    flag: usize,
) -> usize {
    let skip = SKIP_UPDATE.with(Cell::take);
    // SAFETY: the game passes the update a non-null (checked first) pointer
    // to the `u32` survivor id it updates, readable for the call.
    if skip.is_some() && !npc.is_null() && skip == Some(unsafe { read_at(npc.cast(), 0) }) {
        return 0;
    }
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the update's signature.
    unsafe { UPDATE_NEXT.get()(bases, npc, state, flag) }
}

pub(super) unsafe extern "system" fn move_link(
    ctx: *mut *mut u64,
    b: usize,
    c: usize,
    d: usize,
) -> usize {
    let skip = MOVE_ACTIVE.load(Ordering::Acquire)
        && !ctx.is_null()
        && SURVIVORS_FUSE
            .run("a survivor move", || {
                // SAFETY: the game passes the move the non-null (checked
                // earlier) behaviour context of the survivor it moves, on the
                // system's thread while it runs.
                unsafe { skip_move(ctx) }
            })
            .unwrap_or(false);
    if skip {
        return 0;
    }
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the move's signature.
    unsafe { MOVE_NEXT.get()(ctx, b, c, d) }
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
    let (Some(loader), Some(engine)) = (LOADED.get(), crate::ecs::resolved()) else {
        return false;
    };
    let Some(layout) = layout(loader) else {
        return false;
    };
    let mut moving = MOVING.relock();
    let Some((survivor, target, at)) = *moving else {
        return false;
    };
    if at.elapsed() > MOVE_WINDOW {
        *moving = None;
        MOVE_ACTIVE.store(false, Ordering::Release);
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
        MOVE_ACTIVE.store(false, Ordering::Release);
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

pub(super) unsafe extern "system" fn bed_check_link(ctx: *mut u8) -> u8 {
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the bed check's signature.
    let valid = unsafe { BED_CHECK_NEXT.get()(ctx) };
    if valid == 0 || ctx.is_null() {
        return valid;
    }
    // SAFETY: the game passes the bed check the non-null (checked earlier)
    // behaviour context of the survivor it checks, on the system's thread
    // while it runs.
    let keep = SURVIVORS_FUSE.run("a bed check", || unsafe { bed_kept(ctx.cast_const()) });
    if keep.unwrap_or(true) { valid } else { 0 }
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
    let (Some(loader), Some(engine), Some(sealed)) =
        (LOADED.get(), crate::ecs::resolved(), SEALED.get())
    else {
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
        survivor,
        entity,
        bed,
        // SAFETY: `saved` and `transform` are null or the survivor's live
        // components, which the behaviour context holds while it runs.
        home: unsafe { home(saved, transform) },
        bed_at,
    };
    sealed
        .bed_checks
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
        .all(|r| {
            let user = core::ptr::with_exposed_provenance_mut(r.user);
            // SAFETY: the registered `BedCheckFn` lives in the mod's DLL,
            // which stays loaded for the process, and takes the `user` word
            // it registered; `check` outlives the call.
            crate::profile::call(r.owner, crate::profile::Feature::Survivors, || unsafe {
                (r.callback)(&raw const check, user) != 0
            })
        })
}
