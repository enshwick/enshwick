//! The stat values the server sent this client for its player's own
//! character, by stat key: each message holds every value, so it replaces
//! the last, and they go when the client's link goes or its game ends.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::STATE;

/// Keeps the finite values of a stat values message.
pub(super) fn received(values: impl Iterator<Item = (u32, f32)>) {
    let kept: FxHashMap<u32, f32> = values.filter(|(_, value)| value.is_finite()).collect();
    STATE.own_stats.store(Some(Arc::new(kept)));
}

/// The value of the stat with key `key` the server last sent; any thread,
/// without a lock.
pub(crate) fn own_stat(key: u32) -> Option<f32> {
    STATE
        .own_stats
        .load()
        .as_ref()
        .and_then(|values| values.get(&key).copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_message_replaces_the_values_kept_before_and_a_lost_link_drops_them() {
        received([(1, 2.0), (2, 3.0)].into_iter());
        received([(2, 4.0), (3, f32::NAN)].into_iter());
        assert_eq!(own_stat(1), None, "replaced, not merged");
        assert_eq!(own_stat(2), Some(4.0));
        assert_eq!(own_stat(3), None, "not finite");
        super::super::link_lost();
        assert_eq!(own_stat(2), None, "the server that sent them is gone");
    }
}
