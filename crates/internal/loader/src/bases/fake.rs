//! A bases object of the game's layout for tests, grown to hold the window,
//! with the game's new base code as the research reads it.

use enshwick::raw::{read_at, write_at};

use super::loops::{Layout, MapFields, ObjectFields, RecordFields, StockFields};
use super::{Answer, Container, Fields};
use crate::base_groups::limit::Aabb;
use crate::base_groups::window;

/// The build table's bases fields on server 399 and client 426.
pub(super) const FIELDS: Fields = Fields {
    count: 0x24,
    stride: 0x130,
    members: 0x140,
    zone_list: 0x1330,
    zone_list_len: 0x1338,
    zone_size: 0x20,
    zone_high: 0x10,
    item_altar: 0x43,
    max_altars_reached: 0x40,
    balancing: 0x170,
    kind: 0x08,
    allocator: 0x0,
    answer: Answer {
        error: 0x0,
        zero: 0x4,
        index: 0x8,
        boxed: 0x10,
        size: 0x30,
    },
    container: Container {
        data: 0x0,
        len: 0x8,
        cap: 0x10,
        grow: 0x18,
        allocator: 0x20,
        boxed: 0x20,
    },
};

/// The build table's layout of the loops over every base on server 399 and
/// client 426.
pub(super) const LAYOUT: Layout = Layout {
    record: RecordFields {
        list_all: 0x0,
        list_kept: 0x28,
        set: 0x50,
        set_bitmap: 0x58,
        set_buckets: 0x60,
        set_count: 0x88,
        comfort: 0x90,
        comfort_len: 0x44,
        props: 0xd8,
        base_id: 0xe0,
        beds: 0x114,
        npcs_home: 0x120,
        animals: 0x122,
    },
    object: ObjectFields {
        rebuilt: 0x18,
        changes: 0x1c,
        grouped: 0x20,
        animals: 0x28,
        npcs: 0x2c,
        npcs_home: 0x2e,
        npc_map: 0x1560,
        npc_value: 0xc,
        npc_kind: 0x8,
        animal_map: 0x15b8,
        animal_value: 0x3,
        staging: 0x1b90,
        page: 0x1be8,
        position: 0x1bec,
    },
    map: MapFields {
        bitmap: 0x8,
        buckets: 0x10,
        keys: 0x28,
        values: 0x40,
        count: 0x54,
    },
    stock: StockFields {
        entities: 0x18,
        count: 0x20,
        size: 0x28,
    },
    count: 0x24,
};

/// A bases object, 16-aligned as the game's.
#[repr(C, align(16))]
pub(super) struct Object(pub(super) [u8; window::BLOCK]);

impl Object {
    /// A zeroed object on the heap, too large for the stack.
    fn zeroed() -> Box<Self> {
        let layout = std::alloc::Layout::new::<Self>();
        // SAFETY: the layout has a non-zero size.
        let at = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!at.is_null());
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the block comes from the layout of `Self`, aligned to it"
        )]
        let object = at.cast::<Self>();
        // SAFETY: a fresh, zeroed block of the layout of `Self`, for which
        // zero bytes are a valid value, owned by the box from here on.
        unsafe { Box::from_raw(object) }
    }
}

/// A zone entry: low corner, a word, high corner, a word.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub(super) struct Entry(pub(super) [f32; 8]);

/// A bases object and the zone list it points at.
pub(super) struct Fake {
    pub(super) obj: Box<Object>,
    pub(super) zones: Vec<Entry>,
}

impl Fake {
    /// An object of `kind` whose zone list holds `boxes`.
    pub(super) fn new(kind: u8, boxes: &[Aabb]) -> Self {
        let zones: Vec<Entry> = boxes
            .iter()
            .map(|b| {
                Entry([
                    b.min[0], b.min[1], b.min[2], 7.0, b.max[0], b.max[1], b.max[2], 7.0,
                ])
            })
            .collect();
        let mut fake = Self {
            obj: Object::zeroed(),
            zones,
        };
        fake.obj.0[FIELDS.kind] = kind;
        let list = fake.zones.as_ptr() as usize;
        fake.obj.0[0x1330..0x1338].copy_from_slice(&list.to_le_bytes());
        fake.obj.0[0x1338..0x1340].copy_from_slice(&fake.zones.len().to_le_bytes());
        fake
    }

    pub(super) fn ptr(&mut self) -> *mut u8 {
        self.obj.0.as_mut_ptr()
    }

    pub(super) fn u16_at(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.obj.0[at..at + 2].try_into().unwrap())
    }

    pub(super) fn u32_at(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.obj.0[at..at + 4].try_into().unwrap())
    }

    pub(super) fn u64_at(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.obj.0[at..at + 8].try_into().unwrap())
    }

    pub(super) fn f32s_at(&self, at: usize) -> [f32; 3] {
        [0, 4, 8].map(|o| f32::from_le_bytes(self.obj.0[at + o..at + o + 4].try_into().unwrap()))
    }

    /// The lookups' view of the object.
    pub(super) fn view(&mut self) -> super::members::Game {
        // SAFETY: the fake object lives for the test, laid out as `FIELDS`.
        unsafe { super::members::Game::new(self.ptr(), FIELDS) }
    }

    pub(super) fn put<T: bytemuck::NoUninit>(&mut self, at: usize, value: T) {
        let bytes = bytemuck::bytes_of(&value);
        self.obj.0[at..at + bytes.len()].copy_from_slice(bytes);
    }
}

/// Record `i`'s offset in the object.
pub(super) const fn record(i: usize) -> usize {
    window::RECORDS + i * window::RECORD
}

/// The game's list grower, `fn(list, -wanted, exact) -> grown`, over
/// leaked test blocks of u32 or 16-byte elements by the list's own width.
pub(super) unsafe extern "system" fn grow_u32(list: *mut u8, wanted: i64, _exact: u64) -> u8 {
    // SAFETY: a test list.
    unsafe { grow(list, wanted, 4) }
}

pub(super) unsafe extern "system" fn grow_16(list: *mut u8, wanted: i64, _exact: u64) -> u8 {
    // SAFETY: a test list.
    unsafe { grow(list, wanted, 16) }
}

/// Grows a test list of `width`-byte elements to hold `-wanted`.
///
/// # Safety
///
/// - `list` is a test list: data, length, capacity, grower.
unsafe fn grow(list: *mut u8, wanted: i64, width: usize) -> u8 {
    let need = usize::try_from(-wanted).unwrap();
    // SAFETY: the test list's fields.
    let len = usize::try_from(unsafe { read_at::<u64>(list, 8) }).unwrap();
    // SAFETY: the test list: data at 0, length at 8, capacity at 0x10, grower at 0x18.
    let data = unsafe { read_at::<usize>(list, 0) } as *const u8;
    let mut room = vec![0u8; need * 2 * width].into_boxed_slice();
    if len > 0 {
        // SAFETY: the old block holds `len` elements.
        let old = unsafe { core::slice::from_raw_parts(data, len * width) };
        room[..len * width].copy_from_slice(old);
    }
    let room = Box::leak(room);
    // SAFETY: the test list: data at 0, length at 8, capacity at 0x10, grower at 0x18.
    unsafe { write_at(list, 0, room.as_mut_ptr() as usize) };
    // SAFETY: the test list: data at 0, length at 8, capacity at 0x10, grower at 0x18.
    unsafe { write_at(list, 0x10, (need * 2) as u64) };
    1
}

/// The game's new base code as the research reads it: the count up by one,
/// the zone's box copied whole, the zone's bit in the mask, both lists
/// emptied with the grower set.
pub(super) unsafe extern "system" fn new_base(obj: *mut u8, zone: *const u8, index: u32) {
    // SAFETY: the fake object of the test.
    let i = unsafe { read_at::<u32>(obj, 0x24) };
    // SAFETY: the fake object's count, at 0x24 in its buffer.
    unsafe { write_at(obj, 0x24, i + 1) };
    let at = record(usize::try_from(i).unwrap());
    // SAFETY: a zone entry of the fake list and the record's box.
    unsafe { core::ptr::copy_nonoverlapping(zone, obj.wrapping_add(at + 0xf0), 0x20) };
    // SAFETY: the record's mask.
    unsafe { write_at(obj, at + 0x110, 1u32 << (index & 31)) };
    for list in [at, at + 0x28] {
        // SAFETY: the record's list: data, length, capacity, grower.
        unsafe { write_at(obj, list, [0u64; 3]) };
        let grower = grow_u32 as unsafe extern "system" fn(*mut u8, i64, u64) -> u8;
        // SAFETY: the test list: data at 0, length at 8, capacity at 0x10, grower at 0x18.
        unsafe { write_at(obj, list + 0x18, (grower as *const ()).addr()) };
    }
}

/// A box of 10 metres at `x`, `z`.
pub(super) fn square(x: f32, z: f32) -> Aabb {
    Aabb {
        min: [x, 0.0, z],
        max: [x + 10.0, 10.0, z + 10.0],
    }
}

/// `n` zones 100 metres apart along x, each its own base.
pub(super) fn apart(n: u8) -> Vec<Aabb> {
    (0..n).map(|i| square(f32::from(i) * 100.0, 0.0)).collect()
}

/// The grower the fake constructor puts into each record's first list.
pub(super) const CONSTRUCTED_GROWER: u64 = 0x1_4020_09f0;

/// The game's constructor as the research reads it, for the fields the
/// tests check: each of the 16 records' first list grower and the box
/// corners.
pub(super) unsafe extern "system" fn ctor(obj: *mut u8) -> *mut u8 {
    for i in 0..window::VANILLA {
        let at = record(i);
        // SAFETY: the scratch block holds 16 records; the first list's grower.
        unsafe { write_at(obj, at + 0x18, CONSTRUCTED_GROWER) };
        // SAFETY: the same record's box, low corner.
        unsafe { write_at(obj, at + 0xf0, [f32::MAX; 3]) };
        // SAFETY: the same record's box, high corner.
        unsafe { write_at(obj, at + 0x100, [-f32::MAX; 3]) };
    }
    obj
}

unsafe extern "system" fn nothing(_at: *mut u8) {}

unsafe extern "system" fn slot_made(_slot: *mut u8, _allocator: usize, _entries: u64, _grows: u8) {}

/// The window's game code for objects of [`twenty_bases`]: the fake
/// constructor, and a teardown, slot init and slot free that do nothing.
const CODE: super::lifecycle::Code = super::lifecycle::Code {
    ctor,
    teardown: nothing,
    slot_init: slot_made,
    slot_free: nothing,
};

impl Drop for Fake {
    fn drop(&mut self) {
        let obj = self.ptr();
        // SAFETY: the fake object, about to be freed; the fake code frees
        // nothing.
        unsafe { super::lifecycle::destroy(obj, &CODE) };
        super::members::forget_object(obj);
    }
}

/// A server object grouped into one base per zone of [`apart`], 20 bases:
/// 16 in the array and indices 93 to 96 of the window, each with its stock
/// slot made.
pub(super) fn twenty_bases() -> Fake {
    let mut fake = Fake::new(1, &apart(20));
    let obj = fake.ptr();
    // SAFETY: the fake object of the grown size, fresh.
    assert!(unsafe { super::lifecycle::init(obj, &CODE) });
    // SAFETY: the same object, windowed, live for the call.
    let ready = unsafe { super::lifecycle::ready(obj, FIELDS.allocator, CODE) }.unwrap();
    // SAFETY: the fake object, its window initialised, which the fake new
    // base code takes as released.
    let bases =
        unsafe { super::members::settle(obj, FIELDS, window::MOST, new_base, Some(&ready)) };
    assert_eq!(bases.map(|b| b.len()), Some(20));
    fake
}
