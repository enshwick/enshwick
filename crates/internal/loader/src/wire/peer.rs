//! A peer's rate, language and join data.

use std::time::{Duration, Instant};

use super::{BURST, RATE};

/// How long after a peer got the data a hello of its brings it again.
///
/// A client says hello every few seconds until the answer arrives. A hello
/// later than this comes from a client that joined again, whose leave the
/// server never saw, or from one whose answer was lost; both take the data
/// again, which the client applies twice to the same result. A client that joins
/// again sooner than this gets nothing new.
pub const DATA_AGAIN: Duration = Duration::from_secs(15);

/// A peer's rate, language and join data.
///
/// # Examples
///
/// ```
/// use enshwick_loader::wire::Peer;
///
/// let mut peer = Peer::new();
/// assert!(peer.admit());
/// ```
#[derive(Debug)]
pub struct Peer {
    /// Messages the peer may still send now.
    pub(crate) tokens: f64,
    seen: Instant,
    /// The language the peer's player reads the game in, once its hello
    /// named one.
    #[cfg_attr(
        not(windows),
        expect(dead_code, reason = "the server reads it only in the game")
    )]
    pub(crate) language: Option<enshwick::i18n::Language>,
    /// When the peer last got the data a hello brings.
    answered: Option<Instant>,
}

impl Default for Peer {
    fn default() -> Self {
        Self::new()
    }
}

impl Peer {
    /// A peer with a full burst, as one that just said hello.
    ///
    /// # Examples
    ///
    /// See [`Peer`].
    #[must_use]
    pub fn new() -> Self {
        Self::with_tokens(BURST)
    }

    /// A peer that may send `tokens` messages now.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::wire::Peer;
    ///
    /// assert!(!Peer::with_tokens(0.0).admit());
    /// ```
    #[must_use]
    pub fn with_tokens(tokens: f64) -> Self {
        Self {
            tokens,
            seen: Instant::now(),
            language: None,
            answered: None,
        }
    }

    /// Whether the peer's hello at `now` brings the data a client gets on
    /// joining: until it was handed over whole, and again once
    /// [`DATA_AGAIN`] has passed since then.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::Instant;
    /// use enshwick_loader::wire::{DATA_AGAIN, Peer};
    ///
    /// let mut peer = Peer::new();
    /// let now = Instant::now();
    /// assert!(peer.data_due(now));
    /// peer.handed_data(now);
    /// assert!(!peer.data_due(now));
    /// assert!(peer.data_due(now + DATA_AGAIN));
    /// ```
    #[must_use]
    pub fn data_due(&self, now: Instant) -> bool {
        self.answered
            .is_none_or(|at| now.saturating_duration_since(at) >= DATA_AGAIN)
    }

    /// Notes that the peer's data was handed over whole at `now`.
    ///
    /// # Examples
    ///
    /// See [`Peer::data_due`].
    pub fn handed_data(&mut self, now: Instant) {
        self.answered = Some(now);
    }

    /// Takes one message from the peer's rate; false when it is spent.
    ///
    /// # Examples
    ///
    /// See [`Peer`].
    pub fn admit(&mut self) -> bool {
        self.admit_at(Instant::now(), RATE, BURST)
    }

    /// Gives back the message [`Peer::admit`] took, for one that did not
    /// go, up to the burst.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::wire::Peer;
    ///
    /// let mut peer = Peer::with_tokens(1.0);
    /// assert!(peer.admit());
    /// peer.refund();
    /// assert!(peer.admit());
    /// ```
    pub fn refund(&mut self) {
        self.tokens = (self.tokens + 1.0).min(BURST);
    }

    /// Takes one from a rate of `rate` a second up to `burst` at `now`;
    /// false when it is spent.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::time::{Duration, Instant};
    /// use enshwick_loader::wire::Peer;
    ///
    /// let mut peer = Peer::with_tokens(0.0);
    /// let later = Instant::now() + Duration::from_secs(1);
    /// assert!(peer.admit_at(later, 1.0, 1.0));
    /// ```
    pub fn admit_at(&mut self, now: Instant, rate: f64, burst: f64) -> bool {
        let gained = now.saturating_duration_since(self.seen).as_secs_f64() * rate;
        self.seen = now;
        self.tokens = (self.tokens + gained).min(burst);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_peer_s_rate_runs_out_after_its_burst() {
        let mut peer = Peer::new();
        let admitted = (0..100).filter(|_| peer.admit()).count();
        assert!((40..=42).contains(&admitted), "{admitted}");
    }

    #[test]
    fn a_hello_long_after_the_last_answer_brings_the_data_again() {
        let mut peer = Peer::new();
        let start = Instant::now();
        assert!(peer.data_due(start), "the first hello gets the data");
        assert!(
            peer.data_due(start + Duration::from_secs(5)),
            "until the data was handed over whole, every hello brings it"
        );
        peer.handed_data(start + Duration::from_secs(5));
        assert!(
            !peer.data_due(start + Duration::from_secs(10)),
            "a hello sent before the answer arrived gets none again"
        );
        assert!(
            peer.data_due(start + Duration::from_secs(5) + DATA_AGAIN),
            "a client that joined again without a leave gets it"
        );
    }
}
