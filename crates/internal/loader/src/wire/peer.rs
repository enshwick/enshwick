//! A peer's messages on their way in, and its rate.

use std::collections::HashMap;
use std::time::Instant;

use super::frame::Partial;
use super::{BURST, RATE};

/// A peer's messages on their way in, and its rate.
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
    /// Keyed by channel and message id as the peer sends them, so the map
    /// keeps std's keyed hasher.
    pub(crate) partial: HashMap<(u32, u16), Partial>,
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
            partial: HashMap::new(),
            tokens,
            seen: Instant::now(),
            language: None,
        }
    }

    /// Takes one message from the peer's rate; false when it is spent.
    ///
    /// # Examples
    ///
    /// See [`Peer`].
    pub fn admit(&mut self) -> bool {
        self.admit_at(Instant::now(), RATE, BURST)
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
}
