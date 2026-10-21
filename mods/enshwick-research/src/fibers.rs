//! R18: whether the client runs code on fibers, whose stacks no thread
//! points at while they wait. Every ten seconds for five minutes it stops
//! each other thread for a moment and logs the threads whose stack pointer
//! lies outside the stack their thread block records, or whose block says
//! the thread was made a fiber.

use std::time::Duration;

use rustc_hash::FxHashMap;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::Debug::{
    CONTEXT, CONTEXT_CONTROL_AMD64, GetThreadContext,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread, SuspendThread,
    THREAD_GET_CONTEXT, THREAD_QUERY_INFORMATION, THREAD_SUSPEND_RESUME,
};

use crate::memory;

/// How often and how long the threads are looked at.
const EVERY: Duration = Duration::from_secs(10);
const ROUNDS: u32 = 30;
/// The thread block: `StackBase`, `FiberData`, `SameTebFlags` (bit 2 is
/// `HasFiberData`) and `DeallocationStack`, the lowest reserved address of
/// the stack; Windows 10 and 11, x64.
const STACK_BASE: usize = 0x8;
const FIBER_DATA: usize = 0x20;
const SAME_TEB_FLAGS: usize = 0x17ee;
const HAS_FIBER_DATA: u16 = 0x4;
const DEALLOCATION_STACK: usize = 0x1478;

#[repr(C)]
struct BasicInformation {
    exit_status: i32,
    teb: usize,
    process: usize,
    thread: usize,
    affinity: usize,
    priority: i32,
    base_priority: i32,
}

#[link(name = "ntdll", kind = "raw-dylib")]
unsafe extern "system" {
    fn NtQueryInformationThread(
        thread: HANDLE,
        class: u32,
        out: *mut BasicInformation,
        size: u32,
        returned: *mut u32,
    ) -> i32;
}

/// What one thread showed: rounds off its stack, and its fiber data.
#[derive(Debug, Default)]
struct Seen {
    off_stack: u32,
    fiber: Option<u64>,
}

/// Starts the sampling on a thread of its own.
pub(crate) fn start() {
    let started = std::thread::Builder::new()
        .name("research-fibers".into())
        .spawn(run);
    if let Err(why) = started {
        enshwick_loader_api::warn!("R18 off: {why}");
    }
}

fn run() {
    let mut seen: FxHashMap<u32, Seen> = FxHashMap::default();
    let mut threads = 0usize;
    for _ in 0..ROUNDS {
        std::thread::sleep(EVERY);
        let ids = thread_ids();
        threads = threads.max(ids.len());
        for id in ids {
            look(id, &mut seen);
        }
    }
    let odd: Vec<String> = seen
        .iter()
        .filter(|(_, s)| s.off_stack > 0 || s.fiber.is_some())
        .map(|(id, s)| {
            format!(
                "{id}: {} rounds off its stack, fiber {:?}",
                s.off_stack, s.fiber
            )
        })
        .collect();
    if odd.is_empty() {
        enshwick_loader_api::info!(
            "R18 no fibers: {threads} threads at most, each on its own stack in {ROUNDS} rounds"
        );
    } else {
        enshwick_loader_api::info!("R18 threads off their stacks or on fibers: {odd:?}");
    }
}

fn thread_ids() -> Vec<u32> {
    // SAFETY: no preconditions.
    let process = unsafe { GetCurrentProcessId() };
    // SAFETY: no preconditions.
    let me = unsafe { GetCurrentThreadId() };
    // SAFETY: a thread snapshot of the system.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
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
    // SAFETY: a valid snapshot and entry.
    let mut more = unsafe { Thread32First(snapshot, &raw mut entry) } != 0;
    while more {
        if entry.th32OwnerProcessID == process && entry.th32ThreadID != me {
            ids.push(entry.th32ThreadID);
        }
        // SAFETY: as above.
        more = unsafe { Thread32Next(snapshot, &raw mut entry) } != 0;
    }
    // SAFETY: the snapshot is ours.
    unsafe { CloseHandle(snapshot) };
    ids
}

fn look(id: u32, seen: &mut FxHashMap<u32, Seen>) {
    let access = THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT | THREAD_QUERY_INFORMATION;
    // SAFETY: opening a thread of this process.
    let thread = unsafe { OpenThread(access, 0, id) };
    if thread.is_null() {
        return;
    }
    let mut info = BasicInformation {
        exit_status: 0,
        teb: 0,
        process: 0,
        thread: 0,
        affinity: 0,
        priority: 0,
        base_priority: 0,
    };
    let size = u32::try_from(size_of::<BasicInformation>()).unwrap_or(0);
    // SAFETY: a thread handle with query rights and an out struct of `size`.
    let status =
        unsafe { NtQueryInformationThread(thread, 0, &raw mut info, size, core::ptr::null_mut()) };
    // SAFETY: CONTEXT is plain data, valid zeroed.
    let mut context: CONTEXT = unsafe { core::mem::zeroed() };
    context.ContextFlags = CONTEXT_CONTROL_AMD64;
    // SAFETY: a thread of this process opened with suspend rights.
    let stopped = unsafe { SuspendThread(thread) } != u32::MAX;
    let rsp = if stopped {
        // SAFETY: a suspended thread opened with context rights, and an
        // aligned context asking for its control registers.
        let read = unsafe { GetThreadContext(thread, &raw mut context) } != 0;
        // SAFETY: suspended above.
        unsafe { ResumeThread(thread) };
        read.then_some(context.Rsp)
    } else {
        None
    };
    // SAFETY: opened above.
    unsafe { CloseHandle(thread) };
    let (Some(rsp), true) = (rsp, status >= 0 && info.teb != 0) else {
        return;
    };
    let word = |offset: usize| memory::read_u64(info.teb + offset).unwrap_or(0);
    let base = word(STACK_BASE);
    let low = word(DEALLOCATION_STACK);
    let mut flags = [0u8; 2];
    let flags = if memory::read(info.teb + SAME_TEB_FLAGS, &mut flags) == 2 {
        u16::from_le_bytes(flags)
    } else {
        0
    };
    let entry = seen.entry(id).or_default();
    if base != 0 && !(low..=base).contains(&rsp) {
        entry.off_stack += 1;
    }
    if flags & HAS_FIBER_DATA != 0 {
        entry.fiber = Some(word(FIBER_DATA));
    }
}
