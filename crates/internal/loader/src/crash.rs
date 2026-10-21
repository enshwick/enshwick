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

use enshwick_loader_api::abi::HookFlags;

use windows_sys::Win32::Foundation::{
    EXCEPTION_ACCESS_VIOLATION, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
    EXCEPTION_PRIV_INSTRUCTION, EXCEPTION_STACK_OVERFLOW, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HMODULE, INVALID_HANDLE_VALUE, NTSTATUS, SYSTEMTIME,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_APPEND_DATA, FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FlushFileBuffers, OPEN_ALWAYS, SetEndOfFile, SetFilePointerEx, WriteFile,
};
use windows_sys::Win32::System::Diagnostics::Debug::{
    AddVectoredExceptionHandler, CONTEXT, EXCEPTION_POINTERS, IMAGE_RUNTIME_FUNCTION_ENTRY,
    LPTOP_LEVEL_EXCEPTION_FILTER, RtlLookupFunctionEntry, RtlVirtualUnwind,
    SetUnhandledExceptionFilter, UNW_FLAG_NHANDLER,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS, PAGE_READONLY, PAGE_READWRITE,
    PAGE_WRITECOPY, VirtualQuery,
};
use windows_sys::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::SystemInformation::GetSystemTime;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
};

use crate::chains::Next;
use crate::host::Loader;

/// Keeps looking for another handler: the default ends the process.
const CONTINUE_SEARCH: i32 = 0;
/// Frames the filter unwinds looking for a mod's code.
const FRAMES: usize = 48;
/// The longest name kept for a range.
const NAME: usize = 64;
/// The crash line's room.
const LINE: usize = 768;

/// The owner of the loader's own range.
pub(crate) const LOADER: usize = usize::MAX;

/// A module's address range and whom it belongs to, kept without
/// allocation for the filter.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Range {
    /// The first byte.
    pub start: usize,
    /// One past the last byte.
    pub end: usize,
    /// The mod's index, [`LOADER`] for the loader.
    pub owner: usize,
    name: [u8; NAME],
    len: usize,
}

impl Range {
    /// A range named `name`, cut at 64 bytes.
    #[must_use]
    pub(crate) fn new(start: usize, end: usize, owner: usize, name: &str) -> Self {
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
        && ranges[index].owner != LOADER
    {
        return Blame::Code(index);
    }
    if let Some((owner, feature)) = callback {
        return Blame::Callback(ranges.iter().position(|r| r.owner == owner), owner, feature);
    }
    let on_stack = frames.iter().enumerate().find_map(|(frame, &address)| {
        range_of(address)
            .filter(|&i| ranges[i].owner != LOADER)
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

/// The filter's line, used by one thread at a time under [`INSIDE`].
struct Shared(UnsafeCell<Line>);

// SAFETY: only the thread that set `INSIDE` touches the line.
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
    // SAFETY: no preconditions.
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
    // SAFETY: Windows passes the fault's pointers.
    let record = unsafe { pointers.as_ref() }.and_then(|p| {
        // SAFETY: as above.
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
        // SAFETY: no preconditions.
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

// SAFETY: joins the chain of `MiniDumpWriteDump`, whose type it names.
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
/// `pointers` is null or points at the fault's pointers in this process.
unsafe fn fault_code(pointers: *const EXCEPTION_POINTERS) -> Option<NTSTATUS> {
    // SAFETY: as the caller guarantees.
    let pointers = unsafe { pointers.as_ref() }?;
    // SAFETY: the record of the fault, valid while its thread waits in it.
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
    // SAFETY: no preconditions.
    let ours = process_id == unsafe { GetCurrentProcessId() };
    // SAFETY: the caller's parameter, null for a dump of no fault.
    if let Some(exception) = unsafe { exception.as_ref() }
        && (exception.client_pointers == 0 || ours)
        // SAFETY: pointers into this process, as checked above.
        && unsafe { fault_code(exception.pointers) }.is_some_and(fatal)
    {
        let (pointers, thread) = (exception.pointers, exception.thread);
        // SAFETY: pointers into this process, of the thread the dump names,
        // which waits in the fault while its handler writes the dump.
        unsafe { report(pointers, thread) };
    }
    // SAFETY: the rest of the chain takes the caller's arguments.
    unsafe {
        DUMP_NEXT.get()(
            process, process_id, file, kind, exception, streams, callback,
        )
    }
}

/// Links the report on the game's crash dumps: `MiniDumpWriteDump` of
/// `dbghelp.dll`, loaded now if the game has not yet.
pub(crate) fn link(loader: &Loader) {
    let name: Vec<u16> = "dbghelp.dll".encode_utf16().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated name; the module stays loaded for the process.
    let module = unsafe { LoadLibraryW(name.as_ptr()) };
    let found = if module.is_null() {
        None
    } else {
        // SAFETY: a module handle and a NUL-terminated export name.
        unsafe { GetProcAddress(module, c"MiniDumpWriteDump".as_ptr().cast()) }
    };
    let target = found.map(|f| f as *const () as u64);
    if let Some(address) = target {
        crate::diag::name(address, "dbghelp.MiniDumpWriteDump");
    }
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
    // SAFETY: no preconditions; the pseudo handle needs no closing.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: a loaded module and a writable struct of the given size.
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
    // SAFETY: a NUL-terminated path; the handle stays open for the process.
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
        .map(|(owner, id, start, end)| Range::new(*start, *end, *owner, id))
        .collect();
    let state = State {
        ranges,
        log: open(&logs_dir.join("crash.log"), FILE_APPEND_DATA),
        last: open(&last_path, GENERIC_READ | GENERIC_WRITE),
    };
    if STATE.set(state).is_err() {
        return;
    }
    // SAFETY: the handler is a plain function that never unwinds.
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
    // SAFETY: the filter is a plain function that never unwinds.
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

/// The protections whose pages read without a fault.
const READABLE: u32 = PAGE_READONLY
    | PAGE_READWRITE
    | PAGE_WRITECOPY
    | PAGE_EXECUTE_READ
    | PAGE_EXECUTE_READWRITE
    | PAGE_EXECUTE_WRITECOPY;

/// The end of the readable run `address` lies in, from the region
/// `VirtualQuery` reports for it; `None` when its page does not read.
fn readable_end(
    base: usize,
    size: usize,
    state: u32,
    protect: u32,
    address: usize,
) -> Option<usize> {
    let end = base.checked_add(size)?;
    (state == MEM_COMMIT
        && protect & READABLE != 0
        && protect & (PAGE_GUARD | PAGE_NOACCESS) == 0
        && (base..end).contains(&address))
    .then_some(end)
}

/// The first byte and the end of the readable region `address` lies in.
fn readable(address: usize) -> Option<(usize, usize)> {
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
    // SAFETY: querying any address is allowed; the struct is writable.
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
    readable_end(base, info.RegionSize, info.State, info.Protect, address).map(|end| (base, end))
}

/// The word at `address`, `None` unless all its bytes read without a fault.
fn read_word(address: usize) -> Option<u64> {
    let (_, end) = readable(address)?;
    if address.checked_add(8)? > end {
        return None;
    }
    // SAFETY: the eight bytes lie in one committed, readable region.
    Some(unsafe { core::ptr::with_exposed_provenance::<u64>(address).read_unaligned() })
}

/// What unwinding one function's frame reads, from its `UNWIND_INFO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Reach {
    /// The frame register's number and its offset below the frame's base.
    frame: Option<(u8, usize)>,
    /// The bytes above the frame's base the unwind reads, the return
    /// address included.
    bytes: usize,
    /// The `UNWIND_INFO` the frame's unwinding continues in.
    chained: Option<u32>,
}

/// The longest `UNWIND_INFO`: the header, 256 code slots and a chained
/// entry.
const UNWIND_INFO: usize = 4 + 256 * 2 + 12;

/// What unwinding reads for the `UNWIND_INFO` in `info`; `None` for a
/// version or code this does not know, or info cut short.
fn reach(info: &[u8]) -> Option<Reach> {
    let (&head, &count, &frame) = (info.first()?, info.get(2)?, info.get(3)?);
    if !matches!(head & 7, 1 | 2) {
        return None;
    }
    let count = usize::from(count);
    let codes = info.get(4..4 + count * 2)?;
    let slot = |i: usize| -> Option<usize> {
        let bytes = codes.get(i * 2..i * 2 + 2)?;
        Some(usize::from(u16::from_le_bytes([bytes[0], bytes[1]])))
    };
    let wide = |i: usize| -> Option<usize> { Some(slot(i)? | (slot(i + 1)? << 16)) };
    let (mut size, mut saves, mut i) = (0usize, 0usize, 0usize);
    while i < count {
        let byte = codes[i * 2 + 1];
        let op_info = usize::from(byte >> 4);
        match byte & 0xf {
            0 => (size, i) = (size + 8, i + 1),
            1 if op_info == 0 => (size, i) = (size + slot(i + 1)? * 8, i + 2),
            1 => (size, i) = (size + wide(i + 1)?, i + 3),
            2 => (size, i) = (size + op_info * 8 + 8, i + 1),
            3 => i += 1,
            4 => (saves, i) = (saves.max(slot(i + 1)? * 8 + 8), i + 2),
            5 => (saves, i) = (saves.max(wide(i + 1)? + 8), i + 3),
            6 => i += 2,
            8 => (saves, i) = (saves.max(slot(i + 1)? * 16 + 16), i + 2),
            9 => (saves, i) = (saves.max(wide(i + 1)? + 16), i + 3),
            10 => (size, i) = (size + if op_info == 0 { 40 } else { 48 }, i + 1),
            _ => return None,
        }
    }
    let chained = if (head >> 3) & 4 == 0 {
        None
    } else {
        let at = 4 + count.next_multiple_of(2) * 2 + 8;
        let bytes = info.get(at..at + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    };
    let register = frame & 0xf;
    Some(Reach {
        frame: (register != 0).then_some((register, usize::from(frame >> 4) * 16)),
        bytes: (size + 8).max(saves),
        chained,
    })
}

/// What unwinding the function of `entry` in the image at `image` reads,
/// its chained entries included.
///
/// # Safety
///
/// `entry` is the function entry Windows found in the loaded image at
/// `image`.
unsafe fn reach_of(image: usize, entry: *const IMAGE_RUNTIME_FUNCTION_ENTRY) -> Option<Reach> {
    // SAFETY: as the caller guarantees.
    let anonymous = unsafe { (*entry).Anonymous };
    // SAFETY: every field of the union is the same `u32`.
    let mut rva = unsafe { anonymous.UnwindData };
    let mut total: Option<Reach> = None;
    for _ in 0..32 {
        let at = image.checked_add(rva as usize)?;
        let (_, end) = readable(at)?;
        // SAFETY: the bytes up to `end` lie in one committed, readable
        // region of the image.
        let info = unsafe {
            core::slice::from_raw_parts(
                core::ptr::with_exposed_provenance::<u8>(at),
                (end - at).min(UNWIND_INFO),
            )
        };
        let step = reach(info)?;
        let merged = match total {
            None => step,
            Some(first) => Reach {
                bytes: first.bytes + step.bytes - 8,
                chained: step.chained,
                ..first
            },
        };
        match merged.chained {
            Some(next) => (total, rva) = (Some(merged), next),
            None => return Some(merged),
        }
    }
    None
}

/// The value of the register numbered `number` as unwind codes number
/// them.
fn register(context: &CONTEXT, number: u8) -> Option<u64> {
    Some(match number {
        0 => context.Rax,
        1 => context.Rcx,
        2 => context.Rdx,
        3 => context.Rbx,
        4 => context.Rsp,
        5 => context.Rbp,
        6 => context.Rsi,
        7 => context.Rdi,
        8 => context.R8,
        9 => context.R9,
        10 => context.R10,
        11 => context.R11,
        12 => context.R12,
        13 => context.R13,
        14 => context.R14,
        15 => context.R15,
        _ => return None,
    })
}

/// Whether unwinding a frame that reads `reach` from the stack pointer
/// `rsp`, and from the frame register's base when it has one, stays in the
/// readable stack `low..end`.
fn fits(reach: &Reach, rsp: usize, frame: Option<usize>, low: usize, end: usize) -> bool {
    let within = |base: usize| {
        base >= low
            && base
                .checked_add(reach.bytes)
                .is_some_and(|last| last <= end)
    };
    within(rsp) && frame.is_none_or(within)
}

/// The return addresses of the faulting thread's stack, innermost first,
/// unwound from `context`; how many were found. Every read stays in the
/// committed, readable region of the stack, so a broken stack ends the
/// walk instead of faulting, on whichever thread reports.
///
/// # Safety
///
/// `context` is the context of a thread stopped in the fault or running
/// this.
unsafe fn unwind(context: &CONTEXT, frames: &mut [usize; FRAMES]) -> usize {
    let mut aligned = Aligned(*context);
    let context = &mut aligned.0;
    let mut found = 0;
    while found < FRAMES {
        let rip = context.Rip;
        let Ok(rsp) = usize::try_from(context.Rsp) else {
            break;
        };
        if rip == 0 {
            break;
        }
        let Some((low, end)) = readable(rsp) else {
            break;
        };
        let mut image = 0u64;
        // SAFETY: no preconditions beyond a readable out pointer.
        let entry = unsafe { RtlLookupFunctionEntry(rip, &raw mut image, core::ptr::null_mut()) };
        if entry.is_null() {
            // A leaf function: its return address is on top of the stack.
            let Some(back) = read_word(rsp) else {
                break;
            };
            context.Rip = back;
            context.Rsp += 8;
        } else {
            let Ok(base) = usize::try_from(image) else {
                break;
            };
            // SAFETY: the function entry Windows found in the image at
            // `base`.
            let Some(reach) = (unsafe { reach_of(base, entry) }) else {
                break;
            };
            let frame = match reach.frame {
                None => None,
                Some((number, offset)) => {
                    let value = register(context, number)
                        .and_then(|value| usize::try_from(value).ok())
                        .and_then(|value| value.checked_sub(offset));
                    if value.is_none() {
                        break;
                    }
                    value
                }
            };
            if !fits(&reach, rsp, frame, low, end) {
                break;
            }
            let (mut handler, mut establisher) = (core::ptr::null_mut(), 0u64);
            // SAFETY: the function entry Windows found for `rip`, and a
            // context whose stack holds every byte the entry's unwind
            // codes read, as checked above.
            unsafe {
                RtlVirtualUnwind(
                    UNW_FLAG_NHANDLER,
                    image,
                    rip,
                    entry,
                    &raw mut *context,
                    &raw mut handler,
                    &raw mut establisher,
                    core::ptr::null_mut(),
                );
            }
            if context.Rsp <= context_rsp(rsp) || context.Rsp > context_rsp(end) {
                break;
            }
        }
        if context.Rip == rip || context.Rip == 0 {
            break;
        }
        frames[found] = usize::try_from(context.Rip).unwrap_or(0);
        found += 1;
    }
    found
}

/// A `CONTEXT` on the 16-byte boundary Windows writes it with aligned
/// stores; the windows-sys type does not ask for it.
#[repr(C, align(16))]
struct Aligned(CONTEXT);

/// `address` as a context register holds it.
fn context_rsp(address: usize) -> u64 {
    u64::try_from(address).unwrap_or(u64::MAX)
}

/// Writes `bytes` to the open file `handle`.
fn write(handle: usize, bytes: &[u8]) {
    if handle == 0 {
        return;
    }
    let handle: HANDLE = core::ptr::with_exposed_provenance_mut(handle);
    let mut written = 0u32;
    // SAFETY: a handle opened at start for writing, and a readable buffer.
    unsafe {
        WriteFile(
            handle,
            bytes.as_ptr(),
            u32::try_from(bytes.len()).unwrap_or(0),
            &raw mut written,
            core::ptr::null_mut(),
        );
    }
    // SAFETY: as above.
    unsafe { FlushFileBuffers(handle) };
}

/// Empties the open file `handle` from the start.
fn rewind(handle: usize) {
    if handle == 0 {
        return;
    }
    let handle: HANDLE = core::ptr::with_exposed_provenance_mut(handle);
    // SAFETY: a handle opened at start for writing.
    unsafe { SetFilePointerEx(handle, 0, core::ptr::null_mut(), FILE_BEGIN) };
    // SAFETY: as above.
    unsafe { SetEndOfFile(handle) };
}

/// Writes the report of the fault `pointers` names, once per process.
///
/// # Safety
///
/// `pointers` is null or the fault's, its thread `thread` stopped in the
/// fault or running this.
unsafe fn report(pointers: *const EXCEPTION_POINTERS, thread: u32) {
    if INSIDE.swap(true, Ordering::AcqRel) {
        return;
    }
    let Some(state) = STATE.get() else {
        return;
    };
    // SAFETY: as the caller guarantees.
    let Some(pointers) = (unsafe { pointers.as_ref() }) else {
        return;
    };
    // SAFETY: `INSIDE` gives this thread the line alone, for good.
    let line = unsafe { &mut *SHARED.0.get() };
    line.len = 0;
    // SAFETY: as the caller guarantees.
    unsafe { record(state, pointers, thread, line) };
    write(state.log, line.as_bytes());
    rewind(state.last);
    write(state.last, line.as_bytes());
}

unsafe extern "system" fn filter(pointers: *const EXCEPTION_POINTERS) -> i32 {
    // SAFETY: no preconditions.
    let thread = unsafe { GetCurrentThreadId() };
    // SAFETY: Windows passes the fault's pointers on the faulting thread.
    unsafe { report(pointers, thread) };
    let next = NEXT.load(Ordering::Acquire);
    if next == 0 {
        return CONTINUE_SEARCH;
    }
    // SAFETY: the filter the process had before the loader's, as Windows
    // handed it back; it takes the same pointers.
    let next: LPTOP_LEVEL_EXCEPTION_FILTER = unsafe { core::mem::transmute::<usize, _>(next) };
    match next {
        // SAFETY: as above.
        Some(next) => unsafe { next(pointers) },
        None => CONTINUE_SEARCH,
    }
}

/// Describes the fault `pointers` names into `line`.
///
/// # Safety
///
/// `pointers` are the fault's, its thread `thread` stopped in the fault or
/// running this.
unsafe fn record(state: &State, pointers: &EXCEPTION_POINTERS, thread: u32, line: &mut Line) {
    // SAFETY: the fault's records, valid for the call.
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
        // SAFETY: the faulting thread's context.
        unsafe { unwind(context, &mut frames) }
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
    // SAFETY: a writable struct.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges() -> [Range; 3] {
        [
            Range::new(0x1000, 0x2000, 0, "safes"),
            Range::new(0x5000, 0x6000, 1, "survivors"),
            Range::new(0x9000, 0xa000, LOADER, "enshwick"),
        ]
    }

    fn text(blame: Blame, at: usize, access: Option<(u64, u64)>) -> String {
        let mut line = Line::default();
        describe(
            &mut line,
            [2026, 10, 21, 9, 5, 7],
            0xc000_0005,
            at,
            access,
            blame,
            &ranges(),
        );
        String::from_utf8(line.as_bytes().to_vec()).unwrap()
    }

    #[test]
    fn a_fault_in_a_mods_code_is_its_own() {
        let r = ranges();
        assert_eq!(
            blame(&r, 0x5800, Some((0, "events")), &[0x1100]),
            Blame::Code(1)
        );
        assert_eq!(
            text(Blame::Code(1), 0x5800, Some((0, 0x10))),
            "2026-10-21 09:05:07Z fault 0xc0000005 at 0x5800, blamed on survivors: in its code (+0x800 in survivors); reading 0x10\r\n"
        );
    }

    #[test]
    fn a_fault_during_a_callback_is_the_callbacks_mod() {
        let r = ranges();
        let blamed = blame(&r, 0x9100, Some((0, "interaction filter")), &[0x5100]);
        assert_eq!(blamed, Blame::Callback(Some(0), 0, "interaction filter"));
        assert_eq!(
            text(blamed, 0x9100, None),
            "2026-10-21 09:05:07Z fault 0xc0000005 at 0x9100, blamed on safes: in its interaction filter callback (+0x100 in enshwick)\r\n"
        );
    }

    #[test]
    fn a_mods_code_on_the_stack_takes_the_blame() {
        let r = ranges();
        let blamed = blame(&r, 0x77_0000, None, &[0x9100, 0x77_1000, 0x1234]);
        assert_eq!(blamed, Blame::Stack(0, 2));
        assert_eq!(
            text(blamed, 0x77_0000, Some((1, 0))),
            "2026-10-21 09:05:07Z fault 0xc0000005 at 0x770000, blamed on safes: its code is on the stack, frame 2; writing 0x0\r\n"
        );
    }

    #[test]
    fn the_loader_and_the_game_are_named_when_no_mod_is_near() {
        let r = ranges();
        assert_eq!(blame(&r, 0x9100, None, &[0x77_0000]), Blame::Loader(2));
        assert_eq!(blame(&r, 0x77_0000, None, &[0x9100]), Blame::Game);
        assert!(
            text(Blame::Game, 0x77_0000, None)
                .ends_with("in the game: no mod's code on the stack\r\n")
        );
    }

    #[test]
    fn only_faults_that_end_the_process_count_as_crashes_in_a_dump() {
        assert!(fatal(EXCEPTION_ACCESS_VIOLATION));
        assert!(fatal(EXCEPTION_STACK_OVERFLOW));
        // A breakpoint, a C++ exception and a debugger print: dumps a
        // handler writes and survives.
        for code in [0x8000_0003_u32, 0xe06d_7363, 0x4001_0006] {
            assert!(!fatal(code.cast_signed()));
        }
        // SAFETY: null pointers.
        assert_eq!(unsafe { fault_code(core::ptr::null()) }, None);
    }

    #[test]
    fn a_region_reads_only_where_committed_and_readable() {
        let end = readable_end(0x1000, 0x2000, MEM_COMMIT, PAGE_READWRITE, 0x1800);
        assert_eq!(end, Some(0x3000));
        assert_eq!(
            readable_end(0x1000, 0x2000, MEM_COMMIT, PAGE_READWRITE, 0x3000),
            None
        );
        assert_eq!(
            readable_end(0x1000, 0x2000, 0x2000, PAGE_READWRITE, 0x1800),
            None
        );
        assert_eq!(
            readable_end(0x1000, 0x2000, MEM_COMMIT, PAGE_NOACCESS, 0x1800),
            None
        );
        let guarded = PAGE_READWRITE | PAGE_GUARD;
        assert_eq!(
            readable_end(0x1000, 0x2000, MEM_COMMIT, guarded, 0x1800),
            None
        );
        assert_eq!(
            readable_end(usize::MAX, 2, MEM_COMMIT, PAGE_READONLY, usize::MAX),
            None
        );
    }

    #[test]
    fn a_word_is_read_only_from_memory_that_reads() {
        use windows_sys::Win32::System::Memory::{
            MEM_RELEASE, MEM_RESERVE, VirtualAlloc, VirtualFree,
        };
        let word = 0x1122_3344_5566_7788_u64;
        assert_eq!(read_word((&raw const word).addr()), Some(word));
        assert_eq!(read_word(0), None);
        assert_eq!(read_word(16), None);
        // SAFETY: a fresh page, released below.
        let page = unsafe { VirtualAlloc(core::ptr::null(), 4096, MEM_RESERVE, PAGE_READWRITE) };
        assert!(!page.is_null());
        assert_eq!(read_word(page.addr()), None, "reserved, not committed");
        // SAFETY: the page reserved above.
        unsafe { VirtualFree(page, 0, MEM_RELEASE) };
        assert_eq!(read_word(page.addr()), None, "released");
    }

    #[test]
    fn unwind_codes_say_how_far_a_frame_reads() {
        // Version 1, three codes: sub rsp, 0x28 (alloc small, 4 * 8 + 8),
        // push rbx, and a save of rsi at rsp + 0x60.
        let info = [
            1, 0x10, 4, 0, 0x08, 0x42, 0x04, 0x00, 0x0c, 0x34, 0x0c, 0x00,
        ];
        assert_eq!(
            reach(&info),
            Some(Reach {
                frame: None,
                bytes: 0x68,
                chained: None,
            })
        );
        // rbp as the frame register, offset 2 * 16, set_fpreg plus a large
        // allocation of 0x1000 bytes in one more slot.
        let framed = [1, 0x10, 3, 0x25, 0x10, 0x01, 0x00, 0x02, 0x08, 0x03];
        assert_eq!(
            reach(&framed),
            Some(Reach {
                frame: Some((5, 32)),
                bytes: 0x1008,
                chained: None,
            })
        );
        // Chained: one code, padded to two, then the parent entry.
        let mut chained = vec![(4 << 3) | 1, 0, 1, 0, 0x00, 0x30, 0, 0];
        chained.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(reach(&chained).and_then(|r| r.chained), Some(0x1234_5678));
        assert_eq!(reach(&[3, 0, 0, 0]), None, "an unknown version");
        assert_eq!(reach(&[1, 0, 2, 0, 0, 0x07]), None, "cut short");
        assert_eq!(reach(&[1, 0, 1, 0, 0, 0x0f]), None, "an unknown code");
    }

    #[test]
    fn a_frame_unwinds_only_when_its_reads_stay_on_the_stack() {
        let reach = Reach {
            frame: None,
            bytes: 0x40,
            chained: None,
        };
        assert!(fits(&reach, 0x1000, None, 0x800, 0x2000));
        assert!(!fits(&reach, 0x1fd0, None, 0x800, 0x2000));
        assert!(!fits(&reach, 0x1000, Some(0x700), 0x800, 0x2000));
        assert!(!fits(&reach, usize::MAX - 8, None, 0, usize::MAX));
    }

    #[test]
    fn the_bounded_walk_unwinds_a_live_stack_and_stops_on_a_broken_one() {
        use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureContext;
        // SAFETY: an all-zero context is a valid value to capture into.
        let mut aligned = Aligned(unsafe { core::mem::zeroed() });
        // SAFETY: a writable context on its 16-byte boundary.
        unsafe { RtlCaptureContext(&raw mut aligned.0) };
        let context = aligned.0;
        let mut frames = [0usize; FRAMES];
        // SAFETY: this thread's own context.
        let found = unsafe { unwind(&context, &mut frames) };
        assert!(found >= 2, "{found} frames");
        let mut broken = context;
        broken.Rsp = 0x10;
        // SAFETY: a context whose stack does not read; the walk checks it.
        assert_eq!(unsafe { unwind(&broken, &mut frames) }, 0);
    }

    #[test]
    fn a_line_never_overflows() {
        let mut line = Line::default();
        for _ in 0..100 {
            line.text(b"0123456789");
        }
        assert_eq!(line.as_bytes().len(), LINE);
        let mut line = Line::default();
        line.hex(0).text(b" ").hex(u64::MAX).text(b" ").dec(7, 3);
        assert_eq!(line.as_bytes(), b"0x0 0xffffffffffffffff 007");
    }
}
