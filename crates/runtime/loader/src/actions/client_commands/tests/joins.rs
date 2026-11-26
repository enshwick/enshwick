//! Joining and leaving a game through the commands' frame.

use super::*;

#[test]
fn a_panic_in_a_join_or_leave_turns_off_only_the_commands() {
    play_frame(|| panic!("a test"), false, || false, |_| false);
    assert!(FUSE.is_off(), "the commands, not the interface frame");
}

#[test]
fn a_join_and_a_leave_report_a_phase_from_the_queue_on() {
    assert_eq!(Kind::Join.queued_phase(), Status::PHASE_QUEUED);
    assert_eq!(Kind::Leave.queued_phase(), Status::PHASE_IN_WORLD);
    assert_eq!(Kind::Open(9).queued_phase(), 0);
}

#[test]
fn the_stage_tells_once_when_play_starts_and_once_when_it_ends() {
    let _menu = play::note_stage(2, 1);
    assert_eq!(play::note_stage(0, 2), None, "a menu after a menu");
    assert_eq!(play::note_stage(4, 3), Some(true), "play starts loading");
    assert_eq!(play::note_stage(7, 4), None, "loading into the world");
    assert_eq!(play::note_stage(7, 5), None, "still in the world");
    assert_eq!(play::note_stage(2, 6), Some(false), "back at a menu");
    assert_eq!(play::note_stage(2, 7), None, "the menu again");
}

#[test]
fn a_leave_starts_only_in_play_and_only_one_at_a_time() {
    play::note_stage(2, 10);
    assert_eq!(
        play::can_leave(10),
        Err(play::StartError::NotNow),
        "the menu"
    );
    play::note_stage(7, 20);
    assert_eq!(play::can_leave(20), Ok(()));
    assert_eq!(play::leave(5, 20), Ok(()));
    assert_eq!(
        play::can_leave(20),
        Err(play::StartError::NotNow),
        "a leave runs"
    );
}
