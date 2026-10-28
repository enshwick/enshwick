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

use enshwick::abi::{HookFlags, Status};
use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::members::Game;
use super::past16::call_target;
use crate::base_groups::lookup::{self, Bases};
use crate::base_groups::window;
use crate::chains::Next;
use crate::code::CodePatch;
use crate::host::Loader;
use crate::mod_index::ModIndex;

/// Sorts one entity into the first base holding it:
/// `fn(out, obj, entity, filtered) -> out`.
const SORT: &str = "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18 48 89 7C 24 20 41 56 \
                    48 81 EC 80 00 00 00 41 8B 18";
/// Sorts every stock entity into the bases: `fn(obj, world)`.
const SORT_ALL: &str = "48 89 5C 24 10 48 89 6C 24 18 56 57 41 56 B8 60 20 00 00 E8 ?? ?? ?? ??";
/// The client's logic entity sets: `fn(obj, world)`.
const REBUILD: &str =
    "40 55 57 41 56 48 8D AC 24 60 E0 FF FF B8 A0 20 00 00 E8 ?? ?? ?? ?? 48 2B E0";
/// In the rebuild: `mov r8d, [rip+hash]`, the entity query's call, the
/// set insert's call.
const REBUILD_HASH_AT: usize = 0xbc;
const REBUILD_QUERY_AT: usize = 0xef;
const REBUILD_INSERT_AT: usize = 0x1d1;
const MOV_R8D_RIP: [u8; 3] = [0x44, 0x8B, 0x05];
/// The NPC totals: `fn(obj)`.
const NPC_TOTALS: &str =
    "48 89 5C 24 18 48 89 74 24 20 41 56 48 83 EC 20 45 33 F6 48 8B D9 41 8B F6";
/// The server's update, which counts the animals of each base: `fn(obj,
/// world)`.
const SERVER_UPDATE: &str =
    "40 56 41 54 48 83 EC 38 48 8B F1 4C 89 6C 24 30 48 8B CA 4C 89 74 24 28";
/// The building stock cache, one base a call: `fn(out, obj) -> out`; its
/// advance call and its find-or-free call.
const STOCK_CACHE: &str =
    "40 56 41 56 41 57 48 83 EC 30 8B 82 EC 1B 00 00 45 33 FF 4C 8B F2 48 8B F1";
const STOCK_ADVANCE_AT: usize = 0x88;
const STOCK_FIND_AT: usize = 0x196;
/// The base ids of the 16 records: `fn(vec, obj)`.
const BASE_IDS: &str = "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8D 5A 30 48 8B F9 \
                        48 8D B3 00 13 00 00";
/// The three cleanups, `fn(obj)`, each with the first byte after it: every
/// leaf has a dead twin of the same bytes before it.
const BED_CLEANUP: &str = "45 33 C9 4C 8B C1 41 8B D1 39 51 24 76 30 48 8D 81 48 01 00 00 66 66 66 \
                           0F 1F 84 00 00 00 00 00 44 89 48 FC 48 FF C2 44 89 08 44 89 48 04 48 8D \
                           80 30 01 00 00 41 8B 48 24 48 3B D1 72 E2 C3 CC 33";
const COMFORT_CLEANUP: &str = "33 D2 4C 8B C1 39 51 24 76 31 48 8D 81 C0 00 00 00 0F 57 C0 33 C9 48 \
                               FF C2 0F 11 00 0F 11 40 10 0F 11 40 20 0F 11 40 30 89 48 40 48 8D 80 \
                               30 01 00 00 41 8B 48 24 48 3B D1 72 D6 C3 CC CC CC CC 45";
const PROPS_CLEANUP: &str = "45 33 C9 45 8B C1 44 39 41 24 76 29 48 8D 91 08 01 00 00 0F 1F 40 00 \
                             66 0F 1F 84 00 00 00 00 00 4C 89 0A 48 8D 92 30 01 00 00 8B 41 24 49 \
                             FF C0 4C 3B C0 72 EB C3 CC CC CC CC CC CC CC CC CC CC 40";
/// The client's base list, which sizes itself by the count getter it calls
/// here; the getter, `mov eax, [rcx+0x24]; ret`, and the padding up to the
/// next function, which a jump to ours replaces.
const UI_BASES: &str =
    "40 53 55 56 57 41 55 41 56 41 57 48 83 EC 40 41 B8 18 00 00 00 48 8D 54 24 20";
const UI_GETTER_AT: usize = 0x31;
/// The count the getter reads, `[rcx+0x24]` in its first bytes.
const GETTER_COUNT: usize = 0x24;
const GETTER: [u8; 14] = [
    0x8B, 0x41, 0x24, 0xC3, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC, 0xCC,
];

/// A position's fixed point to metres, as the sort and the rebuild scale
/// it: 2⁻³².
const FIXED_TO_METRES: f32 = 1.0 / 4_294_967_296.0;

/// A record's fields, from its start.
mod record {
    /// The stock entity lists: all, and those the filter keeps.
    pub(super) const LIST_ALL: usize = 0x00;
    pub(super) const LIST_KEPT: usize = 0x28;
    /// The client's logic entity set: bitmap, bucket count, entry count.
    pub(super) const SET: usize = 0x50;
    pub(super) const SET_BITMAP: usize = 0x58;
    pub(super) const SET_BUCKETS: usize = 0x60;
    pub(super) const SET_COUNT: usize = 0x88;
    /// Best comfort per category, 17 u32.
    pub(super) const COMFORT: usize = 0x90;
    pub(super) const COMFORT_LEN: usize = 17 * 4;
    /// The crafting prop mask, u64.
    pub(super) const PROPS: usize = 0xd8;
    /// The base's `BaseId`, 16 bytes.
    pub(super) const BASE_ID: usize = 0xe0;
    /// Bed counts, three u32.
    pub(super) const BEDS: [usize; 3] = [0x114, 0x118, 0x11c];
    /// NPCs at home, u16.
    pub(super) const NPCS_HOME: usize = 0x120;
    /// Animal counts, five u16: all, kind 1, kinds 2 to 9, flag 4, flag 2.
    pub(super) const ANIMALS: usize = 0x122;
    pub(super) const ANIMALS_KIND_1: usize = 0x124;
    pub(super) const ANIMALS_KINDS: usize = 0x126;
    pub(super) const ANIMALS_FLAG_4: usize = 0x128;
    pub(super) const ANIMALS_FLAG_2: usize = 0x12a;
}

/// The bases object's fields past the records.
mod object {
    /// The zone list version the rebuild last saw, the change counter and
    /// the version the grouping last saw, u32 each.
    pub(super) const REBUILT: usize = 0x18;
    pub(super) const CHANGES: usize = 0x1c;
    pub(super) const GROUPED: usize = 0x20;
    /// Animals in every base, u32; NPCs in every base and NPCs at home, u16.
    pub(super) const ANIMALS: usize = 0x28;
    pub(super) const NPCS: usize = 0x2c;
    pub(super) const NPCS_HOME: usize = 0x2e;
    /// Entity to `PlacedNpcInfo` (base u8 at 0, kind u8 at 8, 12 bytes).
    pub(super) const NPC_MAP: usize = 0x1560;
    pub(super) const NPC_VALUE: usize = 0xc;
    pub(super) const NPC_KIND_AT: usize = 8;
    pub(super) const NPC_AT_HOME: u8 = 7;
    /// Entity to `{base, flags, kind}`: animals and their beds.
    pub(super) const ANIMAL_MAP: usize = 0x15b8;
    pub(super) const ANIMAL_VALUE: usize = 3;
    /// The stock cache's staging map, page and position.
    pub(super) const STAGING: usize = 0x1b90;
    pub(super) const PAGE: usize = 0x1be8;
    pub(super) const POSITION: usize = 0x1bec;
}

/// The game's fixed-capacity hash map: bitmap of occupied buckets, bucket
/// count, keys (u32), values, entry count.
mod map {
    pub(super) const BITMAP: usize = 0x08;
    pub(super) const BUCKETS: usize = 0x10;
    pub(super) const KEYS: usize = 0x28;
    pub(super) const VALUES: usize = 0x40;
    pub(super) const COUNT: usize = 0x54;
}

/// The game's container: data, length, capacity, grow callback.
mod vec {
    pub(super) const DATA: usize = 0x00;
    pub(super) const LEN: usize = 0x08;
    pub(super) const CAP: usize = 0x10;
    pub(super) const GROW: usize = 0x18;
}

/// The stock cache's page of entities a call hands back.
const PAGE_ENTITIES: u32 = 40;
/// The sort's answers.
const SORTED: u8 = 0x00;
const NOT_IN_A_BASE: u8 = 0x1f;
const GROW_FAILED: u8 = 0x27;

type SortFn = unsafe extern "system" fn(
    out: *mut u8,
    obj: *mut u8,
    entity: *const u8,
    filtered: u64,
) -> *mut u8;
type ObjWorldFn = unsafe extern "system" fn(obj: *mut u8, world: *mut u8);
type ObjFn = unsafe extern "system" fn(obj: *mut u8) -> u64;
type StockFn = unsafe extern "system" fn(out: *mut u8, obj: *mut u8) -> *mut u8;
type BaseIdsFn = unsafe extern "system" fn(vec: *mut u8, obj: *mut u8) -> u64;
type QueryFn = unsafe extern "system" fn(found: *mut u8, world: *mut u8, hash: u32) -> u8;
type InsertFn =
    unsafe extern "system" fn(set: *mut u8, result: *mut u8, key: *const u32) -> *mut u8;
type FindFn = unsafe extern "system" fn(map: *mut u8, key: *const u32) -> u64;
type AdvanceFn = unsafe extern "system" fn(obj: *mut u8);
type GrowFn = unsafe extern "system" fn(vec: *mut u8, wanted: i64, exact: u64) -> u8;

/// Every loop site, as addresses.
#[derive(Debug)]
pub(super) struct Sites {
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
    getter: Option<usize>,
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
        self.getter
            .map(|at| CodePatch {
                owner: ModIndex::LOADER,
                address: at,
                expected: GETTER.into(),
                replacement: jump_to(
                    (shown_bases as unsafe extern "system" fn(*const u8) -> u32 as *const ())
                        .addr(),
                )
                .into(),
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

/// The offset a `mov reg, [rip+d]` at `at`, `opcode.len()` bytes of opcode
/// and a 4-byte displacement, reads.
fn rip_operand(code: &[u8], at: usize, opcode: &[u8]) -> Result<isize, String> {
    let end = at + opcode.len() + 4;
    let bytes = code.get(at..end).ok_or("an operand is past the code")?;
    if &bytes[..opcode.len()] != opcode {
        return Err(format!("{at:#x} holds {bytes:02X?}, not the expected load"));
    }
    let disp = i32::from_le_bytes(bytes[opcode.len()..].try_into().map_err(|_| "cut")?);
    let end = isize::try_from(end).map_err(|e| e.to_string())?;
    Ok(end + isize::try_from(disp).map_err(|e| e.to_string())?)
}

/// Finds every loop site in `code`, which starts at `start`; the client's
/// base list only on a `client`.
pub(super) fn find(code: &[u8], start: usize, client: bool) -> Result<Sites, String> {
    let find = |pattern: &str| crate::code::find(code, pattern);
    let at = |offset: usize| (start + offset) as u64;
    let rebuild = find(REBUILD)?;
    let stock_cache = find(STOCK_CACHE)?;
    let hash = rip_operand(code, rebuild + REBUILD_HASH_AT, &MOV_R8D_RIP)?;
    let getter = if client {
        let ui = find(UI_BASES)?;
        let getter = call_target(code, ui + UI_GETTER_AT)?;
        if code.get(getter..getter + GETTER.len()) != Some(&GETTER[..]) {
            return Err("the base list's count getter is not the expected one".into());
        }
        Some(start + getter)
    } else {
        None
    };
    Ok(Sites {
        sort: at(find(SORT)?),
        sort_all: at(find(SORT_ALL)?),
        rebuild: at(rebuild),
        npc_totals: at(find(NPC_TOTALS)?),
        server_update: at(find(SERVER_UPDATE)?),
        stock_cache: at(stock_cache),
        base_ids: at(find(BASE_IDS)?),
        cleanups: [
            at(find(BED_CLEANUP)?),
            at(find(COMFORT_CLEANUP)?),
            at(find(PROPS_CLEANUP)?),
        ],
        hash_at: start
            .checked_add_signed(hash)
            .ok_or("the hash is out of reach")?,
        query: start + call_target(code, rebuild + REBUILD_QUERY_AT)?,
        insert: start + call_target(code, rebuild + REBUILD_INSERT_AT)?,
        find: start + call_target(code, stock_cache + STOCK_FIND_AT)?,
        advance: start + call_target(code, stock_cache + STOCK_ADVANCE_AT)?,
        getter,
    })
}

// SAFETY: `SortFn` is the signature of the sort `SORT` finds, and `link`
// uses this slot only as that function's next.
static SORT_NEXT: Next<SortFn> = unsafe { Next::new() };
// SAFETY: `ObjWorldFn` is the signature of the sort of every entity
// `SORT_ALL` finds, and `link` uses this slot only as its next.
static SORT_ALL_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };
// SAFETY: `ObjWorldFn` is the signature of the rebuild `REBUILD` finds, and
// `link` uses this slot only as its next.
static REBUILD_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the NPC totals `NPC_TOTALS` finds,
// and `link` uses this slot only as its next.
static NPC_TOTALS_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjWorldFn` is the signature of the server update
// `SERVER_UPDATE` finds, and `link` uses this slot only as its next.
static SERVER_UPDATE_NEXT: Next<ObjWorldFn> = unsafe { Next::new() };
// SAFETY: `StockFn` is the signature of the stock cache `STOCK_CACHE`
// finds, and `link` uses this slot only as its next.
static STOCK_CACHE_NEXT: Next<StockFn> = unsafe { Next::new() };
// SAFETY: `BaseIdsFn` is the signature of the base id list `BASE_IDS`
// finds, and `link` uses this slot only as its next.
static BASE_IDS_NEXT: Next<BaseIdsFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup `BED_CLEANUP` finds, and
// `link` uses this slot only as its next.
static BED_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup `COMFORT_CLEANUP` finds,
// and `link` uses this slot only as its next.
static COMFORT_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };
// SAFETY: `ObjFn` is the signature of the cleanup `PROPS_CLEANUP` finds,
// and `link` uses this slot only as its next.
static PROPS_CLEANUP_NEXT: Next<ObjFn> = unsafe { Next::new() };

/// Links every loop site and resolves the code the replacements call;
/// each link's name and status.
pub(super) fn link(loader: &Loader, sites: &Sites) -> Vec<(&'static str, Status)> {
    // SAFETY: the hash's u32 sits in the game's mapped image where the
    // rebuild's own `mov r8d, [rip+d]` reads it.
    let hash = unsafe { read_at::<u32>(sites.hash_at as *const u8, 0) };
    // SAFETY: each address is a game function a call in a matched function
    // reaches, of the type it becomes (server 399, client 426).
    let code = Code {
        hash,
        // SAFETY: the entity query the rebuild calls at `REBUILD_QUERY_AT`.
        query: unsafe { core::mem::transmute::<usize, QueryFn>(sites.query) },
        // SAFETY: the set insert the rebuild calls at `REBUILD_INSERT_AT`.
        insert: unsafe { core::mem::transmute::<usize, InsertFn>(sites.insert) },
        // SAFETY: the find-or-free the stock cache calls at `STOCK_FIND_AT`.
        find: unsafe { core::mem::transmute::<usize, FindFn>(sites.find) },
        // SAFETY: the advance the stock cache calls at `STOCK_ADVANCE_AT`.
        advance: unsafe { core::mem::transmute::<usize, AdvanceFn>(sites.advance) },
    };
    let _first = CODE.set(code);
    let none = HookFlags::NONE;
    let only = HookFlags::EXCLUSIVE;
    vec![
        (
            "sort into bases",
            loader.link(sites.sort, &SORT_NEXT, sort, only),
        ),
        (
            "sort every entity",
            loader.link(sites.sort_all, &SORT_ALL_NEXT, sort_all, none),
        ),
        (
            "logic entity sets",
            loader.link(sites.rebuild, &REBUILD_NEXT, rebuild, only),
        ),
        (
            "NPC totals",
            loader.link(sites.npc_totals, &NPC_TOTALS_NEXT, npc_totals, none),
        ),
        (
            "animal totals",
            loader.link(
                sites.server_update,
                &SERVER_UPDATE_NEXT,
                server_update,
                none,
            ),
        ),
        (
            "building stock cache",
            loader.link(sites.stock_cache, &STOCK_CACHE_NEXT, stock_cache, only),
        ),
        (
            "base ids",
            loader.link(sites.base_ids, &BASE_IDS_NEXT, base_ids, none),
        ),
        (
            "bed count cleanup",
            loader.link(sites.cleanups[0], &BED_CLEANUP_NEXT, bed_cleanup, none),
        ),
        (
            "comfort cleanup",
            loader.link(
                sites.cleanups[1],
                &COMFORT_CLEANUP_NEXT,
                comfort_cleanup,
                none,
            ),
        ),
        (
            "crafting prop cleanup",
            loader.link(sites.cleanups[2], &PROPS_CLEANUP_NEXT, props_cleanup, none),
        ),
    ]
}

/// The bases of `obj` when it holds more than the game's 16 and the window
/// is armed; `None` sends a call to the game's own code.
///
/// # Safety
///
/// - `obj` is null or a live bases object, read on a thread the grouping
///   does not run beside.
unsafe fn windowed(obj: *const u8) -> Option<Game> {
    if !super::past_16() {
        return None;
    }
    // SAFETY: the caller's guarantee, passed on.
    let game = unsafe { super::view(obj) }?;
    (game.count() > window::VANILLA).then_some(game)
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
/// - `map` is a live map of the game's layout, unchanged while the iterator
///   is used.
unsafe fn occupied(map: *const u8) -> impl Iterator<Item = usize> {
    // SAFETY: the caller guarantees a live map; its bitmap pointer and
    // bucket count sit at `map::BITMAP` and `map::BUCKETS`.
    let bitmap = unsafe { read_ptr_at::<u8>(map, map::BITMAP) };
    // SAFETY: the same map.
    let buckets = usize::try_from(unsafe { read_at::<u64>(map, map::BUCKETS) }).unwrap_or(0);
    let words = if bitmap.is_null() {
        0
    } else {
        buckets.div_ceil(64)
    };
    (0..words).flat_map(move |w| {
        // SAFETY: word `w` of the bitmap, which holds a bit per bucket.
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
    // SAFETY: the caller guarantees a live u16.
    let value = unsafe { read_at::<u16>(base, at) };
    // SAFETY: the same u16.
    unsafe { write_at(base, at, value.wrapping_add(by)) };
}

/// Appends `bytes` to the game's container `vec` through its own grow
/// callback; `false` when it cannot grow.
///
/// # Safety
///
/// - `vec` is the game's live container of `T`-sized elements, whose grow
///   callback, when set, is the game's.
unsafe fn append<T: enshwick::raw::NoUninit>(vec: *mut u8, value: T) -> bool {
    // SAFETY: the caller guarantees a live container.
    let len = unsafe { read_at::<u64>(vec, vec::LEN) };
    // SAFETY: the same container.
    let cap = unsafe { read_at::<u64>(vec, vec::CAP) };
    if cap < len + 1 {
        // SAFETY: the same container; its grow callback, null or the game's.
        let grow = unsafe { read_ptr_at::<u8>(vec, vec::GROW) };
        if grow.is_null() {
            return false;
        }
        // SAFETY: a non-null callback the game stored in its own container.
        let grow: GrowFn = unsafe { core::mem::transmute::<*mut u8, GrowFn>(grow) };
        let wanted = i64::try_from(len + 1).map_or(i64::MIN, |n| -n);
        // SAFETY: the game's callback on its own container, as it calls it.
        if unsafe { grow(vec, wanted, 1) } == 0 {
            return false;
        }
    }
    // SAFETY: the container now holds room for `len + 1` elements.
    let data = unsafe { read_ptr_at::<u8>(vec, vec::DATA) };
    let Ok(at) = usize::try_from(len) else {
        return false;
    };
    // SAFETY: element `len` is room the container owns.
    unsafe { write_at(data, at * size_of::<T>(), value) };
    // SAFETY: the same container.
    unsafe { write_at(vec, vec::LEN, len + 1) };
    true
}

unsafe extern "system" fn sort(
    out: *mut u8,
    obj: *mut u8,
    entity: *const u8,
    filtered: u64,
) -> *mut u8 {
    // SAFETY: the sort's own object, in the serial bases update.
    if let (Some(game), false) = (unsafe { windowed(obj) }, out.is_null() || entity.is_null()) {
        // SAFETY: the sort's own answer, entity and object.
        let done = super::BASES_FUSE.run("the sort into bases", || unsafe {
            sort_into(out, obj, &game, entity, filtered & 0xff != 0);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the sort's link, so the slot holds the
    // next function of that signature; the game's arguments pass unchanged.
    unsafe { SORT_NEXT.get()(out, obj, entity, filtered) }
}

/// The sort over every base of `game`.
///
/// # Safety
///
/// - `out` is writable for 8 bytes, `entity` readable for 0x20, `obj` the
///   live object `game` views, in the serial bases update.
unsafe fn sort_into(out: *mut u8, obj: *mut u8, game: &Game, entity: *const u8, filtered: bool) {
    // SAFETY: the caller guarantees a writable answer; its zero word.
    unsafe { write_at(out, 4, 0u32) };
    // SAFETY: the caller guarantees the entity's position at 8.
    let p = unsafe { point(entity.wrapping_add(8)) };
    let answer = match window::indices(game.count()).find(|&i| lookup::in_base(game, i, p)) {
        Some(i) => {
            let list = rec(obj, i).wrapping_add(if filtered {
                record::LIST_KEPT
            } else {
                record::LIST_ALL
            });
            // SAFETY: the entity's u32 id comes first.
            let id = unsafe { read_at::<u32>(entity, 0) };
            // SAFETY: record `i` holds a base the game's new base code
            // started, whose lists are the game's with its grower.
            if unsafe { append(list, id) } {
                SORTED
            } else {
                GROW_FAILED
            }
        }
        None => NOT_IN_A_BASE,
    };
    // SAFETY: the caller guarantees a writable answer.
    unsafe { write_at(out, 0, answer) };
}

unsafe extern "system" fn sort_all(obj: *mut u8, world: *mut u8) {
    // SAFETY: the object of the serial bases update.
    if let Some(game) = unsafe { windowed(obj) } {
        // SAFETY: the same object.
        let _done = super::BASES_FUSE.run("the sort of every entity", || unsafe {
            empty_lists(obj, &game);
        });
    }
    // SAFETY: this detour runs only as the sort of every entity's link.
    unsafe { SORT_ALL_NEXT.get()(obj, world) };
}

/// Empties both stock lists of every window base of `game`, as the sort of
/// every entity empties the array's before it sorts.
///
/// # Safety
///
/// - `obj` is the live object `game` views, in the serial bases update.
unsafe fn empty_lists(obj: *mut u8, game: &Game) {
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its lists' u64 lengths.
        unsafe { write_at(rec(obj, i), record::LIST_ALL + vec::LEN, 0u64) };
        // SAFETY: the same record's second list.
        unsafe { write_at(rec(obj, i), record::LIST_KEPT + vec::LEN, 0u64) };
    }
}

/// The rebuild's entity list: the game's container header over room of
/// ours, which our callback grows.
#[repr(C)]
struct Found {
    data: *mut Element,
    len: u64,
    cap: u64,
    grow: GrowFn,
    room: Vec<Element>,
}

/// One entity the query found: its id, then its component's bytes.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Element([u8; 0x40]);

/// Our grow callback for `Found`, in the game's contract: a negative
/// `wanted` asks for at least `-wanted` elements, else the capacity becomes
/// `wanted`.
unsafe extern "system" fn grow_found(vec: *mut u8, wanted: i64, _keep: u64) -> u8 {
    let result = std::panic::catch_unwind(|| {
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the game hands back our own `Found`"
        )]
        let found = vec.cast::<Found>();
        // SAFETY: the query hands back the `Found` the rebuild gave it.
        let found = unsafe { &mut *found };
        let need = usize::try_from(wanted.unsigned_abs()).unwrap_or(usize::MAX);
        let len = usize::try_from(found.len)
            .unwrap_or(0)
            .min(found.room.len());
        let cap = if wanted < 0 {
            let mut cap = usize::try_from(found.cap).unwrap_or(1).max(1);
            while cap < need {
                cap = cap.saturating_mul(2);
            }
            cap
        } else {
            need
        };
        if cap <= found.room.len() && wanted < 0 {
            return 1;
        }
        let mut room = vec![Element([0; 0x40]); cap.max(len)];
        room[..len].copy_from_slice(&found.room[..len]);
        found.room = room;
        found.data = found.room.as_mut_ptr();
        found.cap = found.room.len() as u64;
        found.len = found.len.min(found.cap);
        1
    });
    result.unwrap_or(0)
}

unsafe extern "system" fn rebuild(obj: *mut u8, world: *mut u8) {
    // SAFETY: the client's object in its serial bases update.
    if let (Some(game), Some(code)) = (unsafe { windowed(obj) }, CODE.get()) {
        // SAFETY: the rebuild's own object and world.
        let done = super::BASES_FUSE.run("the logic entity sets", || unsafe {
            rebuild_sets(obj, world, &game, code);
        });
        if done.is_some() {
            return;
        }
    }
    // SAFETY: this detour runs only as the rebuild's link.
    unsafe { REBUILD_NEXT.get()(obj, world) };
}

/// The rebuild over every base of `game`.
///
/// # Safety
///
/// - `obj` is the live client object `game` views and `world` its world,
///   in the serial bases update.
unsafe fn rebuild_sets(obj: *mut u8, world: *mut u8, game: &Game, code: &Code) {
    // SAFETY: the caller guarantees a live object; its u32 versions.
    let grouped = unsafe { read_at::<u32>(obj, object::GROUPED) };
    // SAFETY: the same object.
    if unsafe { read_at::<u32>(obj, object::REBUILT) } == grouped {
        return;
    }
    // SAFETY: the same object.
    unsafe { write_at(obj, object::REBUILT, grouped) };
    let indices = || window::indices(game.count());
    for i in indices() {
        let set = rec(obj, i);
        // SAFETY: record `i` holds a base the client's new base code started,
        // its set made with a bitmap of a bit per bucket.
        if unsafe { read_at::<u32>(set, record::SET_COUNT) } != 0 {
            // SAFETY: the same set.
            let bitmap = unsafe { read_ptr_at::<u8>(set, record::SET_BITMAP) };
            // SAFETY: the same set.
            let buckets = unsafe { read_at::<u64>(set, record::SET_BUCKETS) };
            let bytes = usize::try_from(((buckets + 63) >> 3) & !7).unwrap_or(0);
            if !bitmap.is_null() {
                // SAFETY: the set's bitmap, as long as the game clears it.
                unsafe { bitmap.write_bytes(0, bytes) };
            }
            // SAFETY: the same set.
            unsafe { write_at(set, record::SET_COUNT, 0u32) };
        }
    }
    let mut found = Found {
        data: core::ptr::null_mut(),
        len: 0,
        cap: 0,
        grow: grow_found,
        room: Vec::new(),
    };
    // SAFETY: the game's entity query on its world, appending only through
    // our callback.
    let _complete = unsafe { (code.query)((&raw mut found).cast(), world, code.hash) };
    let len = usize::try_from(found.len)
        .unwrap_or(0)
        .min(found.room.len());
    for element in &found.room[..len] {
        let at = element.0.as_ptr();
        // SAFETY: the element's position, three i64 at 8.
        let p = unsafe { point(at.wrapping_add(8)) };
        let Some(i) = indices().find(|&i| lookup::in_base(game, i, p)) else {
            continue;
        };
        let mut result = [0u8; 16];
        // SAFETY: the element's u32 id comes first.
        let id = unsafe { read_at::<u32>(at, 0) };
        // SAFETY: the game's insert into record `i`'s set, which the client's
        // new base code made with room, as the game's rebuild calls it.
        let _same = unsafe {
            (code.insert)(
                rec(obj, i).wrapping_add(record::SET),
                result.as_mut_ptr(),
                &raw const id,
            )
        };
        if result[0] == 0 {
            break;
        }
    }
    // SAFETY: the same object; its u32 change counter.
    let changes = unsafe { read_at::<u32>(obj, object::CHANGES) };
    // SAFETY: the same counter.
    unsafe { write_at(obj, object::CHANGES, changes.wrapping_add(1)) };
}

unsafe extern "system" fn npc_totals(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the NPC totals' link.
    let vanilla = unsafe { NPC_TOTALS_NEXT.get()(obj) };
    // SAFETY: the object of the serial bases update.
    if let Some(game) = unsafe { windowed(obj) } {
        // SAFETY: the same object.
        let _done = super::BASES_FUSE.run("the NPC totals", || unsafe { count_npcs(obj, &game) });
    }
    vanilla
}

/// The NPC totals' work for the window of `game`.
///
/// # Safety
///
/// - `obj` is the live object `game` views, in the serial bases update.
unsafe fn count_npcs(obj: *mut u8, game: &Game) {
    let mut home = [0u16; window::INDICES];
    let mut placed = [0u16; window::INDICES];
    let map = obj.wrapping_add(object::NPC_MAP);
    // SAFETY: the object's NPC map, live and unchanged in the update.
    let values = unsafe { read_ptr_at::<u8>(map, map::VALUES) };
    // SAFETY: the same map.
    for b in unsafe { occupied(map) } {
        let value = values.wrapping_add(b * object::NPC_VALUE);
        // SAFETY: an occupied bucket's value, `NPC_VALUE` bytes.
        let base = usize::from(unsafe { read_at::<u8>(value, 0) });
        // SAFETY: the same value.
        let kind = unsafe { read_at::<u8>(value, object::NPC_KIND_AT) };
        placed[base] = placed[base].wrapping_add(1);
        if kind == object::NPC_AT_HOME {
            home[base] = home[base].wrapping_add(1);
        }
    }
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its u16 count.
        unsafe { write_at(rec(obj, i), record::NPCS_HOME, home[i]) };
        // SAFETY: the live object's u16 total of placed NPCs.
        unsafe { add_u16(obj, object::NPCS, placed[i]) };
        // SAFETY: the live object's u16 total of NPCs at home.
        unsafe { add_u16(obj, object::NPCS_HOME, home[i]) };
    }
}

unsafe extern "system" fn server_update(obj: *mut u8, world: *mut u8) {
    // SAFETY: this detour runs only as the server update's link.
    unsafe { SERVER_UPDATE_NEXT.get()(obj, world) };
    // SAFETY: the server's object, after its serial update.
    if let Some(game) = unsafe { windowed(obj) } {
        // SAFETY: the same object.
        let _done =
            super::BASES_FUSE.run("the animal totals", || unsafe { count_animals(obj, &game) });
    }
}

/// The animal counts' work for the window of `game`.
///
/// # Safety
///
/// - `obj` is the live server object `game` views, in its serial update.
unsafe fn count_animals(obj: *mut u8, game: &Game) {
    let mut counts = [[0u16; 5]; window::INDICES];
    let map = obj.wrapping_add(object::ANIMAL_MAP);
    // SAFETY: the object's animal map, live and unchanged in the update.
    let values = unsafe { read_ptr_at::<u8>(map, map::VALUES) };
    // SAFETY: the same map.
    for b in unsafe { occupied(map) } {
        // SAFETY: an occupied bucket's value, `{base, flags, kind}`.
        let [base, flags, kind] = unsafe { read_at::<[u8; 3]>(values, b * object::ANIMAL_VALUE) };
        let count = &mut counts[usize::from(base)];
        count[0] = count[0].wrapping_add(1);
        if kind == 1 {
            count[1] = count[1].wrapping_add(1);
        } else if kind.wrapping_sub(2) <= 7 {
            count[2] = count[2].wrapping_add(1);
            count[3] = count[3].wrapping_add(u16::from(flags & 4 != 0));
            count[4] = count[4].wrapping_add(u16::from(flags & 2 != 0));
        }
    }
    let fields = [
        record::ANIMALS,
        record::ANIMALS_KIND_1,
        record::ANIMALS_KINDS,
        record::ANIMALS_FLAG_4,
        record::ANIMALS_FLAG_2,
    ];
    for i in window_of(game) {
        for (at, count) in fields.into_iter().zip(counts[i]) {
            // SAFETY: record `i` holds a base; its u16 counts.
            unsafe { write_at(rec(obj, i), at, count) };
        }
        // SAFETY: the object's u32 total.
        let total = unsafe { read_at::<u32>(obj, object::ANIMALS) };
        let total = total.wrapping_add(u32::from(counts[i][0]));
        // SAFETY: the same total.
        unsafe { write_at(obj, object::ANIMALS, total) };
    }
}

unsafe extern "system" fn stock_cache(out: *mut u8, obj: *mut u8) -> *mut u8 {
    // SAFETY: the cache's own object, read beside no grouping.
    if let (Some(game), Some(code), false) = (unsafe { windowed(obj) }, CODE.get(), out.is_null()) {
        // SAFETY: the cache's own answer and object.
        let done = super::BASES_FUSE.run("the building stock cache", || unsafe {
            cache_stock(out, obj, &game, code);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the stock cache's link.
    unsafe { STOCK_CACHE_NEXT.get()(out, obj) }
}

/// Empties stock slot `slot`'s map, keeping its room.
///
/// # Safety
///
/// - `slot` is a live map of the game's layout.
unsafe fn clear_map(slot: *mut u8) {
    // SAFETY: the caller guarantees a live map; its u32 count.
    if unsafe { read_at::<u32>(slot, map::COUNT) } == 0 {
        return;
    }
    // SAFETY: the same map.
    let bitmap = unsafe { read_ptr_at::<u8>(slot, map::BITMAP) };
    // SAFETY: the same map.
    let buckets = unsafe { read_at::<u64>(slot, map::BUCKETS) };
    if !bitmap.is_null() {
        let bytes = usize::try_from(((buckets + 63) >> 3) & !7).unwrap_or(0);
        // SAFETY: the map's bitmap of a bit per bucket.
        unsafe { bitmap.write_bytes(0, bytes) };
    }
    // SAFETY: the same map.
    unsafe { write_at(slot, map::COUNT, 0u32) };
}

/// The stock cache's step over the index list of `game`, with the object's
/// position as a place in that list.
///
/// # Safety
///
/// - `out` is writable for 0x28 bytes; `obj` is the live server object
///   `game` views, its stock slots made for every index in its list.
unsafe fn cache_stock(out: *mut u8, obj: *mut u8, game: &Game, code: &Code) {
    // SAFETY: the caller guarantees a writable answer of five qwords.
    unsafe { out.write_bytes(0, 0x28) };
    // SAFETY: the caller guarantees a live object; its u32 position.
    let mut position = unsafe { read_at::<u32>(obj, object::POSITION) };
    if usize::try_from(position).map_or(true, |p| p >= game.count()) {
        // SAFETY: the same object; page and position as one qword.
        unsafe { write_at(obj, object::PAGE, 0u64) };
        position = 0;
    }
    let Some(i) = usize::try_from(position).ok().and_then(window::index) else {
        return;
    };
    let record = rec(obj, i);
    let slot = obj.wrapping_add(window::SLOTS + i * window::SLOT);
    // SAFETY: record `i` holds a base; its list of every stock entity.
    let entities = unsafe { read_ptr_at::<u32>(record, record::LIST_ALL + vec::DATA) };
    // SAFETY: the same list.
    let len = unsafe { read_at::<u64>(record, record::LIST_ALL + vec::LEN) };
    if len == 0 {
        // SAFETY: slot `i` is made for every index in the list.
        unsafe { clear_map(slot) };
        // SAFETY: the game's advance, which reads no count and no record.
        unsafe { (code.advance)(obj) };
        return;
    }
    // SAFETY: the same object; its u32 page.
    let page = unsafe { read_at::<u32>(obj, object::PAGE) };
    let first = u64::from(page.wrapping_mul(PAGE_ENTITIES));
    if first < len {
        // SAFETY: the same page.
        unsafe { write_at(obj, object::PAGE, page.wrapping_add(1)) };
        let staging = obj.wrapping_add(object::STAGING);
        let Ok(from) = usize::try_from(first) else {
            return;
        };
        // SAFETY: the caller guarantees five writable qwords.
        unsafe { write_at(out, 0, staging as u64) };
        // SAFETY: the same answer.
        unsafe { write_at(out, 0x18, entities.wrapping_add(from) as u64) };
        // SAFETY: the same answer.
        unsafe { write_at(out, 0x20, (len - first).min(u64::from(PAGE_ENTITIES))) };
        return;
    }
    // SAFETY: slot `i` is made for every index in the list.
    unsafe { clear_map(slot) };
    let staging = obj.wrapping_add(object::STAGING);
    // SAFETY: the object's staging map, filled by the game.
    let keys = unsafe { read_ptr_at::<u8>(staging, map::KEYS) };
    // SAFETY: the same map.
    let values = unsafe { read_ptr_at::<u8>(staging, map::VALUES) };
    // SAFETY: the slot's own bucket count, keys, values and bitmap.
    let buckets = unsafe { read_at::<u64>(slot, map::BUCKETS) };
    // SAFETY: the same slot.
    let slot_keys = unsafe { read_ptr_at::<u8>(slot, map::KEYS) };
    // SAFETY: the same slot.
    let slot_values = unsafe { read_ptr_at::<u8>(slot, map::VALUES) };
    // SAFETY: the same slot.
    let bitmap = unsafe { read_ptr_at::<u8>(slot, map::BITMAP) };
    // SAFETY: the staging map, live and unchanged here.
    for b in unsafe { occupied(staging) } {
        let key = keys.wrapping_add(b * 4);
        // SAFETY: the game's find-or-free on the slot's map with an occupied
        // key of the staging map.
        let j = unsafe { (code.find)(slot, key.cast()) };
        if j >= buckets {
            continue;
        }
        let Ok(j) = usize::try_from(j) else {
            continue;
        };
        let word_at = j / 64 * 8;
        // SAFETY: bucket `j` is below the slot's bucket count; its bitmap
        // word.
        let word = unsafe { read_at::<u64>(bitmap, word_at) };
        if word >> (j % 64) & 1 == 0 {
            // SAFETY: the same word.
            unsafe { write_at(bitmap, word_at, word | 1 << (j % 64)) };
            // SAFETY: the slot's u32 count.
            let count = unsafe { read_at::<u32>(slot, map::COUNT) };
            // SAFETY: the same count.
            unsafe { write_at(slot, map::COUNT, count + 1) };
            // SAFETY: an occupied key of the staging map.
            let id = unsafe { read_at::<u32>(key, 0) };
            // SAFETY: bucket `j`'s key, u32.
            unsafe { write_at(slot_keys, j * 4, id) };
        }
        // SAFETY: bucket `b`'s value of the staging map, u64.
        let value = unsafe { read_at::<u64>(values, b * 8) };
        // SAFETY: bucket `j`'s value, u64.
        unsafe { write_at(slot_values, j * 8, value) };
    }
    // SAFETY: the game's advance.
    unsafe { (code.advance)(obj) };
}

unsafe extern "system" fn base_ids(vec: *mut u8, obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the base id list's link.
    let vanilla = unsafe { BASE_IDS_NEXT.get()(vec, obj) };
    // SAFETY: the list's own object.
    if let (Some(game), false) = (unsafe { windowed(obj) }, vec.is_null()) {
        // SAFETY: the list's own container and object.
        let _done =
            super::BASES_FUSE.run("the base ids", || unsafe { append_ids(vec, obj, &game) });
    }
    vanilla
}

/// Appends the id of every window base of `game` that has one, in index
/// order, until the container cannot grow.
///
/// # Safety
///
/// - `vec` is the game's live container of 16-byte ids; `obj` the live
///   object `game` views.
unsafe fn append_ids(vec: *mut u8, obj: *mut u8, game: &Game) {
    for i in window_of(game) {
        // SAFETY: record `i` holds a base; its 16-byte id.
        let id = unsafe { read_at::<[u64; 2]>(rec(obj, i), record::BASE_ID) };
        // SAFETY: the caller's live container of 16-byte ids.
        if id != [0, 0] && !unsafe { append(vec, id) } {
            break;
        }
    }
}

/// The cleanups' post-hook: `clear` for every window record of `obj`.
///
/// # Safety
///
/// - `obj` is null or a live bases object.
unsafe fn clean(obj: *mut u8, site: &str, clear: unsafe fn(*mut u8)) {
    // SAFETY: the caller's guarantee.
    if let Some(game) = unsafe { windowed(obj) } {
        // SAFETY: the same live object, viewed by `game`.
        let _done = super::BASES_FUSE.run(site, || unsafe { clean_window(obj, &game, clear) });
    }
}

/// `clear` for every window record of the object `game` views.
///
/// # Safety
///
/// - `obj` is the live object `game` views; `clear` writes only inside the
///   record it gets.
unsafe fn clean_window(obj: *mut u8, game: &Game, clear: unsafe fn(*mut u8)) {
    for i in window_of(game) {
        // SAFETY: record `i` holds a base of the caller's live object.
        unsafe { clear(rec(obj, i)) };
    }
}

/// The bed count cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base.
unsafe fn clear_beds(base: *mut u8) {
    for at in record::BEDS {
        // SAFETY: the caller's live record; its u32 bed counts.
        unsafe { write_at(base, at, 0u32) };
    }
}

/// The comfort cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base.
unsafe fn clear_comfort(base: *mut u8) {
    let comfort = base.wrapping_add(record::COMFORT);
    // SAFETY: the caller's live record; its comfort array.
    unsafe { comfort.write_bytes(0, record::COMFORT_LEN) };
}

/// The crafting prop cleanup's work on one record.
///
/// # Safety
///
/// - `base` is a live record holding a base.
unsafe fn clear_props(base: *mut u8) {
    // SAFETY: the caller's live record; its u64 prop mask.
    unsafe { write_at(base, record::PROPS, 0u64) };
}

unsafe extern "system" fn bed_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link.
    let vanilla = unsafe { BED_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the bed count cleanup", clear_beds) };
    vanilla
}

unsafe extern "system" fn comfort_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link.
    let vanilla = unsafe { COMFORT_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the comfort cleanup", clear_comfort) };
    vanilla
}

unsafe extern "system" fn props_cleanup(obj: *mut u8) -> u64 {
    // SAFETY: this detour runs only as the cleanup's link.
    let vanilla = unsafe { PROPS_CLEANUP_NEXT.get()(obj) };
    // SAFETY: the cleanup's own object; `clean` hands each window record
    // that holds a base.
    unsafe { clean(obj, "the crafting prop cleanup", clear_props) };
    vanilla
}

/// The client's count getter, which the base list sizes itself by: the
/// game's count, or one past the highest index in use, so the list has an
/// entry at every window index a base holds. Patched in place of the
/// game's, which it never calls.
unsafe extern "system" fn shown_bases(obj: *const u8) -> u32 {
    // SAFETY: the base list's own live object, which the game's getter reads
    // the same way; its u32 count sits at `GETTER_COUNT`, which `find`
    // checked in the getter's bytes.
    let count = unsafe { read_at::<u32>(obj, GETTER_COUNT) };
    let shown = super::BASES_FUSE.run("the base list's count", || {
        // SAFETY: the same object, read in the client's UI system.
        unsafe { windowed(obj) }.map(|game| {
            let count = usize::try_from(count).unwrap_or(0);
            u32::try_from(window::shown(count, game.count())).unwrap_or(u32::MAX)
        })
    });
    shown.flatten().unwrap_or(count)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::bases::fake::{self, Fake, record as at, twenty_bases};
    use crate::locks::Relock;

    /// What the fake game code was called with, in order.
    static CALLS: Mutex<Vec<(&'static str, usize, u32)>> = Mutex::new(Vec::new());

    /// No call seen, as each test starts.
    fn fresh() {
        CALLS.relock().clear();
    }

    fn calls(name: &str) -> Vec<(usize, u32)> {
        CALLS
            .relock()
            .iter()
            .filter(|(n, ..)| *n == name)
            .map(|&(_, a, b)| (a, b))
            .collect()
    }

    /// An entity of the sort's input: its id, then its fixed-point position.
    #[repr(C, align(8))]
    struct Entity {
        id: u32,
        pad: u32,
        position: [i64; 3],
    }

    #[expect(clippy::cast_possible_truncation, reason = "test positions are small")]
    fn entity(id: u32, metres: [f32; 3]) -> Entity {
        Entity {
            id,
            pad: 0,
            position: metres.map(|m| (f64::from(m) * 4_294_967_296.0) as i64),
        }
    }

    /// The u32 elements of record `i`'s list at `list`.
    fn list(fake: &Fake, i: usize, list: usize) -> Vec<u32> {
        let len = usize::try_from(fake.u64_at(at(i) + list + vec::LEN)).unwrap();
        let data = fake.u64_at(at(i) + list) as *const u32;
        if len == 0 {
            return Vec::new();
        }
        // SAFETY: the list holds `len` elements the fake grower made.
        unsafe { core::slice::from_raw_parts(data, len) }.to_vec()
    }

    fn sorted(fake: &mut Fake, entity: &Entity, filtered: bool) -> [u8; 0x10] {
        let obj = fake.ptr();
        let game = fake.view();
        let mut out = [0xAAu8; 0x10];
        let from = core::ptr::from_ref(entity).cast();
        // SAFETY: a writable answer, the test's entity and object.
        unsafe { sort_into(out.as_mut_ptr(), obj, &game, from, filtered) };
        out
    }

    #[test]
    fn the_sort_reaches_a_window_base_and_answers_as_the_game() {
        let mut fake = twenty_bases();
        let out = sorted(&mut fake, &entity(77, [1805.0, 5.0, 5.0]), false);
        assert_eq!(
            (out[0], &out[1..8]),
            (SORTED, &[0xAA, 0xAA, 0xAA, 0, 0, 0, 0][..])
        );
        assert_eq!(list(&fake, 95, record::LIST_ALL), [77]);
        let _ = sorted(&mut fake, &entity(78, [1805.0, 5.0, 5.0]), true);
        assert_eq!(list(&fake, 95, record::LIST_KEPT), [78]);
        let _ = sorted(&mut fake, &entity(79, [5.0, 5.0, 5.0]), false);
        assert_eq!(
            list(&fake, 0, record::LIST_ALL),
            [79],
            "the array's bases too"
        );
        let out = sorted(&mut fake, &entity(80, [50.0, 5.0, 5.0]), false);
        assert_eq!(out[0], NOT_IN_A_BASE);
        fake.put(at(96) + record::LIST_ALL + vec::GROW, 0u64);
        let out = sorted(&mut fake, &entity(81, [1905.0, 5.0, 5.0]), false);
        assert_eq!(out[0], GROW_FAILED);
    }

    #[test]
    fn the_sort_of_every_entity_empties_the_window_lists() {
        let mut fake = twenty_bases();
        for i in [3, 93, 96] {
            fake.put(at(i) + record::LIST_ALL + vec::LEN, 5u64);
            fake.put(at(i) + record::LIST_KEPT + vec::LEN, 6u64);
        }
        let obj = fake.ptr();
        let game = fake.view();
        // SAFETY: the test's object.
        unsafe { empty_lists(obj, &game) };
        for i in [93, 96] {
            assert_eq!(fake.u64_at(at(i) + record::LIST_ALL + vec::LEN), 0);
            assert_eq!(fake.u64_at(at(i) + record::LIST_KEPT + vec::LEN), 0);
        }
        assert_eq!(
            fake.u64_at(at(3) + record::LIST_ALL + vec::LEN),
            5,
            "the game's own"
        );
    }

    /// The entity query: appends an entity in base 18 (index 95) and one in
    /// no base, through the list's grow callback.
    unsafe extern "system" fn query(found: *mut u8, world: *mut u8, hash: u32) -> u8 {
        CALLS.relock().push(("query", world.addr(), hash));
        for e in [entity(41, [1805.0, 5.0, 5.0]), entity(42, [50.0, 5.0, 5.0])] {
            // SAFETY: the rebuild's list: data, length, capacity, grower.
            let len = unsafe { read_at::<u64>(found, vec::LEN) };
            // SAFETY: the same list's grow callback.
            let grow = unsafe { read_at::<usize>(found, vec::GROW) };
            // SAFETY: the callback the rebuild stored, of this type.
            let grow: GrowFn = unsafe { core::mem::transmute::<usize, GrowFn>(grow) };
            // SAFETY: the list's own callback, as the game calls it.
            let grown = unsafe { grow(found, -i64::try_from(len + 1).unwrap(), 1) };
            assert_eq!(grown, 1);
            // SAFETY: the list now has room for one more.
            let data = unsafe { read_at::<usize>(found, vec::DATA) } as *mut u8;
            let element = data.wrapping_add(usize::try_from(len).unwrap() * 0x40);
            // SAFETY: the new element's id.
            unsafe { write_at(element, 0, e.id) };
            // SAFETY: the new element's position.
            unsafe { write_at(element, 8, e.position) };
            // SAFETY: the list's length.
            unsafe { write_at(found, vec::LEN, len + 1) };
        }
        1
    }

    unsafe extern "system" fn insert(set: *mut u8, result: *mut u8, key: *const u32) -> *mut u8 {
        // SAFETY: the rebuild's key.
        let key = unsafe { read_at::<u32>(key.cast(), 0) };
        CALLS.relock().push(("insert", set.addr(), key));
        // SAFETY: the rebuild's 16-byte result.
        unsafe { write_at(result, 0, 0x0101u16) };
        result
    }

    unsafe extern "system" fn find(map: *mut u8, key: *const u32) -> u64 {
        // SAFETY: the cache's key.
        let key = unsafe { read_at::<u32>(key.cast(), 0) };
        CALLS.relock().push(("find", map.addr(), key));
        u64::from(key % 64)
    }

    unsafe extern "system" fn advance(obj: *mut u8) {
        CALLS.relock().push(("advance", obj.addr(), 0));
        // SAFETY: the test's object; its position.
        let position = unsafe { read_at::<u32>(obj, object::POSITION) };
        // SAFETY: the same position, raised as the game's advance does.
        unsafe { write_at(obj, object::POSITION, position + 1) };
        // SAFETY: the page, restarted as the game's advance does.
        unsafe { write_at(obj, object::PAGE, 0u32) };
    }

    const CODE: Code = Code {
        hash: 0x87f5_0dfb,
        query,
        insert,
        find,
        advance,
    };

    #[test]
    fn the_logic_entity_sets_take_window_bases_once_a_version() {
        fresh();
        let mut fake = Fake::new(0, &fake::apart(20));
        let obj = fake.ptr();
        let ready = |_: usize| {};
        // SAFETY: the fake client object, its window zeroed.
        let settled = unsafe {
            super::super::members::settle(
                obj,
                fake::FIELDS,
                window::MOST,
                fake::new_base,
                Some(&ready),
            )
        };
        assert!(settled.is_some());
        let mut bitmap = [u64::MAX; 2];
        fake.put(at(95) + record::SET_BITMAP, bitmap.as_mut_ptr() as u64);
        fake.put(at(95) + record::SET_BUCKETS, 128u64);
        fake.put(at(95) + record::SET_COUNT, 3u32);
        fake.put(object::GROUPED, 5u32);
        fake.put(object::REBUILT, 4u32);
        let world = core::ptr::without_provenance_mut::<u8>(0x5000);
        let game = fake.view();
        // SAFETY: the fake client object and a world the fake query ignores.
        unsafe { rebuild_sets(obj, world, &game, &CODE) };
        assert_eq!(calls("query"), [(0x5000, 0x87f5_0dfb)]);
        assert_eq!(calls("insert"), [(obj.addr() + at(95) + record::SET, 41)]);
        assert_eq!(bitmap, [0, 0]);
        assert_eq!(fake.u32_at(at(95) + record::SET_COUNT), 0);
        assert_eq!(
            (fake.u32_at(object::REBUILT), fake.u32_at(object::CHANGES)),
            (5, 1)
        );
        let game = fake.view();
        // SAFETY: the same object, its version seen.
        unsafe { rebuild_sets(obj, world, &game, &CODE) };
        assert_eq!(calls("query").len(), 1, "nothing to do for a seen version");
    }

    /// A map of the game's layout over test blocks: occupied buckets with
    /// their values.
    struct Map {
        bitmap: Vec<u64>,
        keys: Vec<u32>,
        values: Vec<u8>,
    }

    impl Map {
        fn new(buckets: usize, width: usize, filled: &[(usize, &[u8])]) -> Self {
            let mut map = Self {
                bitmap: vec![0; buckets.div_ceil(64)],
                keys: vec![0; buckets],
                values: vec![0; buckets * width],
            };
            for &(b, value) in filled {
                map.bitmap[b / 64] |= 1 << (b % 64);
                map.keys[b] = u32::try_from(b).unwrap() + 100;
                map.values[b * width..b * width + value.len()].copy_from_slice(value);
            }
            map
        }

        fn put(&mut self, fake: &mut Fake, at: usize) {
            fake.put(at + map::BITMAP, self.bitmap.as_mut_ptr() as u64);
            fake.put(at + map::BUCKETS, u64::try_from(self.keys.len()).unwrap());
            fake.put(at + map::KEYS, self.keys.as_mut_ptr() as u64);
            fake.put(at + map::VALUES, self.values.as_mut_ptr() as u64);
        }
    }

    #[test]
    fn the_npc_totals_add_the_window_bases_to_the_game_s() {
        let mut fake = twenty_bases();
        let npc = |base: u8, kind: u8| {
            let mut v = [0u8; 12];
            v[0] = base;
            v[8] = kind;
            v
        };
        let (a, b, c, d) = (npc(95, 7), npc(95, 3), npc(3, 7), npc(96, 7));
        let mut map = Map::new(128, 12, &[(0, &a), (1, &b), (2, &c), (70, &d)]);
        map.put(&mut fake, object::NPC_MAP);
        fake.put(object::NPCS, 10u16);
        fake.put(object::NPCS_HOME, 4u16);
        fake.put(at(95) + record::NPCS_HOME, 9u16);
        let obj = fake.ptr();
        let game = fake.view();
        // SAFETY: the test's object and map.
        unsafe { count_npcs(obj, &game) };
        assert_eq!(fake.u16_at(at(95) + record::NPCS_HOME), 1);
        assert_eq!(fake.u16_at(at(96) + record::NPCS_HOME), 1);
        assert_eq!(fake.u16_at(at(93) + record::NPCS_HOME), 0);
        assert_eq!(
            (fake.u16_at(object::NPCS), fake.u16_at(object::NPCS_HOME)),
            (13, 6)
        );
    }

    #[test]
    fn the_animal_counts_add_the_window_bases_to_the_game_s() {
        let mut fake = twenty_bases();
        let animals: [[u8; 3]; 5] = [[93, 0, 1], [93, 6, 5], [93, 2, 9], [94, 4, 10], [3, 0, 1]];
        let filled: Vec<(usize, &[u8])> = animals
            .iter()
            .enumerate()
            .map(|(b, v)| (b * 3, &v[..]))
            .collect();
        let mut map = Map::new(64, 3, &filled);
        map.put(&mut fake, object::ANIMAL_MAP);
        fake.put(object::ANIMALS, 7u32);
        fake.put(at(94) + record::ANIMALS_KINDS, 9u16);
        let obj = fake.ptr();
        let game = fake.view();
        // SAFETY: the test's object and map.
        unsafe { count_animals(obj, &game) };
        let counts = |i: usize| {
            [
                record::ANIMALS,
                record::ANIMALS_KIND_1,
                record::ANIMALS_KINDS,
                record::ANIMALS_FLAG_4,
                record::ANIMALS_FLAG_2,
            ]
            .map(|f| fake.u16_at(at(i) + f))
        };
        assert_eq!(counts(93), [3, 1, 2, 1, 2]);
        assert_eq!(counts(94), [1, 0, 0, 0, 0], "kind 10 is neither");
        assert_eq!(fake.u32_at(object::ANIMALS), 7 + 4);
    }

    #[test]
    fn the_stock_cache_pages_a_window_base_and_fills_its_slot() {
        fresh();
        let mut fake = twenty_bases();
        let mut entities: Vec<u32> = (0..45).collect();
        fake.put(at(93) + record::LIST_ALL, entities.as_mut_ptr() as u64);
        fake.put(at(93) + record::LIST_ALL + vec::LEN, 45u64);
        fake.put(object::POSITION, 16u32);
        let mut staging = Map::new(64, 8, &[(3, &222u64.to_le_bytes())]);
        staging.put(&mut fake, object::STAGING);
        let slot_at = window::SLOTS + 93 * window::SLOT;
        let mut slot = Map::new(64, 8, &[]);
        slot.put(&mut fake, slot_at);
        let obj = fake.ptr();
        let step = |fake: &mut Fake| {
            let game = fake.view();
            let mut out = [0xAAu64; 5];
            // SAFETY: a writable answer and the test's object.
            unsafe { cache_stock(out.as_mut_ptr().cast(), obj, &game, &CODE) };
            out
        };
        let out = step(&mut fake);
        let staged = obj.addr() + object::STAGING;
        assert_eq!(out, [staged as u64, 0, 0, entities.as_ptr() as u64, 40]);
        let out = step(&mut fake);
        assert_eq!(out[3..], [entities.as_ptr().wrapping_add(40) as u64, 5]);
        assert_eq!(fake.u32_at(object::PAGE), 2);
        let out = step(&mut fake);
        assert_eq!(out, [0; 5]);
        assert_eq!(calls("find"), [(obj.addr() + slot_at, 103)]);
        assert_eq!(slot.bitmap[0], 1 << (103 % 64));
        assert_eq!(slot.keys[103 % 64], 103);
        let value = (103 % 64) * 8;
        assert_eq!(slot.values[value..value + 8], 222u64.to_le_bytes());
        assert_eq!(fake.u32_at(slot_at + map::COUNT), 1);
        assert_eq!(
            (fake.u32_at(object::POSITION), fake.u32_at(object::PAGE)),
            (17, 0)
        );
        fake.put(object::POSITION, 20u32);
        fake.put(object::PAGE, 3u32);
        let out = step(&mut fake);
        assert_eq!(out, [0; 5], "base 0 has no stock");
        assert_eq!(
            (fake.u32_at(object::POSITION), fake.u32_at(object::PAGE)),
            (1, 0),
            "past the list it starts over"
        );
    }

    #[test]
    fn the_base_ids_of_the_window_follow_the_game_s() {
        let mut fake = twenty_bases();
        fake.put(at(93) + record::BASE_ID, [1u64, 2]);
        fake.put(at(95) + record::BASE_ID, [3u64, 4]);
        let grower = fake::grow_16 as unsafe extern "system" fn(*mut u8, i64, u64) -> u8;
        let mut vec = [0u64, 0, 0, (grower as *const ()).addr() as u64];
        let obj = fake.ptr();
        let game = fake.view();
        // SAFETY: a container of 16-byte ids with the fake grower.
        unsafe { append_ids(vec.as_mut_ptr().cast(), obj, &game) };
        assert_eq!(vec[1], 2);
        // SAFETY: the container holds two ids.
        let ids = unsafe { core::slice::from_raw_parts(vec[0] as *const [u64; 2], 2) };
        assert_eq!(ids, [[1, 2], [3, 4]]);
    }

    #[test]
    fn the_cleanups_clear_the_window_records_and_leave_the_rest() {
        let mut fake = twenty_bases();
        for i in [5, 93, 96, 97] {
            fake.put(at(i) + record::BEDS[1], 3u32);
            fake.put(at(i) + record::COMFORT + 64, 4u32);
            fake.put(at(i) + record::PROPS, 5u64);
        }
        let obj = fake.ptr();
        let game = fake.view();
        for clear in [clear_beds as unsafe fn(*mut u8), clear_comfort, clear_props] {
            // SAFETY: the test's object; each clear writes in its record.
            unsafe { clean_window(obj, &game, clear) };
        }
        for i in [93, 96] {
            assert_eq!(fake.u32_at(at(i) + record::BEDS[1]), 0);
            assert_eq!(fake.u32_at(at(i) + record::COMFORT + 64), 0);
            assert_eq!(fake.u64_at(at(i) + record::PROPS), 0);
        }
        for i in [5, 97] {
            assert_eq!(fake.u32_at(at(i) + record::BEDS[1]), 3, "record {i}");
            assert_eq!(fake.u64_at(at(i) + record::PROPS), 5, "record {i}");
        }
    }

    #[test]
    fn the_count_getter_jump_lands_on_ours() {
        let bytes = jump_to(0x1122_3344_5566_7788);
        assert_eq!(bytes[..6], [0xFF, 0x25, 0, 0, 0, 0]);
        assert_eq!(bytes[6..], 0x1122_3344_5566_7788u64.to_le_bytes());
        assert_eq!(usize::from(GETTER[2]), GETTER_COUNT);
    }

    #[test]
    fn a_rip_operand_resolves_and_another_load_is_refused() {
        let mut code = vec![0xCC; 0x20];
        code[4..11].copy_from_slice(&[0x44, 0x8B, 0x05, 0x10, 0, 0, 0]);
        assert_eq!(rip_operand(&code, 4, &MOV_R8D_RIP), Ok(11 + 0x10));
        assert!(rip_operand(&code, 3, &MOV_R8D_RIP).is_err());
    }
}
