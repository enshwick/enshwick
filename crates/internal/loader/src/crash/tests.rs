use windows_sys::Win32::System::Memory::PAGE_READWRITE;

use super::unwind::{Reach, fits, reach, read_word, unwind};
use super::*;

#[test]
fn a_filter_that_chains_back_runs_once() {
    static CALLS: AtomicU32 = AtomicU32::new(0);
    // Another module's filter, set after ours, calls the one it
    // replaced: ours.
    unsafe extern "system" fn other(pointers: *const EXCEPTION_POINTERS) -> i32 {
        if CALLS.fetch_add(1, Ordering::Relaxed) > 4 {
            return CONTINUE_SEARCH;
        }
        // SAFETY: the test's own null pointers, passed on unchanged.
        unsafe { filter(pointers) }
    }
    NEXT.store(other as *const () as usize, Ordering::Release);
    // SAFETY: no pointers: the report stops at once.
    assert_eq!(unsafe { filter(core::ptr::null()) }, CONTINUE_SEARCH);
    assert_eq!(CALLS.load(Ordering::Relaxed), 1);
}

fn ranges() -> [Range; 3] {
    [
        Range::new(0x1000, 0x2000, ModIndex::new(0), "you.trading-post"),
        Range::new(0x5000, 0x6000, ModIndex::new(1), "you.market-stalls"),
        Range::new(0x9000, 0xa000, ModIndex::LOADER, "enshwick"),
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
        "2026-10-21 09:05:07Z fault 0xc0000005 at 0x5800, blamed on you.market-stalls: in its code (+0x800 in you.market-stalls); reading 0x10\r\n"
    );
}

#[test]
fn a_fault_during_a_callback_is_the_callbacks_mod() {
    let r = ranges();
    let blamed = blame(&r, 0x9100, Some((0, "interaction filter")), &[0x5100]);
    assert_eq!(blamed, Blame::Callback(Some(0), 0, "interaction filter"));
    assert_eq!(
        text(blamed, 0x9100, None),
        "2026-10-21 09:05:07Z fault 0xc0000005 at 0x9100, blamed on you.trading-post: in its interaction filter callback (+0x100 in enshwick)\r\n"
    );
}

#[test]
fn a_mods_code_on_the_stack_takes_the_blame() {
    let r = ranges();
    let blamed = blame(&r, 0x77_0000, None, &[0x9100, 0x77_1000, 0x1234]);
    assert_eq!(blamed, Blame::Stack(0, 2));
    assert_eq!(
        text(blamed, 0x77_0000, Some((1, 0))),
        "2026-10-21 09:05:07Z fault 0xc0000005 at 0x770000, blamed on you.trading-post: its code is on the stack, frame 2; writing 0x0\r\n"
    );
}

#[test]
fn the_loader_and_the_game_are_named_when_no_mod_is_near() {
    let r = ranges();
    assert_eq!(blame(&r, 0x9100, None, &[0x77_0000]), Blame::Loader(2));
    assert_eq!(blame(&r, 0x77_0000, None, &[0x9100]), Blame::Game);
    assert!(
        text(Blame::Game, 0x77_0000, None).ends_with("in the game: no mod's code on the stack\r\n")
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
    // SAFETY: a null pointer, which `fault_code` takes.
    assert_eq!(unsafe { fault_code(core::ptr::null()) }, None);
}

#[test]
fn a_word_is_read_only_from_memory_that_reads() {
    use windows_sys::Win32::System::Memory::{MEM_RELEASE, MEM_RESERVE, VirtualAlloc, VirtualFree};
    let word = 0x1122_3344_5566_7788_u64;
    assert_eq!(read_word((&raw const word).expose_provenance()), Some(word));
    assert_eq!(read_word(0), None);
    assert_eq!(read_word(16), None);
    // SAFETY: `VirtualAlloc` reserves a fresh page for the test, which
    // frees it at the end.
    let page = unsafe { VirtualAlloc(core::ptr::null(), 4096, MEM_RESERVE, PAGE_READWRITE) };
    assert!(!page.is_null());
    assert_eq!(
        read_word(page.expose_provenance()),
        None,
        "reserved, not committed"
    );
    // SAFETY: `page` is the test's own reservation, released once.
    unsafe { VirtualFree(page, 0, MEM_RELEASE) };
    assert_eq!(read_word(page.expose_provenance()), None, "released");
}

#[test]
fn unwind_codes_say_how_far_a_frame_reads() {
    // Version 1, three codes: sub rsp, 0x28 (alloc small, 4 * 8 + 8),
    // push rbx, and a save of rsi at rsp + 0x60.
    let info = [
        1, 0x10, 4, 0, 0x08, 0x42, 0x04, 0x00, 0x0c, 0x34, 0x0c, 0x00,
    ];
    assert_eq!(
        reach(&info, usize::MAX),
        Some(Reach {
            frame: None,
            bytes: 0x68,
            chained: None,
        })
    );
    // Stopped at offset 3, before any code's instruction has run: only the
    // return address is read.
    assert_eq!(
        reach(&info, 3),
        Some(Reach {
            frame: None,
            bytes: 8,
            chained: None,
        })
    );
    // Stopped at offset 8: the alloc (offset 8) and the push (offset 4) have
    // run, the save of rsi (offset 0xc) has not.
    assert_eq!(
        reach(&info, 8),
        Some(Reach {
            frame: None,
            bytes: 0x38,
            chained: None,
        })
    );
    // rbp as the frame register, offset 2 * 16, set_fpreg plus a large
    // allocation of 0x1000 bytes in one more slot.
    let framed = [1, 0x10, 3, 0x25, 0x10, 0x01, 0x00, 0x02, 0x08, 0x03];
    assert_eq!(
        reach(&framed, usize::MAX),
        Some(Reach {
            frame: Some((5, 32)),
            bytes: 0x1008,
            chained: None,
        })
    );
    // Chained: one code, padded to two, then the parent entry.
    let mut chained = vec![(4 << 3) | 1, 0, 1, 0, 0x00, 0x30, 0, 0];
    chained.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0x78, 0x56, 0x34, 0x12]);
    assert_eq!(
        reach(&chained, usize::MAX).and_then(|r| r.chained),
        Some(0x1234_5678)
    );
    assert_eq!(reach(&[3, 0, 0, 0], usize::MAX), None, "an unknown version");
    assert_eq!(reach(&[1, 0, 2, 0, 0, 0x07], usize::MAX), None, "cut short");
    assert_eq!(
        reach(&[1, 0, 1, 0, 0, 0x0f], usize::MAX),
        None,
        "an unknown code"
    );
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
    let mut aligned = crate::detour::ThreadContext(unsafe { core::mem::zeroed() });
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
