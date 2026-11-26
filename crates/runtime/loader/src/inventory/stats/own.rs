//! The local player's own values of the shown stat kinds, those with a
//! group of the Character screen: only the server works values out, so it
//! sends each player's client its character's values when they move and
//! when the client says hello, and the client's net keeps the last ones
//! for the screen's rows. A hosting client reaches its own player in
//! process, the same way.

use enshwick_core::abi::StatChange;
use enshwick_core::stats::key_of;

use super::{SEALED, STATE, Sealed, Watched, World, wanted};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// Whether kind `index` shows on the Character screen.
pub(super) fn shown(sealed: &Sealed, index: usize) -> bool {
    sealed
        .defs
        .get(index)
        .is_some_and(|def| def.group.is_some() && !def.hidden)
        && crate::engine::loader::loader().is_none_or(|loader| runs(loader, sealed, index))
}

/// Whether the owner of kind `index` runs: a kind of a mod the loader
/// turned off, a mod of side both on a server without it or one whose fuse
/// blew, shows nowhere. Only atomic loads.
pub(super) fn runs(loader: &Loader, sealed: &Sealed, index: usize) -> bool {
    sealed
        .kinds
        .get(index)
        .and_then(|kind| kind.def.as_ref())
        .is_some_and(|&(_, _, owner)| !loader.passed(ModIndex::new(owner), false))
}

/// Each shown kind's key and value in `values`, a watched entity's values
/// in kind order.
fn shown_values(sealed: &Sealed, values: &[f32]) -> Vec<(u32, f32)> {
    values
        .iter()
        .enumerate()
        .filter(|&(index, _)| shown(sealed, index))
        .map(|(index, &value)| (key_of(&sealed.kinds[index].id).0, value))
        .collect()
}

/// Sends each player whose shown values moved in `changes`, and each one
/// they were owed to, all its shown values, after the step that worked
/// them out; a message its link had no room for is owed to the next step.
pub(super) fn send_changed(sealed: &Sealed, state: &mut World, changes: &[StatChange]) {
    send_due(sealed, state, changes, |watched| send(sealed, watched));
}

/// [`send_changed`] through `send`.
pub(super) fn send_due(
    sealed: &Sealed,
    state: &mut World,
    changes: &[StatChange],
    mut send: impl FnMut(&Watched) -> Sent,
) {
    let mut due = core::mem::take(&mut state.values_owed);
    for change in changes {
        let index = usize::try_from(change.stat.index).unwrap_or(usize::MAX);
        if !due.contains(&change.entity.get()) && shown(sealed, index) {
            due.push(change.entity.get());
        }
    }
    for entity in due {
        let owed = state
            .watched
            .get(&entity)
            .is_some_and(|watched| send(watched) == Sent::NoRoom);
        if owed {
            state.values_owed.push(entity);
        }
    }
}

/// How a message to a player's client went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sent {
    /// It went, or there was nothing to send.
    Done,
    /// The client has not said hello: its hello asks for the values.
    NotReady,
    /// Its link had no room.
    NoRoom,
}

/// Sends a watched player's client its shown values.
fn send(sealed: &Sealed, watched: &Watched) -> Sent {
    let Some(key) = watched.player else {
        return Sent::Done;
    };
    let values = shown_values(sealed, &watched.values);
    if values.is_empty() {
        Sent::Done
    } else if !crate::base::net::ready(key) {
        Sent::NotReady
    } else if crate::base::net::stat_values(key, &values) {
        Sent::Done
    } else {
        Sent::NoRoom
    }
}

/// Sends the client of player `key`, which just said hello, its shown
/// values.
pub(crate) fn hello(key: u64) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    let mut world = STATE.world.relock();
    let Some(world) = world.as_mut() else {
        return;
    };
    let owed = world
        .watched
        .iter()
        .find(|(_, w)| w.player == Some(key))
        .and_then(|(&entity, watched)| (send(sealed, watched) == Sent::NoRoom).then_some(entity));
    if let Some(entity) = owed.filter(|entity| !world.values_owed.contains(entity)) {
        world.values_owed.push(entity);
    }
}

/// The local player's value of kind `index`, whose stat key is `key`, as
/// the server last sent it, else the kind's default; `None` for a kind that
/// does not show. Any thread, without a lock.
pub(crate) fn own_value(index: u32, key: u32) -> Option<f32> {
    let sealed = SEALED.get()?;
    let index = usize::try_from(index).ok()?;
    if !shown(sealed, index) {
        return None;
    }
    Some(crate::base::net::own_stat(key).unwrap_or(sealed.defaults[index]))
}
