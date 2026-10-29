//! The game's functions that walk every base, extended to the window.
//!
//! Each walks indices `0 .. count`, and the count stays at most 16, so they
//! never reach a window record. While an object holds more than 16 bases,
//! the sort into bases, the logic entity rebuild and the building stock
//! cache are replaced by code that walks the object's index list; the sort
//! of every entity, the NPC and animal totals, the cleanups and the list of
//! base ids do the same work for the window beside the game's. The client's
//! base list sizes itself by a count getter, patched to reach the highest
//! index in use. With 16 bases or fewer every function runs as the game's.
//! Facts from server 399 and client 426, where the bodies are the same.

mod rebuild;
mod sort;
mod stock;
#[cfg(test)]
mod tests;
mod totals;

use enshwick::abi::HookFlags;
use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::Container;
use super::members::Game;
use super::past16::{Pins, SiteError};
use crate::base_groups::lookup::Bases;
use crate::base_groups::window;
use crate::code::CodePatch;
use crate::host::{Loader, Target};
use crate::mod_index::ModIndex;
use crate::pinned::{Constant, bases};

/// The client's count getter the base list sizes itself by, `mov eax,
/// [rcx+count]; ret`, and the padding up to the next function, which a
/// jump to ours replaces.
const fn getter(count: u8) -> [u8; 14] {
    [
        0x8B, 0x41, count, 0xC3, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
    ]
}

/// A position's fixed point to metres, as the sort and the rebuild scale
/// it: 2⁻³².
const FIXED_TO_METRES: f32 = 1.0 / 4_294_967_296.0;

/// The bytes a stock entity list spans in a record: the game's container,
/// data to grow callback.
const LIST_BYTES: usize = 32;
/// The bed counts in a record, u32 each, and the animal counts, u16 each:
/// all, kind 1, kinds 2 to 9, flag 4, flag 2.
pub(super) const BEDS: usize = 3;
pub(super) const ANIMAL_COUNTS: usize = 5;

/// What the loops over every base read and write, from the build table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Layout {
    pub(super) record: RecordFields,
    pub(super) object: ObjectFields,
    pub(super) map: MapFields,
    pub(super) stock: StockFields,
    /// The object's u32 base count, which the client's count getter reads.
    pub(super) count: usize,
}

/// A record's fields, from its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecordFields {
    /// The stock entity lists: all, and those the filter keeps.
    pub(super) list_all: usize,
    pub(super) list_kept: usize,
    /// The client's logic entity set: itself, its bitmap, bucket count and
    /// u32 entry count.
    pub(super) set: usize,
    pub(super) set_bitmap: usize,
    pub(super) set_buckets: usize,
    pub(super) set_count: usize,
    /// Best comfort per category, u32 each, and the bytes they span.
    pub(super) comfort: usize,
    pub(super) comfort_len: usize,
    /// The crafting prop mask, u64.
    pub(super) props: usize,
    /// The base's `BaseId`, 16 bytes.
    pub(super) base_id: usize,
    /// [`BEDS`] bed counts, u32 each.
    pub(super) beds: usize,
    /// NPCs at home, u16.
    pub(super) npcs_home: usize,
    /// [`ANIMAL_COUNTS`] animal counts, u16 each.
    pub(super) animals: usize,
}

/// The bases object's fields past the records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ObjectFields {
    /// The zone list version the rebuild last saw, the change counter and
    /// the version the grouping last saw, u32 each.
    pub(super) rebuilt: usize,
    pub(super) changes: usize,
    pub(super) grouped: usize,
    /// Animals in every base, u32; NPCs in every base and NPCs at home, u16.
    pub(super) animals: usize,
    pub(super) npcs: usize,
    pub(super) npcs_home: usize,
    /// Entity to `PlacedNpcInfo`: the map, each value's size and its u8
    /// kind, after its u8 base.
    pub(super) npc_map: usize,
    pub(super) npc_value: usize,
    pub(super) npc_kind: usize,
    /// Entity to `{base, flags, kind}`, animals and their beds: the map and
    /// each value's size.
    pub(super) animal_map: usize,
    pub(super) animal_value: usize,
    /// The stock cache's staging map, and its u32 page and position.
    pub(super) staging: usize,
    pub(super) page: usize,
    pub(super) position: usize,
}

/// The game's fixed-capacity hash map: bitmap of occupied buckets, bucket
/// count, keys (u32), values, u32 entry count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MapFields {
    pub(super) bitmap: usize,
    pub(super) buckets: usize,
    pub(super) keys: usize,
    pub(super) values: usize,
    pub(super) count: usize,
}

/// The stock cache's answer: the staging map first, the page's entities and
/// their u64 count, in `size` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StockFields {
    pub(super) entities: usize,
    pub(super) count: usize,
    pub(super) size: usize,
}

/// The first table entry of the game's container `c` whose field lies past
/// the bytes a record's stock list spans; `None` when every one fits.
pub(super) fn list_misfit(c: &Container) -> Option<&'static str> {
    [
        (bases::CONTAINER_DATA, c.data),
        (bases::CONTAINER_LEN, c.len),
        (bases::CONTAINER_CAP, c.cap),
        (bases::CONTAINER_GROW, c.grow),
    ]
    .into_iter()
    .find(|&(_, at)| at + 8 > LIST_BYTES)
    .map(|(id, _)| id.id())
}

impl Layout {
    /// The layout as `offset` reads the build table's constants, each field
    /// checked to lie inside its record, the object below its window
    /// records, or the stock cache's answer.
    pub(super) fn read(offset: &dyn Fn(Constant) -> Option<usize>) -> Result<Self, SiteError> {
        let f = |id: Constant| offset(id).ok_or(SiteError::Missing(id.id()));
        let layout = Self {
            record: RecordFields {
                list_all: f(bases::RECORD_LIST_ALL)?,
                list_kept: f(bases::RECORD_LIST_KEPT)?,
                set: f(bases::RECORD_SET)?,
                set_bitmap: f(bases::RECORD_SET_BITMAP)?,
                set_buckets: f(bases::RECORD_SET_BUCKETS)?,
                set_count: f(bases::RECORD_SET_COUNT)?,
                comfort: f(bases::RECORD_COMFORT)?,
                comfort_len: f(bases::RECORD_COMFORT_LEN)?,
                props: f(bases::RECORD_PROPS)?,
                base_id: f(bases::RECORD_BASE_ID)?,
                beds: f(bases::RECORD_BEDS)?,
                npcs_home: f(bases::RECORD_NPCS_HOME)?,
                animals: f(bases::RECORD_ANIMALS)?,
            },
            object: ObjectFields {
                rebuilt: f(bases::REBUILT)?,
                changes: f(bases::CHANGES)?,
                grouped: f(bases::GROUPED)?,
                animals: f(bases::ANIMALS)?,
                npcs: f(bases::NPCS)?,
                npcs_home: f(bases::NPCS_HOME)?,
                npc_map: f(bases::NPC_MAP)?,
                npc_value: f(bases::NPC_VALUE)?,
                npc_kind: f(bases::NPC_KIND)?,
                animal_map: f(bases::ANIMAL_MAP)?,
                animal_value: f(bases::ANIMAL_VALUE)?,
                staging: f(bases::STAGING)?,
                page: f(bases::PAGE)?,
                position: f(bases::POSITION)?,
            },
            map: MapFields {
                bitmap: f(bases::MAP_BITMAP)?,
                buckets: f(bases::MAP_BUCKETS)?,
                keys: f(bases::MAP_KEYS)?,
                values: f(bases::MAP_VALUES)?,
                count: f(bases::MAP_COUNT)?,
            },
            stock: StockFields {
                entities: f(bases::STOCK_ENTITIES)?,
                count: f(bases::STOCK_COUNT)?,
                size: f(bases::STOCK_ANSWER_SIZE)?,
            },
            count: f(bases::COUNT)?,
        };
        layout
            .misfit()
            .map_or(Ok(layout), |id| Err(SiteError::Fit(id)))
    }

    /// The first table entry whose field lies outside its record, the
    /// object's fields below the window's records and stock slots, a map
    /// outside a stock slot, a set field before its set, or a field outside
    /// the stock cache's answer; `None` when every one fits.
    fn misfit(&self) -> Option<&'static str> {
        let r = &self.record;
        let o = &self.object;
        let m = &self.map;
        let s = &self.stock;
        let object_end = (window::RECORDS + window::WINDOW * window::RECORD)
            .min(window::SLOTS + window::WINDOW * window::SLOT);
        let map_end = [
            m.bitmap + 8,
            m.buckets + 8,
            m.keys + 8,
            m.values + 8,
            m.count + 4,
        ]
        .into_iter()
        .max()
        .unwrap_or(0);
        let in_record = [
            (bases::RECORD_LIST_ALL, r.list_all + LIST_BYTES),
            (bases::RECORD_LIST_KEPT, r.list_kept + LIST_BYTES),
            (bases::RECORD_SET_BITMAP, r.set_bitmap + 8),
            (bases::RECORD_SET_BUCKETS, r.set_buckets + 8),
            (bases::RECORD_SET_COUNT, r.set_count + 4),
            (bases::RECORD_COMFORT_LEN, r.comfort + r.comfort_len),
            (bases::RECORD_PROPS, r.props + 8),
            (bases::RECORD_BASE_ID, r.base_id + 16),
            (bases::RECORD_BEDS, r.beds + BEDS * 4),
            (bases::RECORD_NPCS_HOME, r.npcs_home + 2),
            (bases::RECORD_ANIMALS, r.animals + ANIMAL_COUNTS * 2),
        ]
        .map(|(id, end)| (id, end, window::RECORD));
        let in_object = [
            (bases::REBUILT, o.rebuilt + 4),
            (bases::CHANGES, o.changes + 4),
            (bases::GROUPED, o.grouped + 4),
            (bases::ANIMALS, o.animals + 4),
            (bases::NPCS, o.npcs + 2),
            (bases::NPCS_HOME, o.npcs_home + 2),
            (bases::NPC_MAP, o.npc_map + map_end),
            (bases::ANIMAL_MAP, o.animal_map + map_end),
            (bases::STAGING, o.staging + map_end),
            (bases::PAGE, o.page + 8),
            (bases::POSITION, o.position + 4),
            (bases::COUNT, self.count + 4),
        ]
        .map(|(id, end)| (id, end, object_end));
        let in_values = [
            (bases::MAP_COUNT, map_end, window::SLOT),
            (
                bases::RECORD_SET,
                r.set,
                r.set_bitmap.min(r.set_buckets).min(r.set_count),
            ),
            (bases::NPC_KIND, o.npc_kind + 1, o.npc_value),
            (bases::ANIMAL_VALUE, 3, o.animal_value),
            (bases::STOCK_ENTITIES, s.entities + 8, s.size),
            (bases::STOCK_COUNT, s.count + 8, s.size),
        ];
        in_record
            .into_iter()
            .chain(in_object)
            .chain(in_values)
            .find(|&(_, end, room)| end > room)
            .map(|(id, ..)| id.id())
    }
}

type ObjWorldFn = unsafe extern "system" fn(obj: *mut u8, world: *mut u8);
type ObjFn = unsafe extern "system" fn(obj: *mut u8) -> u64;
type QueryFn = unsafe extern "system" fn(found: *mut u8, world: *mut u8, hash: u32) -> u8;
type InsertFn =
    unsafe extern "system" fn(set: *mut u8, result: *mut u8, key: *const u32) -> *mut u8;
type FindFn = unsafe extern "system" fn(map: *mut u8, key: *const u32) -> u64;
type AdvanceFn = unsafe extern "system" fn(obj: *mut u8);
type GrowFn = unsafe extern "system" fn(vec: *mut u8, wanted: i64, exact: u64) -> u8;

/// Every loop site, as addresses.
#[derive(Debug)]
pub(super) struct Sites {
    pub(super) layout: Layout,
    sort: u64,
    sort_all: u64,
    rebuild: u64,
    npc_totals: u64,
    server_update: u64,
    stock_cache: u64,
    base_ids: u64,
    cleanups: [u64; 3],
    hash_at: usize,
    query: usize,
    insert: usize,
    find: usize,
    advance: usize,
    getter: Option<(usize, [u8; 14])>,
}

impl Sites {
    pub(super) fn targets(&self) -> impl Iterator<Item = u64> + '_ {
        [
            self.sort,
            self.sort_all,
            self.rebuild,
            self.npc_totals,
            self.server_update,
            self.stock_cache,
            self.base_ids,
        ]
        .into_iter()
        .chain(self.cleanups)
    }

    /// The patch that sends the client's count getter to ours, on a client.
    pub(super) fn patches(&self) -> Vec<CodePatch> {
        let ours = totals::shown_bases as unsafe extern "system" fn(*const u8) -> u32;
        self.getter
            .map(|(at, expected)| CodePatch {
                owner: ModIndex::LOADER,
                address: at,
                expected: expected.into(),
                replacement: jump_to((ours as *const ()).addr()).into(),
                needs: Box::default(),
            })
            .into_iter()
            .collect()
    }
}

/// `jmp [rip+0]` and the absolute address: 14 bytes.
fn jump_to(to: usize) -> [u8; 14] {
    let mut bytes = [0u8; 14];
    bytes[..6].copy_from_slice(&[0xFF, 0x25, 0, 0, 0, 0]);
    bytes[6..].copy_from_slice(&(to as u64).to_le_bytes());
    bytes
}

/// The game's code the replaced loops call.
#[derive(Clone, Copy)]
struct Code {
    hash: u32,
    query: QueryFn,
    insert: InsertFn,
    find: FindFn,
    advance: AdvanceFn,
}

static CODE: std::sync::OnceLock<Code> = std::sync::OnceLock::new();

/// Finds every loop site in `code`, which starts at `start`, from the build
/// table's `pins`; the client's count getter only on a `client`.
pub(super) fn find(
    code: &[u8],
    start: usize,
    client: bool,
    pins: &Pins<'_>,
) -> Result<Sites, SiteError> {
    let address = |id| pins.at(id).map(|offset| start + offset);
    let at = |id| address(id).map(|a| a as u64);
    let layout = Layout::read(&|id| pins.offset(id))?;
    let getter = if client {
        let getter = pins.at(bases::COUNT_GETTER)?;
        let expected = u8::try_from(layout.count)
            .map(self::getter)
            .map_err(|_| SiteError::Getter)?;
        if code.get(getter..getter + expected.len()) != Some(&expected[..]) {
            return Err(SiteError::Getter);
        }
        Some((start + getter, expected))
    } else {
        None
    };
    Ok(Sites {
        layout,
        sort: at(bases::SORT)?,
        sort_all: at(bases::SORT_ALL)?,
        rebuild: at(bases::REBUILD)?,
        npc_totals: at(bases::NPC_TOTALS)?,
        server_update: at(bases::SERVER_UPDATE)?,
        stock_cache: at(bases::STOCK_CACHE)?,
        base_ids: at(bases::BASE_IDS)?,
        cleanups: [
            at(bases::BED_CLEANUP)?,
            at(bases::COMFORT_CLEANUP)?,
            at(bases::PROPS_CLEANUP)?,
        ],
        hash_at: address(bases::ENTITY_HASH)?,
        query: address(bases::ENTITY_QUERY)?,
        insert: address(bases::SET_INSERT)?,
        find: address(bases::STOCK_FIND)?,
        advance: address(bases::STOCK_ADVANCE)?,
        getter,
    })
}

/// Links every loop site and resolves the code the replacements call;
/// whether each link holds.
pub(super) fn link(loader: &Loader, sites: &Sites) -> [bool; 10] {
    // SAFETY: the table's `bases.entity_hash`, a u32 the table pins inside
    // the game's mapped image through the rebuild's own `mov r8d, [rip+d]`,
    // so it is readable before the rebuild's code check; a rebuild that does
    // not hold is never linked, so the value goes unused.
    let hash = unsafe { read_at::<u32>(sites.hash_at as *const u8, 0) };
    let code = Code {
        hash,
        // SAFETY: the table's `bases.entity_query`, the entity query the
        // rebuild calls.
        query: unsafe { core::mem::transmute::<usize, QueryFn>(sites.query) },
        // SAFETY: the table's `bases.set_insert`, the set insert the rebuild
        // calls.
        insert: unsafe { core::mem::transmute::<usize, InsertFn>(sites.insert) },
        // SAFETY: the table's `bases.stock_find`, the find-or-free the stock
        // cache calls.
        find: unsafe { core::mem::transmute::<usize, FindFn>(sites.find) },
        // SAFETY: the table's `bases.stock_advance`, the advance the stock
        // cache calls.
        advance: unsafe { core::mem::transmute::<usize, AdvanceFn>(sites.advance) },
    };
    let _first = CODE.set(code);
    let none = HookFlags::NONE;
    let only = HookFlags::EXCLUSIVE;
    let at = |symbol| Target::Pinned(symbol);
    [
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::SORT),
            &sort::SORT_NEXT,
            sort::sort,
            only,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::SORT_ALL),
            &sort::SORT_ALL_NEXT,
            sort::sort_all,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::BASE_IDS),
            &sort::BASE_IDS_NEXT,
            sort::base_ids,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::REBUILD),
            &rebuild::REBUILD_NEXT,
            rebuild::rebuild,
            only,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::STOCK_CACHE),
            &stock::STOCK_CACHE_NEXT,
            stock::stock_cache,
            only,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::NPC_TOTALS),
            &totals::NPC_TOTALS_NEXT,
            totals::npc_totals,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::SERVER_UPDATE),
            &totals::SERVER_UPDATE_NEXT,
            totals::server_update,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::BED_CLEANUP),
            &totals::BED_CLEANUP_NEXT,
            totals::bed_cleanup,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::COMFORT_CLEANUP),
            &totals::COMFORT_CLEANUP_NEXT,
            totals::comfort_cleanup,
            none,
        ),
        loader.link_feature(
            super::past16::PAST_16,
            at(bases::PROPS_CLEANUP),
            &totals::PROPS_CLEANUP_NEXT,
            totals::props_cleanup,
            none,
        ),
    ]
}

/// The bases of `obj` when it holds more than the game's 16 and the window
/// is armed, with the window's layout; `None` sends a call to the game's own
/// code. Every whole-array function enters here, so it also counts entries
/// while the grouping lends the count.
///
/// # Safety
///
/// - `obj` is null or a live bases object, read on a thread the grouping
///   does not run beside.
unsafe fn windowed(obj: *const u8) -> Option<(Game, &'static Layout)> {
    super::members::note_entry();
    if !super::past_16() {
        return None;
    }
    let layout = &super::past16::sites()?.loops.layout;
    // SAFETY: the caller's guarantee, passed on.
    let game = unsafe { super::view(obj) }?;
    (game.count() > window::VANILLA).then_some((game, layout))
}

/// The window's layout once its sites are found, armed or not: the count
/// getter's patch reads its count by it.
fn found_layout() -> Option<&'static Layout> {
    super::past16::found().map(|sites| &sites.loops.layout)
}

/// The window indices of `game`'s bases.
fn window_of(game: &Game) -> impl Iterator<Item = usize> {
    window::window_indices(game.count())
}

/// Record `i` of `obj`.
fn rec(obj: *mut u8, i: usize) -> *mut u8 {
    obj.wrapping_add(window::RECORDS + i * window::RECORD)
}

/// The point of an entity's fixed-point position, three i64 at `at`.
///
/// # Safety
///
/// - `at` is readable for three i64.
#[expect(clippy::cast_precision_loss, reason = "the game converts the same way")]
unsafe fn point(at: *const u8) -> [f32; 3] {
    // SAFETY: the caller guarantees three readable i64.
    let fixed = unsafe { read_at::<[i64; 3]>(at, 0) };
    fixed.map(|v| v as f32 * FIXED_TO_METRES)
}

/// Each occupied bucket of the game's map at `map`.
///
/// # Safety
///
/// - `map` is a live map of the game's, laid out as `fields`, unchanged
///   while the iterator is used.
unsafe fn occupied(map: *const u8, fields: &MapFields) -> impl Iterator<Item = usize> {
    // SAFETY: the caller guarantees a live map laid out as `fields`; its
    // bitmap pointer.
    let bitmap = unsafe { read_ptr_at::<u8>(map, fields.bitmap) };
    // SAFETY: the caller's live map laid out as `fields`; its u64 bucket
    // count.
    let buckets = usize::try_from(unsafe { read_at::<u64>(map, fields.buckets) }).unwrap_or(0);
    let words = if bitmap.is_null() {
        0
    } else {
        buckets.div_ceil(64)
    };
    (0..words).flat_map(move |w| {
        // SAFETY: word `w` of the map's bitmap, which holds a bit per
        // bucket, `words` u64 words.
        let mut word = unsafe { read_at::<u64>(bitmap, w * 8) };
        core::iter::from_fn(move || {
            (word != 0).then(|| {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                w * 64 + bit
            })
        })
        .filter(move |&b| b < buckets)
    })
}

/// Adds `by` to the u16 at `at` in `base`, wrapping as the game does.
///
/// # Safety
///
/// - `base + at` is a live, writable u16.
unsafe fn add_u16(base: *mut u8, at: usize, by: u16) {
    // SAFETY: the caller guarantees a live u16 at `at`.
    let value = unsafe { read_at::<u16>(base, at) };
    // SAFETY: the caller guarantees the u16 at `at` is writable.
    unsafe { write_at(base, at, value.wrapping_add(by)) };
}

/// Appends `value` to the game's container `vec`, laid out as `c`, through
/// its own grow callback; `false` when it cannot grow or holds no room, as
/// the game.
///
/// # Safety
///
/// - `vec` is the game's live container of `T`-sized elements, laid out as
///   `c`, whose grow callback, when set, is the game's.
unsafe fn append<T: enshwick::raw::NoUninit>(vec: *mut u8, c: &Container, value: T) -> bool {
    // SAFETY: the caller's live container laid out as `c`; its u64 length.
    let len = unsafe { read_at::<u64>(vec, c.len) };
    // SAFETY: the caller's live container; its u64 capacity.
    let cap = unsafe { read_at::<u64>(vec, c.cap) };
    if cap < len + 1 {
        // SAFETY: the caller's live container; its grow callback, null or
        // the game's.
        let grow = unsafe { read_ptr_at::<u8>(vec, c.grow) };
        if grow.is_null() {
            return false;
        }
        // SAFETY: a non-null callback the game stored in its own container,
        // of the shape the game calls it with.
        let grow: GrowFn = unsafe { core::mem::transmute::<*mut u8, GrowFn>(grow) };
        let wanted = i64::try_from(len + 1).map_or(i64::MIN, |n| -n);
        // SAFETY: the game's callback on its own container, as it calls it.
        if unsafe { grow(vec, wanted, 1) } == 0 {
            return false;
        }
    }
    // SAFETY: the caller's live container; its data pointer, with room for
    // `len + 1` elements now.
    let data = unsafe { read_ptr_at::<u8>(vec, c.data) };
    let Ok(at) = usize::try_from(len) else {
        return false;
    };
    if data.is_null() {
        return false;
    }
    // SAFETY: element `len` is room the container owns, past its length.
    unsafe { write_at(data, at * size_of::<T>(), value) };
    // SAFETY: the caller's live container; its u64 length.
    unsafe { write_at(vec, c.len, len + 1) };
    true
}
