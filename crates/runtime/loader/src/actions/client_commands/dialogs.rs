//! The command that answers the loader's dialog on screen, a mod's or the
//! loader's own: the interface's dialog draw, a layer above, takes the
//! answer in its frame and answers the dialog as the button would.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{Handoff, Progress};

/// A handed answer waits for the dialog draw.
const WAITING: u8 = 1;
/// The draw claimed the answer and gives it in this frame.
const ACTING: u8 = 2;
/// The dialog closed with the answer.
const ANSWERED: u8 = 3;
/// No dialog showed, or the one that showed had no button for it.
const MISSING: u8 = 4;

/// The dialog on screen as the interface tells it: its buttons, a bit per
/// answer, and the load-order index of its mod, `None` for the loader's own.
pub(crate) type Shown = (u32, Option<usize>);

/// The dialog command's state, one per process.
struct State {
    /// The command handed to the dialog draw.
    handoff: Handoff,
    /// The handed command's answer, stored before its stage shows.
    answer: AtomicU32,
    /// The dialog on screen, `None` while dialogs do not show in game, once
    /// the interface registered.
    shown: OnceLock<fn() -> Option<Shown>>,
}

static STATE: State = State {
    handoff: Handoff::new(),
    answer: AtomicU32::new(0),
    shown: OnceLock::new(),
};

/// Registers what tells the dialog on screen, from the interface that draws
/// the dialogs, a layer above: `None` while they do not show in game, else
/// the buttons of the one on screen, 0 for none.
pub(crate) fn register(shown: fn() -> Option<Shown>) {
    let _first = STATE.shown.set(shown);
}

/// The dialog on screen; its buttons are 0 for none, and `None` while
/// dialogs do not show in game.
pub(crate) fn shown() -> Option<Shown> {
    STATE.shown.get().and_then(|shown| shown())
}

/// Whether a dialog shows in game for an answer to go to.
pub(super) fn runs() -> bool {
    shown().is_some_and(|(buttons, _)| buttons != 0)
}

/// Whether the dialog on screen has a button for `answer`, or none shows
/// to tell.
pub(super) fn takes(answer: u32) -> bool {
    shown().is_none_or(|(buttons, _)| buttons == 0 || buttons & 1 << answer != 0)
}

/// Hands command `id`'s `answer` to the dialog draw; false while another
/// is handed.
pub(super) fn hand(id: u64, answer: u32) -> bool {
    STATE.handoff.arm(id, WAITING, || {
        STATE.answer.store(answer, Ordering::Release)
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
/// answer, once, so a cancel after it cannot win; one atomic load while
/// none waits.
pub(crate) fn take() -> Option<(u64, u32)> {
    let (id, WAITING) = STATE.handoff.current()? else {
        return None;
    };
    let answer = STATE.answer.load(Ordering::Acquire);
    STATE
        .handoff
        .advance(id, WAITING, ACTING)
        .then_some((id, answer))
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

    /// Held by every test that arms the dialog command's process-wide
    /// handoff, since `cargo test` runs tests as threads of one process.
    pub(crate) static TEST_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn a_handed_answer_is_claimed_once_and_ends_as_the_draw_tells() {
        let _state = TEST_STATE.relock();
        assert!(hand(51, 7));
        assert_eq!(progress(51), Some(Progress::Waiting));
        assert_eq!(take(), Some((51, 7)));
        assert_eq!(take(), None, "claimed once");
        answered(51, true);
        assert_eq!(progress(51), Some(Progress::Done));
        release(51);
        assert!(hand(52, 6));
        release(52);
        assert_eq!(take(), None, "a cancel before the draw leaves nothing");
        assert!(hand(53, 6));
        assert_eq!(take(), Some((53, 6)));
        answered(53, false);
        assert_eq!(progress(53), Some(Progress::Unavailable));
        release(53);
    }
}
