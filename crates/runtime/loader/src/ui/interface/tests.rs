use super::*;

#[test]
fn the_arena_keeps_texts_until_its_buffer_comes_round() {
    let mut arena = Arena::new();
    arena.next_frame();
    let first = arena.text(&[b"enshwick/", b"lamps"]);
    assert_eq!(first.len, 14);
    // SAFETY: `first` points at 14 bytes the arena just copied into its
    // current buffer, which nothing clears before `next_frame`.
    let bytes = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
    assert_eq!(bytes, b"enshwick/lamps");
    let big = vec![b'x'; ARENA_BYTES];
    assert_eq!(arena.text(&[&big]).len, 0);
    arena.next_frame();
    assert_eq!(arena.text(&[b"next"]).len, 4);
    // SAFETY: one frame on, the arena cleared only the next of its
    // `ARENA_FRAMES` buffers, so the 14 bytes of `first` are untouched.
    let again = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
    assert_eq!(again, b"enshwick/lamps");
}

#[test]
fn the_arena_doubles_each_backslash_so_the_game_shows_a_windows_path_whole() {
    let mut arena = Arena::new();
    arena.next_frame();
    let path = arena.text(&[br"world3-mod-data\enshwick", br"\owners.toml\"]);
    let want = br"world3-mod-data\\enshwick\\owners.toml\\";
    assert_eq!(usize::try_from(path.len), Ok(want.len()));
    // SAFETY: `path` points at `want.len()` bytes the arena just copied into
    // its current buffer, which nothing clears before `next_frame`.
    let bytes = unsafe { core::slice::from_raw_parts(path.ptr, want.len()) };
    assert_eq!(bytes, want);
    arena.next_frame();
    let half = vec![b'\\'; arena.buffers[arena.current].capacity() / 2 + 1];
    let fills = arena.text(&[&half]);
    assert_eq!(fills.len, 0, "backslashes that fit only undoubled");
}

#[test]
fn the_arena_turns_once_a_frame_whichever_hook_is_wired() {
    assert!(!hud_turns_frames(true));
    assert!(hud_turns_frames(false));
}

#[test]
fn a_hud_that_cannot_link_is_off_for_its_draw_too() {
    static HUD: Fuse = Fuse::new("test HUD");
    let loader = crate::start::tests::bare_loader();
    assert!(!link_hud(&loader, &HUD));
    assert!(
        HUD.is_off(),
        "the draw runs under the feature whose link failed"
    );
    let listed = crate::engine::diag::parts_off();
    assert!(listed.iter().any(|p| p.part == HUD.feature()), "{listed:?}");
}

#[test]
fn each_optional_part_turns_off_the_feature_its_work_runs_under() {
    let same = |a: &Fuse, b: &Fuse| core::ptr::from_ref(a) == core::ptr::from_ref(b);
    for (part, fuse) in Part::ALL.iter().zip(PART_FUSES) {
        assert_eq!(fuse.feature(), part.name, "in the order of Part::ALL");
        assert!(
            !same(fuse, Fuse::named(part.name)),
            "{} has a feature of its own, not one found by name",
            part.name
        );
    }
    assert!(same(PART_FUSES[2], &HUD_FUSE));
}

#[test]
fn the_mods_screen_shows_only_while_the_interface_its_entry_and_itself_are_on() {
    let fuses = || {
        [
            Fuse::new("test interface"),
            Fuse::new("test entry"),
            Fuse::new("test screen"),
        ]
    };
    let [interface, entry, screen] = fuses();
    assert!(mods::screen_drawn(&interface, &entry, &screen));
    for off in 0..3 {
        let all = fuses();
        all[off].blow("a test");
        let [interface, entry, screen] = &all;
        assert!(!mods::screen_drawn(interface, entry, screen), "{off} off");
    }
}

#[test]
fn dialogs_draw_in_game_only_while_the_interface_and_its_dialogs_are_on() {
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    assert!(off::both_on(&interface, &dialogs));
    dialogs.blow("a test");
    assert!(!off::both_on(&interface, &dialogs));
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    interface.blow("a test");
    assert!(
        !off::both_on(&interface, &dialogs),
        "the interface draws them"
    );
}

#[test]
fn a_link_runs_only_while_wired_and_its_feature_on() {
    // SAFETY: the slot is this test's own and never called; `PopupFn` is a
    // pointer-sized function pointer type.
    static WIRED: Next<PopupFn> = unsafe { Next::new() };
    let feature = Fuse::new("test popup");
    assert!(!runs(&WIRED, &feature), "not wired");
    WIRED.slot().store(0x1000, Ordering::Release);
    assert!(runs(&WIRED, &feature));
    feature.blow("a test");
    assert!(!runs(&WIRED, &feature), "wired, but its feature is off");
}

#[test]
fn app_states_map_to_screens() {
    assert_eq!(screen(7), UiFrame::IN_GAME);
    assert_eq!(screen(6), UiFrame::LOADING);
    assert_eq!(screen(1), UiFrame::MENU);
    assert_eq!(screen(0), UiFrame::MENU);
}
