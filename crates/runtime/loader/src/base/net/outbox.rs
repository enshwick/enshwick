//! The messages for a client that said hello but holds no welcome yet.
//!
//! The client drops every message that comes before the server's welcome,
//! so the server keeps them per client, in order and bounded, and sends
//! them once its welcome went; until then the client is known, not
//! welcomed. A link that starts over makes its client unwelcomed again
//! until its next hello.

use std::collections::VecDeque;

use super::Links;
use crate::base::wire::Peer;

/// Messages held for one client, at most.
pub(super) const MAX_HELD: usize = 64;
/// Bytes held for one client, at most.
pub(super) const MAX_HELD_BYTES: usize = 64 * 1024;

/// What waits for one client's welcome.
#[derive(Debug, Default)]
pub(super) struct Outbox {
    messages: VecDeque<(u32, Vec<u8>)>,
    bytes: usize,
    /// The welcome went: what is left waits only for room on the link.
    welcomed: bool,
    /// The client is owed the join data once the outbox is empty.
    data: bool,
}

/// Where a message for one client goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hold {
    /// No client of that key said hello.
    Unknown,
    /// The client is welcomed and nothing waits before it: send it now.
    Ready,
    /// Kept for the client's welcome.
    Held,
    /// The client's outbox has no room; the message is dropped.
    Full,
}

/// Keeps `data` on `channel` for client `key` while it waits for its
/// welcome, or says where it goes instead.
pub(super) fn hold(state: &mut Links, key: u64, channel: u32, data: &[u8]) -> Hold {
    if !state.peers.contains_key(&key) {
        return Hold::Unknown;
    }
    let Some(outbox) = state.unwelcomed.get_mut(&key) else {
        return Hold::Ready;
    };
    if outbox.messages.len() >= MAX_HELD || outbox.bytes + data.len() > MAX_HELD_BYTES {
        return Hold::Full;
    }
    outbox.bytes += data.len();
    outbox.messages.push_back((channel, data.to_vec()));
    Hold::Held
}

/// Makes client `key`, known, wait for its next welcome: at its first
/// hello, and when its link starts over, which also gives it a fresh rate
/// and makes its join data due again, as for a new link.
pub(super) fn unwelcome(state: &mut Links, key: u64) {
    let Some(peer) = state.peers.get_mut(&key) else {
        return;
    };
    let language = peer.language;
    *peer = Peer::new();
    peer.language = language;
    state.sending.remove(&key);
    state.unwelcomed.entry(key).or_default().welcomed = false;
}

/// Notes that client `key`'s welcome went, owing it the join data when
/// `data`; what it holds goes next.
pub(super) fn welcomed(state: &mut Links, key: u64, data: bool) {
    if !state.peers.contains_key(&key) {
        return;
    }
    let outbox = state.unwelcomed.entry(key).or_default();
    outbox.welcomed = true;
    outbox.data |= data;
}

/// Whether client `key`'s last welcome went: false between a link's start
/// over and its next welcome, and before the first.
pub(super) fn welcome_went(state: &Links, key: u64) -> bool {
    state
        .unwelcomed
        .get(&key)
        .is_none_or(|outbox| outbox.welcomed)
}

/// Forgets client `key` when the server kept out the hello on its new link
/// while it waited for a welcome there, as a client that never said hello.
pub(super) fn refused(state: &mut Links, key: u64) {
    if state.peers.contains_key(&key) && !welcome_went(state, key) {
        state.peers.remove(&key);
        state.sending.remove(&key);
        state.unwelcomed.remove(&key);
    }
}

/// What [`take`] found for one client.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Taken {
    /// Nothing to do: no outbox, or no welcome went yet.
    Nothing,
    /// These go now, in order; what fails goes back with [`put_back`].
    Messages(VecDeque<(u32, Vec<u8>)>),
    /// The outbox is empty and gone: the client is ready, owed the join
    /// data when `data`.
    Ready {
        /// The join data is due.
        data: bool,
    },
}

/// Takes what client `key` holds once its welcome went.
pub(super) fn take(state: &mut Links, key: u64) -> Taken {
    let Some(outbox) = state.unwelcomed.get_mut(&key).filter(|o| o.welcomed) else {
        return Taken::Nothing;
    };
    if outbox.messages.is_empty() {
        let data = outbox.data;
        state.unwelcomed.remove(&key);
        return Taken::Ready { data };
    }
    outbox.bytes = 0;
    Taken::Messages(core::mem::take(&mut outbox.messages))
}

/// Puts the messages `rest` that did not go back before what client `key`
/// got held since, within the outbox's bounds: how many of the newest it
/// dropped to stay there.
pub(super) fn put_back(state: &mut Links, key: u64, mut rest: VecDeque<(u32, Vec<u8>)>) -> usize {
    let Some(outbox) = state.unwelcomed.get_mut(&key) else {
        return 0;
    };
    rest.append(&mut outbox.messages);
    let mut bytes = 0;
    let kept = rest
        .iter()
        .take(MAX_HELD)
        .take_while(|(_, data)| {
            bytes += data.len();
            bytes <= MAX_HELD_BYTES
        })
        .count();
    let dropped = rest.len() - kept;
    rest.truncate(kept);
    outbox.bytes = rest.iter().map(|(_, data)| data.len()).sum();
    outbox.messages = rest;
    dropped
}

/// The clients whose welcome went and whose outbox still waits for room,
/// among `live`.
pub(super) fn stalled(state: &Links, live: &[u64]) -> Vec<u64> {
    state
        .unwelcomed
        .iter()
        .filter(|(key, outbox)| outbox.welcomed && live.contains(key))
        .map(|(&key, _)| key)
        .collect()
}
