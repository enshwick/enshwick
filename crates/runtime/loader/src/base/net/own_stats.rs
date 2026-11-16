//! The stat values the server sent this client for its player's own
//! character, by stat key: each message holds every value, so it replaces
//! the last, and they go when the client leaves the server.

use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use rustc_hash::FxHashMap;

use crate::base::wire::MAX_STAT_VALUES;

static OWN: OnceLock<ArcSwap<FxHashMap<u32, f32>>> = OnceLock::new();

fn own() -> &'static ArcSwap<FxHashMap<u32, f32>> {
    OWN.get_or_init(ArcSwap::default)
}

/// Keeps the values of a stat values message, the finite ones, at most
/// [`MAX_STAT_VALUES`].
pub(super) fn received(values: impl Iterator<Item = (u32, f32)>) {
    let kept: FxHashMap<u32, f32> = values
        .filter(|(_, value)| value.is_finite())
        .take(MAX_STAT_VALUES)
        .collect();
    own().store(Arc::new(kept));
}

/// Forgets the values of the server the client left.
pub(super) fn forget() {
    own().store(Arc::default());
}

/// The value of the stat with key `key` the server last sent; any thread,
/// without a lock.
pub(crate) fn own_stat(key: u32) -> Option<f32> {
    own().load().get(&key).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_message_replaces_the_values_kept_before_and_drops_what_is_not_finite() {
        received([(1, 2.0), (2, 3.0)].into_iter());
        received([(2, 4.0), (3, f32::NAN)].into_iter());
        assert_eq!(own_stat(1), None, "replaced, not merged");
        assert_eq!(own_stat(2), Some(4.0));
        assert_eq!(own_stat(3), None, "not finite");
        forget();
        assert_eq!(own_stat(2), None);
    }
}
