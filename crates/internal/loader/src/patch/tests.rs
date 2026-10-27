use std::sync::atomic::AtomicU64;

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
                windows_sys::Win32::System::Memory::PAGE_EXECUTE_READWRITE,
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
    use enshwick::abi::{HookFlags, Registers};

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
fn the_unwind_data_describes_the_frame() {
    assert_eq!(FRAME % 16, 8);
    assert_eq!(u32::from(UNWIND[6]) * 8, FRAME);
    assert_eq!(UNWIND[1], PROLOG);
}
