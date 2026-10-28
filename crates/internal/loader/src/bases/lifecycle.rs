//! The window records of a grown bases object, from its create to its
//! destroy, done with the game's own code so every container is made and
//! freed through the allocators the game stores in it.
//!
//! The game's constructor fills its 16 records; a create hook copies the
//! record it makes into every window record. Its teardown releases 16
//! records from any record it is handed and is a no-op on a released one, so
//! a regroup and a destroy release the window in [`window::teardown_starts`]
//! chunks. The server's object keeps a building stock slot per base, a map
//! the game makes at create; a window slot is made, on either side, when its
//! index is first handed out, since a stock reader reaches any index point
//! in base answers, and freed at destroy. Facts from server 399 and client
//! 426, where the bodies are the same; the allocator does not clear the
//! block, so nothing assumes a window byte is zero.

use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;

use enshwick::raw::read_at;

use crate::base_groups::window;

/// The constructor, `fn(obj) -> obj`: header, 16 records, the tail up to the
/// stock slots; no call, no allocation.
pub(super) type CtorFn = unsafe extern "system" fn(obj: *mut u8) -> *mut u8;
/// The teardown of 16 records from the record it is handed.
pub(super) type TeardownFn = unsafe extern "system" fn(first: *mut u8);
/// Makes a stock slot's map for `entries` through `allocator`, after
/// releasing what it held.
pub(super) type SlotInitFn =
    unsafe extern "system" fn(slot: *mut u8, allocator: usize, entries: u64, grows: u8);
/// Frees everything a stock slot's map holds.
pub(super) type SlotFreeFn = unsafe extern "system" fn(slot: *mut u8);

/// The bytes the constructor writes from the object's start, up to the
/// first stock slot.
const CONSTRUCTED: usize = window::SLOTS;
/// The world allocator a bases object keeps first.
const OBJECT_ALLOCATOR: usize = 0x0;
/// The entries of a stock slot's map, as create makes the 16 of the array.
const SLOT_ENTRIES: u64 = 0x106;

/// The game's code the window needs.
#[derive(Clone, Copy, Debug)]
pub(super) struct Code {
    pub(super) ctor: CtorFn,
    pub(super) teardown: TeardownFn,
    pub(super) slot_init: SlotInitFn,
    pub(super) slot_free: SlotFreeFn,
}

/// One grown object whose window records are initialised, and which window
/// stock slots it made.
#[derive(Debug)]
struct Windowed {
    owner: AtomicUsize,
    slots: [AtomicU64; 4],
}

impl Windowed {
    const fn new() -> Self {
        Self {
            owner: AtomicUsize::new(0),
            slots: [const { AtomicU64::new(0) }; 4],
        }
    }

    fn has_slot(&self, i: usize) -> bool {
        self.slots[i / 64].load(Ordering::Relaxed) >> (i % 64) & 1 == 1
    }

    fn take_slot(&self, i: usize) {
        self.slots[i / 64].fetch_or(1 << (i % 64), Ordering::Relaxed);
    }

    fn made_slots(&self) -> impl Iterator<Item = usize> + '_ {
        (window::WINDOW..window::INDICES).filter(|&i| self.has_slot(i))
    }
}

/// The objects whose window is initialised: the server's and the client's
/// of a hosting client, and room for a world that opens before the last one
/// is destroyed.
static WINDOWED: [Windowed; 4] = [const { Windowed::new() }; 4];

/// The record the constructor makes, copied into every window record.
static CONSTRUCTED_RECORD: OnceLock<[u8; window::RECORD]> = OnceLock::new();

/// The entry of `obj`; none for null, which marks a free entry.
fn windowed(obj: *const u8) -> Option<&'static Windowed> {
    if obj.is_null() {
        return None;
    }
    WINDOWED
        .iter()
        .find(|w| w.owner.load(Ordering::Acquire) == obj.addr())
}

/// A zeroed block of `len` bytes, 16-aligned as the game's objects.
fn zeroed(len: usize) -> Box<[u128]> {
    vec![0u128; len.div_ceil(16)].into_boxed_slice()
}

/// The record the constructor makes on a zeroed block.
fn constructed(ctor: CtorFn) -> &'static [u8; window::RECORD] {
    CONSTRUCTED_RECORD.get_or_init(|| {
        let mut scratch = zeroed(CONSTRUCTED);
        let at = scratch.as_mut_ptr().cast::<u8>();
        // SAFETY: the game's constructor writes only the header, the 16
        // records and the tail below the first stock slot, `CONSTRUCTED`
        // bytes, all inside the zeroed, 16-aligned scratch block; it calls
        // nothing and allocates nothing.
        let _obj = unsafe { ctor(at) };
        // SAFETY: record 0 lies inside the scratch block, written by the
        // constructor or zero.
        unsafe { read_at::<[u8; window::RECORD]>(at, window::RECORDS) }
    })
}

/// Fills every window record of the freshly created object `obj` as the
/// constructor fills a record, and keeps it as windowed; `false` when there
/// is no room to keep it, and its window stays unused.
///
/// # Safety
///
/// - `obj` is a live bases object of [`window::BLOCK`] bytes that its create
///   has just returned, on the create's thread, before any other code has
///   it.
pub(super) unsafe fn init(obj: *mut u8, code: &Code) -> bool {
    let Some(entry) = WINDOWED.iter().find(|w| {
        w.owner
            .compare_exchange(0, obj.addr(), Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }) else {
        return false;
    };
    let record = constructed(code.ctor);
    for i in window::WINDOW..window::INDICES {
        let at = obj.wrapping_add(window::RECORDS + i * window::RECORD);
        // SAFETY: record `i` of the window lies inside the caller's block
        // of `window::BLOCK` bytes, which no one else has yet, and the
        // constructed record is `window::RECORD` bytes of our own.
        unsafe { core::ptr::copy_nonoverlapping(record.as_ptr(), at, window::RECORD) };
    }
    for word in &entry.slots {
        word.store(0, Ordering::Relaxed);
    }
    true
}

/// Releases every window record of `obj`, as the game's grouping releases
/// its bases before it starts them again; nothing for an object whose window
/// is not initialised.
///
/// # Safety
///
/// - `obj` is a live bases object, on the grouping's or the destroy's
///   thread, with no reader meanwhile.
pub(super) unsafe fn release(obj: *mut u8, code: &Code) {
    if windowed(obj).is_none() {
        return;
    }
    for k in window::teardown_starts() {
        // SAFETY: the object is windowed, so its block holds every window
        // record, initialised by `init` or since started by the game's new
        // base code; the teardown reads and frees only the 16 records from
        // record `k`, which end at index 255 at the latest.
        unsafe { (code.teardown)(obj.wrapping_add(window::RECORDS + k * window::RECORD)) };
    }
}

/// What readies window index `i` of `obj` before a base starts there: makes
/// the index's stock slot the first time, on either side, since a stock
/// reader may reach any index point in base answers; `None` when the
/// object's window is not initialised.
///
/// # Safety
///
/// - `obj` is a live bases object, on the grouping's thread, and stays so
///   for as long as the answer is used.
pub(super) unsafe fn ready(obj: *mut u8, code: Code) -> Option<impl Fn(usize)> {
    let entry = windowed(obj)?;
    Some(move |i: usize| {
        if entry.has_slot(i) || i < window::WINDOW {
            return;
        }
        let slot = obj.wrapping_add(window::SLOTS + i * window::SLOT);
        // SAFETY: slot `i` lies in the window of the caller's live, grown
        // object, which no game code reads before the index is handed out;
        // zeroed, it holds no map the init would release.
        unsafe { slot.write_bytes(0, window::SLOT) };
        // SAFETY: the caller guarantees a live object; its world allocator
        // sits at `OBJECT_ALLOCATOR`.
        let allocator = unsafe { read_at::<usize>(obj, OBJECT_ALLOCATOR) };
        // SAFETY: the game's stock slot init on a zeroed slot with the
        // object's own allocator and entry count, as create calls it for the
        // array's slots.
        unsafe { (code.slot_init)(slot, allocator, SLOT_ENTRIES, 0) };
        entry.take_slot(i);
    })
}

/// Frees the window of `obj` before the game destroys it: every window
/// record and every window stock slot it made, through the game's own code,
/// then forgets the object.
///
/// # Safety
///
/// - `obj` is null or a live bases object the game is about to destroy, on
///   the destroy's thread.
pub(super) unsafe fn destroy(obj: *mut u8, code: &Code) {
    let Some(entry) = windowed(obj) else {
        return;
    };
    // SAFETY: the caller guarantees a live object about to be destroyed,
    // with no other user.
    unsafe { release(obj, code) };
    for i in entry.made_slots() {
        let slot = obj.wrapping_add(window::SLOTS + i * window::SLOT);
        // SAFETY: slot `i` is one `ready` made through the game's init, in
        // the window of the live object, freed once here.
        unsafe { (code.slot_free)(slot) };
        // SAFETY: the same slot, zeroed so nothing frees it twice.
        unsafe { slot.write_bytes(0, window::SLOT) };
    }
    entry.owner.store(0, Ordering::Release);
}

/// Forgets every windowed object without freeing anything: for tests.
#[cfg(test)]
fn forget_all() {
    for entry in &WINDOWED {
        entry.owner.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::raw::write_at;

    use super::*;
    use crate::locks::Relock;

    /// What the fake game code was called with, in order.
    static CALLS: Mutex<Vec<(&'static str, usize)>> = Mutex::new(Vec::new());

    /// The constructor's record fields the tests check: list A's grower and
    /// the box's corners.
    const GROWER: usize = 0x18;
    const LOW: usize = 0xf0;
    const HIGH: usize = 0x100;

    unsafe extern "system" fn ctor(obj: *mut u8) -> *mut u8 {
        CALLS.relock().push(("ctor", obj.addr()));
        for i in 0..window::VANILLA {
            let record = window::RECORDS + i * window::RECORD;
            // SAFETY: the scratch block holds 16 records.
            unsafe { write_at(obj, record + GROWER, 0x1_4020_09f0_u64) };
            // SAFETY: the same record.
            unsafe { write_at(obj, record + LOW, [f32::MAX; 3]) };
            // SAFETY: the same record.
            unsafe { write_at(obj, record + HIGH, [-f32::MAX; 3]) };
        }
        obj
    }

    unsafe extern "system" fn teardown(first: *mut u8) {
        CALLS.relock().push(("teardown", first.addr()));
    }

    unsafe extern "system" fn slot_init(slot: *mut u8, allocator: usize, entries: u64, grows: u8) {
        assert_eq!((entries, grows), (SLOT_ENTRIES, 0));
        // SAFETY: the test's slot, inside its block.
        let bytes = unsafe { core::slice::from_raw_parts(slot, window::SLOT) };
        assert!(bytes.iter().all(|&b| b == 0));
        CALLS.relock().push(("slot_init", slot.addr() ^ allocator));
    }

    unsafe extern "system" fn slot_free(slot: *mut u8) {
        CALLS.relock().push(("slot_free", slot.addr()));
    }

    const CODE: Code = Code {
        ctor,
        teardown,
        slot_init,
        slot_free,
    };

    /// A grown object full of garbage, as the game's allocator may hand it.
    fn garbage() -> Box<[u128]> {
        vec![u128::MAX / 3; window::BLOCK.div_ceil(16)].into_boxed_slice()
    }

    /// No object windowed and no call seen, as each test starts.
    fn fresh() {
        forget_all();
        CALLS.relock().clear();
    }

    fn calls(name: &str) -> Vec<usize> {
        CALLS
            .relock()
            .iter()
            .filter(|(n, _)| *n == name)
            .map(|&(_, at)| at)
            .collect()
    }

    #[test]
    fn every_window_record_takes_the_constructor_s_record() {
        fresh();
        let mut block = garbage();
        let obj = block.as_mut_ptr().cast::<u8>();
        // SAFETY: a fresh block of the grown size.
        assert!(unsafe { init(obj, &CODE) });
        let bytes: &[u8] = bytemuck::cast_slice(&block);
        let made = constructed(ctor);
        for i in window::WINDOW..window::INDICES {
            let at = window::RECORDS + i * window::RECORD;
            assert_eq!(&bytes[at..at + window::RECORD], made, "record {i}");
        }
        assert_eq!(made[GROWER..GROWER + 8], 0x1_4020_09f0_u64.to_le_bytes());
        assert!(made[0x50..0x90].iter().all(|&b| b == 0), "zeroed first");
        let header = window::RECORDS + 15 * window::RECORD;
        assert_eq!(bytes[header], 0x55, "the array is the game's");
        assert!(
            calls("ctor").len() <= 1,
            "the constructor runs once a process"
        );
    }

    #[test]
    fn a_release_tears_the_window_down_in_chunks_and_skips_other_objects() {
        fresh();
        let mut block = garbage();
        let obj = block.as_mut_ptr().cast::<u8>();
        let mut other = garbage();
        // SAFETY: a block of the grown size the fake create never handed
        // out; release ignores it.
        unsafe { release(other.as_mut_ptr().cast(), &CODE) };
        assert!(calls("teardown").is_empty());
        // SAFETY: a fresh block of the grown size.
        assert!(unsafe { init(obj, &CODE) });
        // SAFETY: the windowed block.
        unsafe { release(obj, &CODE) };
        let starts: Vec<usize> = window::teardown_starts()
            .map(|k| obj.addr() + window::RECORDS + k * window::RECORD)
            .collect();
        assert_eq!(calls("teardown"), starts);
    }

    #[test]
    fn the_server_makes_a_window_stock_slot_once_and_frees_it_at_destroy() {
        fresh();
        let mut block = garbage();
        let obj = block.as_mut_ptr().cast::<u8>();
        // SAFETY: the block's first word, its allocator.
        unsafe { write_at(obj, 0, 0x7000_usize) };
        // SAFETY: a fresh block of the grown size.
        assert!(unsafe { init(obj, &CODE) });
        // SAFETY: the windowed block, a server's object.
        let readied = unsafe { ready(obj, CODE) }.unwrap();
        readied(93);
        readied(93);
        readied(200);
        readied(3);
        let slot = |i: usize| obj.addr() + window::SLOTS + i * window::SLOT;
        assert_eq!(
            calls("slot_init"),
            [slot(93) ^ 0x7000, slot(200) ^ 0x7000],
            "once each, never for the array's"
        );
        // SAFETY: the windowed block, about to be destroyed.
        unsafe { destroy(obj, &CODE) };
        assert_eq!(calls("slot_free"), [slot(93), slot(200)]);
        assert_eq!(calls("teardown").len(), window::teardown_starts().count());
        // SAFETY: the same block, no longer windowed.
        unsafe { destroy(obj, &CODE) };
        assert_eq!(
            calls("slot_free").len(),
            2,
            "a second destroy frees nothing"
        );
        // SAFETY: the block, no longer windowed.
        assert!(unsafe { ready(obj, CODE) }.is_none());
    }

    #[test]
    fn a_destroy_before_the_window_is_made_touches_nothing() {
        fresh();
        let mut block = garbage();
        // SAFETY: a block the create's failure path destroys before our
        // init ran.
        unsafe { destroy(block.as_mut_ptr().cast(), &CODE) };
        // SAFETY: null, as the game's null-safe destroy may get.
        unsafe { destroy(core::ptr::null_mut(), &CODE) };
        assert!(calls("teardown").is_empty() && calls("slot_free").is_empty());
    }
}
