//! The game's survivor lookups, bed checks, summoning staffs and client menu,
//! handed to the mods that answer them.
//!
//! The base system keeps one entity per survivor and its callers act on the
//! answer. Callers are told apart by the return address the caller frame
//! records; their frames hold the components of the entity they work on.

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use enshwick::abi::{
    BedCheck, BedCheckFn, Header, HookFlags, Side, Status, Summon, SummonFn, SurvivorLookup,
    SurvivorLookupFn, SurvivorStatusFn,
};
use enshwick::altars::cell_of_fixed;

use enshwick::raw::{read_at, read_ptr_at, write_at};

use crate::chains::Next;
use crate::ecs::Engine;
use crate::host::{Loader, Registered, take};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "ecs.template_guid",
    "npc.bed_check",
    "npc.lookup",
    "npc.lookup_cursor",
    "npc.move",
    "npc.update",
    "survivors.assign_bed",
    "survivors.assign_context",
    "survivors.assign_return_a",
    "survivors.assign_return_b",
    "survivors.assign_targets",
    "survivors.bed_init_return",
    "survivors.bed_return",
    "survivors.bed_transform",
    "survivors.cursor_return",
    "survivors.cursor_return_a",
    "survivors.cursor_return_b",
    "survivors.map_bits",
    "survivors.map_capacity",
    "survivors.map_count",
    "survivors.map_find_slot",
    "survivors.map_keys",
    "survivors.map_values",
    "survivors.move_survivor",
    "survivors.npc_common",
    "survivors.npc_common_bed",
    "survivors.npc_saved_state",
    "survivors.npc_state",
    "survivors.npc_transform",
    "survivors.registry_item_map",
    "survivors.registry_load",
    "survivors.registry_loaded",
    "survivors.roaming_context",
    "survivors.roaming_return",
    "survivors.spawn",
    "survivors.spawn_ammo_item",
    "survivors.spawn_consume",
    "survivors.spawn_return",
    "survivors.spawn_target",
    "survivors.spawn_template",
    "survivors.spawn_used_item",
    "survivors.spawn_used_pide",
    "survivors.spawn_used_slot",
    "survivors.state_status",
    "survivors.ui_map_bits",
    "survivors.ui_map_capacity",
    "survivors.ui_map_count",
    "survivors.ui_map_keys",
    "survivors.ui_map_values",
    "survivors.ui_record_size",
    "survivors.ui_record_status",
    "survivors.unique_context",
    "survivors.unique_return_a",
    "survivors.unique_return_b",
    "survivors.unique_saved_state",
    "survivors.unique_transform",
];

/// How long a staff's move reaches only the entity it was answered.
const MOVE_WINDOW: Duration = Duration::from_secs(2);
/// `ui_npc_status`'s iterator: a header, `ClientActiveNpcState`,
/// `NetworkComfort`, `RenderTransform`, UI offsets, `UiPlayData`.
/// Found on the 1.0 build 1269579, server and client, and checked against
/// the system's reflected components on every build.
const UI_READS: [&str; 3] = ["ClientActiveNpcState", "NetworkComfort", "RenderTransform"];
const UI_WORDS: usize = 6;
const UI_STATE: usize = 1;
const UI_PLAY_DATA: usize = 5;

#[derive(Debug, Clone, Copy)]
struct Callers {
    spawn: usize,
    unique: [usize; 2],
    bed: usize,
    bed_init: usize,
    cursor: [usize; 2],
    /// The cursor site a client alone asks about; 0 on the server.
    client_cursor: usize,
    assign: [usize; 2],
    roaming: usize,
}

/// Offsets in the callers' frames, the behaviour context and the requests.
#[derive(Debug, Clone, Copy)]
struct Offsets {
    unique_context: usize,
    unique_saved_state: usize,
    unique_transform: usize,
    bed_transform: usize,
    npc_common_bed: usize,
    assign_context: usize,
    assign_targets: usize,
    assign_bed: usize,
    roaming_context: usize,
    spawn_target: usize,
    move_survivor: usize,
    npc_transform: usize,
    npc_saved_state: usize,
    npc_common: usize,
    npc_state: usize,
    spawn_template: usize,
    spawn_used_slot: usize,
    spawn_used_item: usize,
    spawn_used_pide: usize,
    spawn_ammo_item: usize,
    spawn_consume: usize,
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

static LOADED: OnceLock<&'static Loader> = OnceLock::new();
static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
static ITEM_MAP: OnceLock<Option<ItemMap>> = OnceLock::new();
static MENU: OnceLock<Option<Menu>> = OnceLock::new();

static LOOKUP: Mutex<Option<Registered<SurvivorLookupFn>>> = Mutex::new(None);
static BED_CHECKS: Mutex<Vec<Registered<BedCheckFn>>> = Mutex::new(Vec::new());
static SUMMONS: Mutex<Vec<Registered<SummonFn>>> = Mutex::new(Vec::new());
static SUMMONED: Mutex<Vec<Registered<SummonFn>>> = Mutex::new(Vec::new());
static ITEMS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());
static STATUS: Mutex<Option<Registered<SurvivorStatusFn>>> = Mutex::new(None);

#[derive(Debug)]
struct Sealed {
    lookup: Option<Registered<SurvivorLookupFn>>,
    bed_checks: Box<[Registered<BedCheckFn>]>,
    summons: Box<[Registered<SummonFn>]>,
    summoned: Box<[Registered<SummonFn>]>,
    items: Box<[(u32, u32)]>,
    status: Option<Registered<SurvivorStatusFn>>,
}

static SEALED: OnceLock<Sealed> = OnceLock::new();

/// The registry index of `CurrentTransform`, looked up after a step.
static TRANSFORM_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);
/// The registry index of `NpcCommon`, looked up after a step.
static NPC_COMMON_TYPE: AtomicU16 = AtomicU16::new(u16::MAX);
/// The type hashes of `CurrentTransform` and `NpcCommon`.
static TYPE_HASHES: OnceLock<Option<[u32; 2]>> = OnceLock::new();
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

// SAFETY: `LookupFn` is the exact signature of the game function at the
// table's `npc.lookup`, and the slot is used only as the `next` of the
// `lookup_link` link on it.
static LOOKUP_NEXT: Next<LookupFn> = unsafe { Next::new() };
// SAFETY: `LookupFn` is the exact signature of the game function at the
// table's `npc.lookup_cursor`, and the slot is used only as the `next` of the
// `cursor_link` link on it.
static CURSOR_NEXT: Next<LookupFn> = unsafe { Next::new() };
// SAFETY: `MoveFn` is the exact signature of the game function at the
// table's `npc.move`, and the slot is used only as the `next` of the
// `move_link` link on it.
static MOVE_NEXT: Next<MoveFn> = unsafe { Next::new() };
// SAFETY: `UpdateFn` is the exact signature of the game function at the
// table's `npc.update`, and the slot is used only as the `next` of the
// `update_link` link on it.
static UPDATE_NEXT: Next<UpdateFn> = unsafe { Next::new() };
// SAFETY: `BedCheckGameFn` is the exact signature of the game function at the
// table's `npc.bed_check`, and the slot is used only as the `next` of the
// `bed_check_link` link on it.
static BED_CHECK_NEXT: Next<BedCheckGameFn> = unsafe { Next::new() };
// SAFETY: `SpawnFn` is the exact signature of the game function at the
// table's `survivors.spawn`, and the slot is used only as the `next` of the
// `spawn_link` link on it.
static SPAWN_NEXT: Next<SpawnFn> = unsafe { Next::new() };
// SAFETY: `RegistryLoadFn` is the exact signature of the game function at the
// table's `survivors.registry_load`, and the slot is used only as the `next`
// of the `registry_link` link on it.
static REGISTRY_NEXT: Next<RegistryLoadFn> = unsafe { Next::new() };
// SAFETY: `SystemFn` is the exact signature of the reflected ECS system
// `ui_npc_status`, and the slot is used only as the `next` of the
// `menu_link` link on its entry.
static MENU_NEXT: Next<SystemFn> = unsafe { Next::new() };

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
    loader.address(name)
}

fn address(loader: &Loader, name: &str) -> Option<usize> {
    symbol(loader, name).and_then(|v| usize::try_from(v).ok())
}

fn size(loader: &Loader, name: &str) -> Option<usize> {
    loader
        .build
        .constant(name)
        .and_then(|v| usize::try_from(v).ok())
}

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let s = |name: &str| address(loader, &format!("survivors.{name}"));
            let c = |name: &str| size(loader, &format!("survivors.{name}"));
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
                    bed_transform: c("bed_transform")?,
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
                    spawn_used_slot: c("spawn_used_slot")?,
                    spawn_used_item: c("spawn_used_item")?,
                    spawn_used_pide: c("spawn_used_pide")?,
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
            let find_slot = address(loader, "survivors.map_find_slot")?;
            Some(ItemMap {
                // SAFETY: the build table's `survivors.map_find_slot` is the
                // address of the game function with the `FindSlotFn`
                // signature on this build.
                find_slot: unsafe { core::mem::transmute::<usize, FindSlotFn>(find_slot) },
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
        let template = address(loader, "ecs.template_guid")?;
        Some(Menu {
            // SAFETY: the build table's `ecs.template_guid` is the address of
            // the game function with the `TemplateFn` signature on this
            // build.
            template: unsafe { core::mem::transmute::<usize, TemplateFn>(template) },
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
    *held = Some(Registered::new(owner, callback, user));
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
        .push(Registered::new(owner, callback, user));
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
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_summoned(
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
    SUMMONED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
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
    *held = Some(Registered::new(owner, callback, user));
    Status::OK
}

/// `ui_npc_status`, while it reads the components its iterator layout
/// assumes.
fn menu_system(loader: &Loader) -> Option<u64> {
    match crate::ecs::components_match(loader, "ui_npc_status", &UI_READS, &[]) {
        Ok(()) => loader
            .reflection
            .system("ui_npc_status")
            .ok()
            .map(enshwick_reflect::System::entry),
        Err(why) => {
            crate::log!(WARN, "survivor menu off: {why}");
            None
        }
    }
}

pub(crate) fn link(loader: &'static Loader) {
    let sealed = Sealed {
        lookup: LOOKUP.lock().unwrap_or_else(PoisonError::into_inner).take(),
        bed_checks: take(&BED_CHECKS).into_boxed_slice(),
        summons: take(&SUMMONS).into_boxed_slice(),
        summoned: take(&SUMMONED).into_boxed_slice(),
        items: take(&ITEMS).into_boxed_slice(),
        status: STATUS.lock().unwrap_or_else(PoisonError::into_inner).take(),
    };
    let (lookup, beds, summons, items, status) = (
        sealed.lookup.is_some(),
        !sealed.bed_checks.is_empty(),
        !sealed.summons.is_empty() || !sealed.summoned.is_empty(),
        !sealed.items.is_empty(),
        sealed.status.is_some(),
    );
    let _first = SEALED.set(sealed);
    let _first = LOADED.set(loader);
    let at = |name: &str| symbol(loader, name);
    if lookup {
        loader.link_feature(
            "npc.lookup",
            at("npc.lookup"),
            &LOOKUP_NEXT,
            lookup_link as LookupFn,
            HookFlags::CALLER_FRAME,
        );
        loader.link_feature(
            "npc.lookup_cursor",
            at("npc.lookup_cursor"),
            &CURSOR_NEXT,
            cursor_link as LookupFn,
            HookFlags::CALLER_FRAME,
        );
        loader.link_feature(
            "npc.move",
            at("npc.move"),
            &MOVE_NEXT,
            move_link,
            HookFlags::NONE,
        );
        loader.link_feature(
            "npc.update",
            at("npc.update"),
            &UPDATE_NEXT,
            update_link,
            HookFlags::NONE,
        );
    }
    if beds {
        loader.link_feature(
            "npc.bed_check",
            at("npc.bed_check"),
            &BED_CHECK_NEXT,
            bed_check_link,
            HookFlags::NONE,
        );
    }
    if summons {
        loader.link_feature(
            "survivors.spawn",
            at("survivors.spawn"),
            &SPAWN_NEXT,
            spawn_link,
            HookFlags::NONE,
        );
    }
    if items {
        loader.link_feature(
            "survivors.registry_load",
            at("survivors.registry_load"),
            &REGISTRY_NEXT,
            registry_link,
            HookFlags::NONE,
        );
    }
    if status {
        let menu = menu_system(loader);
        loader.link_feature(
            "survivor menu",
            menu,
            &MENU_NEXT,
            menu_link,
            HookFlags::NONE,
        );
    }
    if lookup || beds || summons || items || status {
        crate::log!(
            INFO,
            "survivors: lookup {lookup}, bed checks {beds}, staffs {summons}, filed items {items}, menu {status}"
        );
    }
}

/// Looks up the registry indexes of the component types the lookups read,
/// once per hosted world, on the game thread after a step.
pub(crate) fn after_step(loader: &Loader) {
    if LOADED.get().is_none()
        || (TRANSFORM_TYPE.load(Ordering::Relaxed) != u16::MAX
            && NPC_COMMON_TYPE.load(Ordering::Relaxed) != u16::MAX)
    {
        return;
    }
    learn_types(loader);
}

#[cold]
fn learn_types(loader: &Loader) {
    let Some(hashes) = TYPE_HASHES.get_or_init(|| {
        let hash = |name: &str| {
            let info = loader.reflection.type_info(name).ok()?;
            crate::world::type_hash(info.va())
        };
        Some([
            hash("keen::ecs::CurrentTransform")?,
            hash("keen::ecs::NpcCommon")?,
        ])
    }) else {
        return;
    };
    let index = |hash: u32| {
        let (_, index) = crate::world::type_index(loader, hash).ok()?;
        // SAFETY: `world::type_index` returned the non-null `u16` index entry
        // of the type in the live registry, read on the game thread after a
        // step, where nothing writes it.
        Some(unsafe { read_at::<u16>(index.cast(), 0) })
    };
    let (Some(transform), Some(common)) = (index(hashes[0]), index(hashes[1])) else {
        return;
    };
    TRANSFORM_TYPE.store(transform, Ordering::Release);
    NPC_COMMON_TYPE.store(common, Ordering::Release);
    crate::log!(
        INFO,
        "survivors: CurrentTransform is component type {transform}, NpcCommon {common}"
    );
}

pub(crate) fn forget() {
    TRANSFORM_TYPE.store(u16::MAX, Ordering::Release);
    NPC_COMMON_TYPE.store(u16::MAX, Ordering::Release);
    *MOVING.lock().unwrap_or_else(PoisonError::into_inner) = None;
    MOVE_ACTIVE.store(false, Ordering::Release);
}

/// Asks the registered lookup; `None` for the game's own answer.
fn ask(call: &SurvivorLookup) -> Option<u32> {
    let registered = SEALED.get()?.lookup?;
    if crate::host::is_off(registered.owner) {
        return None;
    }
    let mut out = 0u32;
    let user = core::ptr::with_exposed_provenance_mut(registered.user);
    // SAFETY: the registered `SurvivorLookupFn` lives in the mod's DLL, which
    // stays loaded for the process, and takes the `user` word it registered;
    // `call` and `out` outlive the call.
    let answered = crate::profile::call(
        registered.owner,
        crate::profile::Feature::Survivors,
        || unsafe { (registered.callback)(call, user, &raw mut out) },
    );
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

/// The survivors features' switch.
static SURVIVORS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("survivor features");

unsafe extern "system" fn lookup_link(bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: the game's own arguments to `npc.lookup`, passed with the rest
    // of its chain, at the chain's entry on the game's calling thread.
    unsafe { lookup(LOOKUP_NEXT.get(), bases, npc) }
}

unsafe extern "system" fn cursor_link(bases: *mut u8, npc: *const u32) -> u32 {
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
unsafe fn lookup(next: LookupFn, bases: *mut u8, npc: *const u32) -> u32 {
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
unsafe fn resolve(
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
        return ask(&call(survivor, found, SurvivorLookup::CURSOR, 0, [0; 3]));
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
        let answer = ask(&call(survivor, found, SurvivorLookup::UNIQUENESS, me, home))?;
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
            0,
            cell_of_fixed(target),
        ))?;
        if answer != 0 {
            *MOVING.lock().unwrap_or_else(PoisonError::into_inner) =
                Some((survivor, answer, Instant::now()));
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
            0,
            cell_of_fixed(position),
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
    offset: usize,
}

/// The bed `entity` sleeps in: its `NpcCommon.bedId`.
unsafe extern "system" fn bed_of(token: *mut c_void, entity: u32) -> u32 {
    let kind = NPC_COMMON_TYPE.load(Ordering::Acquire);
    if token.is_null() || kind == u16::MAX || entity == 0 {
        return 0;
    }
    // SAFETY: the non-null `token` (checked earlier) is the `BedToken` that
    // `resolve` handed the mod with this function, alive on its stack until
    // the mod's lookup returns, which is the only time the mod may call this.
    let token = unsafe { &*token.cast::<BedToken<'_>>() };
    // SAFETY: `token.ctx` is null or the running caller's live query context,
    // used on its system's thread while the lookup runs.
    let common = unsafe { token.engine.component(token.ctx, kind, entity) };
    if common.is_null() {
        return 0;
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
unsafe fn position_of(engine: &Engine, ctx: *mut u64, entity: u32) -> Option<[i64; 3]> {
    let kind = TRANSFORM_TYPE.load(Ordering::Acquire);
    if kind == u16::MAX || ctx.is_null() || entity == 0 {
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
unsafe fn home(saved: *const u8, transform: *const u8) -> [i64; 3] {
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

unsafe extern "system" fn update_link(
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

unsafe extern "system" fn move_link(ctx: *mut *mut u64, b: usize, c: usize, d: usize) -> usize {
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

unsafe extern "system" fn bed_check_link(ctx: *mut u8) -> u8 {
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
unsafe fn bed_kept(ctx: *const u8) -> bool {
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
    let Some(bed_at) = (unsafe { position_of(engine, ecs, bed) }) else {
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

unsafe extern "system" fn spawn_link(ctx: *mut u64, request: *mut u8) -> u8 {
    // SAFETY: the game passes the spawn its non-null (checked first) live
    // request, which the spawn reads only after this link returns.
    let asked = (!request.is_null())
        .then(|| SURVIVORS_FUSE.run("a summon", || unsafe { summon(request) }))
        .flatten()
        .flatten();
    let call = match asked {
        // A refused use spawns, moves and takes nothing, as a spawn the game
        // turns down itself.
        Some(Asked::Refused) => return 0,
        Some(Asked::Use(call)) => Some(call),
        None => None,
    };
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the spawn's signature.
    let done = unsafe { SPAWN_NEXT.get()(ctx, request) };
    if done != 0
        && let Some(call) = call
    {
        let _done = SURVIVORS_FUSE.run("a summon's answer", || summoned(call));
    }
    done
}

/// What the summon filters made of a use.
enum Asked {
    /// The use goes on.
    Use(Summon),
    /// A filter refused it.
    Refused,
}

/// The `NetworkFuel` type hash, a used item's meter as clients see it.
static FUEL_TYPE: OnceLock<Option<u32>> = OnceLock::new();

/// The meter of the item entity `pide`, `[current, max]`, or zeros.
fn meter(loader: &Loader, pide: u32) -> [u16; 2] {
    let ty = FUEL_TYPE.get_or_init(|| {
        let info = loader.reflection.type_info("keen::ecs::NetworkFuel").ok()?;
        crate::world::type_hash(info.va())
    });
    let Some(data) = ty.and_then(|ty| crate::world::component(loader, pide, ty).ok()) else {
        return [0; 2];
    };
    // SAFETY: `world::component` returned the item's live `NetworkFuel`, two
    // `u16` read as one word, which only its network system writes.
    let both: u32 = unsafe { read_at(data.cast(), 0) };
    let [a, b, c, d] = both.to_le_bytes();
    [u16::from_le_bytes([a, b]), u16::from_le_bytes([c, d])]
}

/// Asks the summon filters and marks the request consuming when one says
/// so; returns the use, for the handlers after a spawn that worked, or that
/// a filter refused it.
///
/// # Safety
///
/// - `request` is the game's live spawn request, with its fields at the
///   table's `survivors.spawn_*` offsets.
/// - The call runs before the spawn reads the request, on its thread, so
///   nothing else reads or writes it during the call.
unsafe fn summon(request: *mut u8) -> Option<Asked> {
    let (Some(loader), Some(sealed)) = (LOADED.get(), SEALED.get()) else {
        return None;
    };
    let layout = layout(loader)?;
    let o = &layout.offsets;
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item at the table's `survivors.spawn_used_item`.
    let item = unsafe { read_at(request, o.spawn_used_item) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the ammo item at the table's `survivors.spawn_ammo_item`.
    let ammo = unsafe { read_at(request, o.spawn_ammo_item) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the template at the table's `survivors.spawn_template`.
    let template = unsafe { read_at(request, o.spawn_template) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps its 32.32 fixed-point target at the table's
    // `survivors.spawn_target`.
    let target = cell_of_fixed(unsafe { read_at(request, o.spawn_target) });
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item's `ItemInstanceInfo` at the table's
    // `survivors.spawn_used_slot`.
    let slot = unsafe { read_at(request, o.spawn_used_slot) };
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps the used item's `u32` entity at the table's
    // `survivors.spawn_used_pide`.
    let pide: u32 = unsafe { read_at(request, o.spawn_used_pide) };
    let mut call = Summon {
        header: Header::of::<Summon>(),
        item,
        ammo,
        template,
        target,
        slot,
        pide,
        meter: if pide == 0 {
            [0; 2]
        } else {
            meter(loader, pide)
        },
        consumed: 0,
        reserved: [0; 3],
    };
    let mut consume = false;
    for r in sealed
        .summons
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        // SAFETY: the registered `SummonFn` lives in the mod's DLL, which
        // stays loaded for the process, and takes the `user` word it
        // registered; `call` outlives the call.
        match crate::profile::call(r.owner, crate::profile::Feature::Survivors, || unsafe {
            (r.callback)(&raw const call, user)
        }) {
            Summon::REFUSE => return Some(Asked::Refused),
            0 => {}
            _ => consume = true,
        }
    }
    if consume {
        // SAFETY: the caller's contract makes `request` the live spawn
        // request, untouched by others during the call, which keeps its
        // one-byte consume flag at the table's `survivors.spawn_consume`.
        unsafe { write_at(request, o.spawn_consume, 1u8) };
    }
    // SAFETY: the caller's contract makes `request` the live spawn request,
    // which keeps its one-byte consume flag at the table's
    // `survivors.spawn_consume`; a filter of the game's may have set it too.
    call.consumed = unsafe { read_at(request, o.spawn_consume) };
    Some(Asked::Use(call))
}

/// Tells the handlers that a use spawned or moved a survivor.
fn summoned(call: Summon) {
    let Some(sealed) = SEALED.get() else {
        return;
    };
    for r in sealed
        .summoned
        .iter()
        .filter(|r| !crate::host::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        // SAFETY: the registered `SummonFn` lives in the mod's DLL, which
        // stays loaded for the process, and takes the `user` word it
        // registered; `call` outlives the call.
        crate::profile::call(r.owner, crate::profile::Feature::Survivors, || unsafe {
            (r.callback)(&raw const call, user)
        });
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
    // SAFETY: `ready` holds only for the non-null registry the game passes
    // its load, live for the call, which keeps its collection pointer, null
    // until it is built, at the table's `survivors.registry_loaded`.
    let built =
        ready.is_some_and(|(m, _)| !unsafe { read_ptr_at::<u8>(registry, m.loaded) }.is_null());
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the registry load's signature.
    unsafe { REGISTRY_NEXT.get()(registry) };
    let Some((map, sealed)) = ready.filter(|_| !built) else {
        return;
    };
    let _filed = SURVIVORS_FUSE.run("the staff item filing", || {
        let mut filed = 0;
        for &(item, survivor) in &sealed.items {
            // SAFETY: the game's load just built the non-null registry, whose
            // staff item map lies at the table's `survivors.registry_item_map`;
            // the load's thread alone uses it during the call.
            if unsafe { file(map, registry.wrapping_add(map.at), item, survivor) } {
                filed += 1;
            }
        }
        crate::log!(
            INFO,
            "survivors: filed {filed} staff items in the survivor registry"
        );
    });
}

/// Maps `item` to `survivor` in the game's map at `table`, as its own build
/// does: the game's find-or-free-slot, which grows the table as needed, then
/// the bit, key and value.
///
/// # Safety
///
/// - `table` is the live staff item map, laid out at `map`'s offsets: a
///   `u64` capacity, an occupancy bitset of a bit per slot, and `u32` keys
///   and values of `capacity` slots each.
/// - `map.find_slot` is the game's find-or-free-slot for that map.
/// - No other thread uses the map during the call.
unsafe fn file(map: &ItemMap, table: *mut u8, item: u32, survivor: u32) -> bool {
    // SAFETY: the caller's contract makes `map.find_slot` the game's own slot
    // search for the live `table`, used by no other thread.
    let slot = unsafe { (map.find_slot)(table, &raw const item) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // `u64` capacity at `map.capacity`; read after the search, which may grow
    // the map.
    let capacity: u64 = unsafe { read_at(table, map.capacity) };
    if slot >= capacity {
        return false;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return false;
    };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // bitset pointer at `map.bits`; read after a possible growth.
    let bits = unsafe { read_ptr_at::<u8>(table, map.bits) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // keys pointer at `map.keys`; read after a possible growth.
    let keys = unsafe { read_ptr_at::<u8>(table, map.keys) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // values pointer at `map.values`; read after a possible growth.
    let values = unsafe { read_ptr_at::<u8>(table, map.values) };
    let word = (slot >> 6) * size_of::<u64>();
    let bit = 1u64 << (slot & 63);
    // SAFETY: `slot` is below the capacity (checked earlier), so its `u64`
    // word lies in the map's bitset of a bit per slot.
    let marks: u64 = unsafe { read_at(bits, word) };
    if marks & bit == 0 {
        // SAFETY: `slot` is below the capacity (checked earlier), so its `u64`
        // word lies in the map's bitset, which no other thread uses.
        unsafe { write_at(bits, word, marks | bit) };
        // SAFETY: the caller's contract makes `table` the live map, holding
        // its `u32` entry count at `map.count`.
        let count: u32 = unsafe { read_at(table, map.count) };
        // SAFETY: the caller's contract makes `table` the live map, holding
        // its `u32` entry count at `map.count`, which no other thread uses.
        unsafe { write_at(table, map.count, count + 1) };
        // SAFETY: `slot` is below the capacity (checked earlier), so its `u32`
        // key lies in the map's keys, which no other thread uses.
        unsafe { write_at(keys, slot * size_of::<u32>(), item) };
    }
    // SAFETY: `slot` is below the capacity (checked earlier), so its `u32`
    // value lies in the map's values, which no other thread uses.
    unsafe { write_at(values, slot * size_of::<u32>(), survivor) };
    true
}

unsafe extern "system" fn menu_link(ctx: *mut u64) {
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the system's signature.
    unsafe { MENU_NEXT.get()(ctx) };
    if ctx.is_null() {
        return;
    }
    // SAFETY: the game passes `ui_npc_status` its non-null (checked earlier)
    // live context, used here on the system's thread after vanilla's pass,
    // while the system still runs.
    let _done = SURVIVORS_FUSE.run("the status merge", || unsafe { merge_statuses(ctx) });
}

/// Hands every survivor's statuses to the merging mod and writes its answer
/// into the menu record.
///
/// # Safety
///
/// - `ctx` is `ui_npc_status`'s live context, whose query iterator is
///   [`UI_WORDS`] words over [`UI_READS`].
/// - The call runs on the system's thread while the system runs.
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
        let mut play_data = 0usize;
        // Vanilla left the row counter at the end: walk again from the start
        // and restore it, as the altars' walks do.
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread.
        let saved = unsafe { engine.row(ctx) };
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread, which alone writes it while the system runs.
        unsafe { engine.set_row(ctx, 0) };
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread with an iterator of `UI_WORDS` words; the query is
        // dropped at the end of this closure, inside the system's run.
        let mut query = unsafe { engine.query::<UI_WORDS>(ctx) };
        while let Some(words) = query.next() {
            let Ok(state) = usize::try_from(words[UI_STATE]) else {
                continue;
            };
            // SAFETY: the caller's contract makes `ctx` the system's live
            // context, on the row the query just moved to.
            let guid = unsafe { (menu.template)(ctx) };
            if state == 0 || guid.is_null() {
                continue;
            }
            play_data = usize::try_from(words[UI_PLAY_DATA]).unwrap_or(0);
            // SAFETY: `state` is the row's non-zero live
            // `ClientActiveNpcState` (checked earlier), which keeps its status
            // bits at the table's `survivors.state_status`.
            let status: u8 = unsafe { crate::memory::read(state, menu.state_status) };
            // SAFETY: the template lookup returned a non-null (checked earlier)
            // pointer to the row's 16-byte template GUID.
            let guid: [u8; 16] = unsafe { read_at(guid.cast(), 0) };
            if guid.iter().all(|&b| b == 0) {
                continue;
            }
            let id = enshwick::content::Guid(guid).id();
            let order = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            rows.push((id, order, status));
        }
        // SAFETY: the system's live context.
        unsafe { engine.set_row(ctx, saved) };
        if play_data == 0 {
            return;
        }
        rows.sort_unstable();
        let user = core::ptr::with_exposed_provenance_mut(merge.user);
        for group in rows.chunk_by(|a, b| a.0 == b.0) {
            let id = group[0].0;
            bits.clear();
            bits.extend(group.iter().map(|&(_, _, status)| status));
            // SAFETY: the registered `SurvivorStatusFn` lives in the mod's
            // DLL, which stays loaded for the process, and takes the `user`
            // word it registered; `bits` outlives the call.
            let answer =
                crate::profile::call(merge.owner, crate::profile::Feature::Survivors, || unsafe {
                    (merge.callback)(id, bits.as_ptr(), bits.len(), user)
                });
            if answer & enshwick::abi::SURVIVOR_STATUS_SET == 0 {
                continue;
            }
            // SAFETY: `play_data` is the client's non-zero (checked earlier)
            // live `UiPlayData` from the system's own query words, used on
            // its thread while it runs.
            if let Some(record) = unsafe { record(menu, play_data, id) } {
                // SAFETY: `record` is a live menu record of the play data's
                // map, which keeps the status byte the menu reads at the
                // table's `survivors.ui_record_status`, on the system's
                // thread.
                unsafe { write_at(record, menu.record_status, answer.to_le_bytes()[0]) };
            }
        }
    });
}

/// The menu record of survivor `id`, found as vanilla finds it.
///
/// # Safety
///
/// - `play_data` is the address of the client's live `UiPlayData`, whose
///   survivor record map lies at `menu`'s offsets: a power-of-two
///   capacity, an occupancy bitset of a bit per slot, and `u32` keys and
///   records of `capacity` slots each.
/// - Nothing writes the map during the call.
unsafe fn record(menu: &Menu, play_data: usize, id: u32) -> Option<*mut u8> {
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's `u64` capacity at the table's
    // `survivors.ui_map_capacity`.
    let capacity: u64 = unsafe { crate::memory::read(play_data, menu.map_capacity) };
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's `u32` entry count at the table's
    // `survivors.ui_map_count`.
    let count: u32 = unsafe { crate::memory::read(play_data, menu.map_count) };
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's bitset pointer at the table's
    // `survivors.ui_map_bits`.
    let bits = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_bits) }.cast_const();
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's keys pointer at the table's
    // `survivors.ui_map_keys`.
    let keys = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_keys) }.cast_const();
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's records pointer at the table's
    // `survivors.ui_map_values`.
    let values = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_values) };
    if count == 0 || capacity == 0 || bits.is_null() || keys.is_null() || values.is_null() {
        return None;
    }
    let capacity = usize::try_from(capacity).ok()?;
    let used = |i: usize| {
        // SAFETY: `i` is below the non-zero capacity, and the caller's
        // contract gives the non-null bitset (checked earlier) a bit per slot.
        let marks: u64 = unsafe { read_at(bits, (i >> 6) * size_of::<u64>()) };
        marks >> (i & 63) & 1 != 0
    };
    let mut i = (capacity - 1) & usize::try_from(id).ok()?;
    for _ in 0..capacity {
        if !used(i) {
            return None;
        }
        // SAFETY: `i` is below the capacity, and the caller's contract gives
        // the non-null keys (checked earlier) a `u32` per slot.
        if unsafe { read_at::<u32>(keys, i * size_of::<u32>()) } == id {
            return Some(values.wrapping_add(i * menu.record_size));
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
            // SAFETY: `file` passes a pointer to its own `u32` item.
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
        // SAFETY: `table` is laid out at `map`'s offsets over the test's own
        // 128-slot bitset, keys and values, `find` is its slot search, and
        // only this thread uses them.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 5) });
        // SAFETY: `table` is laid out at `map`'s offsets over the test's own
        // 128-slot bitset, keys and values, `find` is its slot search, and
        // only this thread uses them.
        assert!(unsafe { file(&map, table.as_mut_ptr(), 70, 6) });
        assert_eq!(bits[1], 1 << 6);
        assert_eq!((keys[70], values[70]), (70, 6));
        assert_eq!(u32::from_le_bytes(table[0x54..0x58].try_into().unwrap()), 1);
    }
}
