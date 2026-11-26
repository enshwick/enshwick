//! The loader's one millisecond clock, which its timers, holds and ages read,
//! so two of them always agree on what "now" is.

use std::sync::OnceLock;
use std::time::Instant;

/// The clock's state.
struct State {
    /// When the clock was first read.
    epoch: OnceLock<Instant>,
}

static STATE: State = State {
    epoch: OnceLock::new(),
};

/// Milliseconds since the clock was first read, plus one, so 0 can stand for
/// "never" in an atomic; monotonic.
pub(crate) fn now_ms() -> u64 {
    let epoch = STATE.epoch.get_or_init(Instant::now);
    u64::try_from(epoch.elapsed().as_millis())
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_never_reads_zero_and_never_goes_back() {
        let first = now_ms();
        let second = now_ms();
        assert_ne!(first, 0);
        assert!(second >= first);
    }
}
