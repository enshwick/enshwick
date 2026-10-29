//! An allocator object of the game's shape, backed by Rust's global
//! allocator, that the loader hands to a game container which has none, so
//! it grows past its inline room from any thread.
//!
//! The game calls `vtable[allocate](self, bytes, align, 4, flag) -> ptr` to
//! allocate and `vtable[free](self, ptr, 0)` to free, with no size, at the
//! slots the build table names; each block keeps its size in a prefix of
//! [`PREFIX`] bytes. Found on server 399 and client 426.

use core::alloc::Layout;
use std::sync::OnceLock;

/// The room before each block for its size, which also keeps the block
/// 16-aligned as the game's reads need.
const PREFIX: usize = 16;
/// The vtable slots the loader's heap holds; the game calls two of them.
pub(super) const SLOTS: usize = 8;

/// Where the game calls an allocator's allocate and free: byte offsets
/// into its vtable, from the build table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Slots {
    pub(super) allocate: usize,
    pub(super) free: usize,
}

impl Slots {
    /// The slots' indices into a vtable of [`SLOTS`] pointers, when both are
    /// pointer-aligned, inside it and apart.
    fn indices(self) -> Option<(usize, usize)> {
        let index = |at: usize| {
            at.is_multiple_of(size_of::<usize>())
                .then(|| at / size_of::<usize>())
                .filter(|&i| i < SLOTS)
        };
        let (allocate, free) = (index(self.allocate)?, index(self.free)?);
        (allocate != free).then_some((allocate, free))
    }

    /// Whether a vtable of [`SLOTS`] pointers holds both slots apart.
    pub(super) fn fit(self) -> bool {
        self.indices().is_some()
    }
}

/// The allocator object: a pointer to its vtable, as the game's.
#[repr(C)]
pub(super) struct Heap {
    vtable: *const usize,
}

// SAFETY: the vtable the pointer names is a static that is never written
// after it is built, and every function in it may run on any thread.
unsafe impl Sync for Heap {}
// SAFETY: as for `Sync`; the object holds nothing tied to a thread.
unsafe impl Send for Heap {}

/// The vtable of the one heap, built for the first slots asked for: a game
/// build calls the same slots for its whole run.
static VTABLE: OnceLock<[usize; SLOTS]> = OnceLock::new();
/// The one allocator the loader hands out.
static HEAP: OnceLock<Heap> = OnceLock::new();

/// The value a game container keeps in its allocator field for the loader's
/// heap: its address with the bit the game sets on a handed allocator; the
/// game calls it at `slots`. `None` when `slots` do not fit a vtable.
pub(super) fn handed(slots: Slots) -> Option<usize> {
    let (allocate_at, free_at) = slots.indices()?;
    let heap = HEAP.get_or_init(|| {
        let vtable = VTABLE.get_or_init(|| {
            let mut vtable = [address(UNUSED as *const ()); SLOTS];
            if let Some(slot) = vtable.get_mut(allocate_at) {
                *slot = address(ALLOCATE as *const ());
            }
            if let Some(slot) = vtable.get_mut(free_at) {
                *slot = address(FREE as *const ());
            }
            vtable
        });
        Heap {
            vtable: vtable.as_ptr(),
        }
    });
    Some(core::ptr::from_ref(heap).addr() | 1)
}

type UnusedFn = unsafe extern "system" fn() -> usize;
type AllocateFn = unsafe extern "system" fn(*const Heap, usize, usize, u32, u8) -> *mut u8;
type FreeFn = unsafe extern "system" fn(*const Heap, *mut u8, usize);

const UNUSED: UnusedFn = unused;
const ALLOCATE: AllocateFn = allocate;
const FREE: FreeFn = free;

/// The address of a function the vtable holds.
fn address(f: *const ()) -> usize {
    f.addr()
}

/// Every slot the game does not call on a container's allocator; answers 0
/// should one ever be called.
unsafe extern "system" fn unused() -> usize {
    0
}

fn layout(bytes: usize, align: usize) -> Option<Layout> {
    let size = bytes.checked_add(PREFIX)?;
    Layout::from_size_align(size, align.max(PREFIX)).ok()
}

unsafe extern "system" fn allocate(
    _heap: *const Heap,
    bytes: usize,
    align: usize,
    _kind: u32,
    _flag: u8,
) -> *mut u8 {
    let Some(layout) = layout(bytes, align).filter(|l| l.align() <= PREFIX) else {
        return core::ptr::null_mut();
    };
    // SAFETY: the layout has a non-zero size, the prefix at least.
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        return base;
    }
    // SAFETY: `base` is a fresh block of at least `PREFIX` bytes, aligned to
    // 16, so the size fits at its start.
    unsafe { enshwick::raw::write_at(base, 0, bytes) };
    base.wrapping_add(PREFIX)
}

unsafe extern "system" fn free(_heap: *const Heap, block: *mut u8, _zero: usize) {
    if block.is_null() {
        return;
    }
    let base = block.wrapping_sub(PREFIX);
    // SAFETY: `block` came from `allocate`, which wrote its size `PREFIX`
    // bytes before it, 16-aligned.
    let bytes = unsafe { enshwick::raw::read_at::<usize>(base, 0) };
    if let Some(layout) = layout(bytes, PREFIX) {
        // SAFETY: `base` was allocated by `allocate` with this layout: the
        // same size and an alignment of 16, the most `allocate` grants.
        unsafe { std::alloc::dealloc(base, layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bases::fake::FIELDS;

    /// The function the game reaches through `heap`'s vtable at byte
    /// offset `at`.
    fn slot(heap: *const Heap, at: usize) -> usize {
        // SAFETY: the loader's heap, a live static.
        let vtable = unsafe { (*heap).vtable };
        // SAFETY: its vtable holds `SLOTS` pointers; `at` is one of the
        // table's slots, inside it.
        unsafe { enshwick::raw::read_at::<usize>(vtable.cast(), at) }
    }

    #[test]
    fn a_block_comes_16_aligned_and_goes_back_through_the_table_s_slots() {
        let slots = FIELDS.heap;
        let heap = (handed(slots).unwrap() & !1) as *const Heap;
        assert_eq!(heap, core::ptr::from_ref(HEAP.get().unwrap()));
        // SAFETY: the table's allocate slot holds `allocate`.
        let allocate =
            unsafe { core::mem::transmute::<usize, AllocateFn>(slot(heap, slots.allocate)) };
        // SAFETY: the table's free slot holds `free`.
        let free = unsafe { core::mem::transmute::<usize, FreeFn>(slot(heap, slots.free)) };
        // SAFETY: the loader's heap, called as the game calls it.
        let block = unsafe { allocate(heap, 32 * 0x20, 0x10, 4, 0) };
        assert!(!block.is_null());
        assert_eq!(block.addr() % 16, 0);
        // SAFETY: the block holds 1024 bytes.
        unsafe { block.write_bytes(0xAB, 32 * 0x20) };
        // SAFETY: a block of this heap, freed once.
        unsafe { free(heap, block, 0) };
        // SAFETY: the loader's heap, asked for an alignment it refuses.
        assert!(unsafe { allocate(heap, 8, 0x40, 4, 0) }.is_null());
        assert_eq!(slot(heap, 0), address(UNUSED as *const ()));
    }

    #[test]
    fn slots_off_the_pointer_grid_outside_the_vtable_or_shared_are_refused() {
        let fit = |allocate, free| Slots { allocate, free }.fit();
        assert!(fit(8, 16));
        assert!(!fit(9, 16), "unaligned");
        assert!(!fit(8, SLOTS * 8), "past the vtable");
        assert!(!fit(8, 8), "shared");
        assert_eq!(
            handed(Slots {
                allocate: 8,
                free: 8
            }),
            None
        );
    }
}
