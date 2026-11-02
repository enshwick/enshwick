//! Crash blame: when the process dies of a fault nothing handled, the loader
//! names the mod whose code faulted, whose callback the loader was running,
//! or whose code was on the faulting thread's stack, in
//! `logs/enshwick/crash.log`, and the next start shows it in its log and the
//! status summary.
//!
//! Only a fault that ends the process is reported: the loader writes when
//! the process's last-chance filter runs, or when the game's crash handler
//! writes its crash dump, never for a fault the game or a mod handles. A
//! first-chance handler only notes, for each fault, which mod callback the
//! faulting thread was running, so the report can name it from another
//! thread. The report runs in a dying process, so it allocates nothing,
//! formats into a buffer of its own and writes through file handles opened
//! at start, then hands the fault on: to the filter the loader replaced, or
//! to the game's dump writer. The status thread puts the loader's filter
//! back on top whenever another filter took its place.

use std::cell::UnsafeCell;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use enshwick::abi::HookFlags;
use unwind::{rewind, unwind, write};
use windows_sys::Win32::Foundation::{
    EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION, EXCEPTION_STACK_OVERFLOW, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HMODULE, INVALID_HANDLE_VALUE, NTSTATUS, SYSTEMTIME,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_ALWAYS,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
    SetUnhandledExceptionFilter,
};
use windows_sys::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::SystemInformation::GetSystemTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::mod_index::ModIndex;

/// Keeps looking for another handler: the default ends the process.
const CONTINUE_SEARCH: i32 = 0;
/// Frames the filter unwinds looking for a mod's code, and the settle walk
/// for return addresses into a code patch.
pub(crate) const FRAMES: usize = 48;
/// The longest name kept for a range.
const NAME: usize = 64;
/// The crash line's room.
const LINE: usize = 768;

/// A module's address range and whom it belongs to, kept without
/// allocation for the filter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    /// The first byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
    /// The mod's index, [`ModIndex::LOADER`] for the loader.
    pub owner: ModIndex,
    name: [u8; NAME],
    len: usize,
}

impl Range {
    /// A range named `name`, cut at 64 bytes.
    #[must_use]
    pub(crate) fn new(start: usize, end: usize, owner: ModIndex, name: &str) -> Self {
        let mut bytes = [0; NAME];
        let len = name.len().min(NAME);
        bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
        Self {
            start,
            end,
            owner,
            name: bytes,
            len,
        }
    }

    /// The owner's name.
    #[must_use]
    pub(crate) fn name(&self) -> &[u8] {
        &self.name[..self.len]
    }

    fn holds(&self, address: usize) -> bool {
        (self.start..self.end).contains(&address)
    }
}

/// Whom a fault is blamed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blame {
    /// It faulted in a mod's code: the range.
    Code(usize),
    /// The loader was running a mod's callback: the mod's range, if known,
    /// the mod's index and the feature.
    Callback(Option<usize>, usize, &'static str),
    /// A mod's code was on the stack: the range and the frame.
    Stack(usize, usize),
    /// It faulted in the loader, with no mod's callback running or code on
    /// the stack.
    Loader(usize),
    /// No mod's code anywhere near it.
    Game,
}

/// Whom the fault at `at` is blamed on, from the ranges, the callback the
/// loader was running, and the return addresses of the stack, innermost
/// first.
#[must_use]
pub(crate) fn blame(
    ranges: &[Range],
    at: usize,
    callback: Option<(usize, &'static str)>,
    frames: &[usize],
) -> Blame {
    let range_of = |address: usize| ranges.iter().position(|r| r.holds(address));
    let at_range = range_of(at);
    if let Some(index) = at_range
        && ranges[index].owner != ModIndex::LOADER
    {
        return Blame::Code(index);
    }
    if let Some((owner, feature)) = callback {
        return Blame::Callback(
            ranges.iter().position(|r| r.owner == ModIndex::new(owner)),
            owner,
            feature,
        );
    }
    let on_stack = frames.iter().enumerate().find_map(|(frame, &address)| {
        range_of(address)
            .filter(|&i| ranges[i].owner != ModIndex::LOADER)
            .map(|i| (i, frame))
    });
    if let Some((index, frame)) = on_stack {
        return Blame::Stack(index, frame);
    }
    match at_range {
        Some(index) => Blame::Loader(index),
        None => Blame::Game,
    }
}

/// A line built without allocation.
#[derive(Debug)]
pub(crate) struct Line {
    bytes: [u8; LINE],
    len: usize,
}

impl Default for Line {
    fn default() -> Self {
        Self {
            bytes: [0; LINE],
            len: 0,
        }
    }
}

impl Line {
    /// Adds `text`, as much as fits.
    pub(crate) fn text(&mut self, text: &[u8]) -> &mut Self {
        let room = LINE - self.len;
        let n = text.len().min(room);
        self.bytes[self.len..self.len + n].copy_from_slice(&text[..n]);
        self.len += n;
        self
    }

    /// Adds `value` as `0x…` in lower-case hex.
    pub(crate) fn hex(&mut self, value: u64) -> &mut Self {
        let mut digits = [0u8; 18];
        digits[0] = b'0';
        digits[1] = b'x';
        let mut n = 2;
        let mut started = false;
        for shift in (0..16).rev() {
            let nibble = ((value >> (shift * 4)) & 0xf) as u8;
            if nibble != 0 || started || shift == 0 {
                started = true;
                digits[n] = b"0123456789abcdef"[nibble as usize];
                n += 1;
            }
        }
        self.text(&digits[..n])
    }

    /// Adds `value` in decimal, padded with zeros to `width` digits.
    pub(crate) fn dec(&mut self, value: u64, width: usize) -> &mut Self {
        let mut digits = [0u8; 20];
        let mut n = 0;
        let mut rest = value;
        loop {
            digits[n] = b'0' + (rest % 10) as u8;
            n += 1;
            rest /= 10;
            if rest == 0 && n >= width {
                break;
            }
        }
        digits[..n].reverse();
        self.text(&digits[..n])
    }

    /// The line so far.
    #[must_use]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The fault in words, as `crash.log` keeps it: time, code, address, whom it
/// is blamed on and where, and what was accessed.
pub(crate) fn describe(
    line: &mut Line,
    time: [u16; 6],
    code: u32,
    at: usize,
    access: Option<(u64, u64)>,
    blame: Blame,
    ranges: &[Range],
) {
    let [year, month, day, hour, minute, second] = time.map(u64::from);
    line.dec(year, 4)
        .text(b"-")
        .dec(month, 2)
        .text(b"-")
        .dec(day, 2);
    line.text(b" ")
        .dec(hour, 2)
        .text(b":")
        .dec(minute, 2)
        .text(b":")
        .dec(second, 2);
    line.text(b"Z fault ")
        .hex(u64::from(code))
        .text(b" at ")
        .hex(at as u64);
    let offset = |line: &mut Line, range: &Range, address: usize| {
        line.text(b" (+")
            .hex(address.wrapping_sub(range.start) as u64)
            .text(b" in ")
            .text(range.name())
            .text(b")");
    };
    match blame {
        Blame::Code(index) => {
            let range = &ranges[index];
            line.text(b", blamed on ")
                .text(range.name())
                .text(b": in its code");
            offset(line, range, at);
        }
        Blame::Callback(range, owner, feature) => {
            line.text(b", blamed on ");
            match range.map(|i| &ranges[i]) {
                Some(range) => line.text(range.name()),
                None => line.text(b"mod #").dec(owner as u64, 1),
            };
            line.text(b": in its ")
                .text(feature.as_bytes())
                .text(b" callback");
            if let Some(range) = ranges.iter().find(|r| r.holds(at)) {
                offset(line, range, at);
            }
        }
        Blame::Stack(index, frame) => {
            let range = &ranges[index];
            line.text(b", blamed on ")
                .text(range.name())
                .text(b": its code is on the stack, frame ")
                .dec(frame as u64, 1);
        }
        Blame::Loader(index) => {
            line.text(b", in the loader");
            offset(line, &ranges[index], at);
        }
        Blame::Game => {
            line.text(b", in the game: no mod's code on the stack");
        }
    }
    if let Some((kind, address)) = access {
        let verb: &[u8] = match kind {
            0 => b"reading",
            1 => b"writing",
            8 => b"running",
            _ => b"touching",
        };
        line.text(b"; ").text(verb).text(b" ").hex(address);
    }
    line.text(b"\r\n");
}

/// What the filter needs, set up at start.
struct State {
    ranges: Box<[Range]>,
    /// `crash.log`, opened for appending.
    log: usize,
    /// `mod-data/enshwick/last-crash.txt`, rewritten with the last crash.
    last: usize,
}

static STATE: OnceLock<State> = OnceLock::new();
/// The last crash of the session before, read at start.
static LAST: OnceLock<String> = OnceLock::new();
/// The filter the loader's replaced, called after it.
static NEXT: AtomicUsize = AtomicUsize::new(0);
/// Set while the filter runs: a fault inside it goes to the next filter.
static INSIDE: AtomicBool = AtomicBool::new(false);
/// The thread the filter handed on to the next filter, 0 before: a filter
/// set after the loader's chains back to it, which must not call that one
/// again.
static HANDED_ON: AtomicU32 = AtomicU32::new(0);

/// The filter's line, used by one thread at a time under [`INSIDE`].
struct Shared(UnsafeCell<Line>);

// SAFETY: `report` touches the line only after its `INSIDE.swap(true)`
// returned false, which one thread in the process ever sees, so the line is
// never shared.
unsafe impl Sync for Shared {}

static SHARED: Shared = Shared(UnsafeCell::new(Line {
    bytes: [0; LINE],
    len: 0,
}));

/// What the first-chance handler noted of a fault: the thread, the address
/// and the mod callback the thread ran, as [`crate::profile::marker`] gives
/// it.
struct Noted {
    thread: AtomicU32,
    address: AtomicUsize,
    marker: AtomicU32,
}

/// The latest faults the first-chance handler saw, oldest overwritten.
static NOTED: [Noted; 8] = [const {
    Noted {
        thread: AtomicU32::new(0),
        address: AtomicUsize::new(0),
        marker: AtomicU32::new(0),
    }
}; 8];
static NEXT_NOTE: AtomicUsize = AtomicUsize::new(0);

/// The callback the thread `thread` ran when it faulted at `at`: read here
/// when this is the thread, else from what the first-chance handler noted.
fn noted(thread: u32, at: usize) -> Option<(usize, crate::profile::Feature)> {
    // SAFETY: `GetCurrentThreadId` takes no arguments and has no
    // preconditions.
    if thread == unsafe { GetCurrentThreadId() } {
        return crate::profile::current_callback();
    }
    NOTED
        .iter()
        .find(|n| {
            n.thread.load(Ordering::Acquire) == thread && n.address.load(Ordering::Acquire) == at
        })
        .and_then(|n| crate::profile::decode(n.marker.load(Ordering::Acquire)))
}

/// Notes the faulting thread's callback, for a report from another thread;
/// handles nothing.
unsafe extern "system" fn first_chance(pointers: *mut EXCEPTION_POINTERS) -> i32 {
    // SAFETY: Windows passes a vectored handler null or the fault's
    // `EXCEPTION_POINTERS`, valid while the handler runs.
    let record = unsafe { pointers.as_ref() }.and_then(|p| {
        // SAFETY: the fault's `EXCEPTION_POINTERS` hold null or its
        // `EXCEPTION_RECORD`, valid while the handler runs.
        unsafe { p.ExceptionRecord.as_ref() }
    });
    if let Some(record) = record
        && FATAL.contains(&record.ExceptionCode)
    {
        let slot = &NOTED[NEXT_NOTE.fetch_add(1, Ordering::Relaxed) % NOTED.len()];
        slot.marker
            .store(crate::profile::marker(), Ordering::Relaxed);
        slot.address
            .store(record.ExceptionAddress.addr(), Ordering::Relaxed);
        // SAFETY: `GetCurrentThreadId` takes no arguments and has no
        // preconditions.
        slot.thread
            .store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    }
    CONTINUE_SEARCH
}

/// The faults a report is written for when nothing handles them.
const FATAL: [NTSTATUS; 5] = [
    EXCEPTION_ACCESS_VIOLATION,
    EXCEPTION_ILLEGAL_INSTRUCTION,
    EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION,
    EXCEPTION_STACK_OVERFLOW,
];

/// The exception parameter of `MiniDumpWriteDump`, packed to 4 as in
/// `dbghelp.h`.
#[repr(C, packed(4))]
struct DumpException {
    thread: u32,
    pointers: *const EXCEPTION_POINTERS,
    client_pointers: i32,
}

type DumpFn = unsafe extern "system" fn(
    process: HANDLE,
    process_id: u32,
    file: HANDLE,
    kind: u32,
    exception: *const DumpException,
    streams: *const core::ffi::c_void,
    callback: *const core::ffi::c_void,
) -> i32;

// SAFETY: `DumpFn` is the exact signature of `dbghelp.MiniDumpWriteDump`, and
// the slot is used only as the `next` of the `dump` link on it.
static DUMP_NEXT: Next<DumpFn> = unsafe { Next::new() };

/// Whether a dump written for a fault coded `code` is of a crash: only the
/// faults [`FATAL`] names are, never the breakpoints, C++ exceptions or
/// reports a handler writes a dump for and survives.
fn fatal(code: NTSTATUS) -> bool {
    FATAL.contains(&code)
}

/// The code of the fault `pointers` names, `None` when either pointer is
/// null.
///
/// # Safety
///
/// - `pointers` is null or points at a fault's `EXCEPTION_POINTERS` in this
///   process, whose `ExceptionRecord` is null or valid.
/// - The faulting thread waits in the fault during the call.
unsafe fn fault_code(pointers: *const EXCEPTION_POINTERS) -> Option<NTSTATUS> {
    // SAFETY: the caller's contract makes `pointers` null or the fault's
    // valid `EXCEPTION_POINTERS`.
    let pointers = unsafe { pointers.as_ref() }?;
    // SAFETY: the caller's contract makes the record null or valid while the
    // faulting thread waits in the fault.
    let record = unsafe { pointers.ExceptionRecord.as_ref() }?;
    Some(record.ExceptionCode)
}

/// The game's crash handler writes a dump: report the fault first when it
/// is one that ends the process.
unsafe extern "system" fn dump(
    process: HANDLE,
    process_id: u32,
    file: HANDLE,
    kind: u32,
    exception: *const DumpException,
    streams: *const core::ffi::c_void,
    callback: *const core::ffi::c_void,
) -> i32 {
    // Pointers "in the client" are this process's own when the dump is of
    // this process, as the game's handler writes it.
    // SAFETY: `GetCurrentProcessId` takes no arguments and has no
    // preconditions.
    let ours = process_id == unsafe { GetCurrentProcessId() };
    // SAFETY: the caller's `MINIDUMP_EXCEPTION_INFORMATION`, null for a dump
    // of no fault, valid for the call.
    if let Some(exception) = unsafe { exception.as_ref() }
        && (exception.client_pointers == 0 || ours)
        // SAFETY: the pointers are this process's own (`client_pointers` is 0,
        // or the dump is of this process, checked first) and name the fault
        // whose thread waits in it while its handler writes the dump.
        && unsafe { fault_code(exception.pointers) }.is_some_and(fatal)
    {
        let (pointers, thread) = (exception.pointers, exception.thread);
        // SAFETY: the fault's pointers in this process, of the thread the
        // dump names, which waits in the fault while its handler writes the
        // dump.
        unsafe { report(pointers, thread) };
    }
    // SAFETY: the caller's own arguments, passed on unchanged to the rest of
    // the chain, which has `MiniDumpWriteDump`'s signature.
    unsafe {
        DUMP_NEXT.get()(
            process, process_id, file, kind, exception, streams, callback,
        )
    }
}

/// Links the report on the game's crash dumps: `MiniDumpWriteDump` of the
/// system's `dbghelp.dll`, which a `dbghelp.dll` proxy in the game's folder
/// forwards to.
pub(crate) fn link(loader: &Loader) {
    let found =
        crate::module::Module::system("dbghelp.dll").and_then(|m| m.export("MiniDumpWriteDump"));
    let target = Target::Found(
        "dbghelp.MiniDumpWriteDump",
        found.map(|f| f as *const () as u64),
    );
    loader.link_feature(
        "crash blame on crash dumps (MiniDumpWriteDump)",
        target,
        &DUMP_NEXT,
        dump,
        HookFlags::NONE,
    );
}

/// The address range of a loaded module, `None` when Windows does not say.
pub(crate) fn range_of(module: HMODULE) -> Option<(usize, usize)> {
    let mut info = MODULEINFO {
        lpBaseOfDll: core::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: core::ptr::null_mut(),
    };
    // SAFETY: `GetCurrentProcess` takes no arguments and has no
    // preconditions; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: `process` is this process's pseudo handle, `module` the
    // caller's loaded module, and `info` a writable `MODULEINFO` of the size
    // passed.
    let ok = unsafe {
        GetModuleInformation(
            process,
            module,
            &raw mut info,
            u32::try_from(size_of::<MODULEINFO>()).unwrap_or(0),
        )
    };
    let start = info.lpBaseOfDll.addr();
    (ok != 0).then(|| (start, start + info.SizeOfImage as usize))
}

/// Opens `path` for the filter, `access` as given; 0 when it cannot.
fn open(path: &Path, access: u32) -> usize {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path alive for the call, and
    // the other arguments are plain values or null; the handle stays open for
    // the process.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            core::ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            core::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        0
    } else {
        handle.expose_provenance()
    }
}

/// The last crash of the session before, if it crashed.
pub(crate) fn last() -> Option<&'static str> {
    LAST.get().map(String::as_str)
}

/// Starts blaming crashes on `modules`, `(index, id, start, end)`, writing
/// `crash.log` to `logs_dir` and the last crash to `mod-data/enshwick` in
/// `data_dir`; reads the last session's crash first.
pub(crate) fn install(modules: &[(usize, String, usize, usize)], logs_dir: &Path, data_dir: &Path) {
    let own = data_dir.join(crate::settings::LOADER_ID);
    let _made = std::fs::create_dir_all(&own);
    let last_path = own.join("last-crash.txt");
    if let Ok(text) = std::fs::read_to_string(&last_path)
        && !text.trim().is_empty()
    {
        let text = text.trim().to_owned();
        crate::log!(WARN, "the last session crashed: {text}");
        let _first = LAST.set(text);
    }
    let _cleared = std::fs::write(&last_path, "");
    let ranges = modules
        .iter()
        .map(|(owner, id, start, end)| Range::new(*start, *end, ModIndex::new(*owner), id))
        .collect();
    let state = State {
        ranges,
        log: open(&logs_dir.join("crash.log"), FILE_APPEND_DATA),
        last: open(&last_path, GENERIC_READ | GENERIC_WRITE),
    };
    if STATE.set(state).is_err() {
        return;
    }
    // SAFETY: `first_chance` has the `PVECTORED_EXCEPTION_HANDLER` signature,
    // lives for the process and never unwinds.
    if unsafe { AddVectoredExceptionHandler(1, Some(first_chance)) }.is_null() {
        crate::log!(
            WARN,
            "crash blame names no callbacks: no first-chance handler"
        );
    }
    reassert();
}

/// Puts the loader's filter back on top when another took its place, and
/// calls that one after it.
pub(crate) fn reassert() {
    if STATE.get().is_none() {
        return;
    }
    // SAFETY: `filter` has the `LPTOP_LEVEL_EXCEPTION_FILTER` signature,
    // lives for the process and never unwinds.
    let previous = unsafe { SetUnhandledExceptionFilter(Some(filter)) };
    let ours = filter as *const () as usize;
    if let Some(previous) = previous
        && previous as *const () as usize != ours
    {
        let next = previous as *const () as usize;
        if NEXT.swap(next, Ordering::AcqRel) != next {
            crate::log!(
                INFO,
                "crash blame runs before the crash filter at {next:#x}, which another module set"
            );
        }
    }
}

/// Writes the report of the fault `pointers` names, once per process.
///
/// # Safety
///
/// - `pointers` is null or points at the fault's valid
///   `EXCEPTION_POINTERS` in this process, whose records are null or valid.
/// - The thread `thread` is stopped in the fault or is the one running this.
unsafe fn report(pointers: *const EXCEPTION_POINTERS, thread: u32) {
    if INSIDE.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(state) = STATE.get() else {
        return;
    };
    // SAFETY: the caller's contract makes `pointers` null or the fault's
    // valid `EXCEPTION_POINTERS`.
    let Some(pointers) = (unsafe { pointers.as_ref() }) else {
        return;
    };
    // SAFETY: this thread's `INSIDE.swap(true)` returned false, which one
    // thread in the process ever sees, so nothing else borrows the line.
    let line = unsafe { &mut *SHARED.0.get() };
    line.len = 0;
    // SAFETY: the caller's contract, passed on: the fault's valid pointers,
    // and `thread` stopped in the fault or running this.
    unsafe { record(state, pointers, thread, line) };
    write(state.log, line.as_bytes());
    rewind(state.last);
    write(state.last, line.as_bytes());
}

unsafe extern "system" fn filter(pointers: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: `GetCurrentThreadId` takes no arguments and has no
    // preconditions.
    let thread = unsafe { GetCurrentThreadId() };
    if HANDED_ON.load(Ordering::Acquire) == thread {
        return CONTINUE_SEARCH;
    }
    // SAFETY: Windows calls an unhandled exception filter on the faulting
    // thread with null or the fault's valid `EXCEPTION_POINTERS`.
    unsafe { report(pointers, thread) };
    HANDED_ON.store(thread, Ordering::Release);
    let next = NEXT.load(Ordering::Acquire);
    if next == 0 {
        return CONTINUE_SEARCH;
    }
    // SAFETY: the filter the process had before the loader's, as Windows
    // handed it back; it takes the same pointers.
    let next: LPTOP_LEVEL_EXCEPTION_FILTER = unsafe { core::mem::transmute::<usize, _>(next) };
    match next {
        // SAFETY: `next` is the filter the process had before the loader's,
        // as `SetUnhandledExceptionFilter` handed it back, called on the
        // faulting thread with the fault's own pointers.
        Some(next) => unsafe { next(pointers) },
        None => CONTINUE_SEARCH,
    }
}

/// Describes the fault `pointers` names into `line`.
///
/// # Safety
///
/// - `pointers` are the fault's, with records null or valid for the call.
/// - The thread `thread` is stopped in the fault or is the one running this.
unsafe fn record(state: &State, pointers: &EXCEPTION_POINTERS, thread: u32, line: &mut Line) {
    // SAFETY: the caller's contract makes both records null or valid for the
    // call.
    let (Some(record), Some(context)) = (unsafe { pointers.ExceptionRecord.as_ref() }, unsafe {
        pointers.ContextRecord.as_ref()
    }) else {
        return;
    };
    let code: NTSTATUS = record.ExceptionCode;
    let at = record.ExceptionAddress.addr();
    let mut frames = [0usize; FRAMES];
    // A stack overflow leaves no room to unwind.
    let found = if code == EXCEPTION_STACK_OVERFLOW {
        0
    } else {
        // SAFETY: `context` is the faulting thread's, which by the caller's
        // contract is stopped in the fault or runs this.
        unsafe { unwind(context, &mut frames).0 }
    };
    let callback = noted(thread, at).map(|(owner, f)| (owner, f.name()));
    let blame = blame(&state.ranges, at, callback, &frames[..found]);
    let access = (code == EXCEPTION_ACCESS_VIOLATION && record.NumberParameters >= 2).then(|| {
        (
            record.ExceptionInformation[0] as u64,
            record.ExceptionInformation[1] as u64,
        )
    });
    let mut time = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: `time` is a writable `SYSTEMTIME`.
    unsafe { GetSystemTime(&raw mut time) };
    describe(
        line,
        [
            time.wYear,
            time.wMonth,
            time.wDay,
            time.wHour,
            time.wMinute,
            time.wSecond,
        ],
        code.cast_unsigned(),
        at,
        access,
        blame,
        &state.ranges,
    );
}

/// The return addresses the crash walk finds on the calling thread's stack,
/// innermost first, for tests of code it must pass.
#[cfg(test)]
#[inline(never)]
pub(crate) fn walk_here() -> Vec<usize> {
    // SAFETY: an all-zero context is a valid value to capture into.
    let mut aligned = crate::detour::ThreadContext(unsafe { core::mem::zeroed() });
    // SAFETY: a writable context on its 16-byte boundary.
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureContext(&raw mut aligned.0);
    }
    let mut frames = [0usize; FRAMES];
    // SAFETY: this thread's own context, its stack unchanged during the walk.
    let (found, _) = unsafe { unwind(&aligned.0, &mut frames) };
    frames[..found].to_vec()
}

#[cfg(test)]
mod tests;
pub(crate) mod unwind;
