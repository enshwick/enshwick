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

use enshwick::abi::{HookFlags, Status};
use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::members::Game;
use super::past16::{SiteError, call_target};
use crate::base_groups::lookup::Bases;
use crate::base_groups::window;
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
        let ours = totals::shown_bases as unsafe extern "system" fn(*const u8) -> u32;
        self.getter
            .map(|at| CodePatch {
                owner: ModIndex::LOADER,
                address: at,
                expected: GETTER.into(),
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

/// The offset a `mov reg, [rip+d]` at `at`, `opcode.len()` bytes of opcode
/// and a 4-byte displacement, reads.
fn rip_operand(code: &[u8], at: usize, opcode: &[u8]) -> Result<usize, SiteError> {
    let end = at + opcode.len() + 4;
    let bytes = code.get(at..end).ok_or(SiteError::Cut(at))?;
    if &bytes[..opcode.len()] != opcode {
        return Err(SiteError::NotALoad {
            at,
            found: bytes.to_vec(),
        });
    }
    let disp = i32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]);
    isize::try_from(disp)
        .ok()
        .and_then(|disp| end.checked_add_signed(disp))
        .ok_or(SiteError::Cut(at))
}

/// Finds every loop site in `code`, which starts at `start`; the client's
/// base list only on a `client`.
pub(super) fn find(code: &[u8], start: usize, client: bool) -> Result<Sites, SiteError> {
    let find = |pattern: &str| crate::code::find(code, pattern).map_err(SiteError::from);
    let at = |offset: usize| (start + offset) as u64;
    let rebuild = find(REBUILD)?;
    let stock_cache = find(STOCK_CACHE)?;
    let hash = rip_operand(code, rebuild + REBUILD_HASH_AT, &MOV_R8D_RIP)?;
    let getter = if client {
        let ui = find(UI_BASES)?;
        let getter = call_target(code, ui + UI_GETTER_AT)?;
        if code.get(getter..getter + GETTER.len()) != Some(&GETTER[..]) {
            return Err(SiteError::Getter);
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
        hash_at: start + hash,
        query: start + call_target(code, rebuild + REBUILD_QUERY_AT)?,
        insert: start + call_target(code, rebuild + REBUILD_INSERT_AT)?,
        find: start + call_target(code, stock_cache + STOCK_FIND_AT)?,
        advance: start + call_target(code, stock_cache + STOCK_ADVANCE_AT)?,
        getter,
    })
}

/// Links every loop site and resolves the code the replacements call;
/// each link's name and status.
pub(super) fn link(loader: &Loader, sites: &Sites) -> Vec<(&'static str, Status)> {
    // SAFETY: the hash's u32 sits in the game's mapped image where the
    // rebuild's own `mov r8d, [rip+d]` reads it.
    let hash = unsafe { read_at::<u32>(sites.hash_at as *const u8, 0) };
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
    let [beds, comfort, props] = sites.cleanups;
    vec![
        (
            "sort into bases",
            loader.link(sites.sort, &sort::SORT_NEXT, sort::sort, only),
        ),
        (
            "sort every entity",
            loader.link(sites.sort_all, &sort::SORT_ALL_NEXT, sort::sort_all, none),
        ),
        (
            "base ids",
            loader.link(sites.base_ids, &sort::BASE_IDS_NEXT, sort::base_ids, none),
        ),
        (
            "logic entity sets",
            loader.link(
                sites.rebuild,
                &rebuild::REBUILD_NEXT,
                rebuild::rebuild,
                only,
            ),
        ),
        (
            "building stock cache",
            loader.link(
                sites.stock_cache,
                &stock::STOCK_CACHE_NEXT,
                stock::stock_cache,
                only,
            ),
        ),
        (
            "NPC totals",
            loader.link(
                sites.npc_totals,
                &totals::NPC_TOTALS_NEXT,
                totals::npc_totals,
                none,
            ),
        ),
        (
            "animal totals",
            loader.link(
                sites.server_update,
                &totals::SERVER_UPDATE_NEXT,
                totals::server_update,
                none,
            ),
        ),
        (
            "bed count cleanup",
            loader.link(beds, &totals::BED_CLEANUP_NEXT, totals::bed_cleanup, none),
        ),
        (
            "comfort cleanup",
            loader.link(
                comfort,
                &totals::COMFORT_CLEANUP_NEXT,
                totals::comfort_cleanup,
                none,
            ),
        ),
        (
            "crafting prop cleanup",
            loader.link(
                props,
                &totals::PROPS_CLEANUP_NEXT,
                totals::props_cleanup,
                none,
            ),
        ),
    ]
}

/// The bases of `obj` when it holds more than the game's 16 and the window
/// is armed; `None` sends a call to the game's own code. Every whole-array
/// function enters here, so it also counts entries while the grouping lends
/// the count.
///
/// # Safety
///
/// - `obj` is null or a live bases object, read on a thread the grouping
///   does not run beside.
unsafe fn windowed(obj: *const u8) -> Option<Game> {
    super::members::note_entry();
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
    // SAFETY: the caller guarantees a live map; its bitmap pointer sits at
    // `map::BITMAP`.
    let bitmap = unsafe { read_ptr_at::<u8>(map, map::BITMAP) };
    // SAFETY: the caller's live map; its u64 bucket count sits at
    // `map::BUCKETS`.
    let buckets = usize::try_from(unsafe { read_at::<u64>(map, map::BUCKETS) }).unwrap_or(0);
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

/// Appends `value` to the game's container `vec` through its own grow
/// callback; `false` when it cannot grow or holds no room, as the game.
///
/// # Safety
///
/// - `vec` is the game's live container of `T`-sized elements, whose grow
///   callback, when set, is the game's.
unsafe fn append<T: enshwick::raw::NoUninit>(vec: *mut u8, value: T) -> bool {
    // SAFETY: the caller's live container; its u64 length at `vec::LEN`.
    let len = unsafe { read_at::<u64>(vec, vec::LEN) };
    // SAFETY: the caller's live container; its u64 capacity at `vec::CAP`.
    let cap = unsafe { read_at::<u64>(vec, vec::CAP) };
    if cap < len + 1 {
        // SAFETY: the caller's live container; its grow callback at
        // `vec::GROW`, null or the game's.
        let grow = unsafe { read_ptr_at::<u8>(vec, vec::GROW) };
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
    // SAFETY: the caller's live container; its data pointer at `vec::DATA`,
    // with room for `len + 1` elements now.
    let data = unsafe { read_ptr_at::<u8>(vec, vec::DATA) };
    let Ok(at) = usize::try_from(len) else {
        return false;
    };
    if data.is_null() {
        return false;
    }
    // SAFETY: element `len` is room the container owns, past its length.
    unsafe { write_at(data, at * size_of::<T>(), value) };
    // SAFETY: the caller's live container; its u64 length at `vec::LEN`.
    unsafe { write_at(vec, vec::LEN, len + 1) };
    true
}
