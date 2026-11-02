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
    assert_eq!(commit(&ready, &[], || {}), Commit::Written);
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
        id: 0,
        address: patched + 1,
        replacement: &[0x02],
    }];
    assert_eq!(commit(&ready, &code, || {}), Commit::Written);
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

/// Writes `code` like [`function`] and registers a function entry for it
/// with an empty unwind info, as a leaf without a frame has.
fn with_entry(code: &[u8]) -> usize {
    with_unwind(code, [1, 0, 0, 0, 0, 0, 0, 0])
}

/// Writes `code` like [`function`] and registers a function entry for it
/// with `info`, an unwind info of at most two code slots.
fn with_unwind(code: &[u8], info: [u8; 8]) -> usize {
    /// Where the unwind info goes in the page, the one-entry table after it.
    const UNWIND: usize = 0x800; // bound, not a game offset: the test's own page
    const TABLE: usize = UNWIND + 8;
    let page = function(code);
    let len = u32::try_from(code.len()).unwrap();
    let rva = u32::try_from(UNWIND).unwrap();
    let word = |at: usize| u32::from_le_bytes(info[at..at + 4].try_into().unwrap());
    // SAFETY: the 4096 writable bytes `function` allocated, of which `code`
    // took fewer than `UNWIND`; the entry spans the code.
    unsafe { ((page + UNWIND) as *mut [u32; 5]).write([word(0), word(4), 0, len, rva]) };
    // SAFETY: one entry naming the code and unwind info in the page, which
    // is never freed.
    let added = unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::RtlAddFunctionTable(
            (page + TABLE) as *const _,
            1,
            page as u64,
        )
    };
    assert!(added);
    page
}

#[test]
fn a_hook_whose_bytes_a_branch_of_its_function_jumps_into_is_refused() {
    let looped = with_entry(&[
        0x31, 0xc0, // xor eax, eax
        0xff, 0xc0, // inc eax <- the loop's head, inside the jump
        0x39, 0xc8, // cmp eax, ecx
        0x72, 0xfa, // jb -6
        0xc3, // ret
    ]);
    assert_eq!(hook(looped), Err(PrepareError::JumpedInto(looped + 6)));
    let after = with_entry(&[
        0x31, 0xc0, // xor eax, eax
        0x31, 0xd2, // xor edx, edx
        0x90, // nop
        0xff, 0xc0, // inc eax <- the loop's head, right after the jump
        0x39, 0xc8, // cmp eax, ecx
        0x72, 0xfa, // jb -6
        0xc3, // ret
    ]);
    let original = hook(after).unwrap();
    assert_eq!(callable(after)(3), 103);
    assert_eq!(callable(original)(3), 3);
}

#[test]
fn a_mid_function_hook_takes_a_loop_head_but_not_the_bytes_after_it() {
    let looped = with_entry(&[
        0x31, 0xc0, // xor eax, eax
        0x90, // nop
        0xff, 0xc0, // inc eax <- the loop's head
        0x39, 0xc8, // cmp eax, ecx
        0x72, 0xfa, // jb -6
        0xc3, // ret
    ]);
    let at = |target: usize| {
        prepare(&[Request {
            target,
            detour: (plus_hundred as Unary as *const ()).addr(),
            inside: true,
        }])
        .remove(0)
        .map(|pending| pending.span())
    };
    assert_eq!(at(looped + 2), Err(PrepareError::JumpedInto(looped + 7)));
    // The back edge is among the moved bytes and lands on the jump's start.
    assert_eq!(at(looped + 3), Ok((looped + 3, 6)));
}

/// `sub rsp, 0x28; call rdx; add rsp, 0x28; ret`, with unwind info for its
/// four-byte prologue that allocates 0x28 bytes: the jump moves the call.
fn framed() -> usize {
    with_unwind(
        &[
            0x48, 0x83, 0xec, 0x28, 0xff, 0xd2, 0x48, 0x83, 0xc4, 0x28, 0xc3, 0xcc,
        ],
        [1, 4, 1, 0, 4, 0x42, 0, 0],
    )
}

/// Writes the crash walk's return addresses through the pointer the
/// trampoline forwards in `rcx`, so the test needs no shared state.
extern "system" fn walk(out: *mut Vec<usize>) -> u32 {
    // SAFETY: the test passes a pointer to its own live `Vec` as the first
    // argument, which the trampoline's moved code leaves in `rcx`.
    unsafe { *out = crate::crash::walk_here() };
    0
}

/// Calls the trampoline, so the walk's caller frame is a return address
/// inside this one function, which the test pins exactly.
#[inline(never)]
extern "system" fn drive(original: usize, walked: *mut Vec<usize>) {
    // SAFETY: the trampoline takes a pointer in rcx and a function in rdx,
    // calls the function with rcx unchanged, then runs the moved `sub rsp`.
    let call = unsafe {
        core::mem::transmute::<usize, extern "system" fn(*mut Vec<usize>, usize) -> u32>(original)
    };
    let walk_at = (walk as extern "system" fn(*mut Vec<usize>) -> u32 as *const ()).addr();
    call(walked, walk_at);
}

#[test]
fn a_stack_walk_from_below_a_trampoline_passes_it_to_the_caller() {
    let original = hook(framed()).unwrap();
    let mut walked = Vec::new();
    drive(original, &raw mut walked);
    let at = walked
        .iter()
        .position(|&frame| frame == original + 6)
        .expect("the walk reaches the trampoline");
    let caller = walked.get(at + 1).copied().expect("and passes it");
    let drive_at = (drive as extern "system" fn(usize, *mut Vec<usize>) as *const ()).addr();
    let function = function_around(drive_at).expect("drive has a function entry");
    assert!(
        (function.begin..function.end).contains(&caller),
        "caller {caller:#x} is the return address inside drive {:#x}..{:#x}",
        function.begin,
        function.end
    );
}

extern "system-unwind" fn panics() -> u32 {
    panic!("below the trampoline");
}

#[test]
fn a_panic_below_a_trampoline_unwinds_through_it_to_its_catch() {
    let original = hook(framed()).unwrap();
    // SAFETY: the trampoline takes a value in ecx and a function in rdx and
    // unwinds as the function it moved, with the unwind data `prepare`
    // registered.
    let call = unsafe {
        core::mem::transmute::<usize, extern "system-unwind" fn(u32, usize) -> u32>(original)
    };
    let below = (panics as extern "system-unwind" fn() -> u32 as *const ()).addr();
    assert!(
        std::panic::catch_unwind(|| {
            scrub();
            call(0, below)
        })
        .is_err()
    );
}

/// Zeroes the stack below the caller, so that unwinding a frame there with
/// wrong unwind data reads no stale return address.
#[inline(never)]
fn scrub() {
    core::hint::black_box([0u8; 4096]);
}

#[test]
fn a_moved_call_out_of_reach_returns_onto_the_jump_back_not_its_data() {
    // A call about 2 GiB ahead or behind: one is past the reach of a
    // trampoline near the target, whichever side it lands, so that one
    // moves in its far form, `call [rip + x]` with the target as data.
    let far = [0x7fff_fff0_i32, -0x7fff_fff0].into_iter().find_map(|rel| {
        let mut code = vec![0xe8];
        code.extend_from_slice(&rel.to_le_bytes());
        code.extend_from_slice(&[0xc3, 0xcc, 0xcc, 0xcc]);
        let target = with_entry(&code);
        let trampoline = prepare(&[Request {
            target,
            detour: (plus_hundred as Unary as *const ()).addr(),
            inside: false,
        }])
        .remove(0)
        .unwrap()
        .original;
        // SAFETY: the trampoline's bytes are readable in its sealed block.
        let bytes =
            unsafe { core::slice::from_raw_parts(trampoline as *const u8, CODE - FAR_JUMP) };
        let mut decoder = Decoder::with_ip(64, bytes, trampoline as u64, DecoderOptions::NONE);
        let call = decoder.iter().find(|i| {
            matches!(
                i.flow_control(),
                FlowControl::Call | FlowControl::IndirectCall
            )
        })?;
        (call.flow_control() == FlowControl::IndirectCall).then(|| {
            // The call returns here: it must be the jump back to target + 5,
            // not the 8-byte target data iced lays after the whole block.
            let returns = usize::try_from(call.next_ip()).unwrap();
            let at = returns - trampoline;
            let next =
                Decoder::with_ip(64, &bytes[at..], returns as u64, DecoderOptions::NONE).decode();
            (next, bytes[..at + 16].to_vec())
        })
    });
    let (next, around) = far.expect("one of the calls moves in its far form");
    assert_eq!(
        next.flow_control(),
        FlowControl::UnconditionalBranch,
        "the call returns onto {around:x?}"
    );
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
        id: 0,
        address: target,
        // mov eax, 2; nop; nop
        replacement: &[0xb8, 0x02, 0x00, 0x00, 0x00, 0x90, 0x90],
    }];
    assert_eq!(commit(&[], &patch, || {}), Commit::Written);
    assert_eq!(thread.join().unwrap(), 1);
    release.join().unwrap();
    assert_eq!(callable(target)(0), 2);
}

#[test]
fn a_code_patch_is_not_written_while_a_thread_stays_inside_it() {
    static FLAGS: [AtomicU32; 2] = [AtomicU32::new(0), AtomicU32::new(0)];
    static DISARMED: AtomicU32 = AtomicU32::new(0);
    let original = [
        0xff, 0xd2, // call rdx <- returns inside the patch
        0xb8, 0x01, 0x00, 0x00, 0x00, // mov eax, 1
        0xc3, // ret
        0xcc, 0xcc,
    ];
    let busy = function(&original);
    let free = function(&[0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3, 0xcc, 0xcc]);
    let thread = waiting_in(busy, &FLAGS);
    let patches = [
        Bytes {
            id: 0,
            address: free + 1,
            replacement: &[0x02],
        },
        Bytes {
            id: 1,
            address: busy,
            replacement: &[0xb8, 0x02, 0x00, 0x00, 0x00, 0x90, 0x90],
        },
    ];
    let disarm = || DISARMED.store(1, Ordering::Relaxed);
    assert_eq!(commit(&[], &patches, disarm), Commit::Busy(vec![1]));
    assert_eq!(DISARMED.load(Ordering::Relaxed), 1);
    // SAFETY: the 10 bytes `function` wrote at `busy`, never freed.
    let now = unsafe { core::slice::from_raw_parts(busy as *const u8, original.len()) };
    assert_eq!(now, original);
    assert_eq!(callable(free)(0), 1);
    FLAGS[0].store(1, Ordering::Release);
    assert_eq!(thread.join().unwrap(), 1);
    assert_eq!(commit(&[], &patches[..1], || {}), Commit::Written);
    assert_eq!(callable(free)(0), 2);
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
        id: 0,
        address: code,
        replacement: &bytes,
    }];
    assert_eq!(patch_returns(&patch), [(code + 2, 0)]);
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
