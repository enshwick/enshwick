//! What the frame lines need on their way out: a bounded queue that takes a
//! message whole or not at all, a count of what was dropped, and the check
//! that a line for one player reached no one else.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

/// Lines waiting to leave, at most a fixed number.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::Outbox;
///
/// let mut outbox = Outbox::new(3);
/// assert!(outbox.push_whole(["a", "b"]));
/// assert!(!outbox.push_whole(["c", "d"]), "a message that does not fit waits not at all");
/// assert_eq!(outbox.take(1).collect::<Vec<_>>(), ["a"]);
/// assert_eq!(outbox.len(), 1);
/// ```
#[derive(Debug)]
pub struct Outbox<T> {
    lines: VecDeque<T>,
    max: usize,
}

impl<T> Outbox<T> {
    /// An empty queue for at most `max` lines.
    ///
    /// # Examples
    ///
    /// See [`Outbox`].
    #[must_use]
    pub const fn new(max: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max,
        }
    }

    /// Queues all of `lines`, one message, or none of them when they do not
    /// fit; whether they were queued.
    ///
    /// # Examples
    ///
    /// See [`Outbox`].
    pub fn push_whole<I>(&mut self, lines: I) -> bool
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
    {
        let lines = lines.into_iter();
        if self.lines.len() + lines.len() > self.max {
            return false;
        }
        self.lines.extend(lines);
        true
    }

    /// Takes the oldest lines, at most `at_most` of them.
    ///
    /// # Examples
    ///
    /// See [`Outbox`].
    pub fn take(&mut self, at_most: usize) -> impl Iterator<Item = T> + '_ {
        let count = self.lines.len().min(at_most);
        self.lines.drain(..count)
    }

    /// The lines waiting.
    ///
    /// # Examples
    ///
    /// See [`Outbox`].
    #[must_use]
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether no line waits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::wire::Outbox;
    ///
    /// let mut outbox = Outbox::new(1);
    /// assert!(outbox.is_empty());
    /// assert!(outbox.push_whole(["a"]));
    /// outbox.clear();
    /// assert!(outbox.is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Drops every waiting line.
    ///
    /// # Examples
    ///
    /// See [`Outbox::is_empty`].
    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

/// A count of dropped lines or messages that says when one is worth a log
/// line: the first, and then once per hundred.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::Drops;
///
/// let drops = Drops::new();
/// assert_eq!(drops.add(1), Some(1), "the first is logged");
/// assert_eq!(drops.add(98), None);
/// assert_eq!(drops.add(3), Some(102), "the hundredth was passed");
/// assert_eq!(drops.total(), 102);
/// ```
#[derive(Debug, Default)]
pub struct Drops(AtomicU64);

impl Drops {
    /// A count at zero.
    ///
    /// # Examples
    ///
    /// See [`Drops`].
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Counts `n` more; the new total when it reaches the first drop or
    /// passes a hundredth, `None` otherwise.
    ///
    /// # Examples
    ///
    /// See [`Drops`].
    pub fn add(&self, n: u64) -> Option<u64> {
        let before = self.0.fetch_add(n, Ordering::Relaxed);
        let after = before.saturating_add(n);
        (n > 0 && (before == 0 || before / 100 != after / 100)).then_some(after)
    }

    /// Everything counted so far.
    ///
    /// # Examples
    ///
    /// See [`Drops`].
    #[must_use]
    pub fn total(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// The connection slots, as a mask, that a line posted for the slot
/// `target` reaches besides it.
///
/// Those are the `active` slots its node's `delivered` mask leaves unset,
/// or every active one when the node did not read (`None`).
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::strays;
///
/// let active = 0b1011;
/// assert_eq!(strays(active, 0, Some(0b1010)), 0, "marked for every other slot");
/// assert_eq!(strays(active, 0, Some(0b0010)), 0b1000);
/// assert_eq!(strays(active, 0, None), 0b1010, "an unread node reaches them all");
/// ```
#[must_use]
pub fn strays(active: u32, target: u32, delivered: Option<u32>) -> u32 {
    let others = active & !1u32.checked_shl(target).unwrap_or(0);
    others & !delivered.unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_waits_whole_or_not_at_all() {
        let mut outbox = Outbox::new(4);
        assert!(outbox.push_whole(["a", "b", "c"]));
        assert!(!outbox.push_whole(["d", "e"]));
        assert_eq!(outbox.len(), 3, "no part of the refused message waits");
        assert!(outbox.push_whole(["d"]));
        assert!(!outbox.push_whole(["e"]));
    }

    #[test]
    fn lines_leave_oldest_first_and_a_bounded_number_at_a_time() {
        let mut outbox = Outbox::new(64);
        assert!(outbox.push_whole(0..40));
        assert_eq!(
            outbox.take(16).collect::<Vec<_>>(),
            (0..16).collect::<Vec<_>>()
        );
        assert_eq!(outbox.len(), 24);
        assert_eq!(outbox.take(100).count(), 24);
        assert!(outbox.is_empty());
    }

    #[test]
    fn drops_are_logged_at_the_first_and_every_hundredth() {
        let drops = Drops::new();
        let logged: Vec<u64> = (0..250).filter_map(|_| drops.add(1)).collect();
        assert_eq!(logged, [1, 100, 200]);
        assert_eq!(drops.add(0), None, "nothing dropped, nothing logged");
        assert_eq!(drops.add(60), Some(310));
        assert_eq!(drops.total(), 310);
    }

    #[test]
    fn a_line_for_one_player_is_kept_only_when_every_other_live_slot_is_marked() {
        let active = 0b1_0110;
        assert_eq!(strays(active, 1, Some(0b1_0100)), 0);
        assert_eq!(
            strays(active, 1, Some(0b0_0100)),
            0b1_0000,
            "slot 4 still gets it"
        );
        assert_eq!(strays(active, 1, Some(0)), 0b1_0100);
        assert_eq!(strays(active, 1, None), 0b1_0100, "the node did not read");
        assert_eq!(
            strays(0b10, 1, None),
            0,
            "alone on the server, it strays nowhere"
        );
        assert_eq!(strays(active, 31, Some(0b0110)), 0b1_0000);
        assert_eq!(strays(active, 40, None), active, "no such slot");
    }
}
