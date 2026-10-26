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

#[test]
fn only_the_copy_that_registered_a_frame_draws_it() {
    extern "system" fn draw(_frame: *const UiFrame, _user: *mut core::ffi::c_void) {}
    let owner = ModIndex::LOADER;
    let current = Frame::new(owner, draw, core::ptr::null_mut());
    let older = Frame {
        generation: current.generation + 1,
        ..current
    };
    let (sealed, late) = ([older, current], vec![current]);
    assert_eq!(
        each(&sealed, Some(&late)).count(),
        2,
        "the sealed and the late current copy"
    );
    assert!(each(&sealed[..1], None).next().is_none());
}
