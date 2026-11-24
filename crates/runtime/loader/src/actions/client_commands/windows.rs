//! Commands for the windows the server's mods show this client: press an
//! item, pick an option of its list or set its text, by the window's and
//! the item's ids.
//!
//! The ids are the mod's own: the window is `mod:window`, and an item is
//! named from the action ids it runs, as [`Target`] reads them. The carried
//! windows' draw, a layer above, takes the command in the interface frame,
//! acts on the window as a click or typing would, and answers whether the
//! window's own state shows it.

use std::sync::Mutex;

use enshwick_base::Relock;
use enshwick_core::__private::rules;
pub(crate) use enshwick_core::__private::rules::WindowItem as Target;

use super::{Handoff, Progress};

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

/// A handed command waits for the carried draw.
const WAITING: u8 = 1;
/// The draw answered [`Answer::Shown`].
const SHOWN: u8 = 2;
/// The draw answered [`Answer::Missing`].
const MISSING: u8 = 3;
/// The draw answered [`Answer::Off`].
const OFF: u8 = 4;

/// The window commands' state, one per process.
struct State {
    /// The command handed to the carried draw.
    handoff: Handoff,
    /// The handed command's ask, filled before its stage shows.
    handed: Mutex<Option<Ask>>,
    /// The asks of queued commands not yet handed, by command id.
    parked: Mutex<Vec<(u64, Ask)>>,
    /// Whether the carried windows draw on this build, once the interface
    /// registered them.
    draws: std::sync::OnceLock<fn() -> bool>,
}

static STATE: State = State {
    handoff: Handoff::new(),
    handed: Mutex::new(None),
    parked: Mutex::new(Vec::new()),
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
    STATE.parked.relock().push((id, ask));
}

/// Hands command `id` to the carried draw; false while another is handed.
pub(super) fn hand(id: u64) -> bool {
    STATE.handoff.arm(id, WAITING, || {
        let mut parked = STATE.parked.relock();
        let ask = parked
            .iter()
            .position(|(parked, _)| *parked == id)
            .map(|at| parked.swap_remove(at).1);
        *STATE.handed.relock() = ask;
    })
}

/// Where the draw stands with command `id`; `None` while it is not handed.
pub(super) fn progress(id: u64) -> Option<Progress> {
    Some(match STATE.handoff.stage(id)? {
        SHOWN => Progress::Done,
        MISSING => Progress::Unavailable,
        OFF => Progress::Cancelled(enshwick_core::abi::ClientCommandStatus::ITEM_OFF),
        _ => Progress::Waiting,
    })
}

/// Forgets command `id`, handed or still parked.
pub(super) fn release(id: u64) {
    STATE.parked.relock().retain(|(parked, _)| *parked != id);
    if STATE.handoff.release(id).is_some() {
        *STATE.handed.relock() = None;
    }
}

/// The command handed to the carried draw and its ask, while it waits for
/// an answer; one atomic load while none is.
pub(crate) fn asked() -> Option<(u64, Ask)> {
    let (id, WAITING) = STATE.handoff.current()? else {
        return None;
    };
    let ask = STATE.handed.relock().clone()?;
    Some((id, ask))
}

/// The carried draw's answer to command `id`.
pub(crate) fn answer(id: u64, answer: Answer) {
    let stage = match answer {
        Answer::Shown => SHOWN,
        Answer::Missing => MISSING,
        Answer::Off => OFF,
    };
    let _moved = STATE.handoff.advance(id, WAITING, stage);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let ask = Ask::new("m:w", "say/0", Act::Text, "hi").expect("a valid ask");
        park(41, ask.clone());
        assert_eq!(progress(41), None, "parked, not handed");
        assert!(hand(41));
        assert_eq!(asked(), Some((41, ask)));
        assert_eq!(progress(41), Some(Progress::Waiting));
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
}
