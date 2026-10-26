//! The game's survivor lookups, bed checks, summoning staffs and client menu,
//! handed to the mods that answer them.
//!
//! The base system keeps one entity per survivor and its callers act on the
//! answer. Callers are told apart by the return address the caller frame
//! records; their frames hold the components of the entity they work on.

use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::{
    BedCheckFn, HookFlags, Side, Status, SummonFn, SurvivorLookupFn, SurvivorStatusFn,
};
use enshwick::raw::read_at;
use lookup::{cursor_link, lookup_link};
use menu::{menu_link, registry_link};
use moves::{bed_check_link, move_link, update_link};
use summons::spawn_link;

use crate::chains::Next;
use crate::host::{Loader, Registered, take};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned::{self, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::ecs::TEMPLATE_GUID.id(),
    pinned::npc::BED_CHECK.id(),
    pinned::npc::LOOKUP.id(),
    pinned::npc::LOOKUP_CURSOR.id(),
    pinned::npc::MOVE.id(),
    pinned::npc::UPDATE.id(),
    pinned::survivors::ASSIGN_BED.id(),
    pinned::survivors::ASSIGN_CONTEXT.id(),
    pinned::survivors::ASSIGN_RETURN_A.id(),
    pinned::survivors::ASSIGN_RETURN_B.id(),
    pinned::survivors::ASSIGN_TARGETS.id(),
    pinned::survivors::BED_INIT_RETURN.id(),
    pinned::survivors::BED_RETURN.id(),
    pinned::survivors::BED_TRANSFORM.id(),
    pinned::survivors::CURSOR_RETURN.id(),
    pinned::survivors::CURSOR_RETURN_A.id(),
    pinned::survivors::CURSOR_RETURN_B.id(),
    pinned::survivors::MAP_BITS.id(),
    pinned::survivors::MAP_CAPACITY.id(),
    pinned::survivors::MAP_COUNT.id(),
    pinned::survivors::MAP_FIND_SLOT.id(),
    pinned::survivors::MAP_KEYS.id(),
    pinned::survivors::MAP_VALUES.id(),
    pinned::survivors::MOVE_SURVIVOR.id(),
    pinned::survivors::NPC_COMMON.id(),
    pinned::survivors::NPC_COMMON_BED.id(),
    pinned::survivors::NPC_SAVED_STATE.id(),
    pinned::survivors::NPC_STATE.id(),
    pinned::survivors::NPC_TRANSFORM.id(),
    pinned::survivors::REGISTRY_ITEM_MAP.id(),
    pinned::survivors::REGISTRY_LOAD.id(),
    pinned::survivors::REGISTRY_LOADED.id(),
    pinned::survivors::ROAMING_CONTEXT.id(),
    pinned::survivors::ROAMING_RETURN.id(),
    pinned::survivors::SPAWN.id(),
    pinned::survivors::SPAWN_AMMO_ITEM.id(),
    pinned::survivors::SPAWN_CONSUME.id(),
    pinned::survivors::SPAWN_RETURN.id(),
    pinned::survivors::SPAWN_TARGET.id(),
    pinned::survivors::SPAWN_TEMPLATE.id(),
    pinned::survivors::SPAWN_USED_ITEM.id(),
    pinned::survivors::SPAWN_USED_PIDE.id(),
    pinned::survivors::SPAWN_USED_SLOT.id(),
    pinned::survivors::STATE_STATUS.id(),
    pinned::survivors::UI_MAP_BITS.id(),
    pinned::survivors::UI_MAP_CAPACITY.id(),
    pinned::survivors::UI_MAP_COUNT.id(),
    pinned::survivors::UI_MAP_KEYS.id(),
    pinned::survivors::UI_MAP_VALUES.id(),
    pinned::survivors::UI_RECORD_SIZE.id(),
    pinned::survivors::UI_RECORD_STATUS.id(),
    pinned::survivors::UNIQUE_CONTEXT.id(),
    pinned::survivors::UNIQUE_RETURN_A.id(),
    pinned::survivors::UNIQUE_RETURN_B.id(),
    pinned::survivors::UNIQUE_SAVED_STATE.id(),
    pinned::survivors::UNIQUE_TRANSFORM.id(),
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

/// Each entity's survivor id, row order and status, and one survivor's statuses.
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

fn symbol(loader: &Loader, name: Symbol) -> Option<u64> {
    loader.address(name)
}

fn address(loader: &Loader, id: Symbol) -> Option<usize> {
    symbol(loader, id).and_then(|v| usize::try_from(v).ok())
}

/// The address of `name`, a `survivors.` id put together at run time.
fn address_by_name(loader: &Loader, name: &str) -> Option<usize> {
    loader
        .address_by_name(name)
        .and_then(|v| usize::try_from(v).ok())
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
            let s = |name: &str| address_by_name(loader, &format!("survivors.{name}"));
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
            let find_slot = address(loader, pinned::survivors::MAP_FIND_SLOT)?;
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
        let template = address(loader, pinned::ecs::TEMPLATE_GUID)?;
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
    owner: ModIndex,
    callback: SurvivorLookupFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let mut held = LOOKUP.relock();
    if held.is_some_and(|r| r.owner != owner) {
        return Status::REFUSED;
    }
    *held = Some(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_bed_check(
    loader: &Loader,
    owner: ModIndex,
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
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_summon(
    loader: &Loader,
    owner: ModIndex,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || symbol(loader, pinned::survivors::SPAWN).is_none() {
        return Status::MISSING;
    }
    SUMMONS
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_summoned(
    loader: &Loader,
    owner: ModIndex,
    callback: SummonFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if layout(loader).is_none() || symbol(loader, pinned::survivors::SPAWN).is_none() {
        return Status::MISSING;
    }
    SUMMONED
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_item(loader: &Loader, item: u32, survivor: u32) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if item_map(loader).is_none() || symbol(loader, pinned::survivors::REGISTRY_LOAD).is_none() {
        return Status::MISSING;
    }
    ITEMS.relock().push((item, survivor));
    Status::OK
}

/// Registers mod `owner`'s menu status merge; client only, one mod.
pub(crate) fn register_status(
    loader: &Loader,
    owner: ModIndex,
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
    let mut held = STATUS.relock();
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
            .map(enshwick_gamedata::reflect::System::entry),
        Err(why) => {
            crate::log!(WARN, "survivor menu off: {why}");
            None
        }
    }
}

pub(crate) fn link(loader: &'static Loader) {
    let sealed = Sealed {
        lookup: LOOKUP.relock().take(),
        bed_checks: take(&BED_CHECKS).into_boxed_slice(),
        summons: take(&SUMMONS).into_boxed_slice(),
        summoned: take(&SUMMONED).into_boxed_slice(),
        items: take(&ITEMS).into_boxed_slice(),
        status: STATUS.relock().take(),
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
    let at = |name: Symbol| symbol(loader, name);
    if lookup {
        loader.link_feature(
            pinned::npc::LOOKUP.id(),
            at(pinned::npc::LOOKUP),
            &LOOKUP_NEXT,
            lookup_link as LookupFn,
            HookFlags::CALLER_FRAME,
        );
        loader.link_feature(
            pinned::npc::LOOKUP_CURSOR.id(),
            at(pinned::npc::LOOKUP_CURSOR),
            &CURSOR_NEXT,
            cursor_link as LookupFn,
            HookFlags::CALLER_FRAME,
        );
        loader.link_feature(
            pinned::npc::MOVE.id(),
            at(pinned::npc::MOVE),
            &MOVE_NEXT,
            move_link,
            HookFlags::NONE,
        );
        loader.link_feature(
            pinned::npc::UPDATE.id(),
            at(pinned::npc::UPDATE),
            &UPDATE_NEXT,
            update_link,
            HookFlags::NONE,
        );
    }
    if beds {
        loader.link_feature(
            pinned::npc::BED_CHECK.id(),
            at(pinned::npc::BED_CHECK),
            &BED_CHECK_NEXT,
            bed_check_link,
            HookFlags::NONE,
        );
    }
    if summons {
        loader.link_feature(
            pinned::survivors::SPAWN.id(),
            at(pinned::survivors::SPAWN),
            &SPAWN_NEXT,
            spawn_link,
            HookFlags::NONE,
        );
    }
    if items {
        loader.link_feature(
            pinned::survivors::REGISTRY_LOAD.id(),
            at(pinned::survivors::REGISTRY_LOAD),
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
    *MOVING.relock() = None;
    MOVE_ACTIVE.store(false, Ordering::Release);
}

/// The survivors features' switch.
static SURVIVORS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("survivor features");

mod lookup;
mod menu;
mod moves;
mod summons;
#[cfg(test)]
mod tests;
