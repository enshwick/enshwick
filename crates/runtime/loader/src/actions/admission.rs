//! The one admission of a player's action run, whichever way it comes:
//! every way passes the same layers in the same order.
//!
//! 1. The rate of its [`Path`]: a player's runs from the interface, a
//!    window or chat share one [`RUN_RATE`]; an administrator, the server's
//!    console and a remote caller are not rated.
//! 2. The peer bounds of `enshwick_base::peer`.
//! 3. The mod's own rights and arguments, read once, then its handler.
//! 4. One reply path, [`crate::actions::reply`].
//!
//! Every refusal answers the caller with its reason, once per flood.

use std::time::Instant;

use enshwick_base::peer::{Admitted, RUN_RATE, Rated};
use enshwick_core::abi::ActionCall;

/// The way a run reaches a mod.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Path {
    /// A player's client: a control, a panel or a window's button.
    Interface,
    /// A command a player typed in chat.
    Chat,
    /// A line on the server's console.
    Console,
    /// A mod's remote console, through `run_command`; that mod keeps its
    /// own login and rate.
    Remote,
}

impl Path {
    /// The question the mod is asked for a run from this path.
    #[must_use]
    pub const fn kind(self) -> u32 {
        match self {
            Self::Interface => ActionCall::RUN,
            Self::Chat => ActionCall::CHAT,
            Self::Console => ActionCall::CONSOLE,
            Self::Remote => ActionCall::REMOTE,
        }
    }

    /// Whether its runs spend the caller's [`RUN_RATE`]: a player's do; the
    /// server's console and a remote caller are trusted.
    #[must_use]
    pub const fn is_rated(self) -> bool {
        matches!(self, Self::Interface | Self::Chat)
    }
}

/// Why the platform refused a run before the mod's handler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The caller is over [`RUN_RATE`].
    Rate,
    /// The run's arguments are out of the peer bounds, which only a changed
    /// client sends.
    Bounds,
    /// The runs waiting for the end of the step fill their queue.
    Busy,
    /// The object the run names stands out of the caller's reach.
    Reach,
    /// No running mod with actions has the run's mod id.
    Unavailable,
}

impl Refusal {
    /// The loader's text that answers the caller.
    #[must_use]
    pub const fn text_key(self) -> &'static str {
        match self {
            Self::Rate | Self::Busy => "remote.slow",
            Self::Bounds => "actions.bad-request",
            Self::Reach => "actions.out-of-reach",
            Self::Unavailable => "actions.unavailable",
        }
    }

    /// The reason the mod that owns the run hears, an
    /// `ActionCall::REFUSED_` value; `None` when no running mod owns it.
    #[must_use]
    pub const fn abi_reason(self) -> Option<u32> {
        match self {
            Self::Rate => Some(ActionCall::REFUSED_RATE),
            Self::Bounds => Some(ActionCall::REFUSED_BOUNDS),
            Self::Busy => Some(ActionCall::REFUSED_BUSY),
            Self::Reach => Some(ActionCall::REFUSED_REACH),
            Self::Unavailable => None,
        }
    }
}

/// What the admission makes of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Let it through to the next layer.
    Run,
    /// Refuse it and answer the caller.
    Refuse(Refusal),
    /// Refuse it unanswered: the rest of a flood whose first run was
    /// answered.
    Drop,
}

/// Every player's rate of runs.
#[derive(Debug)]
pub struct Runs {
    rated: Rated,
}

impl Default for Runs {
    fn default() -> Self {
        Self::new()
    }
}

impl Runs {
    /// No player seen yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rated: Rated::new(RUN_RATE),
        }
    }

    /// Admits a run of the player of account `key` from `path` at `now`;
    /// `is_admin` is asked only of a run over the rate.
    pub fn admit(
        &mut self,
        path: Path,
        key: u64,
        now: Instant,
        is_admin: impl FnOnce() -> bool,
    ) -> Verdict {
        if !path.is_rated() {
            return Verdict::Run;
        }
        match self.rated.admit(key, now) {
            Admitted::Yes => Verdict::Run,
            _ if is_admin() => Verdict::Run,
            Admitted::First => Verdict::Refuse(Refusal::Rate),
            Admitted::Again => Verdict::Drop,
        }
    }

    /// Forgets every player, as when a hosted game ends.
    pub fn clear(&mut self) {
        self.rated.clear();
    }
}

pub use enshwick_base::peer::args_in_bounds;

#[cfg(windows)]
pub(crate) use live::{admit, forget, refuse};

#[cfg(windows)]
mod live {
    use std::sync::Mutex;
    use std::time::Instant;

    use enshwick_base::Relock;

    use super::{Path, Refusal, Runs, Verdict};
    use crate::engine::loader::Loader;

    static RUNS: Mutex<Runs> = Mutex::new(Runs::new());

    /// Admits a run of the player of account `key` from `path` now.
    pub(crate) fn admit(loader: &Loader, path: Path, key: u64) -> Verdict {
        let is_admin =
            || crate::actions::player_actions::player(loader, key).is_some_and(|p| p.is_admin());
        RUNS.relock().admit(path, key, Instant::now(), is_admin)
    }

    /// Answers the player of account `key` the run from `path` refused for
    /// `refusal`.
    pub(crate) fn refuse(loader: &Loader, path: Path, key: u64, refusal: Refusal) {
        let text = crate::engine::texts::for_player(key, || {
            crate::engine::texts::text(refusal.text_key())
        });
        crate::actions::reply::answer(loader, path, key, false, text);
    }

    /// Forgets every player's rate, when a hosted game ends.
    pub(crate) fn forget() {
        RUNS.relock().clear();
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_player_s_interface_and_chat_runs_share_one_rate() {
        let mut runs = Runs::new();
        let now = Instant::now();
        let burst = (0..100)
            .take_while(|i| {
                let path = if i % 2 == 0 {
                    Path::Interface
                } else {
                    Path::Chat
                };
                runs.admit(path, 7, now, || false) == Verdict::Run
            })
            .count();
        assert_eq!(f64::from(u32::try_from(burst).unwrap()), RUN_RATE.burst);
    }

    #[test]
    fn the_first_run_over_the_rate_is_answered_and_the_rest_dropped() {
        let mut runs = Runs::new();
        let now = Instant::now();
        while runs.admit(Path::Interface, 7, now, || false) == Verdict::Run {}
        assert_eq!(
            runs.admit(Path::Chat, 7, now, || false),
            Verdict::Drop,
            "the run that ended the burst was the answered one"
        );
        let later = now + Duration::from_secs(1);
        assert_eq!(runs.admit(Path::Chat, 7, later, || false), Verdict::Run);
    }

    #[test]
    fn an_administrator_the_console_and_a_remote_caller_are_never_rated() {
        let mut runs = Runs::new();
        let now = Instant::now();
        assert!((0..1000).all(|_| runs.admit(Path::Interface, 7, now, || true) == Verdict::Run));
        let mut asked = false;
        for path in [Path::Console, Path::Remote] {
            assert!((0..1000).all(|_| {
                runs.admit(path, 0, now, || {
                    asked = true;
                    false
                }) == Verdict::Run
            }));
        }
        assert!(!asked, "no player lookup for a run that is not rated");
    }

    #[test]
    fn a_refusal_of_a_running_mod_s_run_names_its_own_reason_to_the_mod() {
        let reasons: Vec<Option<u32>> = [
            Refusal::Rate,
            Refusal::Bounds,
            Refusal::Busy,
            Refusal::Reach,
        ]
        .map(Refusal::abi_reason)
        .into();
        assert_eq!(
            reasons,
            [
                Some(ActionCall::REFUSED_RATE),
                Some(ActionCall::REFUSED_BOUNDS),
                Some(ActionCall::REFUSED_BUSY),
                Some(ActionCall::REFUSED_REACH),
            ]
        );
        assert_eq!(
            Refusal::Unavailable.abi_reason(),
            None,
            "no running mod owns the run"
        );
    }

    #[cfg(windows)]
    #[test]
    fn every_refusal_has_a_text_of_the_loader() {
        for refusal in [
            Refusal::Rate,
            Refusal::Bounds,
            Refusal::Busy,
            Refusal::Reach,
            Refusal::Unavailable,
        ] {
            let key = refusal.text_key();
            assert_ne!(crate::engine::texts::text(key), key, "{refusal:?}");
        }
    }

    #[test]
    fn arguments_over_the_bound_or_with_a_control_character_are_out_of_bounds() {
        let max = enshwick_base::peer::MAX_ARGS;
        assert!(args_in_bounds("tBan for griefing\tb1"));
        assert!(args_in_bounds(&"t".repeat(max)));
        assert!(!args_in_bounds(&"t".repeat(max + 1)));
        assert!(!args_in_bounds("tline\nbreak"));
        assert!(!args_in_bounds("tbell\u{7}"));
    }
}
