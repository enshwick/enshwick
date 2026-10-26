//! Game memory at an address the loader holds as an integer, read and written
//! through [`enshwick::raw`], the one reader of game memory.

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

/// Reads the word at `address` on a suspended thread's stack.
///
/// The address sanitizer marks the gaps between another thread's locals as
/// poisoned, so a walk over that stack is left unchecked under it; the
/// word is still read whole and unaligned reads are fine.
///
/// # Safety
///
/// - `address` is readable for a word.
/// - No other thread writes those bytes during the read.
#[cfg_attr(enshwick_asan, sanitize(address = "off"))]
pub(crate) unsafe fn read_stack_word(address: usize) -> usize {
    // SAFETY: the caller guarantees a readable word at `address` with no
    // concurrent writer; `read_unaligned` needs no alignment, and every bit
    // pattern is a `usize`.
    unsafe { at(address, 0).cast::<usize>().read_unaligned() }
}

/// Writes the word at `address` on a suspended thread's stack, unchecked by
/// the address sanitizer, as [`read_stack_word`] reads.
///
/// # Safety
///
/// - `address` is writable for a word.
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

    #[test]
    fn an_address_and_offset_read_what_was_written() {
        let mut bytes = [0u8; 12];
        let base = bytes.as_mut_ptr().expose_provenance();
        // SAFETY: four writable bytes of `bytes` start at offset 5.
        unsafe { write(base, 5, 0xdead_beef_u32) };
        // SAFETY: the same four bytes, now holding a u32.
        assert_eq!(unsafe { read::<u32>(base, 5) }, 0xdead_beef);
    }

    #[test]
    fn a_byte_span_reads_from_an_address() {
        let bytes = *b"xxname";
        let mut out = [0u8; 4];
        // SAFETY: four readable bytes of `bytes` start at offset 2.
        unsafe { read_bytes(bytes.as_ptr().expose_provenance(), 2, &mut out) };
        assert_eq!(&out, b"name");
    }

    #[test]
    fn a_stack_word_reads_and_writes_at_an_address() {
        let mut words = [1usize, 2, 3];
        let base = words.as_mut_ptr().expose_provenance();
        // SAFETY: `words` holds a word at its second slot.
        unsafe { write_stack_word(base + size_of::<usize>(), 7) };
        // SAFETY: the same word.
        assert_eq!(unsafe { read_stack_word(base + size_of::<usize>()) }, 7);
        assert_eq!(words, [1, 7, 3]);
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
