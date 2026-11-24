//! The command that answers the loader's dialog on screen, a mod's or the
//! loader's own: the interface's dialog draw, a layer above, takes the
//! answer in its frame and answers the dialog as the button would.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{CommandError, Handoff, Progress};

/// A handed answer waits for the dialog draw.
const WAITING: u8 = 1;
/// The draw claimed the answer and gives it in this frame.
const ACTING: u8 = 2;
/// The dialog closed with the answer.
const ANSWERED: u8 = 3;
/// The dialog it named no longer showed, or had no button for it.
const MISSING: u8 = 4;

/// The highest number a dialog takes; numbers count round from 1, and 0
/// names none. Bounded so [`Shown`] fits one word; a command waits far less
/// than 65,535 dialogs take to show.
pub(crate) const MAX_NUMBER: u32 = 65_535;

/// The dialog on screen as the interface tells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shown {
    /// Its number, 1 to [`MAX_NUMBER`], 0 while none shows.
    pub(crate) number: u32,
    /// Its buttons, a bit per answer.
    pub(crate) buttons: u32,
    /// The load-order index of its mod, `None` for the loader's own.
    pub(crate) owner: Option<usize>,
}

impl Shown {
    /// The word the interface stores it in, so a reader on any thread sees
    /// one dialog whole: the number in the low 16 bits, the buttons in the
    /// next 16, its mod's index plus 1 above them, 0 for the loader's own.
    pub(crate) fn to_word(self) -> u64 {
        let owner = self
            .owner
            .and_then(|i| u64::try_from(i).ok())
            .map_or(0, |i| i + 1);
        u64::from(self.number & 0xffff) | u64::from(self.buttons & 0xffff) << 16 | owner << 32
    }

    /// The dialog stored in `word`, as [`Shown::to_word`] wrote it.
    pub(crate) fn from_word(word: u64) -> Self {
        Self {
            number: u32::try_from(word & 0xffff).unwrap_or(0),
            buttons: u32::try_from(word >> 16 & 0xffff).unwrap_or(0),
            owner: (word >> 32)
                .checked_sub(1)
                .and_then(|i| usize::try_from(i).ok()),
        }
    }
}

/// An answer for the dialog of one number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    pub(crate) answer: u32,
    /// The dialog's number, as the snapshot gave it.
    pub(crate) dialog: u32,
}

impl Ask {
    /// Whether it answers the dialog `shown` by a button it has.
    pub(crate) fn fits(self, shown: Shown) -> bool {
        shown.number != 0
            && shown.number == self.dialog
            && 1u32
                .checked_shl(self.answer)
                .is_some_and(|bit| shown.buttons & bit != 0)
    }

    fn to_word(self) -> u64 {
        u64::from(self.answer) | u64::from(self.dialog) << 32
    }

    fn from_word(word: u64) -> Self {
        Self {
            answer: u32::try_from(word & u64::from(u32::MAX)).unwrap_or(0),
            dialog: u32::try_from(word >> 32).unwrap_or(0),
        }
    }
}

/// The dialog command's state, one per process.
struct State {
    /// The command handed to the dialog draw.
    handoff: Handoff,
    /// The handed command's [`Ask`], stored before its stage shows.
    ask: AtomicU64,
    /// The dialog on screen, `None` while dialogs do not show in game, once
    /// the interface registered.
    shown: OnceLock<fn() -> Option<Shown>>,
}

static STATE: State = State {
    handoff: Handoff::new(),
    ask: AtomicU64::new(0),
    shown: OnceLock::new(),
};

/// Registers what tells the dialog on screen, from the interface that draws
/// the dialogs, a layer above: `None` while they do not show in game.
pub(crate) fn register(shown: fn() -> Option<Shown>) {
    let _first = STATE.shown.set(shown);
}

/// The dialog on screen, numbered 0 while none shows; `None` while dialogs
/// do not show in game.
pub(crate) fn shown() -> Option<Shown> {
    STATE.shown.get().and_then(|shown| shown())
}

/// Whether dialogs show in game, for an answer to go to: one that waits
/// keeps waiting while its dialog closes.
pub(super) fn runs() -> bool {
    shown().is_some()
}

/// Whether `ask` can be queued: the dialog it names shows and has its
/// button.
pub(super) fn check(ask: Ask) -> Result<(), CommandError> {
    let shown = shown()
        .filter(|shown| shown.number != 0 && shown.number == ask.dialog)
        .ok_or(CommandError::Unavailable)?;
    if ask.fits(shown) {
        Ok(())
    } else {
        Err(CommandError::BadArgs)
    }
}

/// Hands command `id`'s `ask` to the dialog draw; false while another is
/// handed.
pub(super) fn hand(id: u64, ask: Ask) -> bool {
    STATE.handoff.arm(id, WAITING, || {
        STATE.ask.store(ask.to_word(), Ordering::Release);
    })
}

/// Where the draw stands with command `id`; `None` while it is not handed.
pub(super) fn progress(id: u64) -> Option<Progress> {
    Some(match STATE.handoff.stage(id)? {
        ANSWERED => Progress::Done,
        MISSING => Progress::Unavailable,
        _ => Progress::Waiting,
    })
}

/// Forgets command `id`.
pub(super) fn release(id: u64) {
    let _released = STATE.handoff.release(id);
}

/// Claims the answer handed to the dialog draw: the command's id and its
/// ask, once, so a cancel after it cannot win; one atomic load while none
/// waits.
pub(crate) fn take() -> Option<(u64, Ask)> {
    let (id, WAITING) = STATE.handoff.current()? else {
        return None;
    };
    let ask = Ask::from_word(STATE.ask.load(Ordering::Acquire));
    STATE
        .handoff
        .advance(id, WAITING, ACTING)
        .then_some((id, ask))
}

/// The draw's outcome for claimed command `id`: whether the dialog closed
/// with its answer.
pub(crate) fn answered(id: u64, done: bool) {
    let stage = if done { ANSWERED } else { MISSING };
    let _moved = STATE.handoff.advance(id, ACTING, stage);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use enshwick_base::Relock;
    use enshwick_core::abi::UiDialog;

    /// Held by every test that arms the dialog command's process-wide
    /// handoff, since `cargo test` runs tests as threads of one process.
    pub(crate) static TEST_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const OK: Ask = Ask {
        answer: UiDialog::OK,
        dialog: 7,
    };

    #[test]
    fn a_handed_answer_is_claimed_once_and_ends_as_the_draw_tells() {
        let _state = TEST_STATE.relock();
        assert!(hand(51, OK));
        assert_eq!(progress(51), Some(Progress::Waiting));
        assert_eq!(take(), Some((51, OK)));
        assert_eq!(take(), None, "claimed once");
        answered(51, true);
        assert_eq!(progress(51), Some(Progress::Done));
        release(51);
        assert!(hand(52, OK));
        release(52);
        assert_eq!(take(), None, "a cancel before the draw leaves nothing");
        assert!(hand(53, OK));
        assert_eq!(take(), Some((53, OK)));
        answered(53, false);
        assert_eq!(progress(53), Some(Progress::Unavailable));
        release(53);
    }

    #[test]
    fn an_answer_fits_only_the_dialog_it_names_by_a_button_it_has() {
        let shown = |number, buttons| Shown {
            number,
            buttons,
            owner: None,
        };
        let ok_cancel = 1 << UiDialog::OK | 1 << UiDialog::CANCEL;
        assert!(OK.fits(shown(7, ok_cancel)));
        assert!(
            !OK.fits(shown(8, ok_cancel)),
            "a dialog that showed after the snapshot"
        );
        assert!(!OK.fits(shown(7, 1 << UiDialog::CANCEL)), "no OK button");
        assert!(!OK.fits(shown(0, 0)), "none shows");
        let closed = Ask {
            answer: UiDialog::CLOSED,
            ..OK
        };
        assert!(!closed.fits(shown(7, u32::MAX)), "no button past bit 31");
    }

    #[test]
    fn a_dialog_on_screen_reads_back_whole_from_its_word() {
        for shown in [
            Shown {
                number: MAX_NUMBER,
                buttons: UiDialog::BUTTONS,
                owner: Some(3),
            },
            Shown {
                number: 1,
                buttons: 1 << UiDialog::OK,
                owner: None,
            },
            Shown {
                number: 0,
                buttons: 0,
                owner: None,
            },
        ] {
            assert_eq!(Shown::from_word(shown.to_word()), shown);
        }
        assert_eq!(Ask::from_word(OK.to_word()), OK);
    }
}
