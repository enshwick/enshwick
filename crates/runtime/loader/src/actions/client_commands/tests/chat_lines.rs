//! Chat lines through the commands' frame.

use super::*;

/// Command `id`'s line while it is parked.
fn parked(id: u64) -> Option<String> {
    chat::with_line(id, str::to_owned)
}

#[test]
fn a_chat_line_is_posted_by_the_frame_and_observed_once_the_game_s_queue_takes_it() {
    let mut commands = kept(Command {
        id: 77,
        expected: None,
        ..command(Kind::Chat)
    });
    chat::park(77, "hello there".to_owned());
    let mut posted = Vec::new();
    let mut post = |change: FrameChange<'_>, answer| {
        posted.push(format!("{change:?}"));
        answer
    };
    assert!(
        run(&mut commands, Some(world()), None, 150, |c| post(
            c,
            Applied::Busy
        )),
        "a full queue: it waits for the next frame"
    );
    assert_eq!(commands.kept[0].state, Status::PENDING);
    assert!(!run(&mut commands, Some(world()), None, 160, |c| post(
        c,
        Applied::Taken
    )));
    assert_eq!(commands.kept[0].state, Status::OBSERVED);
    let line = format!("{:?}", FrameChange::Chat("hello there"));
    assert_eq!(posted, [line.clone(), line]);
    assert_eq!(parked(77), None, "its line went with it");
}

#[test]
fn a_chat_line_at_the_main_menu_ends_timed_out_and_drops_its_line() {
    let mut commands = kept(Command {
        id: 78,
        expected: None,
        ..command(Kind::Chat)
    });
    chat::park(78, "anyone?".to_owned());
    let menu = Seen::menu(3, LoaderScreen::default());
    let late = 100 + TIMEOUT_MS + 1;
    assert!(!run(&mut commands, None, Some(menu), late, |_| {
        panic!("the menu frame posts no chat")
    }));
    let ended = commands.kept[0];
    assert_eq!(
        (ended.state, ended.reason),
        (Status::CANCELLED, Status::TIMED_OUT)
    );
    assert_eq!(parked(78), None);
}

#[test]
fn a_chat_line_waiting_for_a_full_queue_is_not_cancelled_by_the_player_s_keys() {
    let mut commands = kept(Command {
        id: 79,
        expected: None,
        ..command(Kind::Chat)
    });
    chat::park(79, "wait for me".to_owned());
    let sprinting = Seen {
        held: Some(1 << 12),
        ..world()
    };
    assert!(run(&mut commands, Some(sprinting), None, 150, |_| {
        Applied::Busy
    }));
    assert_eq!(commands.kept[0].state, Status::PENDING, "it still waits");
    assert!(!run(&mut commands, Some(sprinting), None, 160, |_| {
        Applied::Taken
    }));
    assert_eq!(commands.kept[0].state, Status::OBSERVED);
}

#[test]
fn a_chat_line_the_build_cannot_post_ends_unavailable_and_drops_its_line() {
    let mut commands = kept(Command {
        id: 80,
        expected: None,
        ..command(Kind::Chat)
    });
    chat::park(80, "no poster".to_owned());
    assert!(!run(&mut commands, Some(world()), None, 150, |_| {
        Applied::Unavailable
    }));
    assert_eq!(commands.kept[0].state, Status::UNAVAILABLE);
    assert_eq!(parked(80), None);
}
