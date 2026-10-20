//! The game's survivor lookups, bed checks, summoning staffs and client menu,
//! handed to the mods that answer them.
//!
//! The base system keeps one entity per survivor and its callers act on the
//! answer. Callers are told apart by the return address the caller frame
//! records; their frames hold the components of the entity they work on.

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick_loader_api::abi::{
    BedCheck, BedCheckFn, Header, HookFlags, Side, Status, Summon, SummonFn, SurvivorLookup,
    SurvivorLookupFn, SurvivorStatusFn,
};

use crate::chains::{LOADER, Link};
use crate::ecs::Engine;
use crate::host::Loader;

/// How long a staff's move reaches only the entity it was answered.
const MOVE_WINDOW: Duration = Duration::from_secs(2);
const TYPE_SEARCH: u16 = 4096;
/// `ui_npc_status`'s iterator: a header, `ClientActiveNpcState`,
/// `NetworkComfort`, `RenderTransform`, UI offsets, `UiPlayData`.
const UI_WORDS: usize = 6;
const UI_STATE: usize = 1;
const UI_PLAY_DATA: usize = 5;

#[derive(Debug, Clone, Copy)]
struct Callers {
    spawn: u64,
    unique: [u64; 2],
    bed: u64,
    bed_init: u64,
    cursor: [u64; 2],
    /// The cursor site a client alone asks about; 0 on the server.
    client_cursor: u64,
    assign: [u64; 2],
    roaming: u64,
}

/// Offsets in the callers' frames, the behaviour context and the requests.
#[derive(Debug, Clone, Copy)]
struct Offsets {
    unique_context: u64,
    unique_saved_state: u64,
    unique_transform: u64,
    bed_context: u64,
    bed_transform: u64,
    bed_npc_common_type: u64,
    npc_common_bed: u64,
    assign_context: u64,
    assign_targets: u64,
    assign_bed: u64,
    roaming_context: u64,
    spawn_target: u64,
    move_survivor: u64,
    npc_transform: u64,
    npc_saved_state: u64,
    npc_common: u64,
    npc_state: u64,
    spawn_template: u64,
    spawn_used_item: u64,
    spawn_ammo_item: u64,
    spawn_consume: u64,
}

#[derive(Debug, Clone, Copy)]
struct ItemMap {
    find_slot: FindSlotFn,
    loaded: usize,
    at: usize,
    bits: usize,
    capacity: usize,
    keys: usize,
    values: usize,
    count: usize,
}

#[derive(Debug, Clone, Copy)]
struct Menu {
    template: TemplateFn,
    state_status: usize,
    map_bits: usize,
    map_capacity: usize,
    map_keys: usize,
    map_values: usize,
    map_count: usize,
    record_size: usize,
    record_status: usize,
}

#[derive(Debug, Clone, Copy)]
struct Layout {
    callers: Callers,
    offsets: Offsets,
}

#[derive(Debug, Clone, Copy)]
struct Registered<F> {
    owner: usize,
    callback: F,
    user: usize,
}

static LOADED: OnceLock<&'static Loader> = OnceLock::new();
static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
static ITEM_MAP: OnceLock<Option<ItemMap>> = OnceLock::new();
static MENU: OnceLock<Option<Menu>> = OnceLock::new();

static LOOKUP: Mutex<Option<Registered<SurvivorLookupFn>>> = Mutex::new(None);
static BED_CHECKS: Mutex<Vec<Registered<BedCheckFn>>> = Mutex::new(Vec::new());
static SUMMONS: Mutex<Vec<Registered<SummonFn>>> = Mutex::new(Vec::new());
static ITEMS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());
static STATUS: Mutex<Option<Registered<SurvivorStatusFn>>> = Mutex::new(None);

#[derive(Debug)]
struct Sealed {
    lookup: Option<Registered<SurvivorLookupFn>>,
    bed_checks: Box<[Registered<BedCheckFn>]>,
    summons: Box<[Registered<SummonFn>]>,
    items: Box<[(u32, u32)]>,
    status: Option<Registered<SurvivorStatusFn>>,
}

static SEALED: OnceLock<Sealed> = OnceLock::new();

/// The type index of `CurrentTransform`, learned from the bed binding.
static TRANSFORM_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);
/// The type index of `NpcCommon`, read from the bed binding's frame.
static NPC_COMMON_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);
/// The survivor and entity a staff moves, and since when.
static MOVING: Mutex<Option<(u32, u32, Instant)>> = Mutex::new(None);
static MOVE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Each copy's survivor id, row order and status, and one survivor's statuses.
type StatusRows = (Vec<(u32, u32, u8)>, Vec<u8>);

thread_local! {
    /// Set when the uniqueness pass kept another entity than the game's: the
    /// record update that follows on this thread must not take it. It names
    /// the survivor, so an update of another survivor is never skipped.
    static SKIP_UPDATE: Cell<Option<u32>> = const { Cell::new(None) };
    /// The menu pass's rows and one survivor's statuses, kept between frames.
    static STATUS_ROWS: RefCell<StatusRows> = const { RefCell::new((Vec::new(), Vec::new())) };
}

static LOOKUP_NEXT: AtomicUsize = AtomicUsize::new(0);
static CURSOR_NEXT: AtomicUsize = AtomicUsize::new(0);
static MOVE_NEXT: AtomicUsize = AtomicUsize::new(0);
static UPDATE_NEXT: AtomicUsize = AtomicUsize::new(0);
static BED_CHECK_NEXT: AtomicUsize = AtomicUsize::new(0);
static SPAWN_NEXT: AtomicUsize = AtomicUsize::new(0);
static REGISTRY_NEXT: AtomicUsize = AtomicUsize::new(0);
static MENU_NEXT: AtomicUsize = AtomicUsize::new(0);

type LookupFn = unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32;
type MoveFn = unsafe extern "system" fn(*mut *mut u64, usize, usize, usize) -> usize;
type UpdateFn = unsafe extern "system" fn(*mut u8, *const u32, usize, usize) -> usize;
type BedCheckGameFn = unsafe extern "system" fn(*mut u8) -> u8;
type SpawnFn = unsafe extern "system" fn(*mut u64, *mut u8) -> u8;
type RegistryLoadFn = unsafe extern "system" fn(*mut u8);
type SystemFn = unsafe extern "system" fn(*mut u64);
type FindSlotFn = unsafe extern "system" fn(map: *mut u8, key: *const u32) -> u64;
type TemplateFn = unsafe extern "system" fn(ctx: *mut u64) -> *const [u8; 16];

fn symbol(loader: &Loader, name: &str) -> Option<u64> {
    loader.build.symbol(name).map(|rva| loader.base + rva)
}

fn constant(loader: &Loader, name: &str) -> Option<u64> {
    loader.build.constant(name)
}

fn size(loader: &Loader, name: &str) -> Option<usize> {
    constant(loader, name).and_then(|v| usize::try_from(v).ok())
}

unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: forwarded from the caller; `F` is a function pointer.
    unsafe { core::mem::transmute_copy(&address) }
}

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let s = |name: &str| symbol(loader, &format!("survivors.{name}"));
            let c = |name: &str| constant(loader, &format!("survivors.{name}"));
            let client = loader.side == Side::CLIENT;
            Some(Layout {
                callers: Callers {
                    spawn: s("spawn_return")?,
                    unique: [s("unique_return_a")?, s("unique_return_b")?],
                    bed: s("bed_return")?,
                    bed_init: s("bed_init_return")?,
                    cursor: [s("cursor_return_a")?, s("cursor_return_b")?],
                    client_cursor: if client { s("cursor_return")? } else { 0 },
                    assign: [s("assign_return_a")?, s("assign_return_b")?],
                    roaming: s("roaming_return")?,
                },
                offsets: Offsets {
                    unique_context: c("unique_context")?,
                    unique_saved_state: c("unique_saved_state")?,
                    unique_transform: c("unique_transform")?,
                    bed_context: c("bed_context")?,
                    bed_transform: c("bed_transform")?,
                    bed_npc_common_type: c("bed_npc_common_type")?,
                    npc_common_bed: c("npc_common_bed")?,
                    assign_context: c("assign_context")?,
                    assign_targets: c("assign_targets")?,
                    assign_bed: c("assign_bed")?,
                    roaming_context: c("roaming_context")?,
                    spawn_target: c("spawn_target")?,
                    move_survivor: c("move_survivor")?,
                    npc_transform: c("npc_transform")?,
                    npc_saved_state: c("npc_saved_state")?,
                    npc_common: c("npc_common")?,
                    npc_state: c("npc_state")?,
                    spawn_template: c("spawn_template")?,
                    spawn_used_item: c("spawn_used_item")?,
                    spawn_ammo_item: c("spawn_ammo_item")?,
                    spawn_consume: c("spawn_consume")?,
                },
            })
        })
        .as_ref()
}

fn item_map(loader: &Loader) -> Option<&'static ItemMap> {
    ITEM_MAP
        .get_or_init(|| {
            let z = |name: &str| size(loader, &format!("survivors.{name}"));
            Some(ItemMap {
                // SAFETY: the table pins this id to the map's find-or-free-slot.
                find_slot: unsafe { function(symbol(loader, "survivors.map_find_slot")?) },
                loaded: z("registry_loaded")?,
                at: z("registry_item_map")?,
                bits: z("map_bits")?,
                capacity: z("map_capacity")?,
                keys: z("map_keys")?,
                values: z("map_values")?,
                count: z("map_count")?,
            })
        })
        .as_ref()
}

fn menu(loader: &Loader) -> Option<&'static Menu> {
    MENU.get_or_init(|| {
        let z = |name: &str| size(loader, &format!("survivors.{name}"));
        Some(Menu {
            // SAFETY: the table pins this id to the row template lookup.
            template: unsafe { function(symbol(loader, "ecs.template_guid")?) },
            state_status: z("state_status")?,
            map_bits: z("ui_map_bits")?,
            map_capacity: z("ui_map_capacity")?,
            map_keys: z("ui_map_keys")?,
            map_values: z("ui_map_values")?,
            map_count: z("ui_map_count")?,
            record_size: z("ui_record_size")?,
            record_status: z("ui_record_status")?,
        })
    })
    .as_ref()
}

fn too_late(loader: &Loader) -> bool {
    loader.delivery.get().is_some() || SEALED.get().is_some()
}

/// Registers mod `owner`'s survivor lookup; one mod at a time.
pub(crate) fn register_lookup(
    loader: &Loader,
    owner: usize,
    callback: SurvivorLookupFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let mut held = LOOKUP.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_some_and(|r| r.owner != owner) {
        return Status::REFUSED;
    }
    *held = Some(Registered {
        owner,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

pub(crate) fn register_bed_check(
    loader: &Loader,
    owner: usize,
    callback: BedCheckFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    BED_CHECKS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

pub(crate) fn register_summon(
    loader: &Loader,
    owner: usize,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || symbol(loader, "survivors.spawn").is_none() {
        return Status::MISSING;
    }
    SUMMONS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered {
            owner,
            callback,
            user: user.expose_provenance(),
        });
    Status::OK
}

pub(crate) fn register_item(loader: &Loader, item: u32, survivor: u32) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if item_map(loader).is_none() || symbol(loader, "survivors.registry_load").is_none() {
        return Status::MISSING;
    }
    ITEMS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((item, survivor));
    Status::OK
}

/// Registers mod `owner`'s menu status merge; client only, one mod.
pub(crate) fn register_status(
    loader: &Loader,
    owner: usize,
    callback: SurvivorStatusFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if loader.side != Side::CLIENT || menu(loader).is_none() || crate::ecs::engine(loader).is_none()
    {
        return Status::MISSING;
    }
    let mut held = STATUS.lock().unwrap_or_else(PoisonError::into_inner);
    if held.is_some_and(|r| r.owner != owner) {
        return Status::REFUSED;
    }
    *held = Some(Registered {
        owner,
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
}

fn take<T>(list: &Mutex<Vec<T>>) -> Vec<T> {
    std::mem::take(&mut *list.lock().unwrap_or_else(PoisonError::into_inner))
}

#[expect(clippy::too_many_lines, reason = "one link per feature")]
pub(crate) fn link(loader: &'static Loader) {
    let sealed = Sealed {
        lookup: LOOKUP.lock().unwrap_or_else(PoisonError::into_inner).take(),
        bed_checks: take(&BED_CHECKS).into_boxed_slice(),
        summons: take(&SUMMONS).into_boxed_slice(),
        items: take(&ITEMS).into_boxed_slice(),
        status: STATUS.lock().unwrap_or_else(PoisonError::into_inner).take(),
    };
    let (lookup, beds, summons, items, status) = (
        sealed.lookup.is_some(),
        !sealed.bed_checks.is_empty(),
        !sealed.summons.is_empty(),
        !sealed.items.is_empty(),
        sealed.status.is_some(),
    );
    let _first = SEALED.set(sealed);
    let _first = LOADED.set(loader);
    let mut links: Vec<(&str, u64, *const (), &'static AtomicUsize, HookFlags)> = Vec::new();
    let at = |name: &str| symbol(loader, name).unwrap_or(0);
    if lookup {
        links.push((
            "npc.lookup",
            at("npc.lookup"),
            lookup_link as LookupFn as *const (),
            &LOOKUP_NEXT,
            HookFlags::CALLER_FRAME,
        ));
        links.push((
            "npc.lookup_cursor",
            at("npc.lookup_cursor"),
            cursor_link as LookupFn as *const (),
            &CURSOR_NEXT,
            HookFlags::CALLER_FRAME,
        ));
        links.push((
            "npc.move",
            at("npc.move"),
            move_link as MoveFn as *const (),
            &MOVE_NEXT,
            HookFlags::NONE,
        ));
        links.push((
            "npc.update",
            at("npc.update"),
            update_link as UpdateFn as *const (),
            &UPDATE_NEXT,
            HookFlags::NONE,
        ));
    }
    if beds {
        links.push((
            "npc.bed_check",
            at("npc.bed_check"),
            bed_check_link as BedCheckGameFn as *const (),
            &BED_CHECK_NEXT,
            HookFlags::NONE,
        ));
    }
    if summons {
        links.push((
            "survivors.spawn",
            at("survivors.spawn"),
            spawn_link as SpawnFn as *const (),
            &SPAWN_NEXT,
            HookFlags::NONE,
        ));
    }
    if items {
        links.push((
            "survivors.registry_load",
            at("survivors.registry_load"),
            registry_link as RegistryLoadFn as *const (),
            &REGISTRY_NEXT,
            HookFlags::NONE,
        ));
    }
    if status {
        match loader.reflection.system("ui_npc_status") {
            Ok(found) if found.entry() != 0 => links.push((
                "ui_npc_status",
                found.entry(),
                menu_link as SystemFn as *const (),
                &MENU_NEXT,
                HookFlags::NONE,
            )),
            _ => crate::log!(WARN, "survivor menu off: no system ui_npc_status"),
        }
    }
    for (name, target, detour, next, flags) in links {
        if target == 0 {
            crate::log!(WARN, "survivors: {name} is not in this build");
            continue;
        }
        let link = Link {
            owner: LOADER,
            detour: detour.addr(),
            next,
            priority: i32::MIN,
            flags,
        };
        let status = loader
            .chains
            .lock()
            .map_or(Status::REFUSED, |mut chains| chains.add(target, link));
        if !status.is_ok() {
            crate::log!(WARN, "survivors: {name} refused ({})", status.0);
        }
    }
    if lookup || beds || summons || items || status {
        crate::log!(
            INFO,
            "survivors: lookup {lookup}, bed checks {beds}, staffs {summons}, filed items {items}, menu {status}"
        );
    }
}

pub(crate) fn forget() {
    TRANSFORM_TYPE.store(u16::MAX, Ordering::Release);
    NPC_COMMON_TYPE.store(u16::MAX, Ordering::Release);
    *MOVING.lock().unwrap_or_else(PoisonError::into_inner) = None;
    MOVE_ACTIVE.store(false, Ordering::Release);
}

fn next<F: Copy>(slot: &AtomicUsize) -> F {
    let address = slot.load(Ordering::Acquire);
    // SAFETY: the loader points the slot at the rest of the chain, which has
    // the hooked function's signature, before the link can run.
    unsafe { core::mem::transmute_copy::<usize, F>(&address) }
}

unsafe fn read_at<T: Copy>(base: u64, offset: u64) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { (base.wrapping_add(offset) as *const T).read_unaligned() }
}

fn cells_fixed(position: [i64; 3]) -> [i64; 3] {
    enshwick_loader_api::altars::cell_of_fixed(position)
}

/// Asks the registered lookup; `None` for the game's own answer.
fn ask(call: &SurvivorLookup) -> Option<u32> {
    let registered = SEALED.get()?.lookup?;
    if crate::host::is_off(registered.owner) {
        return None;
    }
    let mut out = 0u32;
    let user = core::ptr::with_exposed_provenance_mut(registered.user);
    // SAFETY: the callback lives in the mod's DLL, which stays loaded, and
    // takes the pointer it registered; the call and `out` are valid for it.
    let answered = unsafe { (registered.callback)(call, user, &raw mut out) };
    (answered != 0).then_some(out)
}

fn call(survivor: u32, vanilla: u32, purpose: u32, entity: u32, cell: [i64; 3]) -> SurvivorLookup {
    SurvivorLookup {
        header: Header::of::<SurvivorLookup>(),
        survivor,
        vanilla,
        purpose,
        entity,
        cell,
        bed_of: None,
        token: core::ptr::null_mut(),
    }
}

unsafe extern "system" fn lookup_link(bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { lookup(next::<LookupFn>(&LOOKUP_NEXT), bases, npc) }
}

unsafe extern "system" fn cursor_link(bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: as above.
    unsafe { lookup(next::<LookupFn>(&CURSOR_NEXT), bases, npc) }
}

/// # Safety
///
/// Called from a lookup link with the game's arguments.
unsafe fn lookup(next: LookupFn, bases: *mut u8, npc: *const u32) -> u32 {
    let Some(loader) = LOADED.get() else {
        // SAFETY: the game's arguments.
        return unsafe { next(bases, npc) };
    };
    // Read the frame before the chain runs: a deeper hook takes its own.
    let frame = (loader.caller_frame)();
    // SAFETY: the game's arguments.
    let found = unsafe { next(bases, npc) };
    if npc.is_null() || frame.rsp == 0 {
        return found;
    }
    let (Some(layout), Some(engine)) = (layout(loader), crate::ecs::resolved()) else {
        return found;
    };
    // SAFETY: the frame was taken at entry of this chain.
    let caller = unsafe { frame.return_address() };
    // SAFETY: `npc` points at the survivor id the caller looks up.
    let survivor = unsafe { npc.read() };
    let answered = std::panic::catch_unwind(|| {
        // SAFETY: the caller's frame, as the loader recorded it.
        unsafe { resolve(loader, layout, engine, found, survivor, caller, &frame) }
    });
    answered.ok().flatten().unwrap_or(found)
}

/// # Safety
///
/// `frame` is the caller frame of the running lookup.
#[expect(clippy::too_many_lines, reason = "one arm per caller")]
unsafe fn resolve(
    loader: &Loader,
    layout: &Layout,
    engine: &Engine,
    found: u32,
    survivor: u32,
    caller: u64,
    frame: &enshwick_loader_api::abi::CallerFrame,
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
        return ask(&call(survivor, found, SurvivorLookup::CURSOR, 0, [0; 3]));
    }
    if !hosting {
        return None;
    }
    let stack = frame.stack();
    if c.unique.contains(&caller) {
        // SAFETY: the uniqueness pass keeps its context here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.unique_context) };
        // SAFETY: a live query context.
        let me = unsafe { engine.entity(ctx) };
        if me == 0 {
            return None;
        }
        // SAFETY: the pass keeps its saved state and transform pointers here.
        let saved: u64 = unsafe { read_at(stack, o.unique_saved_state) };
        // SAFETY: as above.
        let transform: u64 = unsafe { read_at(stack, o.unique_transform) };
        // SAFETY: the entity's components.
        let home = unsafe { home(saved, transform) };
        SKIP_UPDATE.with(|skip| skip.set(None));
        let answer = ask(&call(survivor, found, SurvivorLookup::UNIQUENESS, me, home))?;
        SKIP_UPDATE
            .with(|skip| skip.set((answer == me && found != 0 && me != found).then_some(survivor)));
        return Some(answer);
    }
    if caller == c.spawn {
        // SAFETY: the spawn request lies in the caller's `r14`, its target
        // transform at this offset.
        let target: [i64; 3] = unsafe { read_at(frame.r14, o.spawn_target) };
        let answer = ask(&call(
            survivor,
            found,
            SurvivorLookup::SPAWN,
            0,
            cells_fixed(target),
        ))?;
        if answer != 0 {
            *MOVING.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((survivor, answer, Instant::now()));
            MOVE_ACTIVE.store(true, Ordering::Release);
        }
        return Some(answer);
    }
    if caller == c.bed {
        // SAFETY: the bed binding keeps its context here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.bed_context) };
        // SAFETY: and the bed's transform here.
        let transform: u64 = unsafe { read_at(stack, o.bed_transform) };
        // SAFETY: and a pointer to the `NpcCommon` type index here.
        let kind_ptr: u64 = unsafe { read_at(stack, o.bed_npc_common_type) };
        // SAFETY: the binding's live context and the bed's transform.
        unsafe { calibrate(engine, ctx, transform) };
        if transform == 0 {
            return None;
        }
        if kind_ptr != 0 {
            // SAFETY: the type index the pointer names.
            NPC_COMMON_TYPE.store(unsafe { read_at(kind_ptr, 0) }, Ordering::Release);
        }
        // SAFETY: the bed's position, 32.32 fixed point.
        let position: [i64; 3] = unsafe { read_at(transform, 0) };
        return ask(&call(
            survivor,
            found,
            SurvivorLookup::BED_OWNER,
            0,
            cells_fixed(position),
        ));
    }
    if caller == c.bed_init {
        return ask(&call(
            survivor,
            found,
            SurvivorLookup::BED_BINDING,
            0,
            [0; 3],
        ));
    }
    let roaming = caller == c.roaming;
    if !roaming && !c.assign.contains(&caller) {
        return None;
    }
    let (ctx, entity) = if roaming {
        // SAFETY: the idle behaviour keeps its context here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.roaming_context) };
        // SAFETY: a live query context.
        (ctx, unsafe { engine.entity(ctx) })
    } else {
        // SAFETY: the assignment keeps its context and targets here.
        let ctx: *mut u64 = unsafe { read_at(stack, o.assign_context) };
        // SAFETY: as above.
        let targets: u64 = unsafe { read_at(stack, o.assign_targets) };
        if targets == 0 {
            return None;
        }
        // SAFETY: the selected target, the bed, at this offset.
        (ctx, unsafe { read_at::<u32>(targets, o.assign_bed) })
    };
    // SAFETY: a live context and an entity of it.
    let cell = unsafe { position_of(engine, ctx, entity) }?;
    let purpose = if roaming {
        SurvivorLookup::ROAMING
    } else if caller == c.assign[0] {
        SurvivorLookup::ASSIGN_BED
    } else {
        SurvivorLookup::PREVIOUS_OCCUPANT
    };
    let mut lookup = call(survivor, found, purpose, entity, cell);
    let mut token = BedToken {
        engine,
        ctx,
        offset: o.npc_common_bed,
    };
    lookup.bed_of = Some(bed_of);
    lookup.token = (&raw mut token).cast();
    ask(&lookup)
}

struct BedToken<'e> {
    engine: &'e Engine,
    ctx: *mut u64,
    offset: u64,
}

/// The bed `entity` sleeps in: its `NpcCommon.bedId`.
unsafe extern "system" fn bed_of(token: *mut c_void, entity: u32) -> u32 {
    let kind = NPC_COMMON_TYPE.load(Ordering::Acquire);
    if token.is_null() || kind == u16::MAX || entity == 0 {
        return 0;
    }
    // SAFETY: the token of the running lookup.
    let token = unsafe { &*token.cast::<BedToken<'_>>() };
    // SAFETY: a component of an entity in the live context.
    let common = unsafe { token.engine.component(token.ctx, kind, entity) };
    if common.is_null() {
        return 0;
    }
    // SAFETY: `NpcCommon.bedId` at this offset.
    unsafe { read_at(common.addr() as u64, token.offset) }
}

/// Learns the type index of `CurrentTransform` from a bed whose transform the
/// caller already holds.
///
/// # Safety
///
/// `ctx` is the binding's live context and `transform` the bed's transform.
unsafe fn calibrate(engine: &Engine, ctx: *mut u64, transform: u64) {
    if TRANSFORM_TYPE.load(Ordering::Acquire) != u16::MAX || ctx.is_null() || transform == 0 {
        return;
    }
    // SAFETY: a live query context.
    let bed = unsafe { engine.entity(ctx) };
    if bed == 0 {
        return;
    }
    for kind in 0..TYPE_SEARCH {
        // SAFETY: reading a component of the bed in its live context.
        if unsafe { engine.component(ctx, kind, bed) }.addr() as u64 == transform {
            TRANSFORM_TYPE.store(kind, Ordering::Release);
            crate::log!(INFO, "survivors: CurrentTransform is component type {kind}");
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
    let transform = unsafe { engine.component(ctx, kind, entity) };
    if transform.is_null() {
        return None;
    }
    // SAFETY: a transform starts with its position.
    Some(cells_fixed(unsafe { read_at(transform.addr() as u64, 0) }))
}

/// A survivor's home: its saved spawn position, or where it stands.
///
/// # Safety
///
/// Non-zero addresses are the survivor's saved state and transform.
unsafe fn home(saved: u64, transform: u64) -> [i64; 3] {
    if saved != 0 {
        // SAFETY: the saved state starts with the spawn position in metres.
        let spawn: [f32; 3] = unsafe { read_at(saved, 0) };
        if spawn.iter().any(|&v| v.to_bits() != 0) {
            return enshwick_loader_api::altars::cell_of_metres(spawn.map(f64::from));
        }
    }
    if transform == 0 {
        return [i64::MIN; 3];
    }
    // SAFETY: a transform starts with its position.
    cells_fixed(unsafe { read_at(transform, 0) })
}

unsafe extern "system" fn update_link(
    bases: *mut u8,
    npc: *const u32,
    state: usize,
    flag: usize,
) -> usize {
    let skip = SKIP_UPDATE.with(Cell::take);
    // SAFETY: `npc` points at the survivor id the game updates.
    if skip.is_some() && !npc.is_null() && skip == Some(unsafe { npc.read() }) {
        return 0;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next::<UpdateFn>(&UPDATE_NEXT)(bases, npc, state, flag) }
}

unsafe extern "system" fn move_link(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize {
    let skip = MOVE_ACTIVE.load(Ordering::Acquire)
        && !ctx.is_null()
        && std::panic::catch_unwind(|| {
            // SAFETY: the behaviour context of the moving survivor.
            unsafe { skip_move(ctx) }
        })
        .unwrap_or(false);
    if skip {
        return 0;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next::<MoveFn>(&MOVE_NEXT)(ctx, b, c, d) }
}

/// Whether this entity is another entity of the survivor a staff moves.
///
/// # Safety
///
/// `ctx` is a survivor's behaviour context.
unsafe fn skip_move(ctx: *mut *mut u64) -> bool {
    let (Some(loader), Some(engine)) = (LOADED.get(), crate::ecs::resolved()) else {
        return false;
    };
    let Some(layout) = layout(loader) else {
        return false;
    };
    let mut moving = MOVING.lock().unwrap_or_else(PoisonError::into_inner);
    let Some((survivor, target, at)) = *moving else {
        return false;
    };
    if at.elapsed() > MOVE_WINDOW {
        *moving = None;
        MOVE_ACTIVE.store(false, Ordering::Release);
        return false;
    }
    // SAFETY: the first word is the ECS query context.
    let query: *mut u64 = unsafe { ctx.read() };
    // SAFETY: a live query context.
    let me = unsafe { engine.entity(query) };
    if me == target {
        *moving = None;
        MOVE_ACTIVE.store(false, Ordering::Release);
        return false;
    }
    // SAFETY: the context points at the survivor's `ActiveNpcState` here.
    let state: u64 = unsafe { read_at(ctx.addr() as u64, layout.offsets.move_survivor) };
    if state == 0 {
        return false;
    }
    // SAFETY: the state starts with the survivor id.
    let id: u32 = unsafe { read_at(state, 0) };
    id == survivor
}

unsafe extern "system" fn bed_check_link(ctx: *mut u8) -> u8 {
    // SAFETY: the rest of the chain takes the argument the game passed.
    let valid = unsafe { next::<BedCheckGameFn>(&BED_CHECK_NEXT)(ctx) };
    if valid == 0 || ctx.is_null() {
        return valid;
    }
    // SAFETY: the survivor's behaviour context.
    let keep = std::panic::catch_unwind(|| unsafe { bed_kept(ctx.addr() as u64) });
    if keep.unwrap_or(true) { valid } else { 0 }
}

/// Whether the mods keep a bed the game kept.
///
/// # Safety
///
/// `ctx` is a survivor's behaviour context.
unsafe fn bed_kept(ctx: u64) -> bool {
    let (Some(loader), Some(engine), Some(sealed)) =
        (LOADED.get(), crate::ecs::resolved(), SEALED.get())
    else {
        return true;
    };
    let Some(layout) = layout(loader) else {
        return true;
    };
    let o = &layout.offsets;
    // SAFETY: the behaviour context holds these component pointers.
    let common: u64 = unsafe { read_at(ctx, o.npc_common) };
    // SAFETY: as above.
    let state: u64 = unsafe { read_at(ctx, o.npc_state) };
    // SAFETY: as above.
    let saved: u64 = unsafe { read_at(ctx, o.npc_saved_state) };
    // SAFETY: as above.
    let transform: u64 = unsafe { read_at(ctx, o.npc_transform) };
    if common == 0 || state == 0 {
        return true;
    }
    // SAFETY: `NpcCommon.bedId`.
    let bed: u32 = unsafe { read_at(common, o.npc_common_bed) };
    // SAFETY: the state starts with the survivor id.
    let survivor: u32 = unsafe { read_at(state, 0) };
    // SAFETY: the first word of the behaviour context is the ECS context.
    let ecs: *mut u64 = unsafe { read_at(ctx, 0) };
    // SAFETY: a live context.
    let Some(bed_at) = (unsafe { position_of(engine, ecs, bed) }) else {
        return true;
    };
    // SAFETY: as above.
    let entity = unsafe { engine.entity(ecs) };
    let check = BedCheck {
        header: Header::of::<BedCheck>(),
        survivor,
        entity,
        bed,
        // SAFETY: the survivor's components.
        home: unsafe { home(saved, transform) },
        bed_at,
    };
    sealed
        .bed_checks
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
        .all(|r| {
            let user = core::ptr::with_exposed_provenance_mut(r.user);
            // SAFETY: the callback lives in the mod's DLL and takes the
            // pointer it registered; the check is valid for the call.
            unsafe { (r.callback)(&raw const check, user) != 0 }
        })
}

unsafe extern "system" fn spawn_link(ctx: *mut u64, request: *mut u8) -> u8 {
    if !request.is_null() {
        // SAFETY: the game's spawn request, before the spawn reads it.
        let _done = std::panic::catch_unwind(|| unsafe { summon(request) });
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next::<SpawnFn>(&SPAWN_NEXT)(ctx, request) }
}

/// Asks the summon filters and marks the request consuming when one says so.
///
/// # Safety
///
/// `request` is the live spawn request.
unsafe fn summon(request: *mut u8) {
    let (Some(loader), Some(sealed)) = (LOADED.get(), SEALED.get()) else {
        return;
    };
    let Some(layout) = layout(loader) else {
        return;
    };
    let o = &layout.offsets;
    let base = request.addr() as u64;
    // SAFETY: the request holds these fields at these offsets.
    let item = unsafe { read_at(base, o.spawn_used_item) };
    // SAFETY: as above.
    let ammo = unsafe { read_at(base, o.spawn_ammo_item) };
    // SAFETY: as above.
    let template = unsafe { read_at(base, o.spawn_template) };
    // SAFETY: as above.
    let target = cells_fixed(unsafe { read_at(base, o.spawn_target) });
    let call = Summon {
        header: Header::of::<Summon>(),
        item,
        ammo,
        template,
        target,
    };
    let consume = sealed
        .summons
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
        .any(|r| {
            let user = core::ptr::with_exposed_provenance_mut(r.user);
            // SAFETY: the callback lives in the mod's DLL and takes the
            // pointer it registered; the summon is valid for the call.
            unsafe { (r.callback)(&raw const call, user) != 0 }
        });
    if consume {
        // SAFETY: the request's consume flag, which the spawn reads next.
        unsafe {
            request
                .wrapping_add(usize::try_from(o.spawn_consume).unwrap_or(0))
                .write(1);
        };
    }
}

unsafe extern "system" fn registry_link(registry: *mut u8) {
    let ready = LOADED
        .get()
        .and_then(|loader| item_map(loader))
        .zip(SEALED.get())
        .filter(|_| !registry.is_null());
    // The game calls this often and builds only once: a registry that holds
    // its collection already is left as it is.
    // SAFETY: the registry's collection pointer, null until it is built.
    let built = ready.is_some_and(
        |(m, _)| unsafe { read_at::<usize>(registry.addr() as u64, m.loaded as u64) } != 0,
    );
    // SAFETY: the rest of the chain takes the registry the game passed.
    unsafe { next::<RegistryLoadFn>(&REGISTRY_NEXT)(registry) };
    let Some((map, sealed)) = ready.filter(|_| !built) else {
        return;
    };
    let mut filed = 0;
    for &(item, survivor) in &sealed.items {
        // SAFETY: the registry is built; its item map lies at `map.at`.
        if unsafe { file(map, registry.wrapping_add(map.at), item, survivor) } {
            filed += 1;
        }
    }
    crate::log!(
        INFO,
        "survivors: filed {filed} staff items in the survivor registry"
    );
}

/// Maps `item` to `survivor` in the game's map at `table`, as its own build
/// does: the game's find-or-free-slot, which grows the table as needed, then
/// the bit, key and value.
///
/// # Safety
///
/// `table` is the live staff item map, not used by another thread.
unsafe fn file(map: &ItemMap, table: *mut u8, item: u32, survivor: u32) -> bool {
    let base = table.addr() as u64;
    // SAFETY: the game's own slot search on its map.
    let slot = unsafe { (map.find_slot)(table, &raw const item) };
    // SAFETY: the map's header fields, read again after a possible growth.
    let capacity: u64 = unsafe { read_at(base, map.capacity as u64) };
    if slot >= capacity {
        return false;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return false;
    };
    // SAFETY: as above.
    let bits: *mut u64 = unsafe { read_at(base, map.bits as u64) };
    // SAFETY: as above.
    let keys: *mut u32 = unsafe { read_at(base, map.keys as u64) };
    // SAFETY: as above.
    let values: *mut u32 = unsafe { read_at(base, map.values as u64) };
    let word = bits.wrapping_add(slot >> 6);
    let bit = 1u64 << (slot & 63);
    // SAFETY: the slot is below the capacity, so its bit word exists.
    let used = unsafe { word.read() } & bit != 0;
    if !used {
        // SAFETY: as above.
        let marked = unsafe { word.read() } | bit;
        // SAFETY: as above.
        unsafe { word.write(marked) };
        // SAFETY: the map's entry count.
        let count: u32 = unsafe { read_at(base, map.count as u64) };
        // SAFETY: as above.
        unsafe {
            table
                .wrapping_add(map.count)
                .cast::<[u8; 4]>()
                .write((count + 1).to_le_bytes());
        }
        // SAFETY: the slot's key.
        unsafe { keys.wrapping_add(slot).write(item) };
    }
    // SAFETY: the slot's value.
    unsafe { values.wrapping_add(slot).write(survivor) };
    true
}

unsafe extern "system" fn menu_link(ctx: *mut u64) {
    // SAFETY: the original system takes its own context.
    unsafe { next::<SystemFn>(&MENU_NEXT)(ctx) };
    if ctx.is_null() {
        return;
    }
    // SAFETY: the system's live context, after vanilla's pass.
    let _done = std::panic::catch_unwind(|| unsafe { merge_statuses(ctx) });
}

/// Hands every survivor's statuses to the merging mod and writes its answer
/// into the menu record.
///
/// # Safety
///
/// `ctx` is `ui_npc_status`'s live context.
unsafe fn merge_statuses(ctx: *mut u64) {
    let (Some(loader), Some(engine), Some(sealed)) =
        (LOADED.get(), crate::ecs::resolved(), SEALED.get())
    else {
        return;
    };
    let (Some(menu), Some(merge)) = (menu(loader), sealed.status) else {
        return;
    };
    if crate::host::is_off(merge.owner) {
        return;
    }
    STATUS_ROWS.with_borrow_mut(|(rows, bits)| {
        rows.clear();
        let mut play_data = 0u64;
        // SAFETY: the system's live context and its iterator size.
        let mut query = unsafe { engine.query::<UI_WORDS>(ctx) };
        while let Some(words) = query.next() {
            let state = words[UI_STATE];
            // SAFETY: the current row's template.
            let guid = unsafe { (menu.template)(ctx) };
            if state == 0 || guid.is_null() {
                continue;
            }
            play_data = words[UI_PLAY_DATA];
            // SAFETY: the row's `ClientActiveNpcState` status bits.
            let status: u8 = unsafe { read_at(state, menu.state_status as u64) };
            // SAFETY: a template GUID is 16 bytes.
            let guid = unsafe { guid.read() };
            if guid.iter().all(|&b| b == 0) {
                continue;
            }
            let id = enshwick_loader_api::content::Guid(guid).id();
            let order = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            rows.push((id, order, status));
        }
        if play_data == 0 {
            return;
        }
        rows.sort_unstable();
        let user = core::ptr::with_exposed_provenance_mut(merge.user);
        for group in rows.chunk_by(|a, b| a.0 == b.0) {
            let id = group[0].0;
            bits.clear();
            bits.extend(group.iter().map(|&(_, _, status)| status));
            // SAFETY: the callback lives in the mod's DLL and takes the
            // pointer it registered; the statuses are valid for the call.
            let answer = unsafe { (merge.callback)(id, bits.as_ptr(), bits.len(), user) };
            if answer & 0x100 == 0 {
                continue;
            }
            // SAFETY: the play data's survivor record map.
            if let Some(record) = unsafe { record(menu, play_data, id) } {
                // SAFETY: the record's status byte, which the menu reads.
                unsafe {
                    (record.wrapping_add(menu.record_status as u64) as *mut u8)
                        .write(answer.to_le_bytes()[0]);
                };
            }
        }
    });
}

/// The menu record of survivor `id`, found as vanilla finds it.
///
/// # Safety
///
/// `play_data` is the client's `UiPlayData`.
unsafe fn record(menu: &Menu, play_data: u64, id: u32) -> Option<u64> {
    // SAFETY: the map's capacity.
    let capacity: u64 = unsafe { read_at(play_data, menu.map_capacity as u64) };
    // SAFETY: its entry count.
    let count: u32 = unsafe { read_at(play_data, menu.map_count as u64) };
    // SAFETY: its occupancy bitset.
    let bits: u64 = unsafe { read_at(play_data, menu.map_bits as u64) };
    // SAFETY: its keys.
    let keys: u64 = unsafe { read_at(play_data, menu.map_keys as u64) };
    // SAFETY: its values.
    let values: u64 = unsafe { read_at(play_data, menu.map_values as u64) };
    if count == 0 || capacity == 0 || bits == 0 || keys == 0 || values == 0 {
        return None;
    }
    // SAFETY: the occupancy bitset has a bit per slot.
    let used = |i: u64| unsafe { read_at::<u64>(bits, (i >> 6) * 8) } >> (i & 63) & 1 != 0;
    let mut i = (capacity - 1) & u64::from(id);
    for _ in 0..capacity {
        if !used(i) {
            return None;
        }
        // SAFETY: an occupied slot's key.
        if unsafe { read_at::<u32>(keys, i * 4) } == id {
            return Some(values + i * menu.record_size as u64);
        }
        i = if i + 1 < capacity { i + 1 } else { 0 };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filing_marks_a_free_slot_and_overwrites_a_used_one() {
        unsafe extern "system" fn find(_map: *mut u8, key: *const u32) -> u64 {
            // SAFETY: the test passes a live key.
            u64::from(unsafe { key.read() } & 127)
        }
        let mut bits = [0u64; 2];
        let mut keys = [0u32; 128];
        let mut values = [0u32; 128];
        let mut table = [0u8; 0x60];
        table[0x08..0x10].copy_from_slice(&(bits.as_mut_ptr() as u64).to_le_bytes());
        table[0x10..0x18].copy_from_slice(&128u64.to_le_bytes());
        table[0x28..0x30].copy_from_slice(&(keys.as_mut_ptr() as u64).to_le_bytes());
        table[0x40..0x48].copy_from_slice(&(values.as_mut_ptr() as u64).to_le_bytes());
        let map = ItemMap {
            find_slot: find,
            loaded: 0x20,
            at: 0,
            bits: 0x08,
            capacity: 0x10,
            keys: 0x28,
            values: 0x40,
            count: 0x54,
        };
        // SAFETY: a table laid out as the game's, owned by the test.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 5) });
        // SAFETY: as above.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 6) });
        assert_eq!(bits[1], 1 << 6);
        assert_eq!((keys[70], values[70]), (70, 6));
        assert_eq!(u32::from_le_bytes(table[0x54..0x58].try_into().unwrap()), 1);
    }
}
