//! The dialog answer command through the commands' frame.

use super::*;

#[test]
fn a_dialog_answer_is_handed_at_the_menu_and_ends_as_the_dialog_draw_tells() {
    use enshwick_core::abi::UiDialog;
    use std::sync::atomic::AtomicU32;

    static BUTTONS: AtomicU32 = AtomicU32::new(0);
    let _state = dialogs::tests::TEST_STATE.relock();
    dialogs::register(|| Some((BUTTONS.load(Ordering::Relaxed), None)));
    assert!(!dialogs::runs(), "no dialog shows");
    BUTTONS.store(1 << UiDialog::OK, Ordering::Relaxed);
    assert!(dialogs::runs());
    assert!(dialogs::takes(UiDialog::OK));
    assert!(
        !dialogs::takes(UiDialog::CANCEL),
        "the dialog has no Cancel"
    );
    let menu = Seen::menu(3, LoaderScreen::default());
    for (closes, ends) in [(true, Status::OBSERVED), (false, Status::UNAVAILABLE)] {
        let mut commands = kept(Command {
            expected: None,
            ..command(Kind::Answer(UiDialog::OK))
        });
        assert!(run(&mut commands, None, Some(menu), 150, |_| {
            Applied::Taken
        }));
        let (id, answer) = dialogs::take().expect("handed to the dialog draw");
        assert_eq!(answer, UiDialog::OK);
        dialogs::answered(id, closes);
        assert!(!run(&mut commands, None, Some(menu), 160, |_| {
            Applied::Taken
        }));
        assert_eq!(commands.kept[0].state, ends);
    }
    BUTTONS.store(0, Ordering::Relaxed);
}
