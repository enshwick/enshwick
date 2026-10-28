//! The game's bases past 16 altar zones, and the altars each Flame level
//! allows.
//!
//! The game puts zones that touch into one base. It copies only the first 16
//! zones of the world's list into the bases, its array of bases holds 16 with
//! no check (a 17th base would write over the zone list), and a base names
//! its zones in a 32-bit mask. Raised, found by byte patterns:
//!
//! - the bases' copy of the zone list grows to every listed zone;
//! - the game's grouping may start only the bases ours plans, which groups
//!   every listed zone; after it each base takes our box and zones
//!   ([`members`]);
//! - the four lookups that read a base's zones answer from our table;
//! - the placement cursor refuses an altar that would start a base past the
//!   limit, with the game's own "too many altars" answer.
//!
//! Either every hook is armed or the game keeps its 16 zones. The client
//! runs one grouping for its own world and the server it hosts, on two
//! bases objects, and the same hooks serve both.

use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::Constant;

mod members;

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{AltarLimit, AltarLimitFilterFn, HookFlags, Status};
use enshwick::raw::{read_at, write_at};

use self::members::Game;
use crate::base_groups::grouping::{Grouped, MAX_GROUPED};
use crate::base_groups::limit::{self, Limits, MAX_BASES, VANILLA_ZONES, Zone, Zones};
use crate::chains::Next;
use crate::host::{Loader, Registered};
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::bases::COUNT.id(),
    pinned::bases::ITEM_ALTAR.id(),
    pinned::bases::MAX_ALTARS_REACHED.id(),
    pinned::bases::MEMBERS.id(),
    pinned::bases::STRIDE.id(),
    pinned::bases::ZONE_HIGH.id(),
    pinned::bases::ZONE_LIST.id(),
    pinned::bases::ZONE_LIST_LEN.id(),
    pinned::bases::ZONE_SIZE.id(),
    pinned::building::MAX_ALTARS.id(),
];

/// The grouping's first bytes.
const GROUPING: &str = "48 89 5C 24 10 48 89 6C 24 18 48 89 74 24 20 57 41 54 41 55 41 56 41 57 \
                        48 83 EC 50 45 33 E4 4C 8B F2 4C 8B E9 41 8B EC 44 39 61 24";
/// The new base call's first bytes: the count at +0x24, bases 0x130 apart.
const NEW_BASE: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 57 41 56 41 57 48 83 EC 20 \
                        44 8B 49 24 48 8B F1 49 69 D9 30 01 00 00 41 8D 41 01";
/// Which base holds a point: `fn(out, bases, point) -> out`.
const BASE_AT: &str = "48 89 5C 24 08 57 48 83 EC 10 8B 7A 24 45 33 DB 48 8B DA 4C 8B D1 48 85 FF \
                       0F 84 ?? ?? ?? ?? F2 41 0F 10 00 41 8B 40 08";
/// Whether base i holds a point: `fn(bases, i, point) -> bool`.
const IN_BASE: &str =
    "48 83 EC 18 F2 41 0F 10 18 4D 8B D0 F3 41 0F 10 50 08 0F 28 CB 48 69 C2 30 01 00 00";
/// Which box of a base holds a point: `fn(out, bases, point) -> out`.
const HOLDER_AT: &str =
    "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 8B 72 24 33 DB 4D 8B D8 48 8B FA";
/// The zones of the base holding a point: `fn(out, list, bases, point) ->
/// out`.
const ZONES_AT: &str =
    "40 55 57 41 56 48 83 EC 40 F2 41 0F 10 01 49 8B E8 41 8B 41 08 4C 8D 44 24 20";
/// The placement cursor's first bytes, up to `and dword [rcx], 0xfffc9581`.
const CURSOR: &str = "48 8B C4 4C 89 48 20 4C 89 40 18 55 56 57 41 55 41 57 48 8D A8 88 FE FF FF \
                      48 81 EC 50 02 00 00 81 21 81 95 FC FF";
/// In the cursor: an altar (the item's flag at `bases.item_altar`) is refused
/// when `max_altars(ctx balancing, flame level)` is not above the altar count.
const CURSOR_ALTAR: &str = "80 7F ?? 00 0F B6 9D A0 01 00 00 74 ?? F6 C3 10 75 ?? 41 81 0F 00 40 00 00 \
                            48 8B 96 ?? ?? ?? ?? 48 8B 8E ?? ?? ?? ?? 8B 52 14 E8";
/// The altar flag's offset in `CURSOR_ALTAR`'s `cmp byte [rdi+flag], 0`.
const CURSOR_ALTAR_FLAG_AT: usize = 2;
const CURSOR_BALANCING_AT: usize = 35;
const CURSOR_SPAN: usize = 0x400;

/// The bases object's fields, from the build table: the count, each base
/// `stride` apart with its members mask at `members` right after its box,
/// and the zone boxes in a list of `zone_size` bytes each, the low corner
/// first and the high one at `zone_high`; the placement cursor's altar flag
/// in an item and its "too many altars" bit.
#[derive(Debug, Clone, Copy)]
struct Fields {
    count: usize,
    stride: usize,
    members: usize,
    zone_list: usize,
    zone_list_len: usize,
    zone_size: usize,
    zone_high: usize,
    item_altar: usize,
    max_altars_reached: u32,
}

impl Fields {
    fn read(loader: &Loader) -> Result<Self, String> {
        let f = |id: Constant| {
            loader
                .offset(id)
                .ok_or(format!("{id} is not in this build"))
        };
        let fields = Self {
            count: f(pinned::bases::COUNT)?,
            stride: f(pinned::bases::STRIDE)?,
            members: f(pinned::bases::MEMBERS)?,
            zone_list: f(pinned::bases::ZONE_LIST)?,
            zone_list_len: f(pinned::bases::ZONE_LIST_LEN)?,
            zone_size: f(pinned::bases::ZONE_SIZE)?,
            zone_high: f(pinned::bases::ZONE_HIGH)?,
            item_altar: f(pinned::bases::ITEM_ALTAR)?,
            max_altars_reached: u32::try_from(f(pinned::bases::MAX_ALTARS_REACHED)?)
                .map_err(|e| e.to_string())?,
        };
        if fields.members < fields.zone_size || fields.zone_high + 12 > fields.zone_size {
            return Err("the bases' fields do not fit a box before each mask".into());
        }
        Ok(fields)
    }
}

/// Every function the raised limit hooks, and what it reads.
#[derive(Debug, Clone)]
struct Sites {
    grouping: u64,
    new_base: u64,
    base_at: u64,
    in_base: u64,
    holder_at: u64,
    zones_at: u64,
    cursor: u64,
    balancing: usize,
    fields: Fields,
}

impl Sites {
    fn targets(&self) -> [u64; 7] {
        [
            self.grouping,
            self.new_base,
            self.base_at,
            self.in_base,
            self.holder_at,
            self.zones_at,
            self.cursor,
        ]
    }
}

type LimitFilter = Registered<AltarLimitFilterFn>;

static SITES: OnceLock<Sites> = OnceLock::new();
/// The mods' altar limit filters, in load order.
static LIMITS: Registry<LimitFilter> = Registry::new();
static ZONES: OnceLock<ArcSwap<Zones>> = OnceLock::new();
/// How many zones the world has, `u32::MAX` before the first grouping.
static ZONE_COUNT: AtomicU32 = AtomicU32::new(u32::MAX);
/// Set at arming when every site is hooked.
static RAISED: AtomicBool = AtomicBool::new(false);
/// The largest limits the mods asked for.
static ASKED: Asked = Asked {
    zones: AtomicU32::new(VANILLA_ZONES),
    bases: AtomicU32::new(MAX_BASES),
};
/// How many bases the running grouping may start, per bases object kind:
/// set at its first new base, 16 until then.
static PLANNED: [AtomicUsize; 2] = [const { AtomicUsize::new(MAX_GROUPED) }; 2];

/// The largest limits the mods asked for, each at least the game's own.
#[derive(Debug)]
struct Asked {
    zones: AtomicU32,
    bases: AtomicU32,
}

impl Asked {
    fn take(&self, asked: Limits) {
        self.zones.fetch_max(asked.zones, Ordering::Relaxed);
        self.bases.fetch_max(asked.bases, Ordering::Relaxed);
    }

    fn limits(&self) -> Limits {
        Limits {
            zones: self.zones.load(Ordering::Relaxed),
            bases: self.bases.load(Ordering::Relaxed),
        }
    }

    fn bases(&self) -> usize {
        usize::try_from(self.bases.load(Ordering::Relaxed))
            .map_or(MAX_GROUPED, |n| n.min(MAX_GROUPED))
    }
}

// SAFETY: `GroupFn` is the signature of the base grouping the `GROUPING`
// pattern finds, and `link` uses this slot only as that function's next.
static GROUP_NEXT: Next<GroupFn> = unsafe { Next::new() };
// SAFETY: `AddBaseFn` is the signature of the new base code the `NEW_BASE`
// pattern finds, and `link` uses this slot only as that function's next.
static ADD_BASE_NEXT: Next<AddBaseFn> = unsafe { Next::new() };
// SAFETY: `AnswerFn` is the signature of the lookup the `BASE_AT` pattern
// finds, and `link` uses this slot only as that function's next.
static BASE_AT_NEXT: Next<AnswerFn> = unsafe { Next::new() };
// SAFETY: `InBaseFn` is the signature of the lookup the `IN_BASE` pattern
// finds, and `link` uses this slot only as that function's next.
static IN_BASE_NEXT: Next<InBaseFn> = unsafe { Next::new() };
// SAFETY: `AnswerFn` is the signature of the lookup the `HOLDER_AT` pattern
// finds, and `link` uses this slot only as that function's next.
static HOLDER_AT_NEXT: Next<AnswerFn> = unsafe { Next::new() };
// SAFETY: `ZonesAtFn` is the signature of the lookup the `ZONES_AT` pattern
// finds, and `link` uses this slot only as that function's next.
static ZONES_AT_NEXT: Next<ZonesAtFn> = unsafe { Next::new() };
// SAFETY: `CheckFn` is the signature of the placement cursor the `CURSOR`
// pattern finds, and `link` uses this slot only as that function's next.
static CHECK_NEXT: Next<CheckFn> = unsafe { Next::new() };
// SAFETY: `MaxAltarsFn` is the signature of the build table's
// `building.max_altars`, and `link` uses this slot only as that function's
// next.
static MAX_ALTARS_NEXT: Next<MaxAltarsFn> = unsafe { Next::new() };

type GroupFn = unsafe extern "system" fn(bases: *mut u8, world: *mut u8);
type AddBaseFn = unsafe extern "system" fn(bases: *mut u8, zone: *const u8, index: u32);
type AnswerFn =
    unsafe extern "system" fn(out: *mut u8, bases: *const u8, point: *const [f32; 3]) -> *mut u8;
type InBaseFn = unsafe extern "system" fn(bases: *const u8, i: u64, point: *const [f32; 3]) -> bool;
type ZonesAtFn = unsafe extern "system" fn(
    out: *mut u8,
    list: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8;
type CheckFn = unsafe extern "system" fn(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
    mode: u64,
    extra: u64,
);
type MaxAltarsFn = unsafe extern "system" fn(balancing: u64, flame_level: u32) -> u8;

fn too_late(loader: &Loader) -> bool {
    loader.delivery.get().is_some() || LIMITS.is_sealed()
}

/// Adds mod `owner`'s altar limit filter; during init only.
pub(crate) fn add_limit(
    loader: &Loader,
    owner: ModIndex,
    callback: AltarLimitFilterFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if loader.address(pinned::building::MAX_ALTARS).is_none() {
        return Status::MISSING;
    }
    LIMITS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// The most altar zones and bases the game groups now: its own, or the
/// largest request once every hook is armed.
pub(crate) fn base_limit() -> Limits {
    if raised() {
        ASKED.limits()
    } else {
        Limits::VANILLA
    }
}

/// Takes mod `owner`'s request for `asked`, refused past [`Limits::MOST`];
/// the first request finds every site, and a miss leaves the game its own.
pub(crate) fn request(loader: &Loader, asked: Limits) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if !asked.within(Limits::MOST) {
        return Status::BAD_ARGS;
    }
    if SITES.get().is_none() {
        let status = find(loader);
        if !status.is_ok() {
            return status;
        }
    }
    ASKED.take(asked);
    Status::OK
}

/// Finds every site once.
fn find(loader: &Loader) -> Status {
    if crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let (Some(text), Some(code)) = (
        crate::code::text(loader.base),
        crate::code::text_bytes(loader.base),
    ) else {
        return Status::MISSING;
    };
    match Fields::read(loader).and_then(|fields| find_sites(code, fields)) {
        Ok(sites) => {
            let start = text.0 as u64;
            let at = |offset: u64| start + offset;
            let _first = SITES.set(Sites {
                grouping: at(sites.grouping),
                new_base: at(sites.new_base),
                base_at: at(sites.base_at),
                in_base: at(sites.in_base),
                holder_at: at(sites.holder_at),
                zones_at: at(sites.zones_at),
                cursor: at(sites.cursor),
                ..sites
            });
            let _first = ZONES.set(ArcSwap::from_pointee(Zones::default()));
            Status::OK
        }
        Err(reason) => {
            crate::log!(WARN, "bases keep 16 altar zones: {reason}");
            Status::MISSING
        }
    }
}

/// The cursor's altar check in `code`, which must test the altar flag at
/// `item_altar`, the offset the detour reads.
fn cursor_altar(code: &[u8], item_altar: usize) -> Result<usize, String> {
    let altar = crate::code::find(code, CURSOR_ALTAR)?;
    let tested = code
        .get(altar + CURSOR_ALTAR_FLAG_AT)
        .copied()
        .ok_or("the cursor is cut")?;
    if usize::from(tested) == item_altar {
        Ok(altar)
    } else {
        Err(format!(
            "the cursor tests the altar flag at {tested:#x}, the table names {item_altar:#x}"
        ))
    }
}

/// The sites as offsets into `code`.
fn find_sites(code: &[u8], fields: Fields) -> Result<Sites, String> {
    let find = |pattern: &str| crate::code::find(code, pattern).map(|at| at as u64);
    let cursor = crate::code::find(code, CURSOR)?;
    let altar = cursor_altar(code, fields.item_altar)?;
    if !(cursor..cursor + CURSOR_SPAN).contains(&altar) {
        return Err("the cursor's altar check is not in the cursor".into());
    }
    let offset: [u8; 4] = code
        .get(altar + CURSOR_BALANCING_AT..altar + CURSOR_BALANCING_AT + 4)
        .and_then(|b| b.try_into().ok())
        .ok_or("the cursor is cut")?;
    let balancing = usize::try_from(u32::from_le_bytes(offset)).map_err(|e| e.to_string())?;
    Ok(Sites {
        grouping: find(GROUPING)?,
        new_base: find(NEW_BASE)?,
        base_at: find(BASE_AT)?,
        in_base: find(IN_BASE)?,
        holder_at: find(HOLDER_AT)?,
        zones_at: find(ZONES_AT)?,
        cursor: cursor as u64,
        balancing,
        fields,
    })
}

/// Settles, before arming writes anything, whether the limit is raised:
/// every site among the `hooked` targets.
pub(crate) fn arming(hooked: &[u64]) {
    let raised = SITES
        .get()
        .is_some_and(|sites| sites.targets().iter().all(|at| hooked.contains(at)));
    RAISED.store(raised, Ordering::Release);
}

fn raised() -> bool {
    RAISED.load(Ordering::Acquire)
}

/// The zones and bases of the last grouping.
pub(crate) fn zones() -> Option<(u32, u32)> {
    if !raised() {
        return None;
    }
    let count = ZONE_COUNT.load(Ordering::Relaxed);
    let known = ZONES.get()?;
    (count != u32::MAX).then(|| (count, known.load().bases))
}

pub(crate) fn forget() {
    if let Some(zones) = ZONES.get() {
        zones.store(Arc::new(Zones::default()));
    }
    ZONE_COUNT.store(u32::MAX, Ordering::Relaxed);
    members::forget();
}

pub(crate) fn link(loader: &'static Loader) {
    let limits = LIMITS.seal();
    if !limits.is_empty()
        && let Some(target) = loader.address(pinned::building::MAX_ALTARS)
    {
        let status = loader.link(target, &MAX_ALTARS_NEXT, max_altars, HookFlags::NONE);
        if !status.is_ok() {
            crate::log!(WARN, "altar limit: refused ({})", status.0);
        }
    }
    let Some(sites) = SITES.get() else {
        return;
    };
    // A refused link leaves its site unhooked, so arming keeps the game's
    // limits; the other links then hand every call on unchanged.
    let refused = [
        (
            "base grouping",
            loader.link(sites.grouping, &GROUP_NEXT, group, HookFlags::NONE),
        ),
        (
            "new base",
            loader.link(sites.new_base, &ADD_BASE_NEXT, add_base, HookFlags::NONE),
        ),
        (
            "base at a point",
            loader.link(sites.base_at, &BASE_AT_NEXT, base_at, HookFlags::EXCLUSIVE),
        ),
        (
            "point in a base",
            loader.link(sites.in_base, &IN_BASE_NEXT, in_base, HookFlags::EXCLUSIVE),
        ),
        (
            "zone at a point",
            loader.link(
                sites.holder_at,
                &HOLDER_AT_NEXT,
                holder_at,
                HookFlags::EXCLUSIVE,
            ),
        ),
        (
            "zones of a base",
            loader.link(
                sites.zones_at,
                &ZONES_AT_NEXT,
                zones_at,
                HookFlags::EXCLUSIVE,
            ),
        ),
        (
            "placement cursor",
            loader.link(sites.cursor, &CHECK_NEXT, check, HookFlags::NONE),
        ),
    ]
    .into_iter()
    .filter(|(_, status)| !status.is_ok())
    .inspect(|(name, status)| crate::log!(WARN, "{name}: refused ({})", status.0))
    .count();
    if refused == 0 {
        let asked = ASKED.limits();
        crate::log!(
            INFO,
            "bases take {} altar zones in {} bases",
            asked.zones,
            asked.bases
        );
    } else {
        crate::log!(WARN, "bases keep 16 altar zones: a link was refused");
    }
}

/// The raised limit's switch: off, every replaced lookup and the grouping
/// run as the game's, and new bases stay within the game's 16.
static BASES_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("more bases");

unsafe extern "system" fn max_altars(balancing: u64, flame_level: u32) -> u8 {
    // SAFETY: this detour runs only as the `building.max_altars` link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    let vanilla = unsafe { MAX_ALTARS_NEXT.get()(balancing, flame_level) };
    let Some(limits) = LIMITS.sealed() else {
        return vanilla;
    };
    let running = limits.iter().filter(|f| !crate::host::is_off(f.owner));
    chain_limit(flame_level, vanilla, base_limit().zones, running)
}

/// Runs `filters` in order on the limit of `flame_level`, where the game
/// allows `vanilla`, each on the count the last one left; the answer is
/// kept within `zones`.
fn chain_limit<'a>(
    flame_level: u32,
    vanilla: u8,
    zones: u32,
    filters: impl Iterator<Item = &'a LimitFilter>,
) -> u8 {
    let vanilla_limit = AltarLimit {
        header: enshwick::abi::Header::of::<AltarLimit>(),
        flame_level,
        vanilla,
        allowed: vanilla,
        reserved: [0; 2],
    };
    let mut limit = vanilla_limit;
    for filter in filters {
        // SAFETY: the owner is not off, so the mod's DLL that holds the
        // filter stays loaded; the filter gets the user pointer its mod
        // registered and a limit valid for the call, and the safe layer
        // catches its panics.
        crate::profile::call(filter.owner, crate::profile::Feature::Altars, || unsafe {
            (filter.callback)(&raw mut limit, filter.user());
        });
        limit = AltarLimit {
            allowed: limit.allowed,
            ..vanilla_limit
        };
    }
    limit.allowed.min(u8::try_from(zones).unwrap_or(u8::MAX))
}

/// The kind byte of bases object `bases`, as an index of the per-kind
/// state: the client's object 0, the server's 1.
///
/// # Safety
///
/// - `bases` is a live, non-null bases object.
unsafe fn kind(bases: *const u8) -> usize {
    // SAFETY: the caller guarantees a live object; its kind byte sits at
    // `members::KIND_AT`.
    usize::from(unsafe { read_at::<u8>(bases, members::KIND_AT) } & 1)
}

unsafe extern "system" fn group(bases: *mut u8, world: *mut u8) {
    if let Some(planned) = (!bases.is_null())
        // SAFETY: `bases` is the grouping's own, non-null bases object.
        .then(|| &PLANNED[unsafe { kind(bases) }])
    {
        planned.store(MAX_GROUPED, Ordering::Relaxed);
    }
    if let Some(sites) = SITES.get().filter(|_| raised() && !bases.is_null()) {
        // SAFETY: `bases` is the grouping's own live object, non-null by the
        // filter, before the game's grouping copies the world's zone list.
        let _done = BASES_FUSE.run("the zone list", || unsafe {
            members::let_zone_list_grow(bases, sites.fields);
        });
    }
    // SAFETY: this detour runs only as the base grouping's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { GROUP_NEXT.get()(bases, world) };
    let Some(sites) = SITES.get().filter(|_| raised() && !bases.is_null()) else {
        return;
    };
    // SAFETY: `bases` is non-null by the filter before it and is the bases
    // object the grouping has just filled, on the grouping's thread; the
    // game's new base code is what `ADD_BASE_NEXT` holds past our link.
    let settled = BASES_FUSE.run("the base grouping", || unsafe {
        members::settle(bases, sites.fields, ASKED.bases(), ADD_BASE_NEXT.get())
    });
    match settled.flatten() {
        // SAFETY: as above.
        Some(grouped) => unsafe { keep(bases, sites.fields, &grouped) },
        None => BASES_FUSE.blow("the base grouping's count"),
    }
}

/// Keeps what the grouping found, for the cursor and the API.
///
/// # Safety
///
/// - `bases` is the non-null bases object the grouping has just filled,
///   live for the call, on the grouping's thread.
unsafe fn keep(bases: *const u8, fields: Fields, grouped: &Grouped) {
    let Some(known) = ZONES.get() else {
        return;
    };
    // SAFETY: the caller's guarantee.
    let game = unsafe { Game::new(bases, fields) };
    let listed = crate::base_groups::lookup::Bases::listed(&game);
    let zone = |i: usize| Zone {
        bounds: crate::base_groups::lookup::Bases::zone(&game, i),
        based: grouped.iter().any(|base| base.zones.contains(i)),
    };
    let count = u32::try_from(grouped.len()).unwrap_or(u32::MAX);
    ZONE_COUNT.store(
        u32::try_from(listed).unwrap_or(u32::MAX - 1),
        Ordering::Relaxed,
    );
    let current = known.load();
    if current.bases == count
        && current.zones.len() == listed
        && (0..listed).all(|i| current.zones[i] == zone(i))
    {
        return;
    }
    let zones = Zones {
        zones: (0..listed).map(zone).collect(),
        bases: count,
    };
    let apart = zones.apart();
    let shape = |z: &Zones| (z.zones.len(), z.bases, z.apart());
    if shape(&current) != shape(&zones) {
        if apart > 0 {
            crate::log!(
                WARN,
                "{listed} altar zones in {count} bases: {apart} stand apart from every base, since the game holds {} bases; they allow building but no base features until they touch a base's zone",
                ASKED.bases()
            );
        } else {
            crate::log!(INFO, "{listed} altar zones in {count} bases");
        }
    }
    known.store(Arc::new(zones));
}

unsafe extern "system" fn add_base(bases: *mut u8, zone: *const u8, index: u32) {
    if let Some(count) = SITES
        .get()
        .filter(|_| raised() && !bases.is_null())
        .map(|sites| sites.fields.count)
    {
        // SAFETY: `bases` is non-null by the filter before it and is the
        // bases object the grouping passes; its u32 base count sits at the
        // build table's `bases.count`.
        let started = unsafe { read_at::<u32>(bases, count) };
        // SAFETY: as above.
        let planned = &PLANNED[unsafe { kind(bases) }];
        if started == 0
            && let Some(fields) = SITES.get().map(|sites| sites.fields)
        {
            // SAFETY: the game's grouping calls the new base code only after
            // it refilled the object's zone list, on its own thread.
            let plan = BASES_FUSE.run("the base plan", || unsafe {
                members::planned(bases, fields, ASKED.bases())
            });
            planned.store(plan.unwrap_or(MAX_GROUPED), Ordering::Relaxed);
        }
        let limit = planned.load(Ordering::Relaxed).min(MAX_GROUPED);
        if usize::try_from(started).map_or(true, |n| n >= limit) {
            return;
        }
    }
    // SAFETY: this detour runs only as the new base code's link, so the slot
    // holds the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { ADD_BASE_NEXT.get()(bases, zone, index) };
}

/// The view of `bases` for a lookup, while the raised limit holds.
///
/// # Safety
///
/// - `bases` is a live bases object, read on a thread the grouping does not
///   run beside.
unsafe fn view(bases: *const u8) -> Option<Game> {
    let sites = SITES.get().filter(|_| raised() && !BASES_FUSE.is_off())?;
    // SAFETY: the caller's guarantee.
    (!bases.is_null()).then(|| unsafe { Game::new(bases, sites.fields) })
}

/// The point a lookup is asked about; `None` for a null one.
///
/// # Safety
///
/// - `point` is null or readable for three `f32`.
unsafe fn point_of(point: *const [f32; 3]) -> Option<[f32; 3]> {
    // SAFETY: the caller guarantees a readable point when it is not null.
    (!point.is_null()).then(|| unsafe { read_at(point.cast(), 0) })
}

unsafe extern "system" fn base_at(
    out: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
        // SAFETY: the lookup's own answer, writable for 0x10 bytes.
        let done = BASES_FUSE.run("a base lookup", || unsafe {
            members::base_at(out, &game, p);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { BASE_AT_NEXT.get()(out, bases, point) }
}

unsafe extern "system" fn in_base(bases: *const u8, i: u64, point: *const [f32; 3]) -> bool {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), Ok(i)) = (p, game, usize::try_from(i))
        && let Some(inside) = BASES_FUSE.run("a base lookup", || members::in_base(&game, i, p))
    {
        return inside;
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { IN_BASE_NEXT.get()(bases, i, point) }
}

unsafe extern "system" fn holder_at(
    out: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null()) {
        // SAFETY: the lookup's own answer, writable for 0x30 bytes.
        let done = BASES_FUSE.run("a zone lookup", || unsafe {
            members::holder_at(out, &game, p);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { HOLDER_AT_NEXT.get()(out, bases, point) }
}

unsafe extern "system" fn zones_at(
    out: *mut u8,
    list: *mut u8,
    bases: *const u8,
    point: *const [f32; 3],
) -> *mut u8 {
    // SAFETY: the lookup's own point, readable when not null.
    let p = unsafe { point_of(point) };
    // SAFETY: the lookup's own object, live, on a thread beside no grouping.
    let game = unsafe { view(bases) };
    if let (Some(p), Some(game), false) = (p, game, out.is_null() || list.is_null()) {
        // SAFETY: the lookup's own answer and the game's live zone box
        // container with its own grow callback.
        let done = BASES_FUSE.run("a zone list lookup", || unsafe {
            members::zones_at(out, list, &game, p);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the lookup's link, so the slot holds
    // the next function of that signature; the game's arguments pass
    // unchanged.
    unsafe { ZONES_AT_NEXT.get()(out, list, bases, point) }
}

unsafe extern "system" fn check(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
    mode: u64,
    extra: u64,
) {
    // SAFETY: this detour runs only as the placement cursor's link, so the
    // slot holds the next function of that signature; the game's arguments
    // pass unchanged.
    unsafe { CHECK_NEXT.get()(flags, ctx, transform, item, mode, extra) };
    if !raised() || flags.is_null() || ctx.is_null() || transform.is_null() || item.is_null() {
        return;
    }
    // SAFETY: the cursor's own arguments, each non-null by the check before it,
    // on the cursor's thread after the original ran.
    let _done = BASES_FUSE.run("the placement check", || unsafe {
        refuse_past_limit(flags, ctx, transform, item);
    });
}

/// Sets the cursor's "too many altars" bit for an altar whose zone would
/// stand apart from every base once the game holds as many as it may.
///
/// # Safety
///
/// - `flags`, `ctx`, `transform` and `item` are the placement cursor's
///   own, non-null arguments, live for the call.
/// - Called on the cursor's thread after the original ran, which alone
///   writes `flags` meanwhile.
unsafe fn refuse_past_limit(
    flags: *mut u32,
    ctx: *const u8,
    transform: *const i64,
    item: *const u8,
) {
    let (Some(sites), Some(known), Some(engine)) =
        (SITES.get(), ZONES.get(), crate::ecs::resolved())
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
    let zones = known.load();
    let most = u32::try_from(ASKED.bases()).unwrap_or(MAX_BASES);
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::locks::Relock;

    /// The level each run of `plus_vanilla` saw, in order.
    static SEEN: Mutex<Vec<u32>> = Mutex::new(Vec::new());

    unsafe extern "system" fn plus_vanilla(limit: *mut AltarLimit, _user: *mut c_void) {
        // SAFETY: the loader's limit, writable for the call.
        let limit = unsafe { &mut *limit };
        SEEN.relock().push(limit.flame_level);
        limit.allowed = limit.allowed.saturating_add(limit.vanilla);
        limit.vanilla = 0;
        limit.flame_level = 99;
    }

    /// The cursor's altar check with the altar flag at `flag` in the item,
    /// between filler bytes.
    fn cursor_with_flag(flag: u8) -> Vec<u8> {
        let mut code = vec![0xCC; 8];
        code.extend_from_slice(&[
            0x80, 0x7F, flag, 0x00, 0x0F, 0xB6, 0x9D, 0xA0, 0x01, 0x00, 0x00,
        ]);
        code.extend_from_slice(&[0x74, 0x05, 0xF6, 0xC3, 0x10, 0x75, 0x05]);
        code.extend_from_slice(&[0x41, 0x81, 0x0F, 0x00, 0x40, 0x00, 0x00]);
        code.extend_from_slice(&[0x48, 0x8B, 0x96, 0x10, 0x02, 0x00, 0x00]);
        code.extend_from_slice(&[0x48, 0x8B, 0x8E, 0x18, 0x02, 0x00, 0x00]);
        code.extend_from_slice(&[0x8B, 0x52, 0x14, 0xE8, 0, 0, 0, 0]);
        code.extend_from_slice(&[0xCC; 8]);
        code
    }

    #[test]
    fn the_cursor_altar_check_is_found_with_the_flag_where_the_table_puts_it() {
        assert_eq!(cursor_altar(&cursor_with_flag(0x43), 0x43), Ok(8));
        assert_eq!(cursor_altar(&cursor_with_flag(0x47), 0x47), Ok(8));
    }

    #[test]
    fn a_cursor_testing_another_flag_than_the_table_s_is_refused() {
        let refused = cursor_altar(&cursor_with_flag(0x47), 0x43).unwrap_err();
        assert!(
            refused.contains("0x47") && refused.contains("0x43"),
            "{refused}"
        );
    }

    #[test]
    fn limit_filters_chain_on_the_count_so_far_within_the_zones() {
        let filter = LimitFilter::new(ModIndex::new(0), plus_vanilla, core::ptr::null_mut());
        assert_eq!(chain_limit(3, 6, 16, core::iter::empty()), 6);
        assert_eq!(chain_limit(3, 6, 32, [&filter].into_iter()), 12);
        SEEN.relock().clear();
        assert_eq!(
            chain_limit(3, 6, 32, [&filter, &filter].into_iter()),
            18,
            "each filter sees the game's own count"
        );
        assert_eq!(*SEEN.relock(), [3, 3], "each filter sees the level");
        assert_eq!(chain_limit(3, 6, 16, [&filter, &filter].into_iter()), 16);
        assert_eq!(
            chain_limit(11, 16, 255, [&filter, &filter, &filter].into_iter()),
            64
        );
    }
}
