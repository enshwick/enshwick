//! A peer's messages on their way in, and its rate.

use std::collections::HashMap;
use std::time::Instant;

use super::frame::Partial;
use super::{BURST, RATE};

/// A peer's messages on their way in, and its rate.
#[derive(Debug)]
pub(super) struct Peer {
    /// Keyed by channel and message id as the peer sends them, so the map
    /// keeps std's keyed hasher.
    pub(super) partial: HashMap<(u32, u16), Partial>,
    pub(super) tokens: f64,
    seen: Instant,
}

impl Peer {
    pub(super) fn new() -> Self {
        Self::with_tokens(BURST)
    }

    pub(super) fn with_tokens(tokens: f64) -> Self {
        Self {
            partial: HashMap::new(),
            tokens,
            seen: Instant::now(),
        }
    }

    /// Takes one message from the peer's rate; false when it is spent.
    pub(super) fn admit(&mut self) -> bool {
        self.admit_at(Instant::now(), RATE, BURST)
    }

    /// Takes one from a rate of `rate` a second up to `burst` at `now`;
    /// false when it is spent.
    pub(super) fn admit_at(&mut self, now: Instant, rate: f64, burst: f64) -> bool {
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
