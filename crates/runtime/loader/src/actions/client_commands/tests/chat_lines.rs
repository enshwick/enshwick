//! Chat lines through the commands' frame.

use super::*;

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
    assert_eq!(chat::line(77), None, "its line went with it");
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
    assert_eq!(chat::line(78), None);
}
