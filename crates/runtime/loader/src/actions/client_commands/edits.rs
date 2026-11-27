//! Commands for the loader's own Mods screen: bind a key to a mod's
//! control, or set a mod's setting, on that mod's page, as the page's own
//! key capture and field do.
//!
//! The Mods screen's draw, a layer above, takes the command in the
//! interface frame while it shows the mod's page, makes the change through
//! the page's own paths, so the loader writes `mod-controls.toml` or the
//! world's settings file as a click would, and answers whether it went
//! through.

use super::Progress;
use super::desk::Desk;
use enshwick_core::abi::ClientCommandStatus as Status;

/// Which change a command asks of the Mods screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    /// Binds a key to a mod's control.
    Rebind,
    /// Sets a mod's setting.
    Setting,
}

/// A change as the Mods screen's draw takes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Ask {
    /// Binds `bits`, 0 for none, in key `slot` of mod `mod_id`'s control
    /// `control`, by the name the mod declared.
    Rebind {
        mod_id: String,
        control: String,
        slot: usize,
        bits: u32,
    },
    /// Sets mod `mod_id`'s setting `key` to the TOML `value`.
    Setting {
        mod_id: String,
        key: String,
        value: String,
    },
}

impl Ask {
    /// The mod whose page takes the change.
    pub(crate) fn mod_id(&self) -> &str {
        match self {
            Self::Rebind { mod_id, .. } | Self::Setting { mod_id, .. } => mod_id,
        }
    }
}

/// How the Mods screen's draw answered a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The change went through: the file's write is queued.
    Done,
    /// The screen shows another page, or the page has no such control or
    /// setting.
    Missing,
    /// The page refused the change: a value the schema or the mod refuses,
    /// a setting shown read-only, or a file that did not write.
    Refused,
}

/// The draw answered [`Answer::Done`].
const DONE: u8 = 2;
/// The draw answered [`Answer::Missing`].
const MISSING: u8 = 3;
/// The draw answered [`Answer::Refused`].
const REFUSED: u8 = 4;

/// The changes, from their queue to the draw's answer, one per process.
static DESK: Desk<Ask> = Desk::new();

/// Keeps command `id`'s change until it is handed.
pub(super) fn park(id: u64, ask: Ask) {
    DESK.park(id, ask);
}

/// Hands command `id` to the Mods screen's draw; false while another is
/// handed.
pub(super) fn hand(id: u64) -> bool {
    DESK.hand(id)
}

/// Where the draw stands with command `id`; `None` while it is not handed.
pub(super) fn progress(id: u64) -> Option<Progress> {
    Some(match DESK.stage(id)? {
        DONE => Progress::Done,
        MISSING => Progress::Unavailable,
        REFUSED => Progress::Cancelled(Status::CHANGE_REFUSED),
        _ => Progress::Waiting,
    })
}

/// Forgets command `id`, handed or still parked.
pub(super) fn release(id: u64) {
    DESK.release(id);
}

/// The command handed to the Mods screen's draw and its change, while it
/// waits for an answer; one atomic load while none is.
pub(crate) fn asked() -> Option<(u64, Ask)> {
    DESK.asked()
}

/// Claims command `id` for the draw before it makes the change; false once
/// a cancel took it back, so a cancelled command changes nothing.
pub(crate) fn claim(id: u64) -> bool {
    DESK.claim(id)
}

/// The draw's answer to claimed command `id`.
pub(crate) fn answer(id: u64, answer: Answer) {
    DESK.answer(
        id,
        match answer {
            Answer::Done => DONE,
            Answer::Missing => MISSING,
            Answer::Refused => REFUSED,
        },
    );
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Held by every test that arms the changes' process-wide state, since
    /// `cargo test` runs tests as threads of one process.
    pub(crate) static TEST_STATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn setting(value: &str) -> Ask {
        Ask::Setting {
            mod_id: "you.lights".to_owned(),
            key: "range".to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn a_handed_change_reaches_the_draw_and_its_answer_ends_it() {
        use enshwick_base::Relock;
        let _state = TEST_STATE.relock();
        park(61, setting("12"));
        assert_eq!(progress(61), None, "parked, not handed");
        assert!(hand(61));
        assert_eq!(asked(), Some((61, setting("12"))));
        assert!(claim(61));
        assert_eq!(asked(), None, "claimed");
        answer(61, Answer::Refused);
        assert_eq!(
            progress(61),
            Some(Progress::Cancelled(Status::CHANGE_REFUSED))
        );
        release(61);
        park(62, setting("13"));
        assert!(hand(62), "a released desk takes the next");
        assert!(claim(62));
        answer(62, Answer::Done);
        assert_eq!(progress(62), Some(Progress::Done));
        release(62);
        assert_eq!(progress(62), None);
    }

    #[test]
    fn a_change_cancelled_before_the_draw_claims_it_never_applies() {
        use enshwick_base::Relock;
        let _state = TEST_STATE.relock();
        park(63, setting("1"));
        assert!(hand(63));
        assert!(asked().is_some(), "the draw read it");
        release(63);
        assert!(!claim(63), "the cancel won");
        answer(63, Answer::Done);
        assert_eq!(progress(63), None, "nothing comes back to life");
    }

    #[test]
    fn a_change_names_the_mod_whose_page_takes_it() {
        let rebind = Ask::Rebind {
            mod_id: "you.lights".to_owned(),
            control: "toggle".to_owned(),
            slot: 1,
            bits: 0,
        };
        assert_eq!(rebind.mod_id(), "you.lights");
        assert_eq!(setting("1").mod_id(), "you.lights");
    }
}
