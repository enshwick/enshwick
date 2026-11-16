//! The local player's own values of the shown stat kinds, those with a
//! group of the Character screen: only the server works values out, so it
//! sends each player's client its character's values when they move and
//! when the client says hello, and the client's net keeps the last ones
//! for the screen's rows. A hosting client reaches its own player in
//! process, the same way.

use enshwick_core::abi::StatChange;
use enshwick_core::stats::key_of;

use super::{SEALED, STATE, Sealed, Watched, World, wanted};
use crate::base::wire::MAX_STAT_VALUES;
use enshwick_base::Relock;

/// Whether kind `index` shows on the Character screen.
pub(super) fn shown(sealed: &Sealed, index: usize) -> bool {
    sealed
        .defs
        .get(index)
        .is_some_and(|def| def.group.is_some() && !def.hidden)
}

/// Each shown kind's key and value in `values`, a watched entity's values
/// in kind order.
fn shown_values(sealed: &Sealed, values: &[f32]) -> Vec<(u32, f32)> {
    values
        .iter()
        .enumerate()
        .filter(|&(index, _)| shown(sealed, index))
        .map(|(index, &value)| (key_of(&sealed.kinds[index].id).0, value))
        .take(MAX_STAT_VALUES)
        .collect()
}

/// Sends each player whose shown values moved in `changes` all its shown
/// values, after the step that worked them out; a message its link had no
/// room for waits for the next change.
pub(super) fn send_changed(sealed: &Sealed, state: &World, changes: &[StatChange]) {
    let mut sent: Vec<u32> = Vec::new();
    for change in changes {
        let index = usize::try_from(change.stat.index).unwrap_or(usize::MAX);
        if sent.contains(&change.entity.0) || !shown(sealed, index) {
            continue;
        }
        sent.push(change.entity.0);
        if let Some(watched) = state.watched.get(&change.entity.0) {
            send(sealed, watched);
        }
    }
}

/// Sends a watched player's client its shown values.
fn send(sealed: &Sealed, watched: &Watched) {
    let Some(key) = watched.player else {
        return;
    };
    let values = shown_values(sealed, &watched.values);
    if !values.is_empty() {
        let _sent = crate::base::net::stat_values(key, &values);
    }
}

/// Sends the client of player `key`, which just said hello, its shown
/// values.
pub(crate) fn hello(key: u64) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    let world = STATE.world.relock();
    if let Some(watched) = world
        .as_ref()
        .and_then(|world| world.watched.values().find(|w| w.player == Some(key)))
    {
        send(sealed, watched);
    }
}

/// The local player's value of kind `index`, as the server last sent it,
/// else the kind's default; `None` for a kind that does not show. Any
/// thread, without a lock.
pub(crate) fn own_value(index: u32) -> Option<f32> {
    let sealed = SEALED.get()?;
    let index = usize::try_from(index).ok()?;
    if !shown(sealed, index) {
        return None;
    }
    let key = key_of(&sealed.kinds[index].id).0;
    Some(crate::base::net::own_stat(key).unwrap_or(sealed.defaults[index]))
}
