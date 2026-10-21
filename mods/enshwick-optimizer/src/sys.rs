//! The few Windows and Wine calls the mod makes, each behind a safe function.
//!
//! Wine answers every one of them: `ntdll` exports `wine_get_version` there
//! and nowhere else, and the thread and process times come from the Unix
//! side.

use core::ffi::{CStr, c_char};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{
    CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Threading::{
    CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, CreateWaitableTimerExW, GetActiveProcessorCount,
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, GetProcessTimes,
    GetThreadDescription, GetThreadTimes, INFINITE, OpenThread, SetWaitableTimer,
    THREAD_QUERY_LIMITED_INFORMATION, TIMER_ALL_ACCESS, WaitForSingleObject,
};

/// What `GetProcAddress` returns.
type Proc = unsafe extern "system" fn() -> isize;
/// `NtDelayExecution(alertable, interval)`: a negative interval is relative,
/// in 100 ns units.
type DelayExecution = unsafe extern "system" fn(alertable: u8, interval: *const i64) -> i32;
/// `NtQueryTimerResolution(coarsest, finest, current)`, in 100 ns units.
type QueryTimerResolution =
    unsafe extern "system" fn(coarsest: *mut u32, finest: *mut u32, current: *mut u32) -> i32;
/// `wine_get_version()`.
type WineVersion = unsafe extern "C" fn() -> *const c_char;
/// `wine_get_host_version(&sysname, &release)`.
type WineHostVersion =
    unsafe extern "C" fn(sysname: *mut *const c_char, release: *mut *const c_char);

/// `NtDelayExecution`, found once by [`init`].
static DELAY: OnceLock<DelayExecution> = OnceLock::new();

/// The address of an export of `ntdll.dll`, `None` when it has none.
fn ntdll(name: &CStr) -> Option<Proc> {
    let module: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
    // SAFETY: a NUL-terminated wide string; ntdll is loaded in every process.
    let handle = unsafe { GetModuleHandleW(module.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a module handle and a NUL-terminated name.
    unsafe { GetProcAddress(handle, name.as_ptr().cast()) }
}

/// Finds the functions the hooks call; false when `NtDelayExecution` is
/// missing, and the exact waits cannot run.
pub(crate) fn init() -> bool {
    if DELAY.get().is_some() {
        return true;
    }
    let Some(address) = ntdll(c"NtDelayExecution") else {
        return false;
    };
    // SAFETY: ntdll's `NtDelayExecution` has this signature.
    let delay = unsafe { core::mem::transmute::<Proc, DelayExecution>(address) };
    DELAY.set(delay).is_ok() || DELAY.get().is_some()
}

/// Sleeps `ticks` of 100 ns, not alertable, without touching any handle of
/// the game's. Under Wine a sleep that is not alertable is a plain Unix
/// sleep, with no call into the Wine server.
/// Does nothing before [`init`].
pub(crate) fn delay(ticks: u64) {
    let Some(delay) = DELAY.get() else {
        return;
    };
    let interval = -i64::try_from(ticks).unwrap_or(i64::MAX);
    // SAFETY: a relative interval read for the call.
    let _status = unsafe { delay(0, &raw const interval) };
}

/// A high-resolution waitable timer of one thread; closed when the thread
/// ends.
#[derive(Debug)]
struct Timer(HANDLE);

impl Drop for Timer {
    fn drop(&mut self) {
        // SAFETY: the handle this value created, closed once.
        let _closed = unsafe { CloseHandle(self.0) };
    }
}

thread_local! {
    /// The calling thread's timer, `None` where Windows has none (before
    /// Windows 10 1803).
    static TIMER: Option<Timer> = {
        // SAFETY: no attributes and no name; a failure returns null.
        let handle = unsafe {
            CreateWaitableTimerExW(
                core::ptr::null(),
                core::ptr::null(),
                CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
                TIMER_ALL_ACCESS,
            )
        };
        (!handle.is_null()).then_some(Timer(handle))
    };
}

/// Whether this system has high-resolution waitable timers.
pub(crate) fn precise_timers() -> bool {
    TIMER.try_with(Option::is_some).unwrap_or(false)
}

/// Sleeps `ticks` of 100 ns on a high-resolution timer of the calling
/// thread's own, which Windows fires a fraction of a millisecond late
/// whatever the timer resolution; false when there is none and nothing was
/// slept.
pub(crate) fn delay_precise(ticks: u64) -> bool {
    let due = -i64::try_from(ticks).unwrap_or(i64::MAX);
    TIMER
        .try_with(|timer| {
            let Some(timer) = timer else {
                return false;
            };
            // SAFETY: the thread's own timer and a relative due time, no
            // completion routine.
            let set =
                unsafe { SetWaitableTimer(timer.0, &raw const due, 0, None, core::ptr::null(), 0) };
            if set == 0 {
                return false;
            }
            // SAFETY: the timer is set and fires once.
            let _waited = unsafe { WaitForSingleObject(timer.0, INFINITE) };
            true
        })
        .unwrap_or(false)
}

/// The current timer resolution in nanoseconds, `None` when ntdll does not
/// say.
pub(crate) fn timer_resolution_ns() -> Option<u64> {
    let address = ntdll(c"NtQueryTimerResolution")?;
    // SAFETY: ntdll's `NtQueryTimerResolution` has this signature.
    let query = unsafe { core::mem::transmute::<Proc, QueryTimerResolution>(address) };
    let (mut coarsest, mut finest, mut current) = (0u32, 0u32, 0u32);
    // SAFETY: three out pointers valid for the call.
    let status = unsafe { query(&raw mut coarsest, &raw mut finest, &raw mut current) };
    (status >= 0 && current != 0).then(|| u64::from(current) * 100)
}

/// Copies a C string Wine returned.
///
/// # Safety
///
/// `text` is null or points to a NUL-terminated string.
unsafe fn owned(text: *const c_char) -> Option<String> {
    if text.is_null() {
        return None;
    }
    // SAFETY: forwarded from the caller.
    let text = unsafe { CStr::from_ptr(text) };
    Some(text.to_string_lossy().into_owned())
}

/// The Wine version, such as `10.16`; `None` on Windows.
pub(crate) fn wine_version() -> Option<String> {
    let address = ntdll(c"wine_get_version")?;
    // SAFETY: Wine's `wine_get_version` has this signature.
    let version = unsafe { core::mem::transmute::<Proc, WineVersion>(address) };
    // SAFETY: Wine returns a static string.
    let text = unsafe { version() };
    // SAFETY: a static NUL-terminated string or null.
    unsafe { owned(text) }
}

/// The host system and its release under Wine, such as `Linux` and
/// `6.12.1`.
pub(crate) fn wine_host() -> Option<(String, String)> {
    let address = ntdll(c"wine_get_host_version")?;
    // SAFETY: Wine's `wine_get_host_version` has this signature.
    let host = unsafe { core::mem::transmute::<Proc, WineHostVersion>(address) };
    let (mut sysname, mut release) = (core::ptr::null(), core::ptr::null());
    // SAFETY: two out pointers valid for the call.
    unsafe { host(&raw mut sysname, &raw mut release) };
    // SAFETY: Wine points them at static strings.
    let sysname = unsafe { owned(sysname) }?;
    // SAFETY: as above.
    let release = unsafe { owned(release) }.unwrap_or_default();
    Some((sysname, release))
}

/// The logical CPUs Windows, or Wine, reports to the process.
pub(crate) fn logical_cpus() -> u32 {
    /// Every processor group.
    const ALL_GROUPS: u16 = 0xffff;
    // SAFETY: no arguments but a group number.
    unsafe { GetActiveProcessorCount(ALL_GROUPS) }
}

/// The id of the calling thread.
pub(crate) fn current_thread() -> u32 {
    // SAFETY: no arguments.
    unsafe { GetCurrentThreadId() }
}

/// A `FILETIME` as 100 ns units.
fn ticks(time: FILETIME) -> u64 {
    u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime)
}

/// Creation time, then kernel plus user time, from a times call.
fn times(call: impl FnOnce(&mut [FILETIME; 4]) -> i32) -> Option<(u64, u64)> {
    let empty = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let mut out = [empty; 4];
    (call(&mut out) != 0).then(|| (ticks(out[0]), ticks(out[2]) + ticks(out[3])))
}

/// The CPU time of the whole process, kernel and user, in 100 ns units.
pub(crate) fn process_cpu() -> Option<u64> {
    // SAFETY: a pseudo handle that needs no closing.
    let process = unsafe { GetCurrentProcess() };
    times(|out| {
        let [created, exited, kernel, user] = out;
        // SAFETY: four out pointers valid for the call.
        unsafe { GetProcessTimes(process, created, exited, kernel, user) }
    })
    .map(|(_, cpu)| cpu)
}

/// A thread opened to read its times; closed on drop.
#[derive(Debug)]
pub(crate) struct Thread {
    handle: HANDLE,
    /// The thread's id.
    pub(crate) id: u32,
}

// SAFETY: a thread handle may be used and closed from any thread.
unsafe impl Send for Thread {}

impl Thread {
    /// Opens thread `id` of any process for its times.
    pub(crate) fn open(id: u32) -> Option<Self> {
        // SAFETY: plain arguments; a failure returns null.
        let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, id) };
        (!handle.is_null()).then_some(Self { handle, id })
    }

    /// When the thread started and its CPU time, kernel and user, in 100 ns
    /// units.
    fn times(&self) -> Option<(u64, u64)> {
        times(|out| {
            let [created, exited, kernel, user] = out;
            // SAFETY: an open handle and four out pointers valid for the call.
            unsafe { GetThreadTimes(self.handle, created, exited, kernel, user) }
        })
    }

    /// The thread's CPU time, kernel and user, in 100 ns units.
    pub(crate) fn cpu(&self) -> Option<u64> {
        self.times().map(|(_, cpu)| cpu)
    }

    /// When the thread started, in 100 ns units since 1601.
    pub(crate) fn created(&self) -> Option<u64> {
        self.times().map(|(created, _)| created)
    }

    /// The name the thread gave itself, empty when it has none.
    pub(crate) fn name(&self) -> String {
        let mut text: windows_sys::core::PWSTR = core::ptr::null_mut();
        // SAFETY: an open handle and an out pointer valid for the call.
        let status = unsafe { GetThreadDescription(self.handle, &raw mut text) };
        if status < 0 || text.is_null() {
            return String::new();
        }
        let mut len = 0;
        loop {
            // SAFETY: within the NUL-terminated string Windows returned.
            let at = unsafe { text.add(len) };
            // SAFETY: as above; the NUL ends the loop.
            if unsafe { at.read() } == 0 {
                break;
            }
            len += 1;
        }
        // SAFETY: `len` units read above.
        let name = String::from_utf16_lossy(unsafe { core::slice::from_raw_parts(text, len) });
        // SAFETY: the string Windows allocated for this call, freed once.
        let _freed = unsafe { LocalFree(text.cast()) };
        name
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        // SAFETY: the handle this value opened, closed once.
        let _closed = unsafe { CloseHandle(self.handle) };
    }
}

/// The ids of the process's threads.
pub(crate) fn thread_ids() -> Vec<u32> {
    // SAFETY: plain arguments; a failure returns `INVALID_HANDLE_VALUE`.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
        return Vec::new();
    }
    // SAFETY: no arguments.
    let process = unsafe { GetCurrentProcessId() };
    let mut entry = THREADENTRY32 {
        dwSize: u32::try_from(size_of::<THREADENTRY32>()).unwrap_or(0),
        cntUsage: 0,
        th32ThreadID: 0,
        th32OwnerProcessID: 0,
        tpBasePri: 0,
        tpDeltaPri: 0,
        dwFlags: 0,
    };
    let mut ids = Vec::new();
    // SAFETY: an open snapshot and an entry with its size set.
    let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == process {
            ids.push(entry.th32ThreadID);
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the snapshot opened above, closed once.
    let _closed = unsafe { CloseHandle(snapshot) };
    ids
}

/// The process's first thread still running: the game's main thread.
pub(crate) fn main_thread() -> Option<Thread> {
    thread_ids()
        .into_iter()
        .filter_map(Thread::open)
        .filter_map(|thread| Some((thread.created()?, thread)))
        .min_by_key(|(created, _)| *created)
        .map(|(_, thread)| thread)
}
