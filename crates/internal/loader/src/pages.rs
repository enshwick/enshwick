//! The pages of the process: what Windows says about the region an address
//! lies in, fresh pages, and changes of a range's protection.

use core::cell::Cell;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::System::Diagnostics::Debug::{FlushInstructionCache, ReadProcessMemory};
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
    /// The first byte of the allocation it lies in, zero for free pages.
    pub(crate) allocation: usize,
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

    /// Whether the pages are committed, unguarded and read-only.
    pub(crate) const fn is_read_only(&self) -> bool {
        self.state == MEM_COMMIT && self.protect == PAGE_READONLY
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
        allocation: info.AllocationBase.addr(),
        state: info.State,
        protect: info.Protect,
    })
}

/// How many bytes from `address` on, at most `max`, lie in read-only pages.
///
/// They are the run of committed, unguarded `PAGE_READONLY` regions that
/// starts there, which no thread of the process writes without first
/// changing their protection; 0 when `address` lies in none. Reads none of
/// them.
///
/// Each thread remembers the runs it found until a world goes away
/// ([`forget`]), since a query costs 0.8 to 1.4 ms on the game's 490 MB
/// region of template set-up data, which stays read-only for a world's life
/// (server 26430253); a pointer into a remembered run makes no query.
#[must_use]
pub fn read_only(address: usize, max: usize) -> usize {
    let wanted = address.saturating_add(max);
    let mut end = address;
    while end < wanted
        && let Some(next) = run_end(end)
    {
        end = next;
    }
    end.min(wanted) - address
}

/// How many runs of read-only pages each thread remembers; the game keeps
/// its template set-up data in one.
const RUNS: usize = 8;

/// The runs of read-only pages one thread found since the last [`forget`].
struct Found {
    /// The [`FORGOTTEN`] count the runs were found under.
    epoch: Cell<usize>,
    /// Each run as its start and end; an empty one is unused.
    runs: [Cell<(usize, usize)>; RUNS],
    /// The slot the next new run takes.
    next: Cell<usize>,
}

impl Found {
    /// No runs, found under no [`forget`] yet.
    const fn new() -> Self {
        Self {
            epoch: Cell::new(0),
            runs: [const { Cell::new((0, 0)) }; RUNS],
            next: Cell::new(0),
        }
    }

    /// The end of the remembered run `address` lies in, after dropping the
    /// runs found before the last [`forget`].
    fn end(&self, address: usize) -> Option<usize> {
        let epoch = FORGOTTEN.load(Ordering::Acquire);
        if self.epoch.replace(epoch) != epoch {
            self.runs.iter().for_each(|run| run.set((0, 0)));
        }
        self.runs
            .iter()
            .map(Cell::get)
            .find(|&(start, end)| start <= address && address < end)
            .map(|(_, end)| end)
    }

    /// Remembers `start..end`, unless a [`forget`] came since [`Found::end`];
    /// a remembered run with the same end is the same run, and grows down.
    fn keep(&self, start: usize, end: usize) {
        if FORGOTTEN.load(Ordering::Acquire) != self.epoch.get() {
            return;
        }
        if let Some(same) = self.runs.iter().find(|run| run.get().1 == end) {
            same.set((same.get().0.min(start), end));
            return;
        }
        let slot = self.next.get();
        self.next.set((slot + 1) % RUNS);
        if let Some(run) = self.runs.get(slot) {
            run.set((start, end));
        }
    }
}

/// How many times a world went away and may have freed the runs any
/// thread remembers.
static FORGOTTEN: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static FOUND: Found = const { Found::new() };
}

/// Drops the runs of read-only pages every thread remembers, when a world
/// goes away and may free them.
pub fn forget() {
    FORGOTTEN.fetch_add(1, Ordering::Release);
}

/// The end of the run of read-only pages `address` lies in, remembered or
/// found and then remembered; `None` when it lies in none.
fn run_end(address: usize) -> Option<usize> {
    FOUND
        .try_with(|found| {
            if let Some(end) = found.end(address) {
                return Some(end);
            }
            let (start, end) = find(address)?;
            found.keep(start, end);
            Some(end)
        })
        .unwrap_or_else(|_| find(address).map(|(_, end)| end))
}

/// The start and end of the run of read-only pages `address` lies in, from
/// its allocation's base when the run starts there, so that a pointer
/// below `address` in the same run finds it remembered: Windows reports a
/// region from the page it is asked about, not from where the run begins.
fn find(address: usize) -> Option<(usize, usize)> {
    let region = query(address).filter(Region::is_read_only)?;
    if region.base == region.allocation {
        return Some((region.base, region.end));
    }
    let start = query(region.allocation)
        .filter(|first| first.is_read_only() && first.end > address)
        .map_or(region.base, |first| first.base);
    Some((start, region.end))
}

/// The `N` bytes at `address`; `None` unless all of them lie in one region
/// that [`Region::reads`], and still read when copied, though another thread
/// may free or protect the pages in between. Allocates nothing, for the
/// crash hook.
pub(crate) fn read<const N: usize>(address: usize) -> Option<[u8; N]> {
    let region = query(address).filter(Region::reads)?;
    if address.checked_add(N)? > region.end {
        return None;
    }
    copy(address)
}

/// The `N` bytes at `address`, copied by the kernel, which refuses a page
/// that does not read where a plain read would fault.
fn copy<const N: usize>(address: usize) -> Option<[u8; N]> {
    let mut bytes = [0u8; N];
    copy_into(address, &mut bytes).then_some(bytes)
}

/// Fills `bytes` from `address` like [`copy`]; whether all of them read.
pub(crate) fn copy_into(address: usize, bytes: &mut [u8]) -> bool {
    let mut copied = 0;
    // SAFETY: `GetCurrentProcess` takes no arguments and returns a
    // pseudo-handle that needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `ReadProcessMemory` reads this process's
    // `address..address + bytes.len()` through the kernel and fails rather
    // than faults when any of it is not accessible ("the entire area to be
    // read must be accessible, or the operation fails", Win32 docs); it
    // writes at most `bytes.len()` bytes into `bytes` and their count into
    // `copied`.
    let done = unsafe {
        ReadProcessMemory(
            process,
            core::ptr::with_exposed_provenance(address),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
            &raw mut copied,
        )
    };
    done != 0 && copied == bytes.len()
}

/// Up to `max` bytes at `address`: as many as the regions from there that
/// [`Region::reads`] hold, so code just before an unreadable page still
/// reads; `None` when not one byte does.
pub(crate) fn read_up_to(address: usize, max: usize) -> Option<Vec<u8>> {
    let wanted = address.checked_add(max)?;
    let mut end = address;
    while end < wanted {
        match query(end) {
            Some(next) if next.reads() && next.end > end => end = next.end,
            _ => break,
        }
    }
    let mut bytes = vec![0; end.min(wanted) - address];
    (!bytes.is_empty() && copy_into(address, &mut bytes)).then_some(bytes)
}

/// Reserves and commits `len` bytes of fresh read-write pages, at `address`
/// rounded down to the 64 KiB allocation granularity when one is given and
/// where Windows picks otherwise; `None` when Windows refused or the range
/// is taken.
pub(crate) fn allocate(address: Option<usize>, len: usize) -> Option<NonNull<u8>> {
    let at = address.map_or(core::ptr::null(), core::ptr::with_exposed_provenance);
    // SAFETY: the `VirtualAlloc` documentation says `MEM_RESERVE` fails on
    // a range any allocation already reserved, so the call touches no
    // existing memory; it returns null then, and when no memory is left.
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
    use windows_sys::Win32::System::Memory::{
        MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READ, VirtualFree,
    };

    use super::*;

    fn region(state: u32, protect: u32) -> Region {
        Region {
            base: 0x1000,
            end: 0x3000,
            allocation: 0x1000,
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

    #[test]
    fn bytes_read_only_where_all_of_them_read() {
        const PAGE: usize = 4096;
        let page = allocate(None, PAGE).expect("a fresh page");
        let start = page.as_ptr().expose_provenance();
        let last = start + PAGE - 8;
        // SAFETY: the eight bytes at `last` lie in the read-write page just
        // committed.
        unsafe { crate::memory::write_bytes(last, 0, &[1, 2, 3, 4, 5, 6, 7, 8]) };
        assert_eq!(read::<8>(last), Some([1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(read::<8>(last + 1), None, "past the page");
        assert_eq!(read::<8>(0), None);
        // SAFETY: `page` is the test's own allocation, released once.
        unsafe { VirtualFree(page.as_ptr().cast(), 0, MEM_RELEASE) };
        assert_eq!(read::<8>(start), None, "released");
        assert_eq!(
            copy::<8>(start),
            None,
            "a page released after the query is refused, not a fault"
        );
    }

    #[test]
    fn the_read_only_count_runs_over_read_only_regions_to_the_cap_and_is_zero_on_writable_pages() {
        const PAGE: usize = 4096;
        let pages = allocate(None, 3 * PAGE).expect("fresh pages");
        let start = pages.as_ptr().expose_provenance();
        assert_eq!(read_only(start, usize::MAX), 0, "read-write pages");
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { protect(start, PAGE, PAGE_READONLY) }.expect("first page read-only");
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe {
            protect(start + PAGE, PAGE, PAGE_READONLY | PAGE_GUARD).expect("second page guarded");
        }
        assert_eq!(
            read_only(start, usize::MAX),
            PAGE,
            "a guarded page ends the run"
        );
        // SAFETY: the three pages are the test's own allocation, used by no
        // one else.
        unsafe { protect(start + PAGE, 2 * PAGE, PAGE_READONLY).expect("all read-only") };
        assert_eq!(
            read_only(start, usize::MAX),
            3 * PAGE,
            "one run, as Windows merges it"
        );
        assert_eq!(read_only(start + 100, 3 * PAGE), 3 * PAGE - 100);
        assert_eq!(read_only(start, 16), 16, "the cap");
        assert_eq!(read_only(0, 16), 0);
        // SAFETY: `pages` is the test's own allocation, released once.
        unsafe { VirtualFree(pages.as_ptr().cast(), 0, MEM_RELEASE) };
        forget();
        assert_eq!(read_only(start, 16), 0, "released and forgotten");
    }

    #[test]
    fn a_found_run_answers_from_its_allocation_base_without_a_query_until_forgotten() {
        const PAGE: usize = 4096;
        let pages = allocate(None, 3 * PAGE).expect("fresh pages");
        let start = pages.as_ptr().expose_provenance();
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { protect(start, 3 * PAGE, PAGE_READONLY) }.expect("read-only");
        assert_eq!(read_only(start + PAGE, usize::MAX), 2 * PAGE);
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { protect(start, 3 * PAGE, PAGE_READWRITE) }.expect("read-write");
        assert_eq!(
            read_only(start + 8, usize::MAX),
            3 * PAGE - 8,
            "remembered from the allocation's base, so no query saw the change"
        );
        forget();
        assert_eq!(read_only(start + 8, usize::MAX), 0, "forgotten, so queried");
        // SAFETY: `pages` is the test's own allocation, released once.
        unsafe { VirtualFree(pages.as_ptr().cast(), 0, MEM_RELEASE) };
    }

    #[test]
    fn a_lower_pointer_into_a_run_found_inside_an_allocation_is_remembered_too() {
        const PAGE: usize = 4096;
        let pages = allocate(None, 4 * PAGE).expect("fresh pages");
        let start = pages.as_ptr().expose_provenance();
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { protect(start + PAGE, 3 * PAGE, PAGE_READONLY) }.expect("read-only");
        assert_eq!(read_only(start + 3 * PAGE, usize::MAX), PAGE);
        assert_eq!(read_only(start + 2 * PAGE, usize::MAX), 2 * PAGE);
        // SAFETY: the pages are the test's own allocation, used by no one else.
        unsafe { protect(start + PAGE, 3 * PAGE, PAGE_READWRITE) }.expect("read-write");
        assert_eq!(
            read_only(start + 2 * PAGE, usize::MAX),
            2 * PAGE,
            "remembered"
        );
        forget();
        // SAFETY: `pages` is the test's own allocation, released once.
        unsafe { VirtualFree(pages.as_ptr().cast(), 0, MEM_RELEASE) };
    }

    #[test]
    fn a_run_with_a_remembered_end_grows_that_one_down_and_new_runs_replace_the_oldest() {
        let found = Found::new();
        assert_eq!(found.end(0x3000), None);
        found.keep(0x3000, 0x4000);
        found.keep(0x2000, 0x4000);
        let used = |found: &Found| found.runs.iter().filter(|run| run.get().1 != 0).count();
        assert_eq!(used(&found), 1);
        assert_eq!(found.end(0x2000), Some(0x4000));
        for run in 1..=RUNS {
            found.keep(0x10000 * run, 0x10000 * run + 0x1000);
        }
        assert_eq!(used(&found), RUNS);
        assert_eq!(found.end(0x2000), None, "the oldest run made room");
        assert_eq!(found.end(0x10000), Some(0x11000));
    }
}
