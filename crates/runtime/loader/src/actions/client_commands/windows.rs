//! Commands for the windows the server's mods show this client: press an
//! item, pick an option of its list or set its text, by the window's and
//! the item's ids.
//!
//! The ids are the mod's own: the window is `mod:window`, and an item is
//! named from the action ids it runs, as [`Target`] reads them. The carried
//! windows' draw, a layer above, takes the command in the interface frame,
//! acts on the window as a click or typing would, and answers whether the
//! window's own state shows it.

use enshwick_core::__private::rules;
pub(crate) use enshwick_core::__private::rules::WindowItem as Target;

use super::Progress;
use super::desk::Desk;

/// What a window command does to its item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Act {
    /// Presses a button or a switch, or flips an on-or-off field.
    Press,
    /// Picks an option of the item's list by its name.
    Pick,
    /// Sets the item's text.
    Text,
}

/// A window command as the carried draw takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ask {
    /// The id of the mod that shows the window.
    pub(crate) mod_id: String,
    /// The window's id among that mod's.
    pub(crate) window: String,
    pub(crate) target: Target,
    pub(crate) act: Act,
    /// The option to pick or the text to set; empty for a press.
    pub(crate) text: String,
}

impl Ask {
    /// The command on `window`, `mod:window`, and its item `item`, with
    /// `text`; `None` for ids no window can hold.
    pub(crate) fn new(window: &str, item: &str, act: Act, text: &str) -> Option<Self> {
        let (mod_id, window) = rules::window_name(window)?;
        let target = Target::parse(item)?;
        Some(Self {
            mod_id: mod_id.to_owned(),
            window: window.to_owned(),
            target,
            act,
            text: text.to_owned(),
        })
    }
}

/// How the carried draw answered a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The window's state shows it: the press clicked, the option is
    /// picked, the field reads the text.
    Shown,
    /// The window that shows holds no such item, or no such window shows.
    Missing,
    /// The window draws the item off, or it takes no such option or text.
    Off,
}

/// The draw answered [`Answer::Shown`].
const SHOWN: u8 = 2;
/// The draw answered [`Answer::Missing`].
const MISSING: u8 = 3;
/// The draw answered [`Answer::Off`].
const OFF: u8 = 4;

/// The window commands' state, one per process.
struct State {
    /// The asks, from their queue to the carried draw's answer.
    desk: Desk<Ask>,
    /// Whether the carried windows draw on this build, once the interface
    /// registered them.
    draws: std::sync::OnceLock<fn() -> bool>,
}

static STATE: State = State {
    desk: Desk::new(),
    draws: std::sync::OnceLock::new(),
};

/// Registers whether the carried windows draw, from the interface that
/// draws them, a layer above; a build without one runs no window command.
pub(crate) fn register(draws: fn() -> bool) {
    let _first = STATE.draws.set(draws);
}

/// Whether window commands run now.
pub(super) fn runs() -> bool {
    STATE.draws.get().is_some_and(|draws| draws())
}

/// Keeps command `id`'s ask until it is handed.
pub(super) fn park(id: u64, ask: Ask) {
    STATE.desk.park(id, ask);
}

/// Hands command `id` to the carried draw; false while another is handed.
pub(super) fn hand(id: u64) -> bool {
    STATE.desk.hand(id)
}

/// Where the draw stands with command `id`; `None` while it is not handed.
pub(super) fn progress(id: u64) -> Option<Progress> {
    Some(match STATE.desk.stage(id)? {
        SHOWN => Progress::Done,
        MISSING => Progress::Unavailable,
        OFF => Progress::Cancelled(enshwick_core::abi::ClientCommandStatus::ITEM_OFF),
        _ => Progress::Waiting,
    })
}

/// Forgets command `id`, handed or still parked.
pub(super) fn release(id: u64) {
    STATE.desk.release(id);
}

/// The command handed to the carried draw and its ask, while it waits for
/// an answer; one atomic load while none is.
pub(crate) fn asked() -> Option<(u64, Ask)> {
    STATE.desk.asked()
}

/// Claims command `id` for the carried draw before it acts on the window;
/// false once a cancel took it back, so a cancelled command never acts.
pub(crate) fn claim(id: u64) -> bool {
    STATE.desk.claim(id)
}

/// Gives claimed command `id` back to wait for the next frame, as when its
/// window did not draw or another click won the frame.
pub(crate) fn retry(id: u64) {
    STATE.desk.retry(id);
}

/// The carried draw's answer to claimed command `id`.
pub(crate) fn answer(id: u64, answer: Answer) {
    let stage = match answer {
        Answer::Shown => SHOWN,
        Answer::Missing => MISSING,
        Answer::Off => OFF,
    };
    STATE.desk.answer(id, stage);
}
#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use enshwick_base::Relock;

    use super::*;

    /// Held by every test that arms the window commands' process-wide
    /// state, since `cargo test` runs tests as threads of one process.
    pub(crate) static TEST_STATE: Mutex<()> = Mutex::new(());

    #[test]
    fn an_ask_names_its_window_by_its_mod_and_its_own_id() {
        let ask = Ask::new("you.windows-check:check", "ban#0", Act::Press, "");
        assert_eq!(
            ask.map(|a| (a.mod_id, a.window, a.target)),
            Some((
                "you.windows-check".into(),
                "check".into(),
                Target::RowButton("ban".into(), 0)
            ))
        );
        assert_eq!(Ask::new("check", "ban", Act::Press, ""), None, "no mod");
        assert_eq!(Ask::new(":check", "ban", Act::Press, ""), None);
        assert_eq!(Ask::new("m:Check", "ban", Act::Press, ""), None);
    }

    #[test]
    fn a_handed_command_carries_its_ask_until_the_draw_answers() {
        let _state = TEST_STATE.relock();
        let ask = Ask::new("m:w", "say/0", Act::Text, "hi").expect("a valid ask");
        park(41, ask.clone());
        assert_eq!(progress(41), None, "parked, not handed");
        assert!(hand(41));
        assert_eq!(asked(), Some((41, ask)));
        assert_eq!(progress(41), Some(Progress::Waiting));
        answer(41, Answer::Off);
        assert_eq!(progress(41), Some(Progress::Waiting), "answered unclaimed");
        assert!(claim(41));
        assert_eq!(asked(), None, "claimed");
        assert_eq!(progress(41), Some(Progress::Waiting));
        retry(41);
        assert!(asked().is_some(), "given back for the next frame");
        assert!(claim(41));
        answer(41, Answer::Off);
        assert_eq!(asked(), None, "answered");
        assert_eq!(
            progress(41),
            Some(Progress::Cancelled(
                enshwick_core::abi::ClientCommandStatus::ITEM_OFF
            ))
        );
        release(41);
        assert_eq!(progress(41), None);
        park(
            42,
            Ask::new("m:w", "@close", Act::Press, "").expect("a valid ask"),
        );
        release(42);
        assert!(hand(42), "a released handoff takes the next");
        assert_eq!(asked().map(|(_, ask)| ask), None, "its parked ask went");
        release(42);
    }

    #[test]
    fn a_command_cancelled_after_the_draw_read_it_cannot_be_claimed() {
        let _state = TEST_STATE.relock();
        park(
            43,
            Ask::new("m:w", "ban#0", Act::Press, "").expect("a valid ask"),
        );
        assert!(hand(43));
        let read = asked();
        assert!(read.is_some(), "the draw read the ask");
        release(43);
        assert!(!claim(43), "the cancel won: the draw acts on nothing");
        answer(43, Answer::Shown);
        assert_eq!(progress(43), None, "nothing comes back to life");
    }
}
