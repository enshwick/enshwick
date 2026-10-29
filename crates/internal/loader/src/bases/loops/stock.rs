//! The building stock cache: one base a call, round robin over the index
//! list, the object's position counting places in that list.

use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::{CODE, Code, Game, Layout, MapFields, occupied, rec, windowed};
use crate::base_groups::lookup::Bases;
use crate::base_groups::window;
use crate::chains::Next;

/// The entities a call hands back.
const PAGE_ENTITIES: u32 = 40;
pub(super) type StockFn = unsafe extern "system" fn(out: *mut u8, obj: *mut u8) -> *mut u8;

// SAFETY: `StockFn` is the signature of the stock cache the table's
// `bases.stock_cache` pins, and `link` uses this slot only as its next.
pub(super) static STOCK_CACHE_NEXT: Next<StockFn> = unsafe { Next::new() };

pub(super) unsafe extern "system" fn stock_cache(out: *mut u8, obj: *mut u8) -> *mut u8 {
    // SAFETY: the cache's own object, read beside no grouping.
    let game = unsafe { windowed(obj) };
    if let (Some((game, layout)), Some(code), false) = (game, CODE.get(), out.is_null()) {
        // SAFETY: the cache's own answer and object.
        let done = crate::bases::BASES_FUSE.run("the building stock cache", || unsafe {
            cache_stock(out, obj, &game, layout, code);
        });
        if done.is_some() {
            return out;
        }
    }
    // SAFETY: this detour runs only as the stock cache's link, so the slot
    // holds the next function of that signature.
    unsafe { STOCK_CACHE_NEXT.get()(out, obj) }
}

/// Empties the map at `slot`, keeping its room, as the game does.
///
/// # Safety
///
/// - `slot` is a live map of the game's, laid out as `map`.
unsafe fn clear_map(slot: *mut u8, map: &MapFields) {
    // SAFETY: the caller's live map; its u32 count.
    if unsafe { read_at::<u32>(slot, map.count) } == 0 {
        return;
    }
    // SAFETY: the caller's live map; its bitmap pointer.
    let bitmap = unsafe { read_ptr_at::<u8>(slot, map.bitmap) };
    // SAFETY: the caller's live map; its u64 bucket count.
    let buckets = unsafe { read_at::<u64>(slot, map.buckets) };
    if !bitmap.is_null() {
        let bytes = usize::try_from(((buckets + 63) >> 3) & !7).unwrap_or(0);
        // SAFETY: the map's bitmap, a bit per bucket, as many bytes as the
        // game clears.
        unsafe { bitmap.write_bytes(0, bytes) };
    }
    // SAFETY: the caller's live map; its u32 count.
    unsafe { write_at(slot, map.count, 0u32) };
}

/// Copies every entry of the staging map at `staging` into the map at
/// `slot`, through the game's find-or-free, as the game does.
///
/// # Safety
///
/// - `staging` and `slot` are live maps of the game's, laid out as `map`,
///   `slot` made with buckets.
unsafe fn copy_staging(staging: *mut u8, slot: *mut u8, map: &MapFields, code: &Code) {
    // SAFETY: the live staging map; its keys pointer, u32 each.
    let keys = unsafe { read_ptr_at::<u8>(staging, map.keys) };
    // SAFETY: the live staging map; its values pointer, u64 each.
    let values = unsafe { read_ptr_at::<u8>(staging, map.values) };
    // SAFETY: the live slot map; its u64 bucket count.
    let buckets = unsafe { read_at::<u64>(slot, map.buckets) };
    // SAFETY: the live slot map; its keys pointer, u32 each.
    let slot_keys = unsafe { read_ptr_at::<u8>(slot, map.keys) };
    // SAFETY: the live slot map; its values pointer, u64 each.
    let slot_values = unsafe { read_ptr_at::<u8>(slot, map.values) };
    // SAFETY: the live slot map; its bitmap pointer, a bit per bucket.
    let bitmap = unsafe { read_ptr_at::<u8>(slot, map.bitmap) };
    // SAFETY: the live staging map, unchanged while it is copied.
    for b in unsafe { occupied(staging, map) } {
        let key = keys.wrapping_add(b * 4);
        // SAFETY: the game's find-or-free on the slot's map with the key of
        // an occupied staging bucket.
        let j = unsafe { (code.find)(slot, key.cast()) };
        let Some(j) = usize::try_from(j).ok().filter(|_| j < buckets) else {
            continue;
        };
        let word_at = j / 64 * 8;
        // SAFETY: bucket `j` is below the slot's bucket count; its bitmap
        // word.
        let word = unsafe { read_at::<u64>(bitmap, word_at) };
        if word >> (j % 64) & 1 == 0 {
            // SAFETY: bucket `j`'s bitmap word, inside the bitmap.
            unsafe { write_at(bitmap, word_at, word | 1 << (j % 64)) };
            // SAFETY: the live slot map; its u32 count.
            let count = unsafe { read_at::<u32>(slot, map.count) };
            // SAFETY: the live slot map; its u32 count.
            unsafe { write_at(slot, map.count, count + 1) };
            // SAFETY: the u32 key of occupied staging bucket `b`.
            let id = unsafe { read_at::<u32>(key, 0) };
            // SAFETY: bucket `j`'s u32 key, below the slot's bucket count.
            unsafe { write_at(slot_keys, j * 4, id) };
        }
        // SAFETY: the u64 value of occupied staging bucket `b`.
        let value = unsafe { read_at::<u64>(values, b * 8) };
        // SAFETY: bucket `j`'s u64 value, below the slot's bucket count.
        unsafe { write_at(slot_values, j * 8, value) };
    }
}

/// The stock cache's step over the index list of `game`, with the object's
/// position as a place in that list. A window index whose stock slot was
/// never made, which only an object made since its last grouping can
/// show, is skipped.
///
/// # Safety
///
/// - `out` is writable for the answer size of `layout`; `obj` is the live
///   object `game` views, its array's stock slots made by the game's create.
pub(super) unsafe fn cache_stock(
    out: *mut u8,
    obj: *mut u8,
    game: &Game,
    layout: &Layout,
    code: &Code,
) {
    let object = &layout.object;
    let answer = &layout.stock;
    let list = layout.record.list_all;
    let container = &game.fields().container;
    // SAFETY: the caller's writable answer of its size.
    unsafe { out.write_bytes(0, answer.size) };
    // SAFETY: the caller's live object; its u32 position.
    let mut position = unsafe { read_at::<u32>(obj, object.position) };
    if usize::try_from(position).map_or(true, |p| p >= game.count()) {
        // SAFETY: the caller's live object; its u32 page and position, as
        // one qword the game writes the same way.
        unsafe { write_at(obj, object.page, 0u64) };
        position = 0;
    }
    let Some(i) = usize::try_from(position).ok().and_then(window::index) else {
        return;
    };
    let made = i < window::VANILLA || crate::bases::lifecycle::has_slot(obj, i);
    let slot = obj.wrapping_add(window::SLOTS + i * window::SLOT);
    // SAFETY: record `i` holds a base; its stock list's data pointer.
    let entities = unsafe { read_ptr_at::<u32>(rec(obj, i), list + container.data) };
    // SAFETY: record `i` holds a base; its stock list's u64 length.
    let len = unsafe { read_at::<u64>(rec(obj, i), list + container.len) };
    if len == 0 || !made {
        if made {
            // SAFETY: slot `i` is made: by the game's create in the array,
            // by `ready` in the window.
            unsafe { clear_map(slot, &layout.map) };
        }
        // SAFETY: the game's advance, which reads no count and no record and
        // moves the position and the page.
        unsafe { (code.advance)(obj) };
        return;
    }
    // SAFETY: the caller's live object; its u32 page.
    let page = unsafe { read_at::<u32>(obj, object.page) };
    let first = u64::from(page.wrapping_mul(PAGE_ENTITIES));
    if first < len {
        // SAFETY: the caller's live object; its u32 page.
        unsafe { write_at(obj, object.page, page.wrapping_add(1)) };
        let Ok(from) = usize::try_from(first) else {
            return;
        };
        let staging = obj.wrapping_add(object.staging) as u64;
        // SAFETY: the caller's writable answer; the staging map first.
        unsafe { write_at(out, 0, staging) };
        // SAFETY: the caller's writable answer; the page's entities.
        unsafe { write_at(out, answer.entities, entities.wrapping_add(from) as u64) };
        let count = (len - first).min(u64::from(PAGE_ENTITIES));
        // SAFETY: the caller's writable answer; the page's entity count.
        unsafe { write_at(out, answer.count, count) };
        return;
    }
    // SAFETY: slot `i` is made: by the game's create in the array, by
    // `ready` in the window.
    unsafe { clear_map(slot, &layout.map) };
    // SAFETY: the object's staging map, filled by the game, and slot `i`,
    // made with buckets.
    unsafe { copy_staging(obj.wrapping_add(object.staging), slot, &layout.map, code) };
    // SAFETY: the game's advance, which reads no count and no record and
    // moves the position and the page.
    unsafe { (code.advance)(obj) };
}
