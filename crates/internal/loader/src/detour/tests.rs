use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::System::Memory::{MEM_COMMIT, MEM_RESERVE, VirtualAlloc};

use super::*;

#[test]
fn only_mapped_code_takes_a_hook() {
    let code = (only_mapped_code_takes_a_hook as fn() as *const ()).addr();
    assert!(hookable(code, READ));
    let data = 7u64;
    assert!(!hookable(core::ptr::from_ref(&data).addr(), READ));
    assert!(!hookable(0, READ));
    assert!(!hookable(0x10, READ));
    assert!(!hookable(usize::MAX - 4, READ));
    assert!(matches!(
        prepare_one(
            Request {
                target: 0x10,
                detour: code,
                inside: false,
            },
            &mut Vec::new()
        ),
        Err(PrepareError::NotMappedCode)
    ));
}

type Unary = extern "system" fn(u32) -> u32;

/// Writes `code` to fresh executable memory and returns its address.
fn function(code: &[u8]) -> usize {
    // SAFETY: a fresh allocation, written before it is executed.
    let page = unsafe {
        VirtualAlloc(
            core::ptr::null(),
            4096,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_EXECUTE_READWRITE,
        )
    };
    assert!(!page.is_null());
    // SAFETY: 4096 writable bytes.
    unsafe {
        page.cast::<u8>()
            .copy_from_nonoverlapping(code.as_ptr(), code.len());
    }
    page.addr()
}

extern "system" fn plus_hundred(value: u32) -> u32 {
    value + 100
}

fn callable(address: usize) -> Unary {
    // SAFETY: every test function takes a u32 in ecx and returns one in eax.
    unsafe { core::mem::transmute::<usize, Unary>(address) }
}

/// Hooks `target` to `plus_hundred`; the trampoline or why not.
fn hook(target: usize) -> Result<usize, PrepareError> {
    let pending = prepare(&[Request {
        target,
        detour: (plus_hundred as Unary as *const ()).addr(),
        inside: false,
    }]);
    let ready: Vec<&Pending> = pending.iter().filter_map(|p| p.as_ref().ok()).collect();
    assert!(commit(&ready, &[]));
    pending.into_iter().next().unwrap().map(|p| p.original)
}

#[test]
fn a_hooked_function_reaches_the_detour_and_the_trampoline_the_original() {
    // mov eax, ecx; add eax, 1; ret; padding
    let target = function(&[0x89, 0xc8, 0x83, 0xc0, 0x01, 0xc3, 0xcc, 0xcc, 0xcc]);
    let original = hook(target).unwrap();
    assert_eq!(callable(target)(1), 101);
    assert_eq!(callable(original)(1), 2);
}

#[test]
fn a_rip_relative_operand_still_points_at_its_data() {
    let code = [
        0x48, 0x8d, 0x05, 0x0e, 0x00, 0x00, 0x00, // lea rax, [rip + 14] -> offset 21
        0x8b, 0x00, // mov eax, [rax]
        0xc3, // ret
        0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, // padding
        0x2a, 0x00, 0x00, 0x00, // 42
    ];
    let original = hook(function(&code)).unwrap();
    assert_eq!(callable(original)(0), 42);
}

#[test]
fn a_moved_short_branch_still_lands_in_the_original() {
    let code = [
        0x85, 0xc9, // test ecx, ecx
        0x74, 0x06, // jz +6 -> offset 10
        0xb8, 0x05, 0x00, 0x00, 0x00, // mov eax, 5
        0xc3, // ret
        0xb8, 0x09, 0x00, 0x00, 0x00, // mov eax, 9
        0xc3, // ret
    ];
    let original = hook(function(&code)).unwrap();
    assert_eq!(callable(original)(1), 5);
    assert_eq!(callable(original)(0), 9);
}

#[test]
fn code_patches_are_written_in_the_batch_with_the_hooks() {
    // mov eax, 1; ret; padding
    let patched = function(&[0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3, 0xcc, 0xcc]);
    let hooked = function(&[0x89, 0xc8, 0x83, 0xc0, 0x01, 0xc3, 0xcc, 0xcc, 0xcc]);
    let pending = prepare(&[Request {
        target: hooked,
        detour: (plus_hundred as Unary as *const ()).addr(),
        inside: false,
    }]);
    let ready: Vec<&Pending> = pending.iter().filter_map(|p| p.as_ref().ok()).collect();
    assert_eq!(ready[0].span(), (hooked, 5));
    let code = [Bytes {
        address: patched + 1,
        bytes: &[0x02],
    }];
    assert!(commit(&ready, &code));
    assert_eq!(callable(patched)(0), 2);
    assert_eq!(callable(hooked)(1), 101);
}

#[test]
fn a_function_that_jumps_away_within_the_jump_is_refused() {
    // jmp +3; the next function's first bytes, which the jump would cover
    let target = function(&[0xeb, 0x03, 0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3, 0xcc]);
    assert!(hook(target).is_err());
    // jmp rel32: the whole jump moves, nothing past it is touched
    let thunk = function(&[0xe9, 0x00, 0x00, 0x00, 0x00, 0x89, 0xc8, 0xc3, 0xcc]);
    let original = hook(thunk).unwrap();
    assert_eq!(callable(original)(4), 4);
}

#[test]
fn a_function_shorter_than_the_jump_is_refused() {
    // ret; int3...
    let target = function(&[0xc3, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc]);
    assert!(hook(target).is_err());
}

/// Sets `flags[1]`, then waits for `flags[0]` and returns; `r8` points at
/// the flags. Touches no stack, so any alignment does.
const SPIN: [u8; 16] = [
    0x41, 0xc7, 0x40, 0x04, 0x01, 0x00, 0x00, 0x00, // mov dword [r8 + 4], 1
    0x41, 0x8b, 0x00, // mov eax, [r8]
    0x85, 0xc0, // test eax, eax
    0x74, 0xf9, // jz -7
    0xc3, // ret
];

type Calling = extern "system" fn(u32, usize, *const AtomicU32) -> u32;

/// A thread running `entry(5, spin, flags)`, once it waits inside the
/// spin.
fn waiting_in(entry: usize, flags: &'static [AtomicU32; 2]) -> std::thread::JoinHandle<u32> {
    let spin = function(&SPIN);
    // SAFETY: each caller's code takes these arguments.
    let call = unsafe { core::mem::transmute::<usize, Calling>(entry) };
    let thread = std::thread::spawn(move || call(5, spin, flags.as_ptr()));
    while flags[1].load(Ordering::Acquire) == 0 {
        std::thread::yield_now();
    }
    thread
}

#[test]
fn a_thread_that_called_out_of_the_hooked_bytes_returns_into_the_trampoline() {
    static FLAGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
    let target = function(&[
        0xff, 0xd2, // call rdx <- returns inside the jump
        0x90, 0x90, 0x90, // nop
        0x8d, 0x41, 0x07, // lea eax, [rcx + 7]
        0xc3, // ret
        0xcc, 0xcc, 0xcc,
    ]);
    let thread = waiting_in(target, &FLAGS);
    let original = hook(target).unwrap();
    FLAGS[0].store(1, Ordering::Release);
    assert_eq!(thread.join().unwrap(), 12);
    assert_eq!(callable(target)(5), 105);
    assert_ne!(original, target);
}

#[test]
fn a_code_patch_waits_for_a_thread_that_returns_into_it() {
    static FLAGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
    let target = function(&[
        0xff, 0xd2, // call rdx <- returns inside the patch
        0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1
        0xc3, // ret
        0xcc, 0xcc,
    ]);
    let thread = waiting_in(target, &FLAGS);
    let release = std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(30));
        FLAGS[0].store(1, Ordering::Release);
    });
    let patch = [Bytes {
        address: target,
        // mov eax, 2; nop; nop
        bytes: &[0xb8, 0x02, 0x00, 0x00, 0x00, 0x90, 0x90],
    }];
    assert!(commit(&[], &patch));
    assert_eq!(thread.join().unwrap(), 1);
    release.join().unwrap();
    assert_eq!(callable(target)(0), 2);
}

#[test]
fn only_instructions_after_the_first_move_and_only_returns_after_calls_count() {
    let pending = Pending {
        original: 0x9000,
        target: 0x1000,
        patch: [0; PATCH],
        stolen: 6,
        trampoline: 0x9000,
        moved: vec![(0, 0), (2, 2), (4, 7)],
    };
    assert_eq!(
        moved_instructions(&[&pending]),
        [(0x1002, 0x9002), (0x1004, 0x9007)]
    );
    // call rdx; mov eax, 1; ret
    let code = function(&[0xff, 0xd2, 0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3]);
    let bytes = [0; 7];
    let patch = [Bytes {
        address: code,
        bytes: &bytes,
    }];
    assert_eq!(patch_returns(&patch), [code + 2]);
}

/// Whether a thread counting on its own stands still while `threads`
/// holds it, and counts again once they are resumed.
fn stops_and_goes(suspend: impl FnOnce(&mut Threads) -> bool) -> bool {
    let count = std::sync::Arc::new(AtomicU32::new(0));
    let stop = std::sync::Arc::new(AtomicU32::new(0));
    let counting = {
        let (count, stop) = (std::sync::Arc::clone(&count), std::sync::Arc::clone(&stop));
        std::thread::spawn(move || {
            while stop.load(Ordering::Relaxed) == 0 {
                count.fetch_add(1, Ordering::Relaxed);
            }
        })
    };
    while count.load(Ordering::Relaxed) == 0 {
        std::thread::yield_now();
    }
    let mut threads = Threads::new();
    let suspended = suspend(&mut threads);
    let before = count.load(Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let still = count.load(Ordering::Relaxed) == before;
    threads.resume();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let moves = count.load(Ordering::Relaxed) != before;
    stop.store(1, Ordering::Relaxed);
    counting.join().unwrap();
    suspended && still && moves
}

#[test]
fn every_other_thread_stands_still_until_resumed() {
    assert!(stops_and_goes(Threads::suspend));
}

#[test]
fn a_snapshot_suspends_the_other_threads_where_the_system_has_no_thread_walk() {
    assert!(stops_and_goes(|threads| {
        // SAFETY: `GetCurrentThreadId` takes no arguments and only reads
        // the calling thread's id.
        let me = unsafe { GetCurrentThreadId() };
        threads.suspend_listed(me, THREAD_SUSPEND_RESUME)
    }));
}

#[test]
fn a_thread_context_sits_on_the_sixteen_byte_boundary_windows_requires() {
    assert_eq!(align_of::<ThreadContext>(), 16);
}
