//! An allocator object of the game's shape, backed by Rust's global
//! allocator, that the loader hands to a game container which has none, so
//! it grows past its inline room from any thread.
//!
//! The game calls `vtable[+0x8](self, bytes, align, 4, flag) -> ptr` to
//! allocate and `vtable[+0x10](self, ptr, 0)` to free, with no size; each
//! block keeps its size in a prefix of [`PREFIX`] bytes. Found on server 399
//! and client 426.

use core::alloc::Layout;

/// The room before each block for its size, which also keeps the block
/// 16-aligned as the game's reads need.
const PREFIX: usize = 16;

#[repr(C)]
struct Vtable {
    first: unsafe extern "system" fn() -> usize,
    allocate: unsafe extern "system" fn(
        heap: *const Heap,
        bytes: usize,
        align: usize,
        kind: u32,
        flag: u8,
    ) -> *mut u8,
    free: unsafe extern "system" fn(heap: *const Heap, block: *mut u8, zero: usize),
}

/// The allocator object: a pointer to its vtable, as the game's.
#[repr(C)]
pub(super) struct Heap {
    vtable: &'static Vtable,
}

static VTABLE: Vtable = Vtable {
    first: unused,
    allocate,
    free,
};

/// The one allocator the loader hands out.
pub(super) static HEAP: Heap = Heap { vtable: &VTABLE };

/// The value a game container keeps in its allocator field for [`HEAP`]:
/// its address with the bit the game sets on a handed allocator.
pub(super) fn handed() -> usize {
    core::ptr::from_ref(&HEAP).addr() | 1
}

/// The vtable's first slot, which the game never calls on a container's
/// allocator; answers 0 should it ever be called.
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

    #[test]
    fn a_block_comes_16_aligned_and_goes_back_through_the_vtable() {
        let heap = (handed() & !1) as *const Heap;
        assert_eq!(heap, core::ptr::from_ref(&HEAP));
        // SAFETY: the loader's heap, called as the game calls it.
        let block = unsafe { (HEAP.vtable.allocate)(heap, 32 * 0x20, 0x10, 4, 0) };
        assert!(!block.is_null());
        assert_eq!(block.addr() % 16, 0);
        // SAFETY: the block holds 1024 bytes.
        unsafe { block.write_bytes(0xAB, 32 * 0x20) };
        // SAFETY: a block of this heap, freed once.
        unsafe { (HEAP.vtable.free)(heap, block, 0) };
        // SAFETY: as above.
        assert!(unsafe { (HEAP.vtable.allocate)(heap, 8, 0x40, 4, 0) }.is_null());
    }
}
