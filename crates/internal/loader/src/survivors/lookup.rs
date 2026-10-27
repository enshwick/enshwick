use core::ffi::c_void;
use std::sync::atomic::Ordering;
use std::time::Instant;

use enshwick::Entity;
use enshwick::abi::{Header, Side, SurvivorLookup};
use enshwick::altars::cell_of_fixed;
use enshwick::raw::read_at;

use crate::ecs::Engine;
use crate::host::Loader;
use crate::locks::Relock;

use super::{
    CURSOR_NEXT, LOADED, LOOKUP_NEXT, Layout, LookupFn, MOVE_ACTIVE, MOVING, NPC_COMMON_TYPE,
    SEALED, SKIP_UPDATE, SURVIVORS_FUSE, TRANSFORM_TYPE, layout,
};

/// Asks the registered lookup; `None` for the game's own answer.
pub(super) fn ask(call: &SurvivorLookup) -> Option<u32> {
    let registered = SEALED.get()?.lookup?;
    if crate::host::is_off(registered.owner) {
        return None;
    }
    let mut out = Entity(0);
    let user = core::ptr::with_exposed_provenance_mut(registered.user);
    // SAFETY: the registered `SurvivorLookupFn` lives in the mod's DLL, which
    // stays loaded for the process, and takes the `user` word it registered;
    // `call` and `out` outlive the call.
    let answered = crate::profile::call(
        registered.owner,
        crate::profile::Feature::Survivors,
        || unsafe { (registered.callback)(call, user, &raw mut out) },
    );
    (answered != 0).then_some(out.0)
}

pub(super) fn call(
    survivor: u32,
    vanilla: u32,
    purpose: u32,
    entity: Entity,
    cell: [i64; 3],
) -> SurvivorLookup {
    SurvivorLookup {
        header: Header::of::<SurvivorLookup>(),
        survivor: enshwick::abi::SurvivorId::new(survivor),
        vanilla: Entity(vanilla),
        purpose,
        entity,
        cell,
        bed_of: None,
        token: core::ptr::null_mut(),
    }
}

pub(super) unsafe extern "system" fn lookup_link(bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: the game's own arguments to `npc.lookup`, passed with the rest
    // of its chain, at the chain's entry on the game's calling thread.
    unsafe { lookup(LOOKUP_NEXT.get(), bases, npc) }
}

pub(super) unsafe extern "system" fn cursor_link(bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: the game's own arguments to `npc.lookup_cursor`, passed with the
    // rest of its chain, at the chain's entry on the game's calling thread.
    unsafe { lookup(CURSOR_NEXT.get(), bases, npc) }
}

/// # Safety
///
/// - `next` is the rest of the chain of the hooked lookup, and `bases` and
///   `npc` are the arguments the game passed it.
/// - `npc` is null or readable for a `u32` survivor id for the call.
/// - The call runs at the chain's entry, on the thread the game called the
///   lookup on, so the loader's caller frame is the lookup's.
pub(super) unsafe fn lookup(next: LookupFn, bases: *mut u8, npc: *const u32) -> u32 {
    let Some(loader) = LOADED.get() else {
        // SAFETY: the caller's contract makes `next` the rest of the chain
        // and `bases` and `npc` the game's own arguments to it.
        return unsafe { next(bases, npc) };
    };
    // Read the frame before the chain runs: a deeper hook takes its own.
    let frame = (loader.caller_frame)();
    // SAFETY: the caller's contract makes `next` the rest of the chain and
    // `bases` and `npc` the game's own arguments to it.
    let found = unsafe { next(bases, npc) };
    if npc.is_null() || frame.rsp == 0 {
        return found;
    }
    let (Some(layout), Some(engine)) = (layout(loader), crate::ecs::resolved()) else {
        return found;
    };
    // SAFETY: by the caller's contract the frame was taken at this chain's
    // entry on this thread, so its non-zero `rsp` (checked earlier) points at
    // the lookup's return address on the live stack.
    let Ok(caller) = usize::try_from(unsafe { frame.return_address() }) else {
        return found;
    };
    // SAFETY: the caller's contract makes the non-null `npc` (checked earlier)
    // readable for the `u32` survivor id the game looks up.
    let survivor: u32 = unsafe { read_at(npc.cast(), 0) };
    let answered = SURVIVORS_FUSE.run("a survivor lookup", || {
        // SAFETY: `frame` is the caller frame of the running lookup, taken at
        // its chain's entry on this thread, and `caller` its return address.
        unsafe { resolve(loader, layout, engine, found, survivor, caller, &frame) }
    });
    answered.flatten().unwrap_or(found)
}

/// # Safety
///
/// - `frame` is the caller frame of the running lookup, taken at its chain's
///   entry on this thread, and `caller` is its return address.
/// - The lookup is still running, so the caller's stack and the registers in
///   `frame` hold what the caller at `caller` keeps there.
#[expect(clippy::too_many_lines, reason = "one arm per caller")]
pub(super) unsafe fn resolve(
    loader: &Loader,
    layout: &Layout,
    engine: &Engine,
    found: u32,
    survivor: u32,
    caller: usize,
    frame: &enshwick::abi::CallerFrame,
) -> Option<u32> {
    let c = &layout.callers;
    let o = &layout.offsets;
    let hosting = crate::world::hosting() || loader.side == Side::SERVER;
    let client_site = c.client_cursor != 0 && caller == c.client_cursor;
    let cursor = client_site || c.cursor.contains(&caller);
    if cursor {
        if !hosting && !client_site {
            return None;
        }
        return ask(&call(
            survivor,
            found,
            SurvivorLookup::CURSOR,
            Entity(0),
            [0; 3],
        ));
    }
    if !hosting {
        return None;
    }
    let stack = usize::try_from(frame.stack()).ok()?;
    if c.unique.contains(&caller) {
        // SAFETY: the caller is the uniqueness pass (return address matched
        // earlier), running while the lookup runs; it keeps its query context
        // pointer at the table's `survivors.unique_context` in its stack.
        let ctx = unsafe { crate::memory::read_ptr::<u64>(stack, o.unique_context) };
        // SAFETY: `ctx` is null or the uniqueness pass's live query context,
        // used on its system's thread while the pass runs.
        let me = unsafe { engine.entity(ctx) };
        if me == 0 {
            return None;
        }
        // SAFETY: the caller is the uniqueness pass, running while the lookup
        // runs; it keeps the entity's saved state pointer at the table's
        // `survivors.unique_saved_state` in its stack.
        let saved =
            unsafe { crate::memory::read_ptr::<u8>(stack, o.unique_saved_state) }.cast_const();
        // SAFETY: the caller is the uniqueness pass, running while the lookup
        // runs; it keeps the entity's transform pointer at the table's
        // `survivors.unique_transform` in its stack.
        let transform =
            unsafe { crate::memory::read_ptr::<u8>(stack, o.unique_transform) }.cast_const();
        // SAFETY: `saved` and `transform` are null or the entity's live
        // components, which the uniqueness pass holds while it runs.
        let home = unsafe { home(saved, transform) };
        SKIP_UPDATE.with(|skip| skip.set(None));
        let answer = ask(&call(
            survivor,
            found,
            SurvivorLookup::UNIQUENESS,
            Entity(me),
            home,
        ))?;
        SKIP_UPDATE
            .with(|skip| skip.set((answer == me && found != 0 && me != found).then_some(survivor)));
        return Some(answer);
    }
    if caller == c.spawn {
        let request = usize::try_from(frame.r14).ok()?;
        // SAFETY: the caller is the staff spawn (return address matched
        // earlier), which holds its live spawn request in `r14` while the
        // lookup runs; the request keeps its target position at the table's
        // `survivors.spawn_target`.
        let target: [i64; 3] = unsafe { crate::memory::read(request, o.spawn_target) };
        let answer = ask(&call(
            survivor,
            found,
            SurvivorLookup::SPAWN,
            Entity(0),
            cell_of_fixed(target),
        ))?;
        if answer != 0 {
            *MOVING.relock() = Some((survivor, answer, Instant::now()));
            MOVE_ACTIVE.store(true, Ordering::Release);
        }
        return Some(answer);
    }
    if caller == c.bed {
        // SAFETY: the caller is the bed binding (return address matched
        // earlier), running while the lookup runs; it keeps the bed's transform
        // pointer at the table's `survivors.bed_transform` in its stack.
        let transform =
            unsafe { crate::memory::read_ptr::<u8>(stack, o.bed_transform) }.cast_const();
        if transform.is_null() {
            return None;
        }
        // SAFETY: `transform` is the bed's non-null live transform (checked
        // earlier), which starts with its 32.32 fixed-point position.
        let position: [i64; 3] = unsafe { read_at(transform, 0) };
        return ask(&call(
            survivor,
            found,
            SurvivorLookup::BED_OWNER,
            Entity(0),
            cell_of_fixed(position),
        ));
    }
    if caller == c.bed_init {
        return ask(&call(
            survivor,
            found,
            SurvivorLookup::BED_BINDING,
            Entity(0),
            [0; 3],
        ));
    }
    let roaming = caller == c.roaming;
    if !roaming && !c.assign.contains(&caller) {
        return None;
    }
    let (ctx, entity) = if roaming {
        // SAFETY: the caller is the idle behaviour (return address matched
        // earlier), running while the lookup runs; it keeps its query context
        // pointer at the table's `survivors.roaming_context` in its stack.
        let ctx = unsafe { crate::memory::read_ptr::<u64>(stack, o.roaming_context) };
        // SAFETY: `ctx` is null or the idle behaviour's live query context,
        // used on its system's thread while it runs.
        (ctx, unsafe { engine.entity(ctx) })
    } else {
        // SAFETY: the caller is a bed assignment (return address matched
        // earlier), running while the lookup runs; it keeps its query context
        // pointer at the table's `survivors.assign_context` in its stack.
        let ctx = unsafe { crate::memory::read_ptr::<u64>(stack, o.assign_context) };
        // SAFETY: the caller is a bed assignment, running while the lookup
        // runs; it keeps its targets pointer at the table's
        // `survivors.assign_targets` in its stack.
        let targets =
            unsafe { crate::memory::read_ptr::<u8>(stack, o.assign_targets) }.cast_const();
        if targets.is_null() {
            return None;
        }
        // SAFETY: `targets` is the assignment's non-null live targets record
        // (checked earlier), which keeps the selected bed's `u32` entity at the
        // table's `survivors.assign_bed`.
        (ctx, unsafe { read_at::<u32>(targets, o.assign_bed) })
    };
    // SAFETY: `ctx` is null or the caller's live query context, used on its
    // system's thread while it runs.
    let cell = unsafe { position_of(engine, ctx, Entity(entity)) }?;
    let purpose = if roaming {
        SurvivorLookup::ROAMING
    } else if caller == c.assign[0] {
        SurvivorLookup::ASSIGN_BED
    } else {
        SurvivorLookup::PREVIOUS_OCCUPANT
    };
    let mut lookup = call(survivor, found, purpose, Entity(entity), cell);
    let mut token = BedToken {
        engine,
        ctx,
        offset: o.npc_common_bed,
    };
    lookup.bed_of = Some(bed_of);
    lookup.token = (&raw mut token).cast();
    ask(&lookup)
}

pub(super) struct BedToken<'e> {
    engine: &'e Engine,
    ctx: *mut u64,
    offset: usize,
}

/// The bed `entity` sleeps in: its `NpcCommon.bedId`.
pub(super) unsafe extern "system" fn bed_of(token: *mut c_void, entity: Entity) -> Entity {
    let kind = NPC_COMMON_TYPE.load(Ordering::Acquire);
    if token.is_null() || kind == u16::MAX || entity == Entity(0) {
        return Entity(0);
    }
    // SAFETY: the non-null `token` (checked earlier) is the `BedToken` that
    // `resolve` handed the mod with this function, alive on its stack until
    // the mod's lookup returns, which is the only time the mod may call this.
    let token = unsafe { &*token.cast::<BedToken<'_>>() };
    // SAFETY: `token.ctx` is null or the running caller's live query context,
    // used on its system's thread while the lookup runs.
    let common = unsafe { token.engine.component(token.ctx, kind, entity) };
    if common.is_null() {
        return Entity(0);
    }
    // SAFETY: `common` is the entity's non-null live `NpcCommon` (checked
    // earlier), which keeps its `u32` bed id at the table's
    // `survivors.npc_common_bed`.
    unsafe { read_at(common, token.offset) }
}

/// Where an entity stands, in whole cells, once the transform type is known.
///
/// # Safety
///
/// - `ctx` is null or a live query context, used on its system's thread
///   while the system runs.
pub(super) unsafe fn position_of(
    engine: &Engine,
    ctx: *mut u64,
    entity: Entity,
) -> Option<[i64; 3]> {
    let kind = TRANSFORM_TYPE.load(Ordering::Acquire);
    if kind == u16::MAX || ctx.is_null() || entity == Entity(0) {
        return None;
    }
    // SAFETY: the caller's contract makes the non-null `ctx` (checked earlier)
    // a live query context on its system's thread.
    let transform = unsafe { engine.component(ctx, kind, entity) };
    if transform.is_null() {
        return None;
    }
    // SAFETY: `transform` is the entity's non-null live `CurrentTransform`
    // (checked earlier), which starts with its 32.32 fixed-point position.
    Some(cell_of_fixed(unsafe { read_at(transform, 0) }))
}

/// A survivor's home: its saved spawn position, or where it stands.
///
/// # Safety
///
/// - `saved` is null or the survivor's live saved state, readable for its
///   three `f32` spawn coordinates at offset 0.
/// - `transform` is null or the survivor's live transform, readable for its
///   three `i64` position words at offset 0.
/// - Nothing writes either during the call.
pub(super) unsafe fn home(saved: *const u8, transform: *const u8) -> [i64; 3] {
    if !saved.is_null() {
        // SAFETY: the caller's contract makes the non-null `saved` (checked
        // earlier) readable for its spawn position in metres at offset 0.
        let spawn: [f32; 3] = unsafe { read_at(saved, 0) };
        if spawn.iter().any(|&v| v.to_bits() != 0) {
            return enshwick::altars::cell_of_metres(spawn.map(f64::from));
        }
    }
    if transform.is_null() {
        return [i64::MIN; 3];
    }
    // SAFETY: the caller's contract makes the non-null `transform` (checked
    // earlier) readable for its 32.32 fixed-point position at offset 0.
    cell_of_fixed(unsafe { read_at(transform, 0) })
}
