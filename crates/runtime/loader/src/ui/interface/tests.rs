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
    let loader = crate::start::tests::bare_loader();
    assert!(!link_hud(&loader));
    assert!(
        HUD_FUSE.is_off(),
        "the draw runs under the feature whose link failed"
    );
    let listed = crate::engine::diag::parts_off();
    assert!(
        listed.iter().any(|p| p.part == Part::HUD.name),
        "{listed:?}"
    );
}

#[test]
fn each_optional_part_turns_off_the_feature_its_work_runs_under() {
    let same = |a: &Fuse, b: &Fuse| core::ptr::from_ref(a) == core::ptr::from_ref(b);
    for part in Part::ALL {
        let fuse = part_fuse(part);
        assert_eq!(fuse.feature(), part.name);
        assert!(
            !same(fuse, Fuse::named(part.name)),
            "{} has a feature of its own, not one found by name",
            part.name
        );
    }
    assert!(same(part_fuse(Part::MAIN_MENU), &mods::MAIN_MENU_FUSE));
    assert!(same(part_fuse(Part::PAUSE_MENU), &mods::PAUSE_MENU_FUSE));
    assert!(same(part_fuse(Part::HUD), &HUD_FUSE));
}

#[test]
fn a_build_without_the_screen_reads_turns_them_and_the_extensions_off() {
    let loader = crate::start::tests::bare_loader();
    extensions::link(&loader);
    assert!(READS_FUSE.is_off(), "frames then read no screen");
    assert!(EXTENSIONS_FUSE.is_off());
}

#[test]
fn dialogs_draw_in_game_only_while_the_interface_and_its_dialogs_are_on() {
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    assert!(both_on(&interface, &dialogs));
    dialogs.blow("a test");
    assert!(!both_on(&interface, &dialogs));
    let (interface, dialogs) = (Fuse::new("test interface"), Fuse::new("test dialogs"));
    interface.blow("a test");
    assert!(!both_on(&interface, &dialogs), "the interface draws them");
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
