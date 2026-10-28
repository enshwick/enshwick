//! The bases past 32 altar zones: the zones of each base in a side table,
//! and the game's member lookups replaced to read it.
//!
//! The game names a base's zones in a 32-bit mask, and copies the world's
//! zone list into room for 16 that cannot grow; given an allocator, it grows
//! to every listed zone. Its grouping still runs, to release the old bases
//! and copy the world's zone list, but it may start
//! only the bases our grouping plans; after it, each base takes our box and
//! zones, the mask its first 32. The four lookups that read the mask answer
//! from the side table instead. Facts from server 399 and client 426, where
//! both bodies are the same.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::Fields;
use crate::base_groups::grouping::{self, Grouped, MAX_GROUPED, MAX_LISTED, ZoneSet};
use crate::base_groups::limit::Aabb;
use crate::base_groups::lookup::{self, Bases, Holder};

/// The byte that tells the server's bases object (1) from the client's (0).
pub(super) const KIND_AT: usize = 0x08;
/// The lookups' answer: an error byte, a u32 the game sets to 0, then the
/// base index or the box that holds the point.
const OUT_ERROR: usize = 0x0;
const OUT_ZERO: usize = 0x4;
const OUT_INDEX: usize = 0x8;
const OUT_BOX: usize = 0x10;
/// The error byte for a point outside every base.
const NOT_IN_A_BASE: u8 = 0x1f;
/// The game's zone box container, also the bases object's own zone list:
/// data, length, capacity, the callback that grows it, and the allocator it
/// grows through, null when it may not leave its inline room.
const VEC_DATA: usize = 0x0;
const VEC_LEN: usize = 0x8;
const VEC_CAP: usize = 0x10;
const VEC_GROW: usize = 0x18;
const VEC_ALLOCATOR: usize = 0x20;
/// The world allocator a bases object keeps first, and the bit the game sets
/// when it hands it to a container.
const OBJECT_ALLOCATOR: usize = 0x0;
const HANDED: usize = 1;

/// The callback that grows a zone box container to hold `-wanted` boxes.
type GrowFn = unsafe extern "system" fn(vec: *mut u8, wanted: i64, exact: u64) -> u8;

/// The zones of each base of one bases object, written only by the
/// grouping, which the game never runs beside a lookup.
#[derive(Debug)]
struct Side {
    /// The object the sets belong to; another object reads its own masks.
    owner: AtomicUsize,
    sets: [[AtomicU64; 4]; MAX_GROUPED],
}

impl Side {
    const fn new() -> Self {
        Self {
            owner: AtomicUsize::new(0),
            sets: [const { [const { AtomicU64::new(0) }; 4] }; MAX_GROUPED],
        }
    }

    fn set(&self, i: usize) -> ZoneSet {
        self.sets.get(i).map_or_else(ZoneSet::default, |words| {
            ZoneSet::from_words(words.each_ref().map(|w| w.load(Ordering::Relaxed)))
        })
    }

    fn store(&self, owner: *const u8, bases: &Grouped) {
        self.owner.store(0, Ordering::Relaxed);
        for (i, words) in self.sets.iter().enumerate() {
            let set = bases
                .get(i)
                .map_or_else(ZoneSet::default, |base| base.zones);
            for (word, value) in words.iter().zip(set.words()) {
                word.store(value, Ordering::Relaxed);
            }
        }
        self.owner.store(owner.addr(), Ordering::Release);
    }
}

/// The side tables of the client's object and the server's, which a
/// hosting client runs both of.
static SIDES: [Side; 2] = [Side::new(), Side::new()];

/// One bases object of the game as the lookups read it.
pub(super) struct Game {
    obj: *const u8,
    fields: Fields,
    side: Option<&'static Side>,
}

impl Game {
    /// The bases object `obj` with its side table, if the grouping filled
    /// one for it.
    ///
    /// # Safety
    ///
    /// - `obj` is a live bases object of the build table's layout, for as
    ///   long as the view is used, with no grouping running meanwhile.
    pub(super) unsafe fn new(obj: *const u8, fields: Fields) -> Self {
        // SAFETY: the caller guarantees a live bases object, whose kind byte
        // sits at `KIND_AT`.
        let kind = unsafe { read_at::<u8>(obj, KIND_AT) };
        let side = SIDES
            .get(usize::from(kind))
            .filter(|side| side.owner.load(Ordering::Acquire) == obj.addr());
        Self { obj, fields, side }
    }

    fn record(&self, i: usize) -> usize {
        i * self.fields.stride
    }

    fn zone_entry(&self, z: usize) -> *const u8 {
        // SAFETY: `Game::new`'s caller guarantees a live object; its zone
        // list pointer sits at the build table's `bases.zone_list`.
        let list = unsafe { read_ptr_at::<u8>(self.obj, self.fields.zone_list) };
        list.cast_const().wrapping_add(z * self.fields.zone_size)
    }

    fn read_box(&self, at: *const u8) -> Aabb {
        // SAFETY: callers pass a zone entry or a record's box, both in the
        // zone layout: the low corner first, the high one at `zone_high`.
        let min = unsafe { read_at::<[f32; 3]>(at, 0) };
        // SAFETY: as above, the high corner at the build table's
        // `bases.zone_high`.
        let max = unsafe { read_at::<[f32; 3]>(at, self.fields.zone_high) };
        Aabb { min, max }
    }

    fn bounds_at(&self, i: usize) -> *const u8 {
        self.obj
            .wrapping_add(self.record(i) + self.fields.members - self.fields.zone_size)
    }
}

impl Bases for Game {
    fn count(&self) -> usize {
        // SAFETY: `Game::new`'s caller guarantees a live object; its u32 base
        // count sits at the build table's `bases.count`.
        let count = unsafe { read_at::<u32>(self.obj, self.fields.count) };
        usize::try_from(count).map_or(MAX_GROUPED, |n| n.min(MAX_GROUPED))
    }

    fn bounds(&self, i: usize) -> Aabb {
        self.read_box(self.bounds_at(i))
    }

    fn members(&self, i: usize) -> ZoneSet {
        self.side.map_or_else(
            || {
                // SAFETY: `i` is below the count, capped at the array's 16,
                // so record `i`'s u32 mask lies in the live object at the
                // build table's `bases.members`.
                let mask =
                    unsafe { read_at::<u32>(self.obj, self.record(i) + self.fields.members) };
                ZoneSet::from_mask(mask)
            },
            |side| side.set(i),
        )
    }

    fn listed(&self) -> usize {
        // SAFETY: `Game::new`'s caller guarantees a live object; the zone
        // list's length sits at the build table's `bases.zone_list_len`.
        let len = unsafe { read_at::<usize>(self.obj, self.fields.zone_list_len) };
        len.min(MAX_LISTED)
    }

    fn zone(&self, z: usize) -> Aabb {
        self.read_box(self.zone_entry(z))
    }
}

/// Lets the zone list of `obj` grow past its inline room of 16 zones, which
/// the game leaves without an allocator, so its copy of the world's list
/// stops at 16: hands it the object's allocator as the game hands it to each
/// base's lists. Every destroy of the object frees the list through it.
///
/// # Safety
///
/// - `obj` is a live bases object, on the grouping's thread before the game
///   copies the world's list, with no reader meanwhile.
pub(super) unsafe fn let_zone_list_grow(obj: *mut u8, fields: Fields) {
    let allocator_at = fields.zone_list + VEC_ALLOCATOR;
    // SAFETY: the caller guarantees a live object; its zone list container
    // starts at the build table's `bases.zone_list`, its allocator field at
    // `VEC_ALLOCATOR` in it.
    let given = unsafe { read_at::<usize>(obj, allocator_at) };
    // SAFETY: as above, the world allocator at `OBJECT_ALLOCATOR`.
    let allocator = unsafe { read_at::<usize>(obj, OBJECT_ALLOCATOR) };
    if given == 0 && allocator != 0 {
        // SAFETY: as above.
        unsafe { write_at(obj, allocator_at, allocator | HANDED) };
    }
}

/// How many bases the grouping running on object `obj` may start: what our
/// grouping plans, at most `cap`, read from the zone list the game has just
/// copied, before its first new base.
///
/// # Safety
///
/// - `obj` is the live bases object of a grouping that has just refilled its
///   zone list, on the grouping's thread.
pub(super) unsafe fn planned(obj: *const u8, fields: Fields, cap: usize) -> usize {
    // SAFETY: the caller's guarantee.
    let game = unsafe { Game::new(obj, fields) };
    plan(&game, cap).len()
}

/// Our grouping of `game`'s listed zones into at most `cap` bases.
fn plan(game: &Game, cap: usize) -> Grouped {
    let mut boxes = [Aabb {
        min: [0.0; 3],
        max: [0.0; 3],
    }; MAX_LISTED];
    let listed = game.listed();
    for (z, slot) in boxes.iter_mut().enumerate().take(listed) {
        *slot = game.zone(z);
    }
    grouping::group(&boxes[..listed], cap)
}

/// Gives the bases the grouping on `obj` left our boxes and zones, starting
/// those it did not with `new_base`, and returns them.
///
/// # Safety
///
/// - `obj` is the live bases object the game's grouping has just filled, on
///   the grouping's thread, with no reader meanwhile.
/// - `new_base` is the game's own new base code, `fn(obj, zone, index)`.
pub(super) unsafe fn settle(
    obj: *mut u8,
    fields: Fields,
    cap: usize,
    new_base: super::AddBaseFn,
) -> Option<Grouped> {
    // SAFETY: the caller's guarantee.
    let game = unsafe { Game::new(obj, fields) };
    let bases = plan(&game, cap);
    for base in bases.iter().skip(game.count()) {
        let first = u32::try_from(base.first).ok()?;
        // SAFETY: the game's new base code on its own object, with zone
        // `first`'s entry of the list it has just copied, below its length,
        // and the zone's index as the grouping passes it.
        unsafe { new_base(obj, game.zone_entry(base.first), first) };
    }
    if game.count() != bases.len() {
        return None;
    }
    for (i, base) in bases.iter().enumerate() {
        let bounds = game.bounds_at(i).cast_mut();
        // SAFETY: record `i` is below the count the new base code reached,
        // inside the 16 records of the live object; its box starts with the
        // low corner.
        unsafe { write_at(bounds, 0, base.bounds.min) };
        // SAFETY: as above, the high corner at the build table's
        // `bases.zone_high`.
        unsafe { write_at(bounds, fields.zone_high, base.bounds.max) };
        // SAFETY: as above, record `i`'s u32 mask at the build table's
        // `bases.members`.
        unsafe { write_at(obj, game.record(i) + fields.members, base.zones.low_mask()) };
    }
    // SAFETY: the caller guarantees a live object, whose kind byte sits at
    // `KIND_AT`.
    let kind = unsafe { read_at::<u8>(obj, KIND_AT) };
    if let Some(side) = SIDES.get(usize::from(kind)) {
        side.store(obj, &bases);
    }
    Some(bases)
}

/// Forgets every side table, when a world closes.
pub(super) fn forget() {
    for side in &SIDES {
        side.owner.store(0, Ordering::Release);
    }
}

/// Writes the lookups' answer at `out`: the error byte and the zero word.
///
/// # Safety
///
/// - `out` is the lookup's answer, writable for its 0x10 bytes.
unsafe fn answer(out: *mut u8, error: u8) {
    // SAFETY: the caller guarantees a writable answer.
    unsafe { write_at(out, OUT_ERROR, error) };
    // SAFETY: as above.
    unsafe { write_at(out, OUT_ZERO, 0u32) };
}

/// Point in base: the first base holding `p`, into `out`.
///
/// # Safety
///
/// - `out` is writable for 0x10 bytes, `obj` a live bases object, `p`
///   readable for three `f32`; the game's own arguments to the lookup.
pub(super) unsafe fn base_at(out: *mut u8, game: &Game, p: [f32; 3]) {
    let found = lookup::base_at(game, p);
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, if found.is_some() { 0 } else { NOT_IN_A_BASE }) };
    let index = found.and_then(|i| u8::try_from(i).ok()).unwrap_or(0);
    // SAFETY: as above, the index byte at `OUT_INDEX`.
    unsafe { write_at(out, OUT_INDEX, index) };
}

/// Point in base `i`.
pub(super) fn in_base(game: &Game, i: usize, p: [f32; 3]) -> bool {
    lookup::in_base(game, i, p)
}

/// The box holding `p`: a one-zone base's box or a zone, into `out`.
///
/// # Safety
///
/// - `out` is writable for 0x30 bytes; the rest as for [`base_at`].
pub(super) unsafe fn holder_at(out: *mut u8, game: &Game, p: [f32; 3]) {
    let Some(holder) = lookup::holder_at(game, p) else {
        // SAFETY: the caller guarantees a writable answer.
        unsafe { answer(out, NOT_IN_A_BASE) };
        let low = out.wrapping_add(OUT_BOX);
        // SAFETY: as above, the box's low corner at `OUT_BOX`.
        unsafe { write_at(low, 0, [f32::MAX; 3]) };
        // SAFETY: as above, its high corner `zone_high` after it.
        unsafe { write_at(low, game.fields.zone_high, [-f32::MAX; 3]) };
        return;
    };
    let from = match holder {
        Holder::Base(i) => game.bounds_at(i),
        Holder::Zone(z) => game.zone_entry(z),
    };
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, 0) };
    // SAFETY: `from` is a live box of the zone layout, `zone_size` bytes,
    // and the answer holds that many at `OUT_BOX`; the two never overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(from, out.wrapping_add(OUT_BOX), game.fields.zone_size);
    }
}

/// Appends the zones of the base holding `p` to the game's container `vec`,
/// through its own grow callback, and answers into `out`.
///
/// # Safety
///
/// - `vec` is the game's zone box container, live and writable, whose grow
///   callback, when set, is the game's; the rest as for [`base_at`].
pub(super) unsafe fn zones_at(out: *mut u8, vec: *mut u8, game: &Game, p: [f32; 3]) {
    let Some(zones) = lookup::zones_at(game, p) else {
        // SAFETY: the caller guarantees a writable answer.
        unsafe { answer(out, NOT_IN_A_BASE) };
        return;
    };
    let size = game.fields.zone_size;
    for z in zones {
        // SAFETY: the caller guarantees a live container; its u64 length
        // and capacity sit at `VEC_LEN` and `VEC_CAP`.
        let len = unsafe { read_at::<u64>(vec, VEC_LEN) };
        // SAFETY: as above.
        let cap = unsafe { read_at::<u64>(vec, VEC_CAP) };
        if cap < len + 1 {
            // SAFETY: as above, the allocator field at `VEC_ALLOCATOR`.
            if unsafe { read_at::<usize>(vec, VEC_ALLOCATOR) } == 0 {
                // SAFETY: as above; the game's caller never reads the field,
                // and its grow and free reach the loader's heap through it.
                unsafe { write_at(vec, VEC_ALLOCATOR, super::heap::handed()) };
            }
            // SAFETY: as above, the grow callback at `VEC_GROW`, null or the
            // game's own.
            let grow = unsafe { read_ptr_at::<u8>(vec, VEC_GROW) };
            if grow.is_null() {
                continue;
            }
            // SAFETY: a non-null callback the game stored in its own
            // container, of the shape the game calls it with.
            let grow: GrowFn = unsafe { core::mem::transmute::<*mut u8, GrowFn>(grow) };
            let wanted = i64::try_from(len + 1).map_or(i64::MIN, |n| -n);
            // SAFETY: the game's callback on its own container, asked for
            // one more box as the game asks.
            if unsafe { grow(vec, wanted, 1) } == 0 {
                continue;
            }
        }
        // SAFETY: the container now holds room for `len + 1` boxes at its
        // data pointer, at `VEC_DATA`.
        let data = unsafe { read_ptr_at::<u8>(vec, VEC_DATA) };
        let Ok(at) = usize::try_from(len) else {
            return;
        };
        // SAFETY: zone `z` is listed, `size` bytes in the live list, and box
        // `len` of the container is room it owns; the two never overlap.
        unsafe {
            core::ptr::copy_nonoverlapping(game.zone_entry(z), data.wrapping_add(at * size), size);
        }
        // SAFETY: as above, the length at `VEC_LEN`.
        unsafe { write_at(vec, VEC_LEN, len + 1) };
    }
    // SAFETY: the caller guarantees a writable answer.
    unsafe { answer(out, 0) };
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: Fields = Fields {
        count: 0x24,
        stride: 0x130,
        members: 0x140,
        zone_list: 0x1330,
        zone_list_len: 0x1338,
        zone_size: 0x20,
        zone_high: 0x10,
        item_altar: 0x43,
        max_altars_reached: 0x4000,
    };
    const OBJECT: usize = 0x1560;

    /// A bases object of the game's layout, 16-aligned as the game's.
    #[repr(C, align(16))]
    struct Object([u8; OBJECT]);

    /// A zone entry: low corner, a word, high corner, a word.
    #[repr(C, align(16))]
    #[derive(Clone, Copy)]
    struct Entry([f32; 8]);

    struct Fake {
        obj: Box<Object>,
        zones: Vec<Entry>,
    }

    impl Fake {
        fn new(kind: u8, boxes: &[Aabb]) -> Self {
            let zones: Vec<Entry> = boxes
                .iter()
                .map(|b| {
                    Entry([
                        b.min[0], b.min[1], b.min[2], 7.0, b.max[0], b.max[1], b.max[2], 7.0,
                    ])
                })
                .collect();
            let mut fake = Self {
                obj: Box::new(Object([0; OBJECT])),
                zones,
            };
            fake.obj.0[KIND_AT] = kind;
            let list = fake.zones.as_ptr() as usize;
            fake.obj.0[0x1330..0x1338].copy_from_slice(&list.to_le_bytes());
            fake.obj.0[0x1338..0x1340].copy_from_slice(&fake.zones.len().to_le_bytes());
            fake
        }

        fn ptr(&mut self) -> *mut u8 {
            self.obj.0.as_mut_ptr()
        }

        fn u32_at(&self, at: usize) -> u32 {
            u32::from_le_bytes(self.obj.0[at..at + 4].try_into().unwrap())
        }

        fn f32s_at(&self, at: usize) -> [f32; 3] {
            [0, 4, 8]
                .map(|o| f32::from_le_bytes(self.obj.0[at + o..at + o + 4].try_into().unwrap()))
        }

        fn view(&mut self) -> Game {
            // SAFETY: the fake object lives for the test, laid out as `FIELDS`.
            unsafe { Game::new(self.ptr(), FIELDS) }
        }
    }

    /// The game's new base code as the research reads it: the count up by
    /// one, the zone's box copied whole, the zone's bit in the mask.
    unsafe extern "system" fn new_base(obj: *mut u8, zone: *const u8, index: u32) {
        // SAFETY: the fake object of the test.
        let i = unsafe { read_at::<u32>(obj, 0x24) };
        // SAFETY: as above.
        unsafe { write_at(obj, 0x24, i + 1) };
        let record = 0x30 + usize::try_from(i).unwrap() * 0x130;
        // SAFETY: a zone entry of the fake list and the record's box.
        unsafe { core::ptr::copy_nonoverlapping(zone, obj.wrapping_add(record + 0xf0), 0x20) };
        // SAFETY: the record's mask.
        unsafe { write_at(obj, record + 0x110, 1u32 << (index & 31)) };
    }

    fn square(x: f32, z: f32) -> Aabb {
        Aabb {
            min: [x, 0.0, z],
            max: [x + 10.0, 10.0, z + 10.0],
        }
    }

    /// 40 zones that touch in an L, then one apart.
    fn l_and_one() -> Vec<Aabb> {
        let mut boxes: Vec<Aabb> = (0u8..20)
            .map(|i| square(f32::from(i) * 10.0, 0.0))
            .collect();
        boxes.extend((1u8..=20).map(|i| square(0.0, f32::from(i) * 10.0)));
        boxes.push(square(1000.0, 1000.0));
        boxes
    }

    #[test]
    fn the_settled_bases_hold_every_zone_with_the_game_s_mask_low() {
        let mut fake = Fake::new(1, &l_and_one());
        let obj = fake.ptr();
        // The game's grouping started one base before ours took over.
        // SAFETY: the fake object and its zone 0.
        unsafe { new_base(obj, fake.zones.as_ptr().cast(), 0) };
        // SAFETY: the fake object, filled as after the game's grouping.
        let bases = unsafe { settle(obj, FIELDS, 16, new_base) }.unwrap();
        assert_eq!(bases.len(), 2);
        assert_eq!(fake.u32_at(0x24), 2);
        assert_eq!(fake.u32_at(0x140), u32::MAX, "zones 0 to 31 in the mask");
        assert_eq!(fake.u32_at(0x140 + 0x130), 0, "zone 40 is past the mask");
        assert_eq!(fake.f32s_at(0x30 + 0xf0), [0.0, 0.0, 0.0]);
        assert_eq!(fake.f32s_at(0x30 + 0x100), [200.0, 10.0, 210.0]);
        let game = fake.view();
        assert_eq!(game.members(0).len(), 40);
        assert!(game.members(1).contains(40));
    }

    #[test]
    fn lookups_answer_for_zones_past_32_as_the_game_lays_out_its_answers() {
        let mut fake = Fake::new(0, &l_and_one());
        let obj = fake.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base) }.unwrap();
        let game = fake.view();
        let mut out = [0xAAu8; 0x30];
        // SAFETY: a writable answer.
        unsafe { base_at(out.as_mut_ptr(), &game, [5.0, 5.0, 205.0]) };
        assert_eq!((out[0], &out[4..8], out[8]), (0, &[0u8; 4][..], 0));
        assert!(in_base(&game, 0, [5.0, 5.0, 205.0]));
        assert!(!in_base(&game, 0, [150.0, 5.0, 150.0]));
        // SAFETY: as above.
        unsafe { base_at(out.as_mut_ptr(), &game, [1005.0, 5.0, 1005.0]) };
        assert_eq!((out[0], out[8]), (0, 1));
        // SAFETY: as above.
        unsafe { base_at(out.as_mut_ptr(), &game, [150.0, 5.0, 150.0]) };
        assert_eq!((out[0], out[8]), (NOT_IN_A_BASE, 0));
        // SAFETY: as above, 0x30 bytes.
        unsafe { holder_at(out.as_mut_ptr(), &game, [5.0, 5.0, 205.0]) };
        let zone_39: [u8; 0x20] = bytemuck::cast(fake.zones[39].0);
        assert_eq!(out[0], 0);
        assert_eq!(out[0x10..0x30], zone_39);
        // SAFETY: as above.
        unsafe { holder_at(out.as_mut_ptr(), &game, [150.0, 5.0, 150.0]) };
        assert_eq!(out[0], NOT_IN_A_BASE);
        assert_eq!(out[0x10..0x14], f32::MAX.to_le_bytes());
        assert_eq!(out[0x20..0x24], (-f32::MAX).to_le_bytes());
    }

    /// The game's zone box container with an inline buffer of 16 boxes and
    /// a grow callback, as its one caller builds it.
    #[repr(C)]
    struct List {
        data: *mut Entry,
        len: u64,
        cap: u64,
        grow: Option<GrowFn>,
        allocator: usize,
    }

    unsafe extern "system" fn grow(vec: *mut u8, wanted: i64, _exact: u64) -> u8 {
        let wanted = usize::try_from(-wanted).unwrap();
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the game hands back the test's own aligned container"
        )]
        let list = vec.cast::<List>();
        // SAFETY: the test's container, a `List` the test aligned.
        let list = unsafe { &mut *list };
        let mut bigger = vec![Entry([0.0; 8]); wanted.next_power_of_two()];
        let len = usize::try_from(list.len).unwrap();
        // SAFETY: the old data holds `len` boxes.
        bigger[..len].copy_from_slice(unsafe { core::slice::from_raw_parts(list.data, len) });
        list.cap = u64::try_from(bigger.len()).unwrap();
        list.data = Box::leak(bigger.into_boxed_slice()).as_mut_ptr();
        1
    }

    #[test]
    fn the_zones_of_a_base_past_32_are_appended_through_the_game_s_callback() {
        let mut fake = Fake::new(1, &l_and_one());
        let obj = fake.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base) }.unwrap();
        let game = fake.view();
        let mut inline = [Entry([0.0; 8]); 16];
        let mut list = List {
            data: inline.as_mut_ptr(),
            len: 0,
            cap: 16,
            grow: Some(grow),
            allocator: 0,
        };
        let mut out = [0xAAu8; 0x10];
        let vec = (&raw mut list).cast::<u8>();
        // SAFETY: a writable answer and the test's container.
        unsafe { zones_at(out.as_mut_ptr(), vec, &game, [5.0, 5.0, 5.0]) };
        assert_eq!((out[0], &out[4..8]), (0, &[0u8; 4][..]));
        assert_eq!(list.len, 40);
        assert_eq!(list.allocator, super::super::heap::handed());
        // SAFETY: the container holds `len` boxes.
        let got = unsafe { core::slice::from_raw_parts(list.data, 40) };
        assert_eq!(got[39].0, fake.zones[39].0);
        let mut stuck = List {
            grow: None,
            ..List {
                data: inline.as_mut_ptr(),
                len: 0,
                cap: 16,
                grow: None,
                allocator: 0,
            }
        };
        // SAFETY: as above, with no grow callback.
        unsafe {
            zones_at(
                out.as_mut_ptr(),
                (&raw mut stuck).cast(),
                &game,
                [5.0, 5.0, 5.0],
            );
        };
        assert_eq!(stuck.len, 16, "boxes past the room are skipped");
    }

    #[test]
    fn the_zone_list_takes_the_object_s_allocator_once() {
        let mut fake = Fake::new(1, &[square(0.0, 0.0)]);
        let obj = fake.ptr();
        // SAFETY: the fake object, without an allocator yet.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(fake.obj.0[0x1350..0x1358], [0; 8], "no allocator to hand");
        fake.obj.0[..8].copy_from_slice(&0x1000usize.to_le_bytes());
        // SAFETY: as above, now with an allocator.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(fake.obj.0[0x1350..0x1358], 0x1001usize.to_le_bytes());
        fake.obj.0[0x1350..0x1358].copy_from_slice(&0x2001usize.to_le_bytes());
        // SAFETY: as above.
        unsafe { let_zone_list_grow(obj, FIELDS) };
        assert_eq!(
            fake.obj.0[0x1350..0x1358],
            0x2001usize.to_le_bytes(),
            "a list that has one keeps it"
        );
    }

    #[test]
    fn more_bases_than_planned_settle_nothing() {
        let mut fake = Fake::new(1, &[square(0.0, 0.0), square(10.0, 0.0)]);
        let obj = fake.ptr();
        for (z, entry) in (0u32..).zip(&fake.zones) {
            // SAFETY: the fake object and zone `z`.
            unsafe { new_base(obj, core::ptr::from_ref(entry).cast(), z) };
        }
        // SAFETY: the fake object, with one base more than our plan.
        assert!(unsafe { settle(obj, FIELDS, 16, new_base) }.is_none());
    }

    #[test]
    fn an_object_without_a_side_table_reads_its_masks() {
        let mut settled = Fake::new(1, &l_and_one());
        let obj = settled.ptr();
        // SAFETY: the fake object, with no base started yet.
        let _bases = unsafe { settle(obj, FIELDS, 16, new_base) }.unwrap();
        let mut other = Fake::new(1, &l_and_one());
        let obj = other.ptr();
        // SAFETY: the fake object and its zone 0.
        unsafe { new_base(obj, other.zones.as_ptr().cast(), 0) };
        assert_eq!(other.view().members(0), ZoneSet::from_mask(1));
        forget();
        assert_eq!(
            settled.view().members(0).len(),
            32,
            "the mask after a forget"
        );
    }
}
