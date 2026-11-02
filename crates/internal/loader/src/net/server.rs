//! The server's side: messages from clients, their hellos and the join data they are owed.

use std::time::Instant;

use super::{State, deliver, dropped, item_data, remember_language, state};
use crate::wire::{CONTROL, Peer, WELCOME, control, read_hello, remote_message};

/// Tries to hand a client its join data, one a second, before the server
/// stops and leaves the queue to other players.
pub(super) const MAX_HANDOVERS: u8 = 5;
/// Clients whose hello is on its way in, at most, on the server.
pub(super) const MAX_GREETING: usize = 64;

/// What a hello the server answers brings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Greeting {
    /// The client is a new peer.
    pub(super) fresh: bool,
    /// The client gets the data a joining client gets: it is new, or it
    /// joined again without the server seeing it leave.
    pub(super) data: bool,
}

/// Makes `key` a peer after its hello at `now`.
pub(super) fn greet(state: &mut State, key: u64, now: Instant) -> Greeting {
    let pending = state.greeting.remove(&key);
    let (peer, fresh) = match state.peers.entry(key) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            (entry.insert(pending.unwrap_or_default()), true)
        }
        std::collections::hash_map::Entry::Occupied(entry) => (entry.into_mut(), false),
    };
    Greeting {
        fresh,
        data: peer.data_due(now),
    }
}

/// Takes a message on `channel` from the client `key` on the server: whether
/// `key` is a peer, or `None` when the message is not taken. A client that
/// is no peer yet may only send control messages, its hello; each message
/// takes from the client's rate.
pub(super) fn take_message(state: &mut State, key: u64, channel: u32) -> Option<bool> {
    if key == 0 {
        return None;
    }
    let known = state.peers.contains_key(&key);
    let peer = if known {
        state.peers.get_mut(&key)?
    } else {
        if channel != CONTROL {
            return None;
        }
        let room = state.greeting.len() < MAX_GREETING;
        match state.greeting.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) if room => entry.insert(Peer::new()),
            std::collections::hash_map::Entry::Vacant(_) => return None,
        }
    };
    if !peer.admit() {
        dropped();
        return None;
    }
    Some(known)
}

/// Makes room for the hello of the client `key` in a full store by
/// forgetting the clients whose key holds no live slot in `live`, read only
/// then; nothing is forgotten when the slots do not read.
pub(super) fn make_greeting_room(
    state: &mut State,
    key: u64,
    live: impl FnOnce() -> Option<Vec<u64>>,
) {
    if !state.peers.contains_key(&key)
        && !state.greeting.contains_key(&key)
        && state.greeting.len() >= MAX_GREETING
        && let Some(live) = live()
    {
        keep_greeting_in(state, &live);
    }
}

/// Forgets the hellos on their way in from clients whose key holds no live
/// player slot in `live`, and sends the join data again to the clients in
/// `live` that are owed it.
pub(crate) fn keep_greeting(live: &[u64]) {
    let owed = state(|s| {
        keep_greeting_in(s, live);
        owed_in(s, live)
    });
    for key in owed {
        hand_data(key);
    }
}

/// The clients still owed their join data: peers with a live slot in
/// `live`; the rest are forgotten.
pub(super) fn owed_in(state: &mut State, live: &[u64]) -> Vec<u64> {
    let peers = &state.peers;
    state
        .owed
        .retain(|(key, _)| live.contains(key) && peers.contains_key(key));
    state.owed.iter().map(|&(key, _)| key).collect()
}

/// What became of a try to hand a client its join data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Handover {
    /// All of it went.
    Whole,
    /// Some was dropped; it goes again. `first` for the first miss.
    Again { first: bool },
    /// Some was dropped [`MAX_HANDOVERS`] times; it goes no more.
    GaveUp,
}

/// Notes whether client `key`'s join data went `whole` at `now`.
pub(super) fn handed(state: &mut State, key: u64, whole: bool, now: Instant) -> Handover {
    let tries = state
        .owed
        .iter()
        .find(|&&(k, _)| k == key)
        .map_or(0, |&(_, tries)| tries);
    state.owed.retain(|&(k, _)| k != key);
    if whole {
        if let Some(peer) = state.peers.get_mut(&key) {
            peer.handed_data(now);
        }
        return Handover::Whole;
    }
    let tries = tries.saturating_add(1);
    if tries >= MAX_HANDOVERS {
        return Handover::GaveUp;
    }
    state.owed.push((key, tries));
    Handover::Again { first: tries == 1 }
}

/// Sends client `key` the data a joining client gets: item data, the
/// disabled recipes and the join values. A dropped part leaves it owed.
fn hand_data(key: u64) {
    let whole = crate::item_data::hello()
        .iter()
        .all(|records| item_data(key, records))
        && crate::recipes::hello(key)
        && crate::remote::sync_to(key);
    match state(|s| handed(s, key, whole, Instant::now())) {
        Handover::Whole | Handover::Again { first: false } => {}
        Handover::Again { first: true } => crate::log!(
            WARN,
            "mod messages: the join data for {key:016x} did not fit the link to the client; it goes again"
        ),
        Handover::GaveUp => crate::log!(
            WARN,
            "mod messages: the join data for {key:016x} did not fit the link to the client in {MAX_HANDOVERS} tries; the player gets it on joining again"
        ),
    }
}

pub(super) fn keep_greeting_in(state: &mut State, live: &[u64]) {
    state.greeting.retain(|key, _| live.contains(key));
}

/// Takes a message a client sent on `channel`, on the server's game
/// thread: `key` is the client's account key.
pub(crate) fn server_carried(key: u64, channel: u32, data: &[u8]) {
    if key == 0 {
        return;
    }
    let Some(known) = state(|s| {
        make_greeting_room(s, key, crate::world::live_keys);
        take_message(s, key, channel)
    }) else {
        return;
    };
    if channel != CONTROL {
        crate::texts::for_player(key, || deliver(channel, key, data));
        return;
    }
    if let Some(hello) = read_hello(data) {
        if let Some(language) = hello.language {
            state(|s| remember_language(s, key, language));
        }
        said_hello(key, &hello);
        return;
    }
    if known
        && let Some(loader) = crate::host::loader()
        && let Some(message) = remote_message(data)
    {
        crate::remote::server(loader, key, message);
    }
}

/// Answers the `hello` of the client `key` unless the server keeps the
/// player out.
fn said_hello(key: u64, hello: &crate::admission::Hello) {
    if !crate::admission::hello(key, hello) {
        return;
    }
    let greeting = state(|s| greet(s, key, Instant::now()));
    if greeting.fresh {
        crate::log!(INFO, "mod messages: the client of {key:016x} said hello");
    } else if greeting.data {
        crate::log!(
            INFO,
            "mod messages: the client of {key:016x} said hello again; it gets its data again"
        );
    }
    // A client takes the server's settings messages only after the
    // welcome; without it, the client says hello again.
    if super::carry(key, CONTROL, &control(WELCOME)) && greeting.data {
        hand_data(key);
    }
}
