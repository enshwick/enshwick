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
mod detours;
#[cfg(test)]
mod fake;
mod heap;
mod lifecycle;
mod loops;
mod members;
mod past16;
#[cfg(test)]
mod tests;

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{AltarLimitFilterFn, HookFlags, Status};

use self::detours::{
    add_base, base_at, check, group, holder_at, in_base, max_altars, view, zones_at,
};
use crate::base_groups::limit::{Limits, MAX_BASES, VANILLA_ZONES, Zones};
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
    pinned::bases::SORT_ANSWER_WORD.id(),
    pinned::bases::SORT_ENTITY_POSITION.id(),
    pinned::bases::RECORDS.id(),
    pinned::bases::SLOTS.id(),
    pinned::bases::SLOT.id(),
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

/// Why the bases' fields are not read, with the table entry it fails on.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
enum FieldsError {
    /// The build table lacks an entry.
    #[error("{}", crate::builds::ABSENT)]
    Missing(&'static str),
    /// An entry's value is out of its range, or the fields it bounds do not
    /// fit the boxes, answers and heap the lookups use.
    #[error("does not fit the boxes, answers and heap the lookups use")]
    Fit(&'static str),
}

impl FieldsError {
    /// The table entry the read fails on.
    const fn entry(&self) -> &'static str {
        match self {
            Self::Missing(id) | Self::Fit(id) => id,
        }
    }
}

impl Fields {
    /// The fields as `offset` reads the build table's constants, checked to
    /// fit the copies and answers the lookups make.
    fn read(offset: &dyn Fn(Constant) -> Option<usize>) -> Result<Self, FieldsError> {
        let f = |id: Constant| offset(id).ok_or(FieldsError::Missing(id.id()));
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
                .map_err(|_| FieldsError::Fit(pinned::bases::MAX_ALTARS_REACHED.id()))?,
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
                    .ok_or(FieldsError::Fit(pinned::bases::ZONE_LIST_ALLOCATOR.id()))?,
            },
            heap: heap::Slots {
                allocate: f(pinned::bases::HEAP_ALLOCATE)?,
                free: f(pinned::bases::HEAP_FREE)?,
            },
        };
        match fields.misfit() {
            Some(entry) => Err(FieldsError::Fit(entry.id())),
            None => Ok(fields),
        }
    }

    /// The first entry that breaks the fit, if any: a zone box holds both
    /// corners, the answer holds its error, zero word, index and a whole
    /// box, the zone list's length is the container's, whose boxes are the
    /// zone list's, and the heap's slots fit its vtable.
    fn misfit(&self) -> Option<Constant> {
        let Answer {
            error,
            zero,
            index,
            boxed,
            size,
        } = self.answer;
        let within =
            |at: usize, len: usize, room: usize| at.checked_add(len).is_some_and(|end| end <= room);
        let checks = [
            (self.members >= self.zone_size, pinned::bases::MEMBERS),
            (
                within(self.zone_high, 12, self.zone_size),
                pinned::bases::ZONE_HIGH,
            ),
            (error < size, pinned::bases::ANSWER_ERROR),
            (within(zero, 4, size), pinned::bases::ANSWER_ZERO),
            (index < size, pinned::bases::ANSWER_INDEX),
            (
                within(boxed, self.zone_size, size),
                pinned::bases::ANSWER_BOX,
            ),
            (
                self.zone_list.checked_add(self.container.len) == Some(self.zone_list_len),
                pinned::bases::CONTAINER_LEN,
            ),
            (
                self.container.boxed == self.zone_size,
                pinned::bases::CONTAINER_BOX,
            ),
            (self.heap.fit(), pinned::bases::HEAP_ALLOCATE),
        ];
        checks
            .into_iter()
            .find(|&(fits, _)| !fits)
            .map(|(_, entry)| entry)
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
        Err(why) => {
            loader.feature_off(RAISED_ZONES, why.entry(), &why.to_string());
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

/// Turns off bases past 16, which rests on code patches, with a plain store:
/// see [`crate::code::disarm`].
pub(crate) fn disarm() {
    past16::disarm();
}

/// Tells once, after the hooks are armed, which base features are off,
/// apart from those a skipped patch already reported among `already`.
pub(crate) fn report(loader: &Loader, already: &[&str]) {
    past16::report(loader, already);
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
