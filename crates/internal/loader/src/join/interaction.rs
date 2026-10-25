//! Holds back the game's own "could not join" dialog after a join the check
//! refused, so the player reads only the loader's.
//!
//! A refused join goes on without its host, so the game's online provider
//! fails it and starts the game's user interaction for a failed join, its
//! dialog (client builds 410 and 426). The loader's link on the start of user
//! interactions answers that one start with 0, as the game itself does when it
//! cannot start one; the provider then goes back to the menu without a dialog.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The build table's id of the game's start of a user interaction.
pub const START_ID: &str = "user.start_interaction";
/// The build table's id of its first eight bytes.
pub const BYTES_ID: &str = "user.start_interaction.bytes";
/// Where an interaction request holds its kind (client builds 410 and 426).
pub const KIND: usize = 0x8;
/// The kind of the interaction that tells the player a join failed (client
/// builds 410 and 426).
pub const JOIN_FAILED: u8 = 10;
/// How long after a refusal the game's dialog is held back: the provider
/// fails the join within a few frames.
const HOLD: Duration = Duration::from_secs(10);

/// Whether the next "could not join" interaction is held back, until when.
///
/// # Examples
///
/// ```
/// use enshwick::join::interaction::{Hold, JOIN_FAILED};
///
/// let hold = Hold::new();
/// hold.arm(1_000);
/// assert!(hold.takes(JOIN_FAILED, 1_500));
/// assert!(!hold.takes(JOIN_FAILED, 1_600), "one dialog per refusal");
/// ```
#[derive(Debug, Default)]
pub struct Hold {
    /// The last millisecond the hold lasts; 0 when none is armed.
    until: AtomicU64,
}

impl Hold {
    /// A hold that holds nothing back.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::{Hold, JOIN_FAILED};
    ///
    /// assert!(!Hold::new().takes(JOIN_FAILED, 1));
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            until: AtomicU64::new(0),
        }
    }

    /// Holds back the next "could not join" interaction that starts within
    /// [`HOLD`] of `now`, in milliseconds.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::{Hold, JOIN_FAILED};
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// assert!(hold.takes(JOIN_FAILED, 10));
    /// ```
    pub fn arm(&self, now: u64) {
        let hold = u64::try_from(HOLD.as_millis()).unwrap_or(u64::MAX);
        self.until
            .store(now.saturating_add(hold).max(1), Ordering::Release);
    }

    /// Whether the interaction of `kind` starting at `now` is held back; a
    /// hold takes one interaction.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::{Hold, JOIN_FAILED};
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// assert!(!hold.takes(JOIN_FAILED + 1, 10), "other dialogs show");
    /// assert!(hold.takes(JOIN_FAILED, 10));
    /// ```
    #[must_use]
    pub fn takes(&self, kind: u8, now: u64) -> bool {
        if kind != JOIN_FAILED {
            return false;
        }
        let until = self.until.load(Ordering::Acquire);
        if until == 0 {
            return false;
        }
        if now > until {
            let _expired =
                self.until
                    .compare_exchange(until, 0, Ordering::AcqRel, Ordering::Acquire);
            return false;
        }
        self.until
            .compare_exchange(until, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour, hold};

#[cfg(windows)]
mod runtime {
    use std::sync::OnceLock;
    use std::time::Instant;

    use crate::chains::Next;

    /// The game's start of a user interaction: the user system and the
    /// request, whose kind is the byte at [`super::KIND`]; the interaction's
    /// id, 0 when none started (client builds 410 and 426).
    pub(crate) type StartFn = unsafe extern "system" fn(users: *mut u8, request: *const u8) -> u32;

    // SAFETY: the slot joins the chain of the start, typed `StartFn`.
    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: Next<StartFn> = unsafe { Next::new() };

    static HOLD: super::Hold = super::Hold::new();
    static EPOCH: OnceLock<Instant> = OnceLock::new();

    fn now() -> u64 {
        let elapsed = EPOCH.get_or_init(Instant::now).elapsed().as_millis();
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }

    /// Holds back the game's dialog for the join being refused now.
    pub(crate) fn hold() {
        HOLD.arm(now());
    }

    /// The loader's link on the start of user interactions.
    pub(crate) unsafe extern "system" fn detour(users: *mut u8, request: *const u8) -> u32 {
        if !request.is_null() {
            // SAFETY: the game's interaction request holds its kind byte at
            // `KIND` (client builds 410 and 426).
            let kind = unsafe { request.wrapping_add(super::KIND).read() };
            if HOLD.takes(kind, now()) {
                crate::log!(INFO, "join refused: the game's own dialog held back");
                return 0;
            }
        }
        // SAFETY: forwarded unchanged.
        unsafe { NEXT.get()(users, request) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_holds_back_one_join_failed_dialog() {
        let hold = Hold::new();
        assert!(!hold.takes(JOIN_FAILED, 5), "nothing is held unarmed");
        hold.arm(100);
        assert!(!hold.takes(JOIN_FAILED - 1, 150));
        assert!(hold.takes(JOIN_FAILED, 150));
        assert!(!hold.takes(JOIN_FAILED, 160));
    }

    #[test]
    fn a_hold_runs_out_so_a_later_failed_join_shows_its_dialog() {
        let hold = Hold::new();
        hold.arm(100);
        let late = 100 + u64::try_from(HOLD.as_millis()).unwrap() + 1;
        assert!(!hold.takes(JOIN_FAILED, late));
        assert!(!hold.takes(JOIN_FAILED, 150), "the expired hold is gone");
    }

    #[test]
    fn a_refusal_at_the_first_millisecond_still_holds() {
        let hold = Hold::new();
        hold.arm(0);
        assert!(hold.takes(JOIN_FAILED, 0));
    }
}
