//! Holds back the game's own "could not join" dialog after a join the check refused.
//
// A refused join goes on without its host, so the game's online provider
// fails it and starts the game's user interaction for a failed join, its
// dialog (client builds 410 and 426). The loader's link on the start of user
// interactions answers that one start with 0, as the game itself does when it
// cannot start one; the provider then goes back to the menu without a dialog.

use core::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The build table's id of the game's start of a user interaction.
pub const START_ID: &str = "user.start_interaction";
/// The build table's id of its first eight bytes.
pub const BYTES_ID: &str = "user.start_interaction.bytes";
/// The build table's id of where an interaction request holds its kind.
pub const KIND_ID: &str = "user.interaction_kind";
/// The build table's id of the kind that tells the player a join failed.
pub const JOIN_FAILED_ID: &str = "user.join_failed_kind";
/// How long after a refusal the game's dialog is held back at most: the
/// provider fails the join within a few frames.
const HOLD: Duration = Duration::from_secs(5);

/// Whether the next "could not join" interaction is held back, until when.
///
/// # Examples
///
/// ```
/// use enshwick::join::interaction::Hold;
///
/// let hold = Hold::new();
/// hold.arm(1_000);
/// assert!(hold.takes(1_500));
/// assert!(!hold.takes(1_600), "one dialog per refusal");
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
    /// use enshwick::join::interaction::Hold;
    ///
    /// assert!(!Hold::new().armed());
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
    /// use enshwick::join::interaction::Hold;
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// assert!(hold.armed());
    /// ```
    pub fn arm(&self, now: u64) {
        let hold = u64::try_from(HOLD.as_millis()).unwrap_or(u64::MAX);
        self.until
            .store(now.saturating_add(hold).max(1), Ordering::Release);
    }

    /// Drops the hold, so the next failed join shows the game's dialog.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::Hold;
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// hold.release();
    /// assert!(!hold.takes(1));
    /// ```
    pub fn release(&self) {
        self.until.store(0, Ordering::Release);
    }

    /// Starts a join from the game's menus: `refused` arms the hold for its
    /// dialog, any other join ends a hold left by an earlier refusal. Every
    /// menu join passes the loader's join check, so no hold outlives the join
    /// it was armed for.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::Hold;
    ///
    /// let hold = Hold::new();
    /// hold.join(true, 0);
    /// hold.join(false, 100);
    /// assert!(!hold.armed());
    /// ```
    pub fn join(&self, refused: bool, now: u64) {
        if refused {
            self.arm(now);
        } else {
            self.release();
        }
    }

    /// Whether a hold is armed, expired or not: the cheap test before the
    /// clock is read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::Hold;
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// assert!(hold.armed());
    /// ```
    #[must_use]
    pub fn armed(&self) -> bool {
        self.until.load(Ordering::Acquire) != 0
    }

    /// Whether the "could not join" interaction starting at `now` is held
    /// back; a hold takes one interaction.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::join::interaction::Hold;
    ///
    /// let hold = Hold::new();
    /// hold.arm(0);
    /// assert!(hold.takes(10));
    /// assert!(!hold.armed());
    /// ```
    #[must_use]
    pub fn takes(&self, now: u64) -> bool {
        let until = self.until.load(Ordering::Acquire);
        if until == 0 {
            return false;
        }
        let taken = self
            .until
            .compare_exchange(until, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        taken && now <= until
    }
}

/// Why the hold stays off: what in the game did not match the build table.
///
/// # Examples
///
/// ```
/// use enshwick::join::interaction::Error;
///
/// assert!(Error::Missing("user.start_interaction").to_string().contains("no `user"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The build table lacks an entry the hold needs.
    #[error("the build table has no `{0}`")]
    Missing(&'static str),
    /// The pinned start lies outside the game's code.
    #[error("the start {start:#x} lies outside the game's code")]
    OutsideCode {
        /// The start's address.
        start: u64,
    },
    /// The start's first bytes are not the pinned ones.
    #[error("the code at the start {start:#x} is {found:#018x}, not the pinned {expected:#018x}")]
    Bytes {
        /// The start's address.
        start: u64,
        /// The bytes found there.
        found: u64,
        /// The bytes the build table pins.
        expected: u64,
    },
}

/// Checks that `start`, the pinned start of user interactions, lies in
/// `code` and begins with the `expected` bytes, read by `read`.
///
/// # Errors
///
/// An [`Error`] naming what did not match.
///
/// # Examples
///
/// ```
/// use enshwick::join::interaction::verify;
///
/// let read = |va| (va == 0x4000).then_some(0x5540_u64);
/// assert_eq!(verify(Some(0x4000), Some(0x5540), read, 0x1000..0x8000), Ok(0x4000));
/// assert!(verify(Some(0x9000), Some(0x5540), read, 0x1000..0x8000).is_err());
/// ```
pub fn verify(
    start: Option<u64>,
    expected: Option<u64>,
    read: impl Fn(u64) -> Option<u64>,
    code: Range<u64>,
) -> Result<u64, Error> {
    let start = start.ok_or(Error::Missing(START_ID))?;
    let expected = expected.ok_or(Error::Missing(BYTES_ID))?;
    let inside = start
        .checked_add(8)
        .is_some_and(|end| code.start <= start && end <= code.end);
    if !inside {
        return Err(Error::OutsideCode { start });
    }
    let found = read(start).ok_or(Error::OutsideCode { start })?;
    if found != expected {
        return Err(Error::Bytes {
            start,
            found,
            expected,
        });
    }
    Ok(start)
}

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour, joining, set_layout};

#[cfg(windows)]
mod runtime {
    use std::sync::OnceLock;
    use std::time::Instant;

    use crate::chains::Next;

    /// The game's start of a user interaction: the user system and the
    /// request, whose kind is a byte in it; the interaction's id, 0 when none
    /// started (client builds 410 and 426).
    pub(crate) type StartFn = unsafe extern "system" fn(users: *mut u8, request: *const u8) -> u32;

    // SAFETY: the slot joins the chain of the start, typed `StartFn`.
    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: Next<StartFn> = unsafe { Next::new() };

    /// Where a request holds its kind, and the kind of a failed join.
    #[derive(Debug, Clone, Copy)]
    struct Layout {
        kind: usize,
        join_failed: u8,
    }

    /// The hold's state: the build's layout, set before the link goes in,
    /// the hold, and the clock it counts on.
    struct State {
        layout: OnceLock<Layout>,
        hold: super::Hold,
        epoch: OnceLock<Instant>,
    }

    static STATE: State = State {
        layout: OnceLock::new(),
        hold: super::Hold::new(),
        epoch: OnceLock::new(),
    };

    fn now() -> u64 {
        let elapsed = STATE.epoch.get_or_init(Instant::now).elapsed().as_millis();
        u64::try_from(elapsed).unwrap_or(u64::MAX)
    }

    /// Sets the build's request layout, from its table; false when the
    /// table pins no kind, so the hold stays off.
    pub(crate) fn set_layout(kind: Option<u64>, join_failed: Option<u64>) -> bool {
        let layout = kind
            .and_then(|kind| usize::try_from(kind).ok())
            .zip(join_failed.and_then(|code| u8::try_from(code).ok()))
            .map(|(kind, join_failed)| Layout { kind, join_failed });
        layout.is_some_and(|layout| STATE.layout.set(layout).is_ok())
    }

    /// A join from the game's menus starts, `refused` by the join check.
    pub(crate) fn joining(refused: bool) {
        STATE.hold.join(refused, now());
    }

    /// The loader's link on the start of user interactions.
    pub(crate) unsafe extern "system" fn detour(users: *mut u8, request: *const u8) -> u32 {
        if STATE.hold.armed()
            && !request.is_null()
            && let Some(layout) = STATE.layout.get()
            // SAFETY: the game's interaction request holds its kind byte at
            // the table's `user.interaction_kind` (client builds 410 and 426).
            && unsafe { crate::world::read::<u8>(request.addr(), layout.kind) } == layout.join_failed
            && held()
        {
            return 0;
        }
        // SAFETY: forwarded unchanged.
        unsafe { NEXT.get()(users, request) }
    }

    /// Whether the held "could not join" dialog is the one to hold back.
    #[cold]
    fn held() -> bool {
        std::panic::catch_unwind(|| {
            let taken = STATE.hold.takes(now());
            if taken {
                crate::log!(INFO, "join refused: the game's own dialog held back");
            }
            taken
        })
        .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_holds_back_one_dialog() {
        let hold = Hold::new();
        assert!(!hold.takes(5), "nothing is held unarmed");
        hold.arm(100);
        assert!(hold.takes(150));
        assert!(!hold.takes(160));
    }

    #[test]
    fn a_hold_runs_out_so_a_later_failed_join_shows_its_dialog() {
        let hold = Hold::new();
        hold.arm(100);
        let late = 100 + u64::try_from(HOLD.as_millis()).unwrap() + 1;
        assert!(!hold.takes(late));
        assert!(!hold.armed(), "the expired hold is gone");
    }

    #[test]
    fn the_hold_ends_once_it_held_back_its_dialog() {
        let hold = Hold::new();
        hold.join(true, 100);
        assert!(hold.takes(120));
        assert!(!hold.armed());
        assert!(!hold.takes(130), "a second failed join shows its dialog");
    }

    #[test]
    fn a_hold_no_dialog_took_never_reaches_the_next_join() {
        let hold = Hold::new();
        // The game started no dialog for the refused join.
        hold.join(true, 100);
        // The next menu join, which the check lets through, fails at once.
        hold.join(false, 200);
        assert!(!hold.takes(210), "the unrelated join's dialog shows");
    }

    #[test]
    fn a_second_refusal_holds_back_its_own_dialog() {
        let hold = Hold::new();
        hold.join(true, 100);
        assert!(hold.takes(110));
        hold.join(true, 300);
        assert!(hold.takes(310));
    }

    #[test]
    fn a_refusal_at_the_first_millisecond_still_holds() {
        let hold = Hold::new();
        hold.arm(0);
        assert!(hold.takes(0));
    }

    #[test]
    fn the_start_is_hooked_only_inside_the_code_with_its_pinned_bytes() {
        let code = 0x1000..0x8000;
        let read = |va| (va == 0x4000).then_some(0x5540_u64);
        assert_eq!(
            verify(Some(0x4000), Some(0x5540), read, code.clone()),
            Ok(0x4000)
        );
        assert_eq!(
            verify(Some(0x4000), Some(0x1111), read, code.clone()),
            Err(Error::Bytes {
                start: 0x4000,
                found: 0x5540,
                expected: 0x1111
            })
        );
        assert_eq!(
            verify(Some(0x7ffc), Some(0x5540), read, code.clone()),
            Err(Error::OutsideCode { start: 0x7ffc })
        );
        assert_eq!(
            verify(None, Some(0x5540), read, code.clone()),
            Err(Error::Missing(START_ID))
        );
        assert_eq!(
            verify(Some(0x4000), None, read, code),
            Err(Error::Missing(BYTES_ID))
        );
    }
}
