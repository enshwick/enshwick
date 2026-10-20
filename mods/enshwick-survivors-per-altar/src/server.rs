//! Survivors per altar on the server.
//!
//! The base system keeps one map from a survivor's id to one entity, and its
//! callers decide what a survivor does from the answer. The hook on that lookup
//! answers some callers with the copy that belongs to the altar they work in
//! (see `copies`):
//!
//! - `unique_npc_spawns` files each copy in its slot; a second copy in a slot,
//!   or a copy outside every zone, gets another entity as the answer, so vanilla
//!   destroys it;
//! - the spawn path is told the slot's copy, which vanilla then moves, or no
//!   copy, so it spawns one; the move command reaches every copy of a survivor,
//!   so the move system skips all but the target for a moment;
//! - `npc_bed` gets the copy of the bed's altar, so each copy keeps its bed;
//!   `npc_bed_init` and the cursor check get no copy while copies exist, which
//!   leaves the binding to `npc_bed` and skips a distance check;
//! - each NPC's own bed check fails for a bed at another altar, so the game
//!   clears a bed a copy inherited or kept.
//!
//! Callers are told apart by the return address of the lookup, from the caller
//! frame the loader records; their frames hold the components of the entity
//! they work on. Every other caller gets the vanilla answer.
//!
//! A client links this half too, for the games it hosts: every caller but the
//! cursor is a server system, which runs only then, and the cursor's lookup is
//! answered for both of the host's worlds here (see `cursor`). What a game
//! taught the hooks is dropped when it stops.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::{CallerFrame, Side};
use enshwick_loader_api::ecs::{Ecs, Entity};
use enshwick_loader_api::events::Shutdown;
use enshwick_loader_api::{Load, Next, Off};

use crate::copies::{Copies, Verdict, Zone};
use crate::zones_service::{self as service, AltarZone, ZonesService};

type ComponentRead =
    unsafe extern "system" fn(ctx: *mut u64, kind: *const u16, entity: u32) -> *const u8;

const REFRESH: Duration = Duration::from_secs(2);
const FORGET_AFTER: Duration = Duration::from_secs(3);
const MOVE_WINDOW: Duration = Duration::from_secs(2);
const TYPE_SEARCH: u16 = 4096;
const MAX_ZONES: usize = 256;

/// Where the lookup returns to, per caller.
#[derive(Debug)]
struct Callers {
    spawn: u64,
    unique: [u64; 2],
    bed: u64,
    bed_init: u64,
    cursor: [u64; 2],
    /// The cursor site the client alone is quieted at; 0 on the server.
    client_cursor: u64,
    assign: [u64; 2],
    roaming: u64,
}

/// Offsets in the callers' frames and the NPC behaviour context.
#[derive(Debug)]
struct Offsets {
    unique_context: u64,
    unique_saved_state: u64,
    unique_transform: u64,
    bed_context: u64,
    bed_transform: u64,
    bed_npc_common_type: u64,
    npc_common_bed: usize,
    assign_context: u64,
    assign_targets: u64,
    assign_bed: usize,
    roaming_context: u64,
    spawn_target: usize,
    move_survivor: usize,
    npc_transform: usize,
    npc_saved_state: usize,
    npc_common: usize,
    npc_state: usize,
    spawn_template: usize,
    spawn_used_item: usize,
    spawn_ammo_item: usize,
    spawn_consume: usize,
}

#[derive(Debug)]
struct Engine {
    ecs: Ecs,
    component_read: ComponentRead,
    callers: Callers,
    offsets: Offsets,
    zones: &'static ZonesService,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// The type index of `CurrentTransform`, learned from `npc_bed`.
static TRANSFORM_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);
/// The type index of `NpcCommon`, read from `npc_bed`'s iterator.
static NPC_COMMON_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);

#[derive(Debug, Default)]
struct Shared {
    copies: Copies,
    /// When the uniqueness pass last saw each copy.
    seen: BTreeMap<u32, Instant>,
    refreshed: Option<Instant>,
    /// The survivor and copy a summoning staff is moving, and since when.
    moving: Option<(u32, u32, Instant)>,
}

static SHARED: Mutex<Option<Shared>> = Mutex::new(None);

thread_local! {
    /// Set when the uniqueness pass keeps a copy that is not the record: the
    /// record update that follows on this thread must not take the copy's base
    /// and happiness.
    static SKIP_UPDATE: Cell<bool> = const { Cell::new(false) };
}

type LookupFn = unsafe extern "system" fn(*mut u8, *const u32) -> u32;
type MoveFn = unsafe extern "system" fn(*mut *mut u64, usize, usize, usize) -> usize;
type UpdateFn = unsafe extern "system" fn(*mut u8, *const u32, usize, usize) -> usize;
type BedCheckFn = unsafe extern "system" fn(*mut u8) -> u8;
type SpawnFn = unsafe extern "system" fn(*mut u64, *mut u8) -> u8;

enshwick_loader_api::detour! {
    static LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 = lookup,
        flags = enshwick_loader_api::abi::HookFlags::CALLER_FRAME;
}
enshwick_loader_api::detour! {
    static CURSOR_LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 = lookup,
        flags = enshwick_loader_api::abi::HookFlags::CALLER_FRAME;
}
enshwick_loader_api::detour! {
    static SPAWN: unsafe extern "system" fn(ctx: *mut u64, request: *mut u8) -> u8 = spawn;
}
enshwick_loader_api::detour! {
    static MOVE: unsafe extern "system" fn(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize = move_npc;
}
enshwick_loader_api::detour! {
    static UPDATE: unsafe extern "system" fn(bases: *mut u8, npc: *const u32, state: usize, flag: usize) -> usize = update;
}
enshwick_loader_api::detour! {
    static BED_CHECK: unsafe extern "system" fn(ctx: *mut u8) -> u8 = bed_check;
}

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
pub(crate) unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: forwarded from the caller; `F` is a function pointer.
    unsafe { core::mem::transmute_copy(&address) }
}

/// Links every server hook.
pub(crate) fn link(load: &mut Load<'_>, staffs: bool) -> Result<(), Off> {
    let client = load.host().build().side == Side::CLIENT;
    let symbol = |name| load.symbol(name);
    let constant = |name| load.constant(name);
    let size = |name| {
        load.constant(name)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    };
    let callers = Callers {
        spawn: symbol("survivors.spawn_return")?,
        unique: [
            symbol("survivors.unique_return_a")?,
            symbol("survivors.unique_return_b")?,
        ],
        bed: symbol("survivors.bed_return")?,
        bed_init: symbol("survivors.bed_init_return")?,
        cursor: [
            symbol("survivors.cursor_return_a")?,
            symbol("survivors.cursor_return_b")?,
        ],
        client_cursor: if client {
            symbol("survivors.cursor_return")?
        } else {
            0
        },
        assign: [
            symbol("survivors.assign_return_a")?,
            symbol("survivors.assign_return_b")?,
        ],
        roaming: symbol("survivors.roaming_return")?,
    };
    let offsets = Offsets {
        unique_context: constant("survivors.unique_context")?,
        unique_saved_state: constant("survivors.unique_saved_state")?,
        unique_transform: constant("survivors.unique_transform")?,
        bed_context: constant("survivors.bed_context")?,
        bed_transform: constant("survivors.bed_transform")?,
        bed_npc_common_type: constant("survivors.bed_npc_common_type")?,
        npc_common_bed: size("survivors.npc_common_bed")?,
        assign_context: constant("survivors.assign_context")?,
        assign_targets: constant("survivors.assign_targets")?,
        assign_bed: size("survivors.assign_bed")?,
        roaming_context: constant("survivors.roaming_context")?,
        spawn_target: size("survivors.spawn_target")?,
        move_survivor: size("survivors.move_survivor")?,
        npc_transform: size("survivors.npc_transform")?,
        npc_saved_state: size("survivors.npc_saved_state")?,
        npc_common: size("survivors.npc_common")?,
        npc_state: size("survivors.npc_state")?,
        spawn_template: size("survivors.spawn_template")?,
        spawn_used_item: size("survivors.spawn_used_item")?,
        spawn_ammo_item: size("survivors.spawn_ammo_item")?,
        spawn_consume: size("survivors.spawn_consume")?,
    };
    // SAFETY: `ZonesService` is the table `flame-altar-zones` publishes.
    let zones: &'static ZonesService = unsafe { load.service(service::NAME, service::VERSION)? };
    let component_read = symbol("ecs.component_read")?;
    let engine = Engine {
        ecs: Ecs::resolve(load)?,
        // SAFETY: the loader pins this id to the component read.
        component_read: unsafe { function(component_read) },
        callers,
        offsets,
        zones,
    };
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Shared::default());
    }
    let _first = ENGINE.set(engine);
    load.hook(&LOOKUP, load.symbol("npc.lookup")?)?;
    load.hook(&CURSOR_LOOKUP, load.symbol("npc.lookup_cursor")?)?;
    load.hook(&MOVE, load.symbol("npc.move")?)?;
    load.hook(&UPDATE, load.symbol("npc.update")?)?;
    load.hook(&BED_CHECK, load.symbol("npc.bed_check")?)?;
    if staffs {
        load.hook(&SPAWN, load.symbol("survivors.spawn")?)?;
    }
    if client {
        // Entities belong to one hosted game; the next starts clean.
        load.on::<Shutdown>(|()| forget())?;
    }
    Ok(())
}

/// Drops what a hosted game taught the hooks.
fn forget() {
    if let Ok(mut shared) = SHARED.lock() {
        *shared = Some(Shared::default());
    }
    TRANSFORM_TYPE.store(u16::MAX, Ordering::Release);
    NPC_COMMON_TYPE.store(u16::MAX, Ordering::Release);
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: u64, offset: u64) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { (base.wrapping_add(offset) as *const T).read_unaligned() }
}

fn lookup(next: Next<LookupFn>, bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let found = unsafe { next.get()(bases, npc) };
    let (Some(engine), Some(host)) = (ENGINE.get(), enshwick_loader_api::host()) else {
        return found;
    };
    if npc.is_null() {
        return found;
    }
    let frame = host.caller_frame();
    // SAFETY: the frame was taken at entry of this chain.
    let caller = unsafe { frame.return_address() };
    // SAFETY: `npc` points at the survivor id the caller looks up.
    let npc = unsafe { npc.read() };
    // SAFETY: the caller's frame, as the loader recorded it.
    unsafe { resolve(engine, found, npc, caller, &frame) }
}

/// # Safety
///
/// `frame` is the caller frame of the running lookup.
unsafe fn resolve(engine: &Engine, found: u32, npc: u32, caller: u64, frame: &CallerFrame) -> u32 {
    let c = &engine.callers;
    let spawn = caller == c.spawn;
    let unique = c.unique.contains(&caller);
    let bed = caller == c.bed;
    let assign = c.assign.contains(&caller);
    let roaming = caller == c.roaming;
    let client_site = c.client_cursor != 0 && caller == c.client_cursor;
    let cursor = client_site || c.cursor.contains(&caller);
    let quiet = caller == c.bed_init;
    if !(spawn || unique || bed || assign || roaming || quiet || cursor) {
        return found;
    }
    let hosting = enshwick_loader_api::host().is_some_and(|host| host.hosting());
    if cursor && !hosting {
        return crate::cursor::answer(false, client_site, 0, found);
    }
    let Ok(mut slot) = SHARED.lock() else {
        return found;
    };
    let Some(shared) = slot.as_mut() else {
        return found;
    };
    refresh(engine, shared);
    if cursor {
        let copies = shared.copies.copies_of(npc).len();
        return crate::cursor::answer(true, client_site, copies, found);
    }
    let stack = frame.stack();
    let o = &engine.offsets;

    if unique {
        // SAFETY: the uniqueness pass keeps its context, saved state and
        // transform pointers at these offsets in its frame.
        return unsafe { file_copy(engine, shared, found, npc, stack) };
    }
    if spawn {
        // SAFETY: the spawn request lies in the caller's `r14`, its target
        // transform at this offset.
        let target: [i64; 3] = unsafe { read_at(frame.r14, o.spawn_target as u64) };
        if let Some(copy) = shared.copies.for_spawn(npc, cells_fixed(target), found) {
            shared.moving = Some((npc, copy, Instant::now()));
            return copy;
        }
        enshwick_loader_api::info!(
            "survivor {npc:#010x}: a new copy at {:?}",
            cells_fixed(target)
        );
        return 0;
    }
    if bed {
        // SAFETY: `npc_bed` keeps its context here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.bed_context) };
        // SAFETY: and the bed's transform here.
        let transform: *const u8 = unsafe { read_at(stack, o.bed_transform) };
        // SAFETY: `npc_bed`'s live context and the bed's transform.
        unsafe { calibrate(engine, ctx, transform) };
    }
    if shared.copies.copies_of(npc).len() <= 1 {
        return found;
    }
    if bed {
        // SAFETY: `npc_bed`'s frame holds its context, the bed transform and the
        // `NpcCommon` type index pointer.
        return unsafe { bed_owner(engine, shared, npc, stack, found) };
    }
    if assign || roaming {
        // SAFETY: the assign and roaming frames hold their contexts and targets.
        return unsafe { assign_or_roam(engine, shared, npc, caller, stack, found) };
    }
    0
}

/// # Safety
///
/// `stack` is the uniqueness pass's frame.
unsafe fn file_copy(engine: &Engine, shared: &mut Shared, found: u32, npc: u32, stack: u64) -> u32 {
    let o = &engine.offsets;
    // SAFETY: the pass keeps its context here.
    let ctx: *mut u64 = unsafe { read_at(stack, o.unique_context) };
    // SAFETY: a live query context.
    let Entity(me) = unsafe { engine.ecs.entity(ctx) };
    if me == 0 {
        return found;
    }
    // SAFETY: the pass keeps its saved state and transform pointers here.
    let saved: *const u8 = unsafe { read_at(stack, o.unique_saved_state) };
    // SAFETY: as above.
    let transform: *const u8 = unsafe { read_at(stack, o.unique_transform) };
    // SAFETY: the copy's components.
    let home = unsafe { home(saved, transform) };
    SKIP_UPDATE.with(|skip| skip.set(false));
    shared.seen.insert(me, Instant::now());
    // Right after a load the zones are not known yet: keep every copy until
    // they are, or each would look like one outside the zones.
    if !shared.copies.knows_altars() {
        SKIP_UPDATE.with(|skip| skip.set(found != 0 && me != found));
        return if found == 0 { 0 } else { me };
    }
    let original = if found == 0 { me } else { found };
    match shared.copies.observe(npc, me, home, original) {
        Verdict::Keep if found == 0 => 0,
        Verdict::Keep => {
            SKIP_UPDATE.with(|skip| skip.set(me != found));
            me
        }
        Verdict::Remove => {
            let holder = shared.copies.at(npc, home, original);
            enshwick_loader_api::info!(
                "survivor {npc:#010x}: copy {me} removed, {holder} keeps the slot"
            );
            if holder == me { original } else { holder }
        }
    }
}

/// The copy a bed belongs to: only its own altar's copy, or nobody, since a
/// copy of another altar would walk across the base to it.
///
/// # Safety
///
/// `stack` is `npc_bed`'s frame.
unsafe fn bed_owner(engine: &Engine, shared: &Shared, npc: u32, stack: u64, found: u32) -> u32 {
    let o = &engine.offsets;
    // SAFETY: `npc_bed` keeps the bed transform pointer and the `NpcCommon`
    // type pointer here.
    let transform: u64 = unsafe { read_at(stack, o.bed_transform) };
    // SAFETY: as above.
    let kind_ptr: u64 = unsafe { read_at(stack, o.bed_npc_common_type) };
    // SAFETY: the bed's position, 32.32 fixed point.
    let position: [i64; 3] = unsafe { read_at(transform, 0) };
    // SAFETY: the type index the pointer names.
    let kind: u16 = unsafe { read_at(kind_ptr, 0) };
    NPC_COMMON_TYPE.store(kind, Ordering::Release);
    let Some(altar) = shared.copies.altar_at(cells_fixed(position)) else {
        return found;
    };
    shared.copies.slot(npc, altar).unwrap_or(0)
}

/// # Safety
///
/// `stack` is the frame of `player_assign_npc` or `npc_active_input`.
unsafe fn assign_or_roam(
    engine: &Engine,
    shared: &Shared,
    npc: u32,
    caller: u64,
    stack: u64,
    found: u32,
) -> u32 {
    let o = &engine.offsets;
    let roaming = caller == engine.callers.roaming;
    let (ctx, entity) = if roaming {
        // SAFETY: `npc_active_input` keeps its context here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.roaming_context) };
        // SAFETY: a live query context.
        (ctx, unsafe { engine.ecs.entity(ctx) }.0)
    } else {
        // SAFETY: `player_assign_npc` keeps its context and targets here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.assign_context) };
        // SAFETY: as above.
        let targets: u64 = unsafe { read_at(stack, o.assign_targets) };
        // SAFETY: the selected target, the bed, at this offset.
        (ctx, unsafe { read_at::<u32>(targets, o.assign_bed as u64) })
    };
    // SAFETY: a live context and an entity of it.
    let Some(position) = (unsafe { position_of(engine, ctx, entity) }) else {
        return found;
    };
    if roaming {
        return shared.copies.at(npc, position, found);
    }
    if caller == engine.callers.assign[0] {
        let answer = shared
            .copies
            .altar_at(position)
            .and_then(|a| shared.copies.slot(npc, a))
            .unwrap_or(0);
        enshwick_loader_api::info!("survivor {npc:#010x}: staff gives bed {entity} to {answer}");
        return answer;
    }
    // The previous occupant: the copy that really sleeps in this bed.
    // SAFETY: a live context.
    unsafe { sleeper(engine, shared, npc, ctx, entity) }.unwrap_or(0)
}

/// The copy of `npc` whose `NpcCommon.bedId` is `bed`.
///
/// # Safety
///
/// `ctx` is a live query context.
unsafe fn sleeper(
    engine: &Engine,
    shared: &Shared,
    npc: u32,
    ctx: *mut u64,
    bed: u32,
) -> Option<u32> {
    let kind = NPC_COMMON_TYPE.load(Ordering::Acquire);
    if kind == u16::MAX || bed == 0 {
        return None;
    }
    shared
        .copies
        .copies_of(npc)
        .into_iter()
        .map(|(e, _)| e)
        .find(|&e| {
            // SAFETY: reading a component of an entity in the live context.
            let common = unsafe { (engine.component_read)(ctx, &raw const kind, e) };
            // SAFETY: `NpcCommon.bedId` at this offset.
            !common.is_null()
                && unsafe {
                    read_at::<u32>(common.addr() as u64, engine.offsets.npc_common_bed as u64)
                } == bed
        })
}

/// Learns the type index of `CurrentTransform` from a bed whose transform the
/// caller already holds.
///
/// # Safety
///
/// `ctx` is `npc_bed`'s live context and `transform` the bed's transform.
unsafe fn calibrate(engine: &Engine, ctx: *mut u64, transform: *const u8) {
    if TRANSFORM_TYPE.load(Ordering::Acquire) != u16::MAX || ctx.is_null() || transform.is_null() {
        return;
    }
    // SAFETY: a live query context.
    let Entity(bed) = unsafe { engine.ecs.entity(ctx) };
    if bed == 0 {
        return;
    }
    for kind in 0..TYPE_SEARCH {
        // SAFETY: reading a component of the bed in its live context.
        if unsafe { (engine.component_read)(ctx, &raw const kind, bed) } == transform {
            TRANSFORM_TYPE.store(kind, Ordering::Release);
            enshwick_loader_api::info!("CurrentTransform is component type {kind}");
            return;
        }
    }
}

/// Where an entity stands, in whole cells, once the transform type is known.
///
/// # Safety
///
/// `ctx` is a live query context.
unsafe fn position_of(engine: &Engine, ctx: *mut u64, entity: u32) -> Option<[i64; 3]> {
    let kind = TRANSFORM_TYPE.load(Ordering::Acquire);
    if kind == u16::MAX || ctx.is_null() || entity == 0 {
        return None;
    }
    // SAFETY: reading a component of an entity in the live context.
    let transform = unsafe { (engine.component_read)(ctx, &raw const kind, entity) };
    if transform.is_null() {
        return None;
    }
    // SAFETY: a transform starts with its position.
    Some(cells_fixed(unsafe { read_at(transform.addr() as u64, 0) }))
}

/// Altars move only when someone builds: read their zones now and then, and
/// let go of copies the uniqueness pass no longer sees.
fn refresh(engine: &Engine, shared: &mut Shared) {
    if shared.refreshed.is_some_and(|at| at.elapsed() < REFRESH) {
        return;
    }
    shared.refreshed = Some(Instant::now());
    let mut buffer = [AltarZone::default(); MAX_ZONES];
    let count = (engine.zones.zones)(buffer.as_mut_ptr(), MAX_ZONES).min(MAX_ZONES);
    if count > 0 {
        shared.copies.set_altars(
            buffer[..count]
                .iter()
                .map(|z| (u64::from(z.altar), Zone::new(z.min, z.max))),
        );
    }
    let gone: Vec<u32> = shared
        .seen
        .iter()
        .filter(|(_, at)| at.elapsed() > FORGET_AFTER)
        .map(|(&e, _)| e)
        .collect();
    for entity in gone {
        shared.seen.remove(&entity);
        shared.copies.forget(entity);
    }
}

/// A copy's home: its saved spawn position, or where it stands.
///
/// # Safety
///
/// Non-null pointers are the copy's saved state and transform.
#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates fit in i64 cells"
)]
unsafe fn home(saved: *const u8, transform: *const u8) -> [i64; 3] {
    if !saved.is_null() {
        // SAFETY: the saved state starts with the spawn position in metres.
        let spawn: [f32; 3] = unsafe { saved.cast::<[f32; 3]>().read_unaligned() };
        if spawn.iter().any(|&v| v.to_bits() != 0) {
            return spawn.map(|m| (f64::from(m) * 2.0).floor() as i64);
        }
    }
    if transform.is_null() {
        return [i64::MIN; 3];
    }
    // SAFETY: a transform starts with its position.
    cells_fixed(unsafe { transform.cast::<[i64; 3]>().read_unaligned() })
}

/// 32.32 fixed-point metres to whole 0.5 m cells.
fn cells_fixed(position: [i64; 3]) -> [i64; 3] {
    position.map(|v| v >> 31)
}

/// Keeps the survivor record on the original: skips the update right after the
/// uniqueness pass kept a copy.
fn update(
    next: Next<UpdateFn>,
    bases: *mut u8,
    npc: *const u32,
    state: usize,
    flag: usize,
) -> usize {
    if SKIP_UPDATE.with(|skip| skip.replace(false)) {
        return 0;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(bases, npc, state, flag) }
}

/// Lets only the copy a summoning staff moves take the move command.
fn spawn(next: Next<SpawnFn>, ctx: *mut u64, request: *mut u8) -> u8 {
    if let Some(engine) = ENGINE.get()
        && !request.is_null()
    {
        // SAFETY: the game's spawn request, before the spawn reads it.
        unsafe { consume_copy_staff(engine, request) };
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(ctx, request) }
}

/// Marks a copy staff's use at an altar as consuming, so the game takes one
/// staff in the same transaction as the spawn or move, and only when that
/// works. The staff's effect spawn, which carries the same item, is left
/// alone, and so is a use outside every altar zone, where the copy would not
/// stay.
///
/// # Safety
///
/// `request` is the live spawn request.
unsafe fn consume_copy_staff(engine: &Engine, request: *mut u8) {
    let o = &engine.offsets;
    let base = request as u64;
    // SAFETY: the request holds the used item's id here.
    let item: u32 = unsafe { read_at(base, o.spawn_used_item as u64) };
    let Some(staff) = crate::staffs::copy_staff(item) else {
        return;
    };
    // SAFETY: and the ammunition's id here, the template and the target.
    let ammo: u32 = unsafe { read_at(base, o.spawn_ammo_item as u64) };
    // SAFETY: as above.
    let template: [u8; 16] = unsafe { read_at(base, o.spawn_template as u64) };
    if ammo != 0 || template != staff.template.0 {
        return;
    }
    // SAFETY: as above.
    let target: [i64; 3] = unsafe { read_at(base, o.spawn_target as u64) };
    let at = cells_fixed(target);
    let Ok(mut slot) = SHARED.lock() else {
        return;
    };
    let Some(shared) = slot.as_mut() else {
        return;
    };
    refresh(engine, shared);
    if shared.copies.altar_at(at).is_none() {
        enshwick_loader_api::info!(
            "copy staff of {} used outside every altar zone: kept",
            staff.key
        );
        return;
    }
    // SAFETY: the request's consume flag, which the spawn reads next.
    unsafe { request.wrapping_add(o.spawn_consume).write(1) };
    enshwick_loader_api::info!(
        "copy staff of {} used at {at:?}: taken on success",
        staff.key
    );
}

fn move_npc(next: Next<MoveFn>, ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize {
    if !ctx.is_null()
        && let Some(engine) = ENGINE.get()
        // SAFETY: the behaviour context of the moving NPC.
        && unsafe { skip_move(engine, ctx) }
    {
        return 0;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(ctx, b, c, d) }
}

/// Whether this NPC is another copy of the survivor a staff is moving.
///
/// # Safety
///
/// `ctx` is an NPC's behaviour context.
unsafe fn skip_move(engine: &Engine, ctx: *mut *mut u64) -> bool {
    let Ok(mut slot) = SHARED.lock() else {
        return false;
    };
    let Some(shared) = slot.as_mut() else {
        return false;
    };
    let Some((survivor, target, at)) = shared.moving else {
        return false;
    };
    if at.elapsed() > MOVE_WINDOW {
        shared.moving = None;
        return false;
    }
    // SAFETY: the first word is the ECS query context.
    let query: *mut u64 = unsafe { ctx.read() };
    // SAFETY: a live query context.
    let Entity(me) = unsafe { engine.ecs.entity(query) };
    // SAFETY: the context points at the NPC's `ActiveNpcState` here.
    let state: u64 = unsafe { read_at(ctx.addr() as u64, engine.offsets.move_survivor as u64) };
    // SAFETY: the state starts with the survivor id.
    let id: u32 = unsafe { read_at(state, 0) };
    if me == target {
        shared.moving = None;
        return false;
    }
    id == survivor
}

/// The game's check that an NPC's bed is still its own, made stricter: a bed at
/// another altar than the copy's home is not, and the game then clears it.
fn bed_check(next: Next<BedCheckFn>, ctx: *mut u8) -> u8 {
    // SAFETY: the rest of the chain takes the argument the game passed.
    let valid = unsafe { next.get()(ctx) };
    if valid == 0 || ctx.is_null() {
        return valid;
    }
    match ENGINE.get() {
        // SAFETY: the NPC's behaviour context.
        Some(engine) if unsafe { foreign_bed(engine, ctx.addr() as u64) } => 0,
        _ => valid,
    }
}

/// # Safety
///
/// `ctx` is an NPC's behaviour context.
unsafe fn foreign_bed(engine: &Engine, ctx: u64) -> bool {
    let o = &engine.offsets;
    // SAFETY: the behaviour context holds these component pointers.
    let common: u64 = unsafe { read_at(ctx, o.npc_common as u64) };
    // SAFETY: as above.
    let state: u64 = unsafe { read_at(ctx, o.npc_state as u64) };
    // SAFETY: as above.
    let saved: *const u8 = unsafe { read_at(ctx, o.npc_saved_state as u64) };
    // SAFETY: as above.
    let transform: *const u8 = unsafe { read_at(ctx, o.npc_transform as u64) };
    if common == 0 || state == 0 {
        return false;
    }
    // SAFETY: `NpcCommon.bedId`.
    let bed: u32 = unsafe { read_at(common, o.npc_common_bed as u64) };
    // SAFETY: the state starts with the survivor id.
    let npc: u32 = unsafe { read_at(state, 0) };
    // SAFETY: the first word of the behaviour context is the ECS context.
    let ecs: *mut u64 = unsafe { read_at(ctx, 0) };
    // SAFETY: a live context.
    let Some(bed_at) = (unsafe { position_of(engine, ecs, bed) }) else {
        return false;
    };
    // SAFETY: the NPC's components.
    let home = unsafe { home(saved, transform) };
    let Ok(slot) = SHARED.lock() else {
        return false;
    };
    let Some(shared) = slot.as_ref() else {
        return false;
    };
    if !shared.copies.knows_altars() || shared.copies.copies_of(npc).len() <= 1 {
        return false;
    }
    let (Some(mine), Some(beds)) = (shared.copies.altar_at(home), shared.copies.altar_at(bed_at))
    else {
        return false;
    };
    mine != beds
}
