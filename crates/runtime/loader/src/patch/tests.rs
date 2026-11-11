use std::sync::atomic::AtomicU64;

use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READWRITE, VirtualAlloc,
};

use super::*;

#[test]
fn a_thunk_jumps_through_its_slot() {
    let slot = AtomicUsize::new(0);
    let mut code = [0u8; THUNK];
    let base = code.as_mut_ptr();
    // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
    // `slot` sits on the same stack, well within 2 GiB.
    unsafe { write_thunk(base, THUNK, &slot, None, None) };
    assert_eq!(code[..2], [0xff, 0x25]);
    let relative = i32::from_le_bytes(code[2..6].try_into().unwrap());
    let target = (base.addr() + 6).wrapping_add_signed(relative as isize);
    assert_eq!(target, core::ptr::from_ref(&slot).addr());
}

#[test]
fn a_frame_slot_that_cannot_be_had_gives_back_the_indices_it_took() {
    let free = TLS_INLINE_SLOTS - 4;
    let mut held = Vec::new();
    loop {
        // SAFETY: `TlsAlloc` takes no arguments; a failure is checked below.
        let index = unsafe { TlsAlloc() };
        assert_ne!(index, TLS_OUT_OF_INDEXES);
        if index == free {
            // SAFETY: `index` came from `TlsAlloc` above and is freed once.
            unsafe { TlsFree(index) };
            break;
        }
        held.push(index);
    }
    assert!(FrameSlot::allocate().is_none());
    // SAFETY: `TlsAlloc` takes no arguments; the index is checked and freed below.
    let next = unsafe { TlsAlloc() };
    assert_eq!(next, free, "the failed allocation kept indices");
    for index in held.into_iter().chain([next]) {
        // SAFETY: each index came from `TlsAlloc` in this test and is freed once.
        unsafe { TlsFree(index) };
    }
}

#[test]
fn a_frame_thunk_saves_the_stack_pointer_first() {
    let slot = AtomicUsize::new(0);
    let mut code = [0u8; THUNK];
    // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
    // `slot` sits on the same stack, well within 2 GiB.
    unsafe {
        write_thunk(
            code.as_mut_ptr(),
            THUNK,
            &slot,
            Some([3, 4, 5, 6, 7, 8, 9, 10, 11]),
            None,
        );
    }
    assert_eq!(code[..5], [0x65, 0x48, 0x89, 0x24, 0x25]);
    assert_eq!(
        u32::from_le_bytes(code[5..9].try_into().unwrap()),
        0x1480 + 24
    );
    // r14, the eighth register, into slot 10.
    assert_eq!(code[63..68], [0x65, 0x4c, 0x89, 0x34, 0x25]);
    assert_eq!(
        u32::from_le_bytes(code[68..72].try_into().unwrap()),
        0x1480 + 80
    );
    assert_eq!(code[81..83], [0xff, 0x25]);
}

#[test]
fn a_counting_frame_thunk_still_fits() {
    let slot = AtomicUsize::new(0);
    let counter = Counter::default();
    let mut code = [0u8; THUNK];
    // SAFETY: `code` is a local buffer of `THUNK` writable bytes, and
    // `slot` and `counter` sit on the same stack, well within 2 GiB.
    let written = unsafe {
        write_thunk(
            code.as_mut_ptr(),
            THUNK,
            &slot,
            Some([3, 4, 5, 6, 7, 8, 9, 10, 11]),
            Some((&counter, false)),
        )
    };
    assert!(written.timed.is_none());
    assert_eq!(code[81..85], [0xf0, 0x48, 0xff, 0x05]);
    assert_eq!(code[89..91], [0xff, 0x25]);
}

/// Executable memory: a thunk at the start, its slot and counter after it.
struct Page {
    base: *mut u64,
}

impl Page {
    fn new() -> Self {
        // SAFETY: `VirtualAlloc` with a null address reserves a fresh
        // page and touches no memory of ours; null on failure, asserted
        // below.
        let base = unsafe {
            VirtualAlloc(
                core::ptr::null(),
                PAGE,
                MEM_RESERVE | MEM_COMMIT,
                PAGE_EXECUTE_READWRITE,
            )
        }
        .cast::<u64>();
        assert!(!base.is_null());
        Self { base }
    }

    fn slot(&self) -> &'static AtomicUsize {
        // SAFETY: `TIMED_THUNK` bytes are under `PAGE`, so word
        // `TIMED_THUNK / 8` lies inside the page at `self.base`.
        let slot = unsafe { self.base.add(TIMED_THUNK / 8) }.cast::<AtomicUsize>();
        // SAFETY: the word is aligned in the committed, zeroed page,
        // which the test never frees, and a zeroed `AtomicUsize` is
        // valid.
        unsafe { &*slot }
    }

    fn counter(&self) -> &'static Counter {
        let at = (TIMED_THUNK + size_of::<usize>()).next_multiple_of(align_of::<Counter>());
        assert!(at + size_of::<Counter>() <= PAGE);
        // SAFETY: the counter ends inside the page at `self.base`, checked
        // by the assertion before it, and starts past the thunk and its slot.
        let counter = unsafe { self.base.add(at / 8) };
        #[expect(
            clippy::cast_ptr_alignment,
            reason = "the page is page-aligned and `at` a multiple of the counter's alignment"
        )]
        let counter = counter.cast::<Counter>();
        // SAFETY: the page is page-aligned and `at` a multiple of the
        // counter's alignment; the page is committed, zeroed and never freed
        // by the test, and a zeroed `Counter` is valid.
        unsafe { &*counter }
    }

    /// The thunk, timed or only counting, jumping to `target`.
    fn thunk(&self, target: usize, timed: bool) -> usize {
        self.slot().store(target, Ordering::Release);
        // SAFETY: the page has `TIMED_THUNK` writable bytes before the
        // slot, which no thread runs yet, and the slot and the counter
        // lie in the same page.
        let written = unsafe {
            write_thunk(
                self.base.cast(),
                TIMED_THUNK,
                self.slot(),
                None,
                Some((self.counter(), timed)),
            )
        };
        assert_eq!(written.timed.is_some(), timed);
        self.base.addr()
    }
}

type Eight = extern "system" fn(u64, u64, u64, u64, u64, u64, u64, u64) -> u64;

extern "system" fn weigh(
    w1: u64,
    w2: u64,
    w3: u64,
    w4: u64,
    w5: u64,
    w6: u64,
    w7: u64,
    w8: u64,
) -> u64 {
    w1 + 2 * w2 + 3 * w3 + 4 * w4 + 5 * w5 + 6 * w6 + 7 * w7 + 8 * w8
}

extern "system" fn halve(value: f64) -> f64 {
    value / 2.0
}

#[test]
fn a_timed_thunk_counts_every_call_and_times_one_in_256() {
    let page = Page::new();
    let thunk = page.thunk((weigh as Eight as *const ()).addr(), true);
    // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
    // `weigh`, so it has `Eight`'s signature.
    let call = unsafe { core::mem::transmute::<usize, Eight>(thunk) };
    for i in 0..600 {
        assert_eq!(call(1, 1, 1, 1, 1, 1, 1, i), 28 + 8 * i);
    }
    let counter = page.counter();
    let load = |value: &AtomicU64| value.load(Ordering::Relaxed);
    assert_eq!(load(&counter.calls), 600);
    // The 256th and the 512th.
    assert_eq!(load(&counter.samples), 2);
    assert!(load(&counter.ticks) > 0);
}

#[test]
fn a_timed_call_keeps_a_floating_point_result() {
    let page = Page::new();
    let thunk = page.thunk(
        (halve as extern "system" fn(f64) -> f64 as *const ()).addr(),
        true,
    );
    // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
    // `halve`, so it has `halve`'s signature.
    let call = unsafe { core::mem::transmute::<usize, extern "system" fn(f64) -> f64>(thunk) };
    for _ in 0..512 {
        assert!((call(3.0) - 1.5).abs() < f64::EPSILON);
    }
    let samples = page.counter().samples.load(Ordering::Relaxed);
    assert_eq!(samples, 2);
}

#[test]
fn a_counting_thunk_counts_without_timing() {
    let page = Page::new();
    let thunk = page.thunk((weigh as Eight as *const ()).addr(), false);
    // SAFETY: `page.thunk` wrote a thunk that jumps through its slot to
    // `weigh`, so it has `Eight`'s signature.
    let call = unsafe { core::mem::transmute::<usize, Eight>(thunk) };
    for _ in 0..300 {
        assert_eq!(call(1, 0, 0, 0, 0, 0, 0, 1), 9);
    }
    let counter = page.counter();
    assert_eq!(counter.calls.load(Ordering::Relaxed), 300);
    assert_eq!(counter.samples.load(Ordering::Relaxed), 0);
}

#[test]
fn the_owners_of_a_failed_mid_function_hook_count_as_failed() {
    use crate::chains::Link;
    use crate::mid::MidLink;
    use crate::mod_index::ModIndex;
    use enshwick_core::abi::{HookFlags, Registers};

    static NEXT: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
    extern "system" fn handler(_: *mut Registers, _: *mut core::ffi::c_void) {}

    let mut chains = Chains::default();
    let link = |owner: ModIndex| Link {
        owner,
        detour: 0x1,
        next: &NEXT[usize::from(owner == ModIndex::LOADER)],
        priority: 0,
        flags: HookFlags::NONE,
        hop: None,
    };
    let _ = chains.add(0x10, link(ModIndex::LOADER));
    let _ = chains.add(0x10, link(ModIndex::new(2)));
    let mut mids = Mids::default();
    let mid = MidLink {
        owner: ModIndex::new(1),
        handler,
        user: 0,
        priority: 0,
        flags: HookFlags::NONE,
    };
    let _ = mids.add(0x20, mid);
    let failures = [Failure::chain(0x10, "x"), Failure::site(0x20, "y")];
    assert_eq!(
        failed_owners(&failures, &chains, &mids),
        [ModIndex::new(2), ModIndex::new(1)]
    );
}

#[test]
fn a_target_another_program_took_stays_its_and_names_who_took_it() {
    use crate::chains::Link;
    use crate::mod_index::ModIndex;
    use enshwick_core::abi::HookFlags;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    static NEXT: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
    /// `mov eax, ecx; add eax, 1; ret`, as the game shipped both functions.
    const FUNCTION: [u8; 6] = [0x89, 0xc8, 0x83, 0xc0, 0x01, 0xc3];
    const LEN: usize = 0x80; // bound, not a game offset: the test page's code
    const FREE: usize = 0x40; // bound, not a game offset: the test page's second function

    // SAFETY: a fresh page, reserved and committed here, checked below and
    // never freed, so the code on it outlives the hooks.
    let page = unsafe {
        VirtualAlloc(
            core::ptr::null(),
            PAGE,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_EXECUTE_READWRITE,
        )
    }
    .addr();
    assert_ne!(page, 0);
    let mut shipped = vec![0xcc; LEN];
    shipped[..FUNCTION.len()].copy_from_slice(&FUNCTION);
    shipped[FREE..FREE + FUNCTION.len()].copy_from_slice(&FUNCTION);
    let kernel32: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
    // SAFETY: `kernel32` is a NUL-terminated wide string; kernel32 is
    // loaded in every process.
    let other = unsafe { GetModuleHandleW(kernel32.as_ptr()) }.addr();
    // Another program's `jmp [rip+0]` over the first function, into its DLL.
    let mut live = shipped.clone();
    live[..6].copy_from_slice(&[0xff, 0x25, 0, 0, 0, 0]);
    live[6..14].copy_from_slice(&(other as u64).to_le_bytes());
    // SAFETY: the page holds `PAGE` writable bytes, more than `LEN`, and no
    // thread runs it yet.
    unsafe { (page as *mut u8).copy_from_nonoverlapping(live.as_ptr(), LEN) };
    crate::shipped::keep(page, shipped);

    let mut chains = Chains::default();
    for (target, next) in [(page, &NEXT[0]), (page + FREE, &NEXT[1])] {
        let link = Link {
            owner: ModIndex::new(1),
            detour: 0x1,
            next,
            priority: 0,
            flags: HookFlags::NONE,
            hop: None,
        };
        assert!(chains.add(target as u64, link).is_ok());
    }
    let armed = arm(
        &mut chains,
        &Mids::default(),
        None,
        |_| false,
        |_| {},
        &[],
        Count::Off,
    );
    let [failure] = &armed.failures[..] else {
        panic!("one failure, not {:?}", armed.failures);
    };
    assert_eq!(failure.target, page as u64);
    assert!(
        failure.reason.eq_ignore_ascii_case("taken by kernel32.dll"),
        "{}",
        failure.reason
    );
    // SAFETY: the page's first `LEN` bytes, readable and never freed.
    let now = unsafe { core::slice::from_raw_parts(page as *const u8, LEN) };
    assert_eq!(now[..14], live[..14], "the other program's jump stays");
    assert_eq!(now[FREE], 0xe9, "the function as shipped is hooked");
}

/// Four patches: one of a mod that is off, two of one mod, one of another.
fn four_patches() -> [CodePatch; 4] {
    use crate::code::WHOLE_MOD;
    use crate::mod_index::ModIndex;

    let patch = |owner: usize, address: usize| CodePatch {
        owner: ModIndex::new(owner),
        feature: WHOLE_MOD,
        address,
        expected: Box::new([0x90]),
        replacement: Box::new([0xcc]),
        needs: Box::default(),
    };
    [
        patch(0, 0x10),
        patch(1, 0x20),
        patch(1, 0x30),
        patch(2, 0x40),
    ]
}

#[test]
fn a_patch_a_thread_stays_in_waits_with_its_feature_and_names_its_owner() {
    use crate::code::Verdict;

    let patches = four_patches();
    let mut verdicts = [Verdict::Off, Verdict::Write, Verdict::Write, Verdict::Write];
    // A thread stayed in patch 1, after the patch of the mod that is off.
    let owners = wait_out(&patches, &mut verdicts, &[1]);
    assert_eq!(
        verdicts,
        [Verdict::Off, Verdict::Wait, Verdict::Wait, Verdict::Write]
    );
    assert_eq!(owners, [patches[1].owner]);
}

#[test]
fn a_loader_patch_a_thread_stays_in_is_skipped_and_never_waits() {
    use crate::code::Verdict;
    use crate::mod_index::ModIndex;

    let mut patches = four_patches();
    patches[3].owner = ModIndex::LOADER;
    let mut verdicts = [
        Verdict::Write,
        Verdict::Write,
        Verdict::Write,
        Verdict::Write,
    ];
    let owners = wait_out(&patches, &mut verdicts, &[3]);
    assert!(matches!(verdicts[3], Verdict::Skip(_)));
    assert_eq!(
        verdicts[..3],
        [Verdict::Write, Verdict::Write, Verdict::Write]
    );
    assert!(owners.is_empty());
}

#[test]
#[should_panic(expected = "outside the batch")]
#[cfg_attr(not(debug_assertions), ignore = "checks a debug assertion")]
fn a_stuck_patch_id_outside_the_batch_is_a_bug() {
    use crate::code::Verdict;

    let patches = four_patches();
    let mut verdicts = [Verdict::Off, Verdict::Write, Verdict::Write, Verdict::Write];
    let _owners = wait_out(&patches, &mut verdicts, &[9]);
}

#[test]
fn the_unwind_data_describes_the_frame() {
    assert_eq!(FRAME % 16, 8);
    assert_eq!(u32::from(UNWIND[6]) * 8, FRAME);
    assert_eq!(UNWIND[1], PROLOG);
}
