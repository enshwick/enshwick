use std::sync::Mutex;

use super::rebuild::rebuild_sets;
use super::sort::{GROW_FAILED, NOT_IN_A_BASE, SORTED, append_ids, empty_lists, sort_into};
use super::stock::cache_stock;
use super::totals::{
    clean_window, clear_beds, clear_comfort, clear_props, count_animals, count_npcs,
};
use super::*;
use crate::bases::Container;
use crate::bases::fake::{self, Fake, record as at, twenty_bases};
use crate::bases::fake::{FIELDS, LAYOUT};
use crate::locks::Relock;

/// The test layout's parts, as the build tables read them.
const RECORD: RecordFields = LAYOUT.record;
const OBJECT: ObjectFields = LAYOUT.object;
const MAP: MapFields = LAYOUT.map;
const VEC: Container = FIELDS.container;

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
    let len = usize::try_from(fake.u64_at(at(i) + list + VEC.len)).unwrap();
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
    unsafe { sort_into(out.as_mut_ptr(), obj, &game, &LAYOUT, from, filtered) };
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
    assert_eq!(list(&fake, 95, RECORD.list_all), [77]);
    let _ = sorted(&mut fake, &entity(78, [1805.0, 5.0, 5.0]), true);
    assert_eq!(list(&fake, 95, RECORD.list_kept), [78]);
    let _ = sorted(&mut fake, &entity(79, [5.0, 5.0, 5.0]), false);
    assert_eq!(
        list(&fake, 0, RECORD.list_all),
        [79],
        "the array's bases too"
    );
    let out = sorted(&mut fake, &entity(80, [50.0, 5.0, 5.0]), false);
    assert_eq!(out[0], NOT_IN_A_BASE);
    fake.put(at(96) + RECORD.list_all + VEC.grow, 0u64);
    let out = sorted(&mut fake, &entity(81, [1905.0, 5.0, 5.0]), false);
    assert_eq!(out[0], GROW_FAILED);
}

#[test]
fn a_list_with_room_but_no_data_fails_to_grow_as_the_game() {
    let mut fake = twenty_bases();
    fake.put(at(94) + RECORD.list_all + VEC.cap, 8u64);
    let out = sorted(&mut fake, &entity(82, [1705.0, 5.0, 5.0]), false);
    assert_eq!(out[0], GROW_FAILED);
    assert_eq!(fake.u64_at(at(94) + RECORD.list_all + VEC.len), 0);
}

#[test]
fn the_sort_of_every_entity_empties_the_window_lists() {
    let mut fake = twenty_bases();
    for i in [3, 93, 96] {
        fake.put(at(i) + RECORD.list_all + VEC.len, 5u64);
        fake.put(at(i) + RECORD.list_kept + VEC.len, 6u64);
    }
    let obj = fake.ptr();
    let game = fake.view();
    // SAFETY: the test's object.
    unsafe { empty_lists(obj, &game, &LAYOUT) };
    for i in [93, 96] {
        assert_eq!(fake.u64_at(at(i) + RECORD.list_all + VEC.len), 0);
        assert_eq!(fake.u64_at(at(i) + RECORD.list_kept + VEC.len), 0);
    }
    assert_eq!(
        fake.u64_at(at(3) + RECORD.list_all + VEC.len),
        5,
        "the game's own"
    );
}

/// The entity query: appends an entity in base 18 (index 95) and one in no
/// base, through the list's grow callback.
unsafe extern "system" fn query(found: *mut u8, world: *mut u8, hash: u32) -> u8 {
    CALLS.relock().push(("query", world.addr(), hash));
    for e in [entity(41, [1805.0, 5.0, 5.0]), entity(42, [50.0, 5.0, 5.0])] {
        // SAFETY: the rebuild's list: its u64 length.
        let len = unsafe { read_at::<u64>(found, VEC.len) };
        // SAFETY: the rebuild's list: its grow callback.
        let grow = unsafe { read_at::<usize>(found, VEC.grow) };
        // SAFETY: the callback the rebuild stored, of this type.
        let grow: GrowFn = unsafe { core::mem::transmute::<usize, GrowFn>(grow) };
        // SAFETY: the list's own callback, as the game calls it.
        let grown = unsafe { grow(found, -i64::try_from(len + 1).unwrap(), 1) };
        assert_eq!(grown, 1);
        // SAFETY: the rebuild's list: its data, with room for one more.
        let data = unsafe { read_at::<usize>(found, VEC.data) } as *mut u8;
        let element = data.wrapping_add(usize::try_from(len).unwrap() * 0x40);
        // SAFETY: the new element's u32 id.
        unsafe { write_at(element, 0, e.id) };
        // SAFETY: the new element's position, three i64 at 8.
        unsafe { write_at(element, 8, e.position) };
        // SAFETY: the rebuild's list: its u64 length.
        unsafe { write_at(found, VEC.len, len + 1) };
    }
    1
}

unsafe extern "system" fn insert(set: *mut u8, result: *mut u8, key: *const u32) -> *mut u8 {
    // SAFETY: the rebuild's u32 key.
    let key = unsafe { read_at::<u32>(key.cast(), 0) };
    CALLS.relock().push(("insert", set.addr(), key));
    // SAFETY: the rebuild's 16-byte result; its u16 answer.
    unsafe { write_at(result, 0, 0x0101u16) };
    result
}

unsafe extern "system" fn find(map: *mut u8, key: *const u32) -> u64 {
    // SAFETY: the cache's u32 key.
    let key = unsafe { read_at::<u32>(key.cast(), 0) };
    CALLS.relock().push(("find", map.addr(), key));
    u64::from(key % 64)
}

unsafe extern "system" fn advance(obj: *mut u8) {
    CALLS.relock().push(("advance", obj.addr(), 0));
    // SAFETY: the test's object; its u32 position.
    let position = unsafe { read_at::<u32>(obj, OBJECT.position) };
    // SAFETY: the test's object; its u32 position, raised as the game's
    // advance does.
    unsafe { write_at(obj, OBJECT.position, position + 1) };
    // SAFETY: the test's object; its u32 page, restarted as the game's
    // advance does.
    unsafe { write_at(obj, OBJECT.page, 0u32) };
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
        super::super::members::settle(obj, FIELDS, window::MOST, fake::new_base, Some(&ready))
    };
    assert!(settled.is_some());
    let mut bitmap = [u64::MAX; 2];
    fake.put(at(95) + RECORD.set_bitmap, bitmap.as_mut_ptr() as u64);
    fake.put(at(95) + RECORD.set_buckets, 128u64);
    fake.put(at(95) + RECORD.set_count, 3u32);
    fake.put(OBJECT.grouped, 5u32);
    fake.put(OBJECT.rebuilt, 4u32);
    let world = core::ptr::without_provenance_mut::<u8>(0x5000);
    let game = fake.view();
    // SAFETY: the fake client object and a world the fake query ignores.
    unsafe { rebuild_sets(obj, world, &game, &LAYOUT, &CODE) };
    assert_eq!(calls("query"), [(0x5000, 0x87f5_0dfb)]);
    assert_eq!(calls("insert"), [(obj.addr() + at(95) + RECORD.set, 41)]);
    assert_eq!(bitmap, [0, 0]);
    assert_eq!(fake.u32_at(at(95) + RECORD.set_count), 0);
    assert_eq!(
        (fake.u32_at(OBJECT.rebuilt), fake.u32_at(OBJECT.changes)),
        (5, 1)
    );
    let game = fake.view();
    // SAFETY: the same object, its version seen.
    unsafe { rebuild_sets(obj, world, &game, &LAYOUT, &CODE) };
    assert_eq!(calls("query").len(), 1, "nothing to do for a seen version");
}

/// A map of the game's layout over test blocks: occupied buckets with their
/// values.
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
        fake.put(at + MAP.bitmap, self.bitmap.as_mut_ptr() as u64);
        fake.put(at + MAP.buckets, u64::try_from(self.keys.len()).unwrap());
        fake.put(at + MAP.keys, self.keys.as_mut_ptr() as u64);
        fake.put(at + MAP.values, self.values.as_mut_ptr() as u64);
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
    map.put(&mut fake, OBJECT.npc_map);
    fake.put(OBJECT.npcs, 10u16);
    fake.put(OBJECT.npcs_home, 4u16);
    fake.put(at(95) + RECORD.npcs_home, 9u16);
    let obj = fake.ptr();
    let game = fake.view();
    // SAFETY: the test's object and map.
    unsafe { count_npcs(obj, &game, &LAYOUT) };
    assert_eq!(fake.u16_at(at(95) + RECORD.npcs_home), 1);
    assert_eq!(fake.u16_at(at(96) + RECORD.npcs_home), 1);
    assert_eq!(fake.u16_at(at(93) + RECORD.npcs_home), 0);
    assert_eq!(
        (fake.u16_at(OBJECT.npcs), fake.u16_at(OBJECT.npcs_home)),
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
    map.put(&mut fake, OBJECT.animal_map);
    fake.put(OBJECT.animals, 7u32);
    fake.put(at(94) + RECORD.animals + 4, 9u16);
    let obj = fake.ptr();
    let game = fake.view();
    // SAFETY: the test's object and map.
    unsafe { count_animals(obj, &game, &LAYOUT) };
    let counts = |i: usize| [0, 1, 2, 3, 4].map(|k| fake.u16_at(at(i) + RECORD.animals + k * 2));
    assert_eq!(counts(93), [3, 1, 2, 1, 2]);
    assert_eq!(counts(94), [1, 0, 0, 0, 0], "kind 10 is neither");
    assert_eq!(fake.u32_at(OBJECT.animals), 7 + 4);
}

/// One step of the stock cache on `fake`, its answer.
fn step(fake: &mut Fake) -> [u64; 5] {
    let obj = fake.ptr();
    let game = fake.view();
    let mut out = [0xAAu64; 5];
    // SAFETY: a writable answer and the test's object.
    unsafe { cache_stock(out.as_mut_ptr().cast(), obj, &game, &LAYOUT, &CODE) };
    out
}

#[test]
fn the_stock_cache_pages_a_window_base_and_fills_its_slot() {
    fresh();
    let mut fake = twenty_bases();
    let mut entities: Vec<u32> = (0..45).collect();
    fake.put(at(93) + RECORD.list_all, entities.as_mut_ptr() as u64);
    fake.put(at(93) + RECORD.list_all + VEC.len, 45u64);
    fake.put(OBJECT.position, 16u32);
    let mut staging = Map::new(64, 8, &[(3, &222u64.to_le_bytes())]);
    staging.put(&mut fake, OBJECT.staging);
    let slot_at = window::SLOTS + 93 * window::SLOT;
    let mut slot = Map::new(64, 8, &[]);
    slot.put(&mut fake, slot_at);
    let obj = fake.ptr();
    let out = step(&mut fake);
    let staged = obj.addr() + OBJECT.staging;
    assert_eq!(out, [staged as u64, 0, 0, entities.as_ptr() as u64, 40]);
    let out = step(&mut fake);
    assert_eq!(out[3..], [entities.as_ptr().wrapping_add(40) as u64, 5]);
    assert_eq!(fake.u32_at(OBJECT.page), 2);
    let out = step(&mut fake);
    assert_eq!(out, [0; 5]);
    assert_eq!(calls("find"), [(obj.addr() + slot_at, 103)]);
    assert_eq!(slot.bitmap[0], 1 << (103 % 64));
    assert_eq!(slot.keys[103 % 64], 103);
    let value = (103 % 64) * 8;
    assert_eq!(slot.values[value..value + 8], 222u64.to_le_bytes());
    assert_eq!(fake.u32_at(slot_at + MAP.count), 1);
    assert_eq!(
        (fake.u32_at(OBJECT.position), fake.u32_at(OBJECT.page)),
        (17, 0)
    );
    fake.put(OBJECT.position, 20u32);
    fake.put(OBJECT.page, 3u32);
    let out = step(&mut fake);
    assert_eq!(out, [0; 5], "base 0 has no stock");
    assert_eq!(
        (fake.u32_at(OBJECT.position), fake.u32_at(OBJECT.page)),
        (1, 0),
        "past the list it starts over"
    );
}

#[test]
fn the_stock_cache_skips_a_window_index_whose_slot_was_never_made() {
    fresh();
    let mut fake = Fake::new(1, &fake::apart(20));
    let obj = fake.ptr();
    let ready = |_: usize| {};
    // SAFETY: the fake object, its window zeroed and never windowed, so no
    // slot of it is made.
    let settled = unsafe {
        super::super::members::settle(obj, FIELDS, window::MOST, fake::new_base, Some(&ready))
    };
    assert!(settled.is_some());
    let mut entities: Vec<u32> = (0..3).collect();
    fake.put(at(93) + RECORD.list_all, entities.as_mut_ptr() as u64);
    fake.put(at(93) + RECORD.list_all + VEC.len, 3u64);
    let slot_at = window::SLOTS + 93 * window::SLOT;
    fake.put(slot_at + MAP.count, 9u32);
    fake.put(OBJECT.position, 16u32);
    let out = step(&mut fake);
    assert_eq!(out, [0; 5]);
    assert_eq!(fake.u32_at(slot_at + MAP.count), 9, "the slot is untouched");
    assert_eq!(calls("advance").len(), 1);
    assert!(calls("find").is_empty());
}

#[test]
fn the_base_ids_of_the_window_follow_the_game_s() {
    let mut fake = twenty_bases();
    fake.put(at(93) + RECORD.base_id, [1u64, 2]);
    fake.put(at(95) + RECORD.base_id, [3u64, 4]);
    let grower = fake::grow_16 as unsafe extern "system" fn(*mut u8, i64, u64) -> u8;
    let mut vec = [0u64, 0, 0, (grower as *const ()).addr() as u64];
    let obj = fake.ptr();
    let game = fake.view();
    // SAFETY: a container of 16-byte ids with the fake grower.
    unsafe { append_ids(vec.as_mut_ptr().cast(), obj, &game, &LAYOUT) };
    assert_eq!(vec[1], 2);
    // SAFETY: the container holds two ids.
    let ids = unsafe { core::slice::from_raw_parts(vec[0] as *const [u64; 2], 2) };
    assert_eq!(ids, [[1, 2], [3, 4]]);
}

#[test]
fn the_cleanups_clear_the_window_records_and_leave_the_rest() {
    let mut fake = twenty_bases();
    for i in [5, 93, 96, 97] {
        fake.put(at(i) + RECORD.beds + 4, 3u32);
        fake.put(at(i) + RECORD.comfort + 64, 4u32);
        fake.put(at(i) + RECORD.props, 5u64);
    }
    let obj = fake.ptr();
    let game = fake.view();
    for clear in [
        clear_beds as unsafe fn(*mut u8, &RecordFields),
        clear_comfort,
        clear_props,
    ] {
        // SAFETY: the test's object; each clear writes in its record.
        unsafe { clean_window(obj, &game, &RECORD, clear) };
    }
    for i in [93, 96] {
        assert_eq!(fake.u32_at(at(i) + RECORD.beds + 4), 0);
        assert_eq!(fake.u32_at(at(i) + RECORD.comfort + 64), 0);
        assert_eq!(fake.u64_at(at(i) + RECORD.props), 0);
    }
    for i in [5, 97] {
        assert_eq!(fake.u32_at(at(i) + RECORD.beds + 4), 3, "record {i}");
        assert_eq!(fake.u64_at(at(i) + RECORD.props), 5, "record {i}");
    }
}

#[test]
fn a_field_past_its_record_object_or_answer_is_named() {
    assert_eq!(LAYOUT.misfit(), None);
    let animals = Layout {
        record: RecordFields {
            animals: 0x12a,
            ..RECORD
        },
        ..LAYOUT
    };
    assert_eq!(animals.misfit(), Some(bases::RECORD_ANIMALS.id()));
    let staging = Layout {
        object: ObjectFields {
            staging: window::RECORDS + window::WINDOW * window::RECORD,
            ..OBJECT
        },
        ..LAYOUT
    };
    assert_eq!(staging.misfit(), Some(bases::STAGING.id()));
    let entities = Layout {
        stock: StockFields {
            entities: 0x28,
            ..LAYOUT.stock
        },
        ..LAYOUT
    };
    assert_eq!(entities.misfit(), Some(bases::STOCK_ENTITIES.id()));
}

#[test]
fn the_count_getter_jump_lands_on_ours() {
    let bytes = jump_to(0x1122_3344_5566_7788);
    assert_eq!(bytes[..6], [0xFF, 0x25, 0, 0, 0, 0]);
    assert_eq!(bytes[6..], 0x1122_3344_5566_7788u64.to_le_bytes());
    assert_eq!(getter(0x24)[..4], [0x8B, 0x41, 0x24, 0xC3]);
}
