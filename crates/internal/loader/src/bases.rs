//! The game's bases past 16 altar zones, and the altars each Flame level
//! allows.
//!
//! The game puts zones that touch into one base. It copies only the first 16
//! zones of the world's list into the bases, its array of bases holds 16 with
//! no check (a 17th base would write over the zone list), and a base names
//! its zones in a 32-bit mask. Raised, at functions the build table checks
//! by a hash of their whole code, so a change to the game's logic
//! there keeps the game's limits:
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
//!
//! Past 16 bases, when a mod asks for more, the objects grow and bases 16
//! and on take window indices behind the game's array ([`past16`]); either
//! every site of that is armed too, or the game keeps 16 bases with the
//! zones raised. The server's object holds the bases the mods asked for, the
//! client's every base the window holds, so a modded client with the window
//! armed groups every base the server does and needs no join rule of its
//! own; a client whose window could not arm groups 16 and its log says so.
//! The placement cursor refuses an altar past the asked limit.

use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::Constant;

mod cursor;
#[cfg(test)]
mod fake;
mod heap;
mod lifecycle;
mod loops;
mod members;
mod past16;

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{AltarLimit, AltarLimitFilterFn, HookFlags, Status};
use enshwick::raw::read_at;

use self::members::{Game, Side, side_of};
use crate::base_groups::grouping::Grouped;
use crate::base_groups::limit::{Limits, MAX_BASES, VANILLA_ZONES, Zone, Zones};
use crate::base_groups::window::{self, Kind};
use crate::builds::Unhooked;
use crate::chains::Next;
use crate::code::{CodePatch, Verdict};
use crate::host::{Loader, Registered, Target};
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::bases::BASE_AT.id(),
    pinned::bases::BASE_IDS.id(),
    pinned::bases::BED_CLEANUP.id(),
    pinned::bases::CLIENT_CREATE.id(),
    pinned::bases::CLIENT_DESTROY.id(),
    pinned::bases::CLIENT_SIZE.id(),
    pinned::bases::COMFORT_CLEANUP.id(),
    pinned::bases::COUNT_GETTER.id(),
    pinned::bases::CTOR.id(),
    pinned::bases::CURSOR.id(),
    pinned::bases::CURSOR_BALANCING.id(),
    pinned::bases::ENTITY_HASH.id(),
    pinned::bases::ENTITY_QUERY.id(),
    pinned::bases::GROUPING.id(),
    pinned::bases::HOLDER_AT.id(),
    pinned::bases::IN_BASE.id(),
    pinned::bases::NEW_BASE.id(),
    pinned::bases::NPC_TOTALS.id(),
    pinned::bases::PROPS_CLEANUP.id(),
    pinned::bases::REBUILD.id(),
    pinned::bases::SERVER_CREATE.id(),
    pinned::bases::SERVER_DESTROY.id(),
    pinned::bases::SERVER_SIZE.id(),
    pinned::bases::SERVER_UPDATE.id(),
    pinned::bases::SET_INSERT.id(),
    pinned::bases::SLOT_FREE.id(),
    pinned::bases::SLOT_INIT.id(),
    pinned::bases::SORT.id(),
    pinned::bases::SORT_ALL.id(),
    pinned::bases::STOCK_ADVANCE.id(),
    pinned::bases::STOCK_CACHE.id(),
    pinned::bases::STOCK_FIND.id(),
    pinned::bases::TEARDOWN.id(),
    pinned::bases::ZONES_AT.id(),
    pinned::bases::COUNT.id(),
    pinned::bases::ITEM_ALTAR.id(),
    pinned::bases::MAX_ALTARS_REACHED.id(),
    pinned::bases::MEMBERS.id(),
    pinned::bases::STRIDE.id(),
    pinned::bases::ZONE_HIGH.id(),
    pinned::bases::ZONE_LIST.id(),
    pinned::bases::ZONE_LIST_LEN.id(),
    pinned::bases::ZONE_SIZE.id(),
    pinned::bases::ALLOCATOR.id(),
    pinned::bases::ANIMAL_MAP.id(),
    pinned::bases::ANIMAL_VALUE.id(),
    pinned::bases::ANIMALS.id(),
    pinned::bases::ANSWER_BOX.id(),
    pinned::bases::ANSWER_ERROR.id(),
    pinned::bases::ANSWER_INDEX.id(),
    pinned::bases::ANSWER_SIZE.id(),
    pinned::bases::ANSWER_ZERO.id(),
    pinned::bases::CHANGES.id(),
    pinned::bases::CONTAINER_BOX.id(),
    pinned::bases::CONTAINER_CAP.id(),
    pinned::bases::CONTAINER_DATA.id(),
    pinned::bases::CONTAINER_GROW.id(),
    pinned::bases::CONTAINER_LEN.id(),
    pinned::bases::GROUPED.id(),
    pinned::bases::KIND.id(),
    pinned::bases::MAP_BITMAP.id(),
    pinned::bases::MAP_BUCKETS.id(),
    pinned::bases::MAP_COUNT.id(),
    pinned::bases::MAP_KEYS.id(),
    pinned::bases::MAP_VALUES.id(),
    pinned::bases::NPC_KIND.id(),
    pinned::bases::NPC_MAP.id(),
    pinned::bases::NPC_VALUE.id(),
    pinned::bases::NPCS.id(),
    pinned::bases::NPCS_HOME.id(),
    pinned::bases::PAGE.id(),
    pinned::bases::POSITION.id(),
    pinned::bases::REBUILT.id(),
    pinned::bases::RECORD_ANIMALS.id(),
    pinned::bases::RECORD_BASE_ID.id(),
    pinned::bases::RECORD_BEDS.id(),
    pinned::bases::RECORD_COMFORT.id(),
    pinned::bases::RECORD_COMFORT_LEN.id(),
    pinned::bases::RECORD_LIST_ALL.id(),
    pinned::bases::RECORD_LIST_KEPT.id(),
    pinned::bases::RECORD_NPCS_HOME.id(),
    pinned::bases::RECORD_PROPS.id(),
    pinned::bases::RECORD_SET.id(),
    pinned::bases::RECORD_SET_BITMAP.id(),
    pinned::bases::RECORD_SET_BUCKETS.id(),
    pinned::bases::RECORD_SET_COUNT.id(),
    pinned::bases::STAGING.id(),
    pinned::bases::STOCK_ANSWER_SIZE.id(),
    pinned::bases::STOCK_COUNT.id(),
    pinned::bases::STOCK_ENTITIES.id(),
    pinned::bases::ZONE_LIST_ALLOCATOR.id(),
    pinned::bases::HEAP_ALLOCATE.id(),
    pinned::bases::HEAP_FREE.id(),
    pinned::building::MAX_ALTARS.id(),
];

/// The raised zones, as the log and the Mods screen name them when a build
/// lacks a site.
const RAISED_ZONES: &str = "altar zones past 16";

/// The functions the raised limit hooks, in the order of [`Sites`]: the
/// grouping, the new base call, the four lookups of a base's zones
/// (`fn(out, bases, point) -> out`, `fn(bases, i, point) -> bool`,
/// `fn(out, bases, point) -> out`, `fn(out, list, bases, point) -> out`)
/// and the placement cursor, which refuses an altar when
/// `max_altars(ctx balancing, flame level)` is not above the altar count.
const SITE_SYMBOLS: [pinned::Symbol; 7] = [
    pinned::bases::GROUPING,
    pinned::bases::NEW_BASE,
    pinned::bases::BASE_AT,
    pinned::bases::IN_BASE,
    pinned::bases::HOLDER_AT,
    pinned::bases::ZONES_AT,
    pinned::bases::CURSOR,
];

/// The bases object's fields, from the build table: the count, each base
/// `stride` apart with its members mask at `members` right after its box,
/// and the zone boxes in a list of `zone_size` bytes each, the low corner
/// first and the high one at `zone_high`; the placement cursor's altar flag
/// in an item and its "too many altars" bit, and the balancing its context
/// holds; the byte that tells the server's object from the client's, the
/// world allocator the object keeps, the lookups' answer and the game's zone
/// box container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    balancing: usize,
    kind: usize,
    allocator: usize,
    answer: Answer,
    container: Container,
    heap: heap::Slots,
}

/// The answer of the zone lookups: an error byte, a u32 the game sets to 0,
/// then the base index or the box that holds the point, in `size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answer {
    error: usize,
    zero: usize,
    index: usize,
    boxed: usize,
    size: usize,
}

/// The game's container, also the bases object's own zone list: data,
/// length, capacity, the callback that grows it, the allocator it grows
/// through, null when it may not leave its inline room, and the bytes of
/// each box it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Container {
    data: usize,
    len: usize,
    cap: usize,
    grow: usize,
    allocator: usize,
    boxed: usize,
}

impl Fields {
    /// The fields as `offset` reads the build table's constants, checked to
    /// fit the copies and answers the lookups make.
    fn read(offset: &dyn Fn(Constant) -> Option<usize>) -> Result<Self, String> {
        let f = |id: Constant| offset(id).ok_or(format!("{id} is not in this build"));
        let zone_list = f(pinned::bases::ZONE_LIST)?;
        let zone_list_allocator = f(pinned::bases::ZONE_LIST_ALLOCATOR)?;
        let fields = Self {
            count: f(pinned::bases::COUNT)?,
            stride: f(pinned::bases::STRIDE)?,
            members: f(pinned::bases::MEMBERS)?,
            zone_list,
            zone_list_len: f(pinned::bases::ZONE_LIST_LEN)?,
            zone_size: f(pinned::bases::ZONE_SIZE)?,
            zone_high: f(pinned::bases::ZONE_HIGH)?,
            item_altar: f(pinned::bases::ITEM_ALTAR)?,
            max_altars_reached: u32::try_from(f(pinned::bases::MAX_ALTARS_REACHED)?)
                .map_err(|e| e.to_string())?,
            balancing: f(pinned::bases::CURSOR_BALANCING)?,
            kind: f(pinned::bases::KIND)?,
            allocator: f(pinned::bases::ALLOCATOR)?,
            answer: Answer {
                error: f(pinned::bases::ANSWER_ERROR)?,
                zero: f(pinned::bases::ANSWER_ZERO)?,
                index: f(pinned::bases::ANSWER_INDEX)?,
                boxed: f(pinned::bases::ANSWER_BOX)?,
                size: f(pinned::bases::ANSWER_SIZE)?,
            },
            container: Container {
                data: f(pinned::bases::CONTAINER_DATA)?,
                len: f(pinned::bases::CONTAINER_LEN)?,
                cap: f(pinned::bases::CONTAINER_CAP)?,
                grow: f(pinned::bases::CONTAINER_GROW)?,
                boxed: f(pinned::bases::CONTAINER_BOX)?,
                allocator: zone_list_allocator
                    .checked_sub(zone_list)
                    .ok_or("the zone list's allocator lies before its list")?,
            },
            heap: heap::Slots {
                allocate: f(pinned::bases::HEAP_ALLOCATE)?,
                free: f(pinned::bases::HEAP_FREE)?,
            },
        };
        if !fields.fits() {
            return Err(format!(
                "the bases' fields do not fit the boxes, answers and heap the lookups use ({fields:x?})"
            ));
        }
        Ok(fields)
    }

    /// Whether a zone box holds both corners, the answer holds its error,
    /// zero word, index and a whole box, and the zone list's length is the
    /// container's, whose boxes are the zone list's, and the heap's slots
    /// fit its vtable.
    fn fits(&self) -> bool {
        let Answer {
            error,
            zero,
            index,
            boxed,
            size,
        } = self.answer;
        self.members >= self.zone_size
            && self.zone_high + 12 <= self.zone_size
            && error < size
            && zero + 4 <= size
            && index < size
            && boxed + self.zone_size <= size
            && self.zone_list + self.container.len == self.zone_list_len
            && self.container.boxed == self.zone_size
            && self.heap.fit()
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
/// The zones and bases of the last grouping, per bases object kind as
/// [`kind_index`] numbers them.
static ZONES: OnceLock<[ArcSwap<Zones>; 2]> = OnceLock::new();
/// How many zones the world has, per kind, `u32::MAX` before the first
/// grouping.
static ZONE_COUNT: [AtomicU32; 2] = [const { AtomicU32::new(u32::MAX) }; 2];
/// Set at arming when every site is hooked.
static RAISED: AtomicBool = AtomicBool::new(false);
/// The largest limits the mods asked for.
static ASKED: Asked = Asked {
    zones: AtomicU32::new(VANILLA_ZONES),
    bases: AtomicU32::new(MAX_BASES),
};
/// How many bases the game's own grouping may start, per bases object kind:
/// set at its first new base, at most the array's 16, 16 until then.
static PLANNED: [AtomicUsize; 2] = [const { AtomicUsize::new(window::VANILLA) }; 2];

/// The per-kind state's index for a `kind` object.
fn kind_index(kind: Kind) -> usize {
    usize::from(kind == Kind::Server)
}

/// Whether bases past 16 are armed, on top of the raised zones.
fn past_16() -> bool {
    raised() && past16::armed()
}

/// The most bases any grouping holds now.
fn most_bases() -> usize {
    if past_16() {
        window::MOST
    } else {
        window::VANILLA
    }
}

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

    /// The most bases a grouping on a `kind` object holds: what was asked
    /// on the server, every base the game can hold on the client.
    fn bases(&self, kind: Kind) -> usize {
        let asked = usize::try_from(self.bases.load(Ordering::Relaxed)).unwrap_or(usize::MAX);
        window::cap(kind, asked, most_bases())
    }

    fn asked_past_16(&self) -> bool {
        self.bases.load(Ordering::Relaxed) > MAX_BASES
    }
}

// SAFETY: `GroupFn` is the signature of the base grouping the table's
// `bases.grouping` pins, and `link` uses this slot only as that function's
// next.
static GROUP_NEXT: Next<GroupFn> = unsafe { Next::new() };
// SAFETY: `AddBaseFn` is the signature of the new base code the table's
// `bases.new_base` pins, and `link` uses this slot only as that function's
// next.
static ADD_BASE_NEXT: Next<AddBaseFn> = unsafe { Next::new() };
// SAFETY: `AnswerFn` is the signature of the lookup the table's
// `bases.base_at` pins, and `link` uses this slot only as that function's
// next.
static BASE_AT_NEXT: Next<AnswerFn> = unsafe { Next::new() };
// SAFETY: `InBaseFn` is the signature of the lookup the table's
// `bases.in_base` pins, and `link` uses this slot only as that function's
// next.
static IN_BASE_NEXT: Next<InBaseFn> = unsafe { Next::new() };
// SAFETY: `AnswerFn` is the signature of the lookup the table's
// `bases.holder_at` pins, and `link` uses this slot only as that function's
// next.
static HOLDER_AT_NEXT: Next<AnswerFn> = unsafe { Next::new() };
// SAFETY: `ZonesAtFn` is the signature of the lookup the table's
// `bases.zones_at` pins, and `link` uses this slot only as that function's
// next.
static ZONES_AT_NEXT: Next<ZonesAtFn> = unsafe { Next::new() };
// SAFETY: `CheckFn` is the signature of the placement cursor the table's
// `bases.cursor` pins, and `link` uses this slot only as that function's
// next.
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
        let asked = ASKED.limits();
        Limits {
            bases: asked
                .bases
                .min(u32::try_from(most_bases()).unwrap_or(MAX_BASES)),
            ..asked
        }
    } else {
        Limits::VANILLA
    }
}

/// The most a mod may ask for: every listed zone in every base the window
/// holds, or the game's 16 bases once arming found them out of reach.
pub(crate) fn most() -> Limits {
    if past16::settled() && !past_16() {
        Limits {
            bases: MAX_BASES,
            ..Limits::MOST
        }
    } else {
        Limits::MOST
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

/// Finds every site once, from the build table.
fn find(loader: &Loader) -> Status {
    if crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    let fields = match Fields::read(&|id| loader.offset(id)) {
        Ok(fields) => fields,
        Err(reason) => {
            crate::log!(WARN, "bases keep 16 altar zones: {reason}");
            return Status::MISSING;
        }
    };
    match sites_of(fields, |symbol| loader.address(symbol)) {
        Ok(sites) => {
            let _first = SITES.set(sites);
            let _first = ZONES.set([
                ArcSwap::from_pointee(Zones::default()),
                ArcSwap::from_pointee(Zones::default()),
            ]);
            Status::OK
        }
        Err(symbol) => {
            loader.feature_off(RAISED_ZONES, symbol.id(), &Unhooked::Absent.to_string());
            Status::MISSING
        }
    }
}

/// The sites where `address` puts them, or the first symbol it lacks.
fn sites_of(
    fields: Fields,
    address: impl Fn(pinned::Symbol) -> Option<u64>,
) -> Result<Sites, pinned::Symbol> {
    let mut at = [0; SITE_SYMBOLS.len()];
    for (slot, symbol) in at.iter_mut().zip(SITE_SYMBOLS) {
        *slot = address(symbol).ok_or(symbol)?;
    }
    let [
        grouping,
        new_base,
        base_at,
        in_base,
        holder_at,
        zones_at,
        cursor,
    ] = at;
    Ok(Sites {
        grouping,
        new_base,
        base_at,
        in_base,
        holder_at,
        zones_at,
        cursor,
        fields,
    })
}

/// Settles, before arming writes anything, whether the limit is raised:
/// every site among the `hooked` targets.
pub(crate) fn arming(patches: &[CodePatch], verdicts: &[Verdict], hooked: &[u64]) {
    let raised = SITES
        .get()
        .is_some_and(|sites| sites.targets().iter().all(|at| hooked.contains(at)));
    RAISED.store(raised, Ordering::Release);
    past16::arming(patches, verdicts, hooked);
}

fn raised() -> bool {
    RAISED.load(Ordering::Acquire)
}

/// The zones and bases of the last grouping.
pub(crate) fn zones() -> Option<(u32, u32)> {
    if !raised() {
        return None;
    }
    let (count, known) = known()?;
    Some((count, known.bases))
}

/// The last grouping's zone count and zones: the server's, which decides,
/// or the client's on a client that hosts none.
fn known() -> Option<(u32, Arc<Zones>)> {
    let zones = ZONES.get()?;
    [Kind::Server, Kind::Client].into_iter().find_map(|kind| {
        let at = kind_index(kind);
        let count = ZONE_COUNT[at].load(Ordering::Relaxed);
        (count != u32::MAX).then(|| (count, zones[at].load_full()))
    })
}

pub(crate) fn forget() {
    if let Some(zones) = ZONES.get() {
        for kind in zones {
            kind.store(Arc::new(Zones::default()));
        }
    }
    for count in &ZONE_COUNT {
        count.store(u32::MAX, Ordering::Relaxed);
    }
    members::forget();
}

pub(crate) fn link(loader: &'static Loader) {
    let limits = LIMITS.seal();
    if !limits.is_empty() {
        loader.link_feature(
            "altar limit",
            Target::Pinned(pinned::building::MAX_ALTARS),
            &MAX_ALTARS_NEXT,
            max_altars,
            HookFlags::NONE,
        );
    }
    let Some(sites) = SITES.get() else {
        return;
    };
    // A refused link leaves its site unhooked, so arming keeps the game's
    // limits; the other links then hand every call on unchanged.
    let at = |symbol| Target::Pinned(symbol);
    let none = HookFlags::NONE;
    let only = HookFlags::EXCLUSIVE;
    let refused = [
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::GROUPING),
            &GROUP_NEXT,
            group,
            none,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::NEW_BASE),
            &ADD_BASE_NEXT,
            add_base,
            none,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::BASE_AT),
            &BASE_AT_NEXT,
            base_at,
            only,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::IN_BASE),
            &IN_BASE_NEXT,
            in_base,
            only,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::HOLDER_AT),
            &HOLDER_AT_NEXT,
            holder_at,
            only,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::ZONES_AT),
            &ZONES_AT_NEXT,
            zones_at,
            only,
        ),
        loader.link_feature(
            RAISED_ZONES,
            at(pinned::bases::CURSOR),
            &CHECK_NEXT,
            check,
            none,
        ),
    ]
    .into_iter()
    .filter(|linked| !linked)
    .count();
    if refused == 0 {
        let asked = ASKED.limits();
        crate::log!(
            INFO,
            "bases take {} altar zones in {} bases",
            asked.zones,
            asked.bases
        );
        if ASKED.asked_past_16() {
            past16::link(loader, sites.fields);
        }
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

unsafe extern "system" fn group(bases: *mut u8, world: *mut u8) {
    // SAFETY: `bases` is the grouping's own bases object when not null.
    let kind = SITES
        .get()
        .filter(|_| !bases.is_null())
        .map(|sites| unsafe { side_of(bases, &sites.fields) });
    if let Some(kind) = kind {
        PLANNED[kind_index(kind)].store(window::VANILLA, Ordering::Relaxed);
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
    let (Some(sites), Some(kind)) = (SITES.get().filter(|_| raised()), kind) else {
        return;
    };
    // SAFETY: `bases` is non-null, since `kind` is known, and is the bases
    // object the grouping has just filled, on the grouping's thread; the
    // game's new base code is what `ADD_BASE_NEXT` holds past our link.
    let settled = BASES_FUSE.run("the base grouping", || unsafe {
        regroup(bases, kind, sites)
    });
    if let Some(grouped) = settled.flatten() {
        // SAFETY: `bases` is non-null and is the object the game's grouping
        // and `settle` have just filled, on this thread.
        unsafe { keep(bases, kind, sites.fields, &grouped) };
    } else {
        Side::of(kind).forget();
        BASES_FUSE.blow("the base grouping's count");
    }
}

/// Releases the window records of `bases`, as the game's grouping has just
/// released its own, and settles our grouping on it.
///
/// # Safety
///
/// - `bases` is the live `kind` object the game's grouping has just filled,
///   on the grouping's thread.
unsafe fn regroup(bases: *mut u8, kind: Kind, sites: &Sites) -> Option<Grouped> {
    let window = past16::sites().map(past16::Sites::code);
    let ready = window.and_then(|code| {
        // SAFETY: the caller guarantees the live object on the grouping's
        // thread, which no reader runs beside.
        unsafe { lifecycle::release(bases, &code) };
        // SAFETY: the same object, live while `settle` uses the answer.
        unsafe { lifecycle::ready(bases, sites.fields.allocator, code) }
    });
    // SAFETY: the caller's guarantees; `ready` is there only for an object
    // whose window `release` has just released.
    unsafe {
        members::settle(
            bases,
            sites.fields,
            ASKED.bases(kind),
            ADD_BASE_NEXT.get(),
            ready.as_ref().map(|f| f as &dyn Fn(usize)),
        )
    }
}

/// Keeps what the grouping found, for the cursor and the API.
///
/// # Safety
///
/// - `bases` is the non-null bases object the grouping has just filled,
///   live for the call, on the grouping's thread.
unsafe fn keep(bases: *const u8, kind: Kind, fields: Fields, grouped: &Grouped) {
    let Some(known) = ZONES.get().map(|zones| &zones[kind_index(kind)]) else {
        return;
    };
    // SAFETY: `keep`'s caller guarantees a live object just grouped, on the
    // grouping's thread, which no reader runs beside.
    let game = unsafe { Game::new(bases, fields) };
    let listed = crate::base_groups::lookup::Bases::listed(&game);
    let zone = |i: usize| Zone {
        bounds: crate::base_groups::lookup::Bases::zone(&game, i),
        based: grouped.iter().any(|base| base.zones.contains(i)),
    };
    let count = u32::try_from(grouped.len()).unwrap_or(u32::MAX);
    ZONE_COUNT[kind_index(kind)].store(
        u32::try_from(listed).unwrap_or(u32::MAX - 1),
        Ordering::Relaxed,
    );
    if grouped.len() > window::VANILLA {
        let entered = members::take_entries_while_lent();
        crate::log!(
            DEBUG,
            "{count} bases grouped; whole-array calls while the count was lent: {entered}"
        );
    }
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
                ASKED.bases(kind)
            );
        } else {
            crate::log!(INFO, "{listed} altar zones in {count} bases");
        }
    }
    known.store(Arc::new(zones));
}

unsafe extern "system" fn add_base(bases: *mut u8, zone: *const u8, index: u32) {
    if let Some(fields) = SITES
        .get()
        .filter(|_| raised() && !bases.is_null())
        .map(|sites| sites.fields)
    {
        // SAFETY: `bases` is non-null by the filter before it and is the
        // bases object the grouping passes; its u32 base count sits at the
        // build table's `bases.count`.
        let started = unsafe { read_at::<u32>(bases, fields.count) };
        // SAFETY: `bases` is non-null by the filter and is the live object
        // the grouping passes to its new base code.
        let kind = unsafe { side_of(bases, &fields) };
        let planned = &PLANNED[kind_index(kind)];
        if started == 0 {
            // SAFETY: the game's grouping calls the new base code only after
            // it refilled the object's zone list, on its own thread.
            let plan = BASES_FUSE.run("the base plan", || unsafe {
                members::planned(bases, fields, ASKED.bases(kind))
            });
            planned.store(plan.unwrap_or(window::VANILLA), Ordering::Relaxed);
        }
        let limit = planned.load(Ordering::Relaxed).min(window::VANILLA);
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
    // SAFETY: `bases` is non-null by the check, and `view`'s caller
    // guarantees a live object read beside no grouping.
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
        && game.holds_index(i)
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
        // SAFETY: the game's live zone box container.
        let len = unsafe { members::list_len(list, &game.fields().container) };
        // SAFETY: the lookup's own answer and the game's live zone box
        // container with its own grow callback.
        let done = BASES_FUSE.run("a zone list lookup", || unsafe {
            members::zones_at(out, list, &game, p);
        });
        if done.is_some() {
            return out;
        }
        // SAFETY: the same container, whose boxes past `len` the game's own
        // lookup appends again.
        unsafe { members::set_list_len(list, &game.fields().container, len) };
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
        cursor::refuse_past_limit(flags, ctx, transform, item);
    });
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

    #[test]
    fn every_site_is_the_table_s_symbol_and_the_first_one_missing_is_named() {
        let address =
            |s: pinned::Symbol| Some(0x1000 + SITE_SYMBOLS.iter().position(|&t| t == s)? as u64);
        let sites = sites_of(fake::FIELDS, address).unwrap();
        assert_eq!(sites.targets().len(), SITE_SYMBOLS.len());
        assert_eq!(sites.cursor, address(pinned::bases::CURSOR).unwrap());
        let without_zones =
            |s: pinned::Symbol| (s != pinned::bases::ZONES_AT).then(|| address(s)).flatten();
        assert_eq!(
            sites_of(fake::FIELDS, without_zones).map(|_| ()),
            Err(pinned::bases::ZONES_AT)
        );
    }

    /// The newest table of each side reads as the fields and the loops'
    /// layout the tests' fake object is laid out by; the client's placement
    /// cursor holds more before its balancing.
    #[test]
    fn the_newest_tables_read_as_the_fake_object_s_layout() {
        use enshwick_gamedata::{Side as TableSide, newest};

        let tables = crate::builds::load();
        for (side, balancing) in [(TableSide::Server, 0x170), (TableSide::Client, 0x180)] {
            let resolved = newest(&tables, side).unwrap().resolve();
            let offset = |id: Constant| {
                resolved
                    .constant(id.id())
                    .and_then(|value| usize::try_from(value).ok())
            };
            let fields = Fields {
                balancing,
                ..fake::FIELDS
            };
            assert_eq!(Fields::read(&offset), Ok(fields), "{side:?}");
            assert_eq!(loops::Layout::read(&offset), Ok(fake::LAYOUT), "{side:?}");
        }
    }

    /// Fields whose answer cannot hold a zone box are refused, with the
    /// fields named.
    #[test]
    fn an_answer_too_small_for_a_zone_box_is_refused() {
        let offset = |id: Constant| {
            if id == pinned::bases::ANSWER_SIZE {
                Some(0x2c)
            } else {
                fake_offset(id)
            }
        };
        let refused = Fields::read(&offset).unwrap_err();
        assert!(refused.contains("do not fit the boxes"), "{refused}");
        assert_eq!(Fields::read(&fake_offset), Ok(fake::FIELDS));
    }

    /// The fake object's fields as the build table would give them.
    fn fake_offset(id: Constant) -> Option<usize> {
        let f = fake::FIELDS;
        let pairs = [
            (pinned::bases::COUNT, f.count),
            (pinned::bases::STRIDE, f.stride),
            (pinned::bases::MEMBERS, f.members),
            (pinned::bases::ZONE_LIST, f.zone_list),
            (pinned::bases::ZONE_LIST_LEN, f.zone_list_len),
            (pinned::bases::ZONE_SIZE, f.zone_size),
            (pinned::bases::ZONE_HIGH, f.zone_high),
            (pinned::bases::ITEM_ALTAR, f.item_altar),
            (
                pinned::bases::MAX_ALTARS_REACHED,
                f.max_altars_reached as usize,
            ),
            (pinned::bases::CURSOR_BALANCING, f.balancing),
            (pinned::bases::KIND, f.kind),
            (pinned::bases::ALLOCATOR, f.allocator),
            (pinned::bases::ANSWER_ERROR, f.answer.error),
            (pinned::bases::ANSWER_ZERO, f.answer.zero),
            (pinned::bases::ANSWER_INDEX, f.answer.index),
            (pinned::bases::ANSWER_BOX, f.answer.boxed),
            (pinned::bases::ANSWER_SIZE, f.answer.size),
            (pinned::bases::CONTAINER_DATA, f.container.data),
            (pinned::bases::CONTAINER_LEN, f.container.len),
            (pinned::bases::CONTAINER_CAP, f.container.cap),
            (pinned::bases::CONTAINER_GROW, f.container.grow),
            (pinned::bases::CONTAINER_BOX, f.container.boxed),
            (pinned::bases::HEAP_ALLOCATE, f.heap.allocate),
            (pinned::bases::HEAP_FREE, f.heap.free),
            (
                pinned::bases::ZONE_LIST_ALLOCATOR,
                f.zone_list + f.container.allocator,
            ),
        ];
        pairs
            .into_iter()
            .find(|&(c, _)| c == id)
            .map(|(_, value)| value)
    }

    /// The cursor's code check covers the offsets the detour reads: on each
    /// game executable `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name, the
    /// cursor's body in its table holds, and its hash covers the altar flag
    /// test and the balancing load at the table's values, outside every
    /// open field; skipped without them.
    #[test]
    fn the_cursor_s_body_fixes_the_offsets_its_detour_reads() {
        use enshwick_gamedata::reflect::{Image, Layout};
        use enshwick_gamedata::{Side, exact};

        let tables = crate::builds::load();
        for (variable, side) in [
            ("ENSH_SERVER_EXE", Side::Server),
            ("ENSH_CLIENT_EXE", Side::Client),
        ] {
            let Some(path) = std::env::var_os(variable) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let resolved = exact(&tables, side, image.fingerprint().unwrap())
                .unwrap()
                .resolve();
            let id = pinned::bases::CURSOR.id();
            let body = resolved.body(id).unwrap();
            let start = image.base() + resolved.symbol(id).unwrap();
            let code = image.read(start, body.len() as u64).unwrap();
            assert!(body.holds(code), "{variable}");
            let constant = |id: Constant| resolved.constant(id.id()).unwrap();
            let flag = u8::try_from(constant(pinned::bases::ITEM_ALTAR)).unwrap();
            let balancing = u32::try_from(constant(pinned::bases::CURSOR_BALANCING)).unwrap();
            let mut load = vec![0x48, 0x8B, 0x8E];
            load.extend_from_slice(&balancing.to_le_bytes());
            let fixed = |want: &[u8]| {
                code.windows(want.len()).enumerate().any(|(at, bytes)| {
                    bytes == want
                        && body.open().iter().all(|&open| {
                            let open = open as usize;
                            open + 4 <= at || at + want.len() <= open
                        })
                })
            };
            assert!(
                fixed(&[0x80, 0x7F, flag, 0x00]),
                "{variable}: the flag test"
            );
            assert!(fixed(&load), "{variable}: the balancing load");
        }
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
