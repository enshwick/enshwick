//! The pages of the process: what Windows says about the region an address
//! lies in, fresh pages, and changes of a range's protection.

use core::ptr::NonNull;

use windows_sys::Win32::System::Diagnostics::Debug::FlushInstructionCache;
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_FREE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ,
    PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY,
    PAGE_READWRITE, PAGE_WRITECOPY, VirtualAlloc, VirtualProtect, VirtualQuery,
};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Protections of code that runs and reads.
const EXECUTABLE: u32 = PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
/// Protections whose pages read without a fault, unless guarded.
const READABLE: u32 = PAGE_READONLY
    | PAGE_READWRITE
    | PAGE_WRITECOPY
    | PAGE_EXECUTE_READ
    | PAGE_EXECUTE_READWRITE
    | PAGE_EXECUTE_WRITECOPY;

/// A run of pages with one state and one protection, as `VirtualQuery`
/// reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Region {
    /// The first byte of the run.
    pub(crate) base: usize,
    /// The byte past its last.
    pub(crate) end: usize,
    /// `MEM_COMMIT`, `MEM_RESERVE` or `MEM_FREE`.
    pub(crate) state: u32,
    /// The `PAGE_*` protection of committed pages, zero otherwise.
    pub(crate) protect: u32,
}

impl Region {
    /// Whether nothing is reserved here.
    pub(crate) const fn is_free(&self) -> bool {
        self.state == MEM_FREE
    }

    /// Whether the pages are committed and read without a fault.
    pub(crate) const fn reads(&self) -> bool {
        self.state == MEM_COMMIT
            && self.protect & READABLE != 0
            && self.protect & (PAGE_GUARD | PAGE_NOACCESS) == 0
    }

    /// Whether the pages are committed code that runs and reads.
    pub(crate) const fn runs(&self) -> bool {
        self.reads() && self.protect & EXECUTABLE != 0
    }

    /// Whether the pages are committed, plain read-write memory, as a
    /// thread's stack is.
    pub(crate) const fn is_read_write(&self) -> bool {
        self.state == MEM_COMMIT && self.protect == PAGE_READWRITE
    }
}

/// The region `address` lies in; `None` when Windows reports none, for an
/// address past the user range.
pub(crate) fn query(address: usize) -> Option<Region> {
    let mut info = MEMORY_BASIC_INFORMATION {
        BaseAddress: core::ptr::null_mut(),
        AllocationBase: core::ptr::null_mut(),
        AllocationProtect: 0,
        PartitionId: 0,
        RegionSize: 0,
        State: 0,
        Protect: 0,
        Type: 0,
    };
    // SAFETY: `VirtualQuery` reads no memory at `address`, any value of which
    // is allowed, and writes at most `size_of::<MEMORY_BASIC_INFORMATION>()`
    // bytes into `info`, a local of that type.
    let known = unsafe {
        VirtualQuery(
            core::ptr::with_exposed_provenance(address),
            &raw mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        )
    };
    if known == 0 {
        return None;
    }
    let base = info.BaseAddress.addr();
    Some(Region {
        base,
        end: base.checked_add(info.RegionSize)?,
        state: info.State,
        protect: info.Protect,
    })
}

/// Reserves and commits `len` bytes of fresh read-write pages, at `address`
/// when one is given and where Windows picks otherwise; `None` when Windows
/// refused or the range is taken.
pub(crate) fn allocate(address: Option<usize>, len: usize) -> Option<NonNull<u8>> {
    let at = address.map_or(core::ptr::null(), core::ptr::with_exposed_provenance);
    // SAFETY: `MEM_RESERVE` takes only pages no allocation holds, so
    // `VirtualAlloc` touches no existing memory; it returns null when the
    // range is taken or no memory is left.
    let block = unsafe { VirtualAlloc(at, len, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) };
    NonNull::new(block.cast::<u8>())
}

/// Sets the protection of the pages that hold `len` bytes at `address`;
/// the protection they had, or `None` when Windows refused.
///
/// # Safety
///
/// - The pages are the caller's own, or code it patches while every other
///   thread that runs them is suspended.
/// - Nothing that relies on the pages' current protection (running code,
///   writing data) uses them until the caller restores it.
pub(crate) unsafe fn protect(address: usize, len: usize, protection: u32) -> Option<u32> {
    let mut old = 0;
    // SAFETY: the caller guarantees the pages are its own or held still, and
    // that nothing relies on their protection meanwhile; `old` is a local
    // `u32` Windows writes the previous protection to.
    let changed = unsafe {
        VirtualProtect(
            core::ptr::with_exposed_provenance(address),
            len,
            protection,
            &raw mut old,
        )
    };
    (changed != 0).then_some(old)
}

/// Makes code just written at `address` run and read but no longer change,
/// and flushes the instruction cache over it; false when Windows refused.
///
/// # Safety
///
/// - The pages that hold the `len` bytes are the caller's own, fully
///   written, and no thread runs them yet.
pub(crate) unsafe fn seal(address: usize, len: usize) -> bool {
    // SAFETY: the caller owns the pages and no thread runs them, so nothing
    // relies on their protection while it changes.
    let sealed = unsafe { protect(address, len, PAGE_EXECUTE_READ) }.is_some();
    flush(address, len);
    sealed
}

/// Flushes the instruction cache over `len` bytes of code at `address`, so
/// a thread that runs them next sees what was just written.
pub(crate) fn flush(address: usize, len: usize) {
    // SAFETY: `GetCurrentProcess` takes no arguments and returns a
    // pseudo-handle that needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `process` is this process; flushing reads and writes no memory
    // in the range, which may be any address.
    unsafe { FlushInstructionCache(process, core::ptr::with_exposed_provenance(address), len) };
}

#[cfg(test)]
mod tests {
    use windows_sys::Win32::System::Memory::{MEM_RESERVE, PAGE_EXECUTE_READ};

    use super::*;

    fn region(state: u32, protect: u32) -> Region {
        Region {
            base: 0x1000,
            end: 0x3000,
            state,
            protect,
        }
    }

    #[test]
    fn only_committed_unguarded_pages_read() {
        assert!(region(MEM_COMMIT, PAGE_READWRITE).reads());
        assert!(!region(MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD).reads());
        assert!(!region(MEM_COMMIT, PAGE_NOACCESS).reads());
        assert!(!region(MEM_RESERVE, 0).reads());
        assert!(!region(MEM_RESERVE, PAGE_READWRITE).reads());
    }

    #[test]
    fn only_plain_committed_read_write_pages_count_as_a_stack() {
        assert!(region(MEM_COMMIT, PAGE_READWRITE).is_read_write());
        assert!(!region(MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD).is_read_write());
        assert!(!region(MEM_COMMIT, PAGE_EXECUTE_READWRITE).is_read_write());
        assert!(!region(MEM_RESERVE, PAGE_READWRITE).is_read_write());
    }

    #[test]
    fn only_unreserved_pages_are_free() {
        assert!(region(MEM_FREE, 0).is_free());
        assert!(!region(MEM_COMMIT, PAGE_READWRITE).is_free());
        assert!(!region(MEM_RESERVE, 0).is_free());
    }

    #[test]
    fn only_executable_pages_run() {
        assert!(region(MEM_COMMIT, PAGE_EXECUTE_READ).runs());
        assert!(!region(MEM_COMMIT, PAGE_READONLY).runs());
    }

    #[test]
    fn a_local_lies_in_a_read_write_region_that_holds_it() {
        let local = 7u64;
        let address = (&raw const local).addr();
        let found = query(address).expect("a stack local is mapped");
        assert!(found.base <= address && address < found.end);
        assert!(found.is_read_write());
    }

    #[test]
    fn code_lies_in_a_region_that_runs() {
        let address = (only_executable_pages_run as fn() as *const ()).addr();
        assert!(query(address).is_some_and(|found| found.runs()));
    }

    #[test]
    fn a_protection_change_reports_the_old_one() {
        let page = Box::new([0u8; 0x2000]);
        let address = page.as_ptr().addr().next_multiple_of(0x1000);
        // SAFETY: the page lies inside `page`, owned here, and nothing uses
        // it until its protection is restored below.
        let old = unsafe { protect(address, 1, PAGE_READONLY) };
        // SAFETY: the same page, owned here and unused while read-only, back
        // to read-write before any check can fail and free the box.
        let restored = unsafe { protect(address, 1, PAGE_READWRITE) };
        assert_eq!(old, Some(PAGE_READWRITE));
        assert_eq!(restored, Some(PAGE_READONLY));
    }
}
