use std::sync::Mutex;

use super::rebuild::rebuild_sets;
use super::sort::{GROW_FAILED, NOT_IN_A_BASE, SORTED, append_ids, empty_lists, sort_into};
use super::stock::cache_stock;
use super::totals::{
    clean_window, clear_beds, clear_comfort, clear_props, count_animals, count_npcs,
};
use super::*;
use crate::bases::fake::{self, Fake, record as at, twenty_bases};
use crate::bases::past16::COUNT_AT;
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
fn a_list_with_room_but_no_data_fails_to_grow_as_the_game() {
    let mut fake = twenty_bases();
    fake.put(at(94) + record::LIST_ALL + vec::CAP, 8u64);
    let out = sorted(&mut fake, &entity(82, [1705.0, 5.0, 5.0]), false);
    assert_eq!(out[0], GROW_FAILED);
    assert_eq!(fake.u64_at(at(94) + record::LIST_ALL + vec::LEN), 0);
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

/// The entity query: appends an entity in base 18 (index 95) and one in no
/// base, through the list's grow callback.
unsafe extern "system" fn query(found: *mut u8, world: *mut u8, hash: u32) -> u8 {
    CALLS.relock().push(("query", world.addr(), hash));
    for e in [entity(41, [1805.0, 5.0, 5.0]), entity(42, [50.0, 5.0, 5.0])] {
        // SAFETY: the rebuild's list: its u64 length.
        let len = unsafe { read_at::<u64>(found, vec::LEN) };
        // SAFETY: the rebuild's list: its grow callback.
        let grow = unsafe { read_at::<usize>(found, vec::GROW) };
        // SAFETY: the callback the rebuild stored, of this type.
        let grow: GrowFn = unsafe { core::mem::transmute::<usize, GrowFn>(grow) };
        // SAFETY: the list's own callback, as the game calls it.
        let grown = unsafe { grow(found, -i64::try_from(len + 1).unwrap(), 1) };
        assert_eq!(grown, 1);
        // SAFETY: the rebuild's list: its data, with room for one more.
        let data = unsafe { read_at::<usize>(found, vec::DATA) } as *mut u8;
        let element = data.wrapping_add(usize::try_from(len).unwrap() * 0x40);
        // SAFETY: the new element's u32 id.
        unsafe { write_at(element, 0, e.id) };
        // SAFETY: the new element's position, three i64 at 8.
        unsafe { write_at(element, 8, e.position) };
        // SAFETY: the rebuild's list: its u64 length.
        unsafe { write_at(found, vec::LEN, len + 1) };
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
    let position = unsafe { read_at::<u32>(obj, object::POSITION) };
    // SAFETY: the test's object; its u32 position, raised as the game's
    // advance does.
    unsafe { write_at(obj, object::POSITION, position + 1) };
    // SAFETY: the test's object; its u32 page, restarted as the game's
    // advance does.
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

/// One step of the stock cache on `fake`, its answer.
fn step(fake: &mut Fake) -> [u64; 5] {
    let obj = fake.ptr();
    let game = fake.view();
    let mut out = [0xAAu64; 5];
    // SAFETY: a writable answer and the test's object.
    unsafe { cache_stock(out.as_mut_ptr().cast(), obj, &game, &CODE) };
    out
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
fn the_stock_cache_skips_a_window_index_whose_slot_was_never_made() {
    fresh();
    let mut fake = Fake::new(1, &fake::apart(20));
    let obj = fake.ptr();
    let ready = |_: usize| {};
    // SAFETY: the fake object, its window zeroed and never windowed, so no
    // slot of it is made.
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
    let mut entities: Vec<u32> = (0..3).collect();
    fake.put(at(93) + record::LIST_ALL, entities.as_mut_ptr() as u64);
    fake.put(at(93) + record::LIST_ALL + vec::LEN, 3u64);
    let slot_at = window::SLOTS + 93 * window::SLOT;
    fake.put(slot_at + map::COUNT, 9u32);
    fake.put(object::POSITION, 16u32);
    let out = step(&mut fake);
    assert_eq!(out, [0; 5]);
    assert_eq!(
        fake.u32_at(slot_at + map::COUNT),
        9,
        "the slot is untouched"
    );
    assert_eq!(calls("advance").len(), 1);
    assert!(calls("find").is_empty());
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
    assert_eq!(usize::from(GETTER[2]), COUNT_AT);
}
