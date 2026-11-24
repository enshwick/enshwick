//! The dialog answer command through the commands' frame.

use super::*;
use dialogs::{Ask, Shown};
use enshwick_core::abi::UiDialog;
use std::sync::atomic::AtomicU64;

/// The dialog the tests show, as the interface's word holds it; every test
/// registers the one reader, since a process registers once.
static SHOWN: AtomicU64 = AtomicU64::new(0);

fn show(shown: Shown) {
    dialogs::register(|| Some(Shown::from_word(SHOWN.load(Ordering::Relaxed))));
    SHOWN.store(shown.to_word(), Ordering::Relaxed);
}

const OK: Ask = Ask {
    answer: UiDialog::OK,
    dialog: 4,
};

const OK_DIALOG: Shown = Shown {
    number: 4,
    buttons: 1 << UiDialog::OK,
    owner: None,
};

const NONE: Shown = Shown {
    number: 0,
    buttons: 0,
    owner: None,
};

#[test]
fn a_dialog_answer_is_handed_at_the_menu_and_ends_as_the_draw_tells_after_its_dialog_closes() {
    let _state = dialogs::tests::TEST_STATE.relock();
    show(NONE);
    assert_eq!(
        dialogs::check(OK),
        Err(CommandError::Unavailable),
        "none shows"
    );
    show(OK_DIALOG);
    assert_eq!(dialogs::check(OK), Ok(()));
    let cancel = Ask {
        answer: UiDialog::CANCEL,
        ..OK
    };
    assert_eq!(
        dialogs::check(cancel),
        Err(CommandError::BadArgs),
        "no Cancel"
    );
    let later = Ask { dialog: 5, ..OK };
    assert_eq!(
        dialogs::check(later),
        Err(CommandError::Unavailable),
        "another dialog shows"
    );
    let menu = Seen::menu(3, LoaderScreen::default());
    for (closes, ends) in [(true, Status::OBSERVED), (false, Status::UNAVAILABLE)] {
        show(OK_DIALOG);
        let mut commands = kept(Command {
            expected: None,
            ..command(Kind::Answer(OK))
        });
        assert!(run(&mut commands, None, Some(menu), 150, |_| {
            Applied::Taken
        }));
        assert_eq!(dialogs::take(), Some((commands.kept[0].id, OK)));
        dialogs::answered(commands.kept[0].id, closes);
        // The draw that answered leaves no dialog on screen.
        show(NONE);
        assert!(!run(&mut commands, None, Some(menu), 160, |_| {
            Applied::Taken
        }));
        assert_eq!(
            (commands.kept[0].state, commands.kept[0].reason),
            (ends, 0),
            "closes: {closes}"
        );
    }
}

#[test]
fn a_dialog_answer_runs_while_the_screen_reads_are_off() {
    let _state = dialogs::tests::TEST_STATE.relock();
    crate::engine::screens::READS_FUSE.blow("a test");
    show(OK_DIALOG);
    assert!(runs(Kind::Answer(OK)));
    assert!(!runs(Kind::Open(9)));
}
