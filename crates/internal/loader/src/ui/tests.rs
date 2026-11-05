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
    let room = vec![b'x'; arena.buffers[arena.current].capacity() - 1];
    let fills = arena.text(&[&room, br"\"]);
    assert_eq!(fills.len, 0, "the doubled backslash does not fit");
}

#[test]
fn the_arena_turns_once_a_frame_whichever_hook_is_wired() {
    assert!(!hud_turns_frames(true));
    assert!(hud_turns_frames(false));
}

#[test]
fn app_states_map_to_screens() {
    assert_eq!(screen(7), UiFrame::IN_GAME);
    assert_eq!(screen(6), UiFrame::LOADING);
    assert_eq!(screen(1), UiFrame::MENU);
    assert_eq!(screen(0), UiFrame::MENU);
}
