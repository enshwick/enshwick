//! Game memory at an address the loader holds as an integer, read and written
//! through [`enshwick::raw`], the one reader of game memory; only the hook
//! engine's walks over other threads' stacks read words of their own.

use enshwick::raw::{AnyBitPattern, NoUninit};

/// The pointer at `base + offset`, with the provenance the game exposed.
fn at(base: usize, offset: usize) -> *mut u8 {
    core::ptr::with_exposed_provenance_mut(base.wrapping_add(offset))
}

/// Reads a `T` at `base + offset`.
///
/// # Safety
///
/// - `base + offset` is readable for `size_of::<T>()` bytes; no alignment is
///   needed.
/// - No other thread writes those bytes during the read.
pub(crate) unsafe fn read<T: AnyBitPattern>(base: usize, offset: usize) -> T {
    // SAFETY: the caller's guarantee for `base + offset` is the one
    // `read_at` asks for at offset 0.
    unsafe { enshwick::raw::read_at(at(base, offset), 0) }
}

/// Reads a pointer to `T` at `base + offset`.
///
/// # Safety
///
/// - `base + offset` is readable for `size_of::<usize>()` bytes; no
///   alignment is needed.
/// - No other thread writes those bytes during the read.
pub(crate) unsafe fn read_ptr<T>(base: usize, offset: usize) -> *mut T {
    // SAFETY: the caller's guarantee for `base + offset` is the one
    // `read_ptr_at` asks for at offset 0.
    unsafe { enshwick::raw::read_ptr_at(at(base, offset), 0) }
}

/// Copies `out.len()` bytes at `base + offset` into `out`.
///
/// # Safety
///
/// - `base + offset` is readable for `out.len()` bytes.
/// - No other thread writes those bytes during the copy.
pub(crate) unsafe fn read_bytes(base: usize, offset: usize, out: &mut [u8]) {
    // SAFETY: the caller's guarantee for `base + offset` is the one
    // `read_bytes_at` asks for at offset 0.
    unsafe { enshwick::raw::read_bytes_at(at(base, offset), 0, out) };
}

/// Writes `value` at `base + offset`.
///
/// # Safety
///
/// - `base + offset` is writable for `size_of::<T>()` bytes; no alignment is
///   needed.
/// - Nothing else reads or writes those bytes during the write.
/// - The game expects a `T` there.
pub(crate) unsafe fn write<T: NoUninit>(base: usize, offset: usize, value: T) {
    // SAFETY: the caller's guarantee for `base + offset` is the one
    // `write_at` asks for at offset 0.
    unsafe { enshwick::raw::write_at(at(base, offset), 0, value) };
}

/// A blob array in game memory, the game's resource format for a reflected
/// array field: where its first element sits and how many there are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlobArray {
    pub(crate) start: usize,
    pub(crate) count: usize,
}

/// The header a blob array field holds: its first element's offset from the
/// header, then its count.
#[derive(Clone, Copy, bytemuck::Zeroable, bytemuck::Pod)]
#[repr(C)]
struct BlobArrayHeader {
    offset: i32,
    count: u32,
}

impl BlobArray {
    /// How many bytes a blob array header takes in its owner.
    pub(crate) const HEADER_SIZE: usize = size_of::<BlobArrayHeader>();

    /// Reads the blob array whose header sits at `header`; `None` for one
    /// that holds nothing, with no elements or no offset to them.
    ///
    /// # Safety
    ///
    /// - `header` is readable for [`Self::HEADER_SIZE`] bytes; no alignment
    ///   is needed.
    /// - No other thread writes those bytes during the read.
    pub(crate) unsafe fn read(header: usize) -> Option<Self> {
        // SAFETY: the caller's guarantee for the header's bytes is the one
        // `read` asks for, and every bit pattern is a header.
        let BlobArrayHeader { offset, count } = unsafe { read(header, 0) };
        (offset != 0 && count != 0).then(|| Self {
            start: header.wrapping_add_signed(offset as isize),
            count: count as usize,
        })
    }
}

/// Reads the word at `address` on a suspended thread's stack, for the hook
/// engine's walks over other threads' stacks.
///
/// It reads past [`enshwick::raw`] because the address sanitizer marks the
/// gaps between another thread's locals as poisoned: the check is turned off
/// on the function that reads, which a test build under the sanitizer sets
/// with `enshwick_asan`, and the one reader of game memory stays checked.
///
/// # Safety
///
/// - `address` is readable for a word; no alignment is needed.
/// - No other thread writes those bytes during the read.
#[cfg_attr(enshwick_asan, sanitize(address = "off"))]
pub(crate) unsafe fn read_stack_word(address: usize) -> usize {
    // SAFETY: the caller guarantees a readable word at `address` with no
    // concurrent writer; `read_unaligned` needs no alignment, and every bit
    // pattern is a `usize`.
    // ast-grep-ignore: game-memory-through-raw
    unsafe { at(address, 0).cast::<usize>().read_unaligned() }
}

/// Writes the word at `address` on a suspended thread's stack, for the hook
/// engine's fix-up of return addresses.
///
/// It writes past [`enshwick::raw`] for the reason [`read_stack_word`] reads
/// past it.
///
/// # Safety
///
/// - `address` is writable for a word; no alignment is needed.
/// - Nothing else reads or writes those bytes during the write.
/// - The thread may find `value` there.
#[cfg_attr(enshwick_asan, sanitize(address = "off"))]
pub(crate) unsafe fn write_stack_word(address: usize, value: usize) {
    // SAFETY: the caller guarantees a writable word at `address` that
    // nothing else touches; `write_unaligned` needs no alignment.
    unsafe { at(address, 0).cast::<usize>().write_unaligned(value) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytemuck::Zeroable as _;

    #[test]
    fn an_address_and_offset_read_what_was_written() {
        const AT: usize = 5; // bound, not a game offset: a test's memory
        let mut bytes = [0u8; 12];
        let base = bytes.as_mut_ptr().expose_provenance();
        // SAFETY: four writable bytes of `bytes` start at offset 5.
        unsafe { write(base, AT, 0xdead_beef_u32) };
        // SAFETY: the same four bytes, now holding a u32.
        assert_eq!(unsafe { read::<u32>(base, AT) }, 0xdead_beef);
    }

    #[test]
    fn a_byte_span_reads_from_an_address() {
        const AT: usize = 2; // bound, not a game offset: a test's memory
        let bytes = *b"xxname";
        let mut out = [0u8; 4];
        // SAFETY: four readable bytes of `bytes` start at offset 2.
        unsafe { read_bytes(bytes.as_ptr().expose_provenance(), AT, &mut out) };
        assert_eq!(&out, b"name");
    }

    #[test]
    fn a_stack_word_reads_and_writes_at_an_address() {
        let mut words = [1usize, 2, 3];
        let base = words.as_mut_ptr().expose_provenance();
        // SAFETY: `words` holds a word at its second slot.
        unsafe { write_stack_word(base + size_of::<usize>(), 7) };
        // SAFETY: `words` holds a word at its second slot, and nothing
        // borrows `words` during the read.
        assert_eq!(unsafe { read_stack_word(base + size_of::<usize>()) }, 7);
        assert_eq!(words, [1, 7, 3]);
    }

    #[test]
    fn a_blob_array_starts_at_its_offset_from_the_header() {
        let element = BlobArrayHeader::zeroed();
        let mut memory = [element, element];
        memory[1] = BlobArrayHeader {
            offset: -i32::try_from(BlobArray::HEADER_SIZE).unwrap_or(i32::MAX),
            count: 2,
        };
        let first = memory.as_ptr().expose_provenance();
        // SAFETY: the second element of `memory` is a whole blob array header.
        let array = unsafe { BlobArray::read(first + BlobArray::HEADER_SIZE) };
        assert_eq!(
            array,
            Some(BlobArray {
                start: first,
                count: 2
            })
        );
    }

    #[test]
    fn a_blob_array_without_an_offset_or_elements_holds_nothing() {
        let element = BlobArrayHeader::zeroed();
        let mut memory = [element, element];
        memory[0].count = 3;
        memory[1].offset = 1;
        let first = memory.as_ptr().expose_provenance();
        for header in [first, first + BlobArray::HEADER_SIZE] {
            // SAFETY: both elements of `memory` are whole blob array headers.
            assert_eq!(unsafe { BlobArray::read(header) }, None);
        }
    }

    #[test]
    fn a_pointer_reads_back_from_an_address() {
        let target = 9u8;
        let slot = [&raw const target];
        let base = slot.as_ptr().expose_provenance();
        // SAFETY: `slot` holds one pointer at offset 0.
        let found = unsafe { read_ptr::<u8>(base, 0) };
        assert_eq!(found.cast_const(), &raw const target);
    }
}
