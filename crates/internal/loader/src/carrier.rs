//! Enshwick's own messages on the game's connection: one message socket per
//! peer on the game channel no game code uses, voice chat, carrying the
//! reliable link of [`wire`](crate::wire).
//!
//! The server opens a socket for each player's machine in game and sends
//! nothing on it until that client was heard, so a vanilla client never
//! gets a message. A client opens its socket only on a server that
//! advertises Enshwick, and greets the server on it at once. Both sides
//! pump their sockets on their own game thread: the server after each step,
//! a client where it sends its chat; a client that hosts does both, with a
//! state of each. The game's per-peer socket lists never hold these
//! sockets, so the game's receive loops never see them.
//!
//! With the developer tools on, each side checks the channel: [`PROBES`]
//! probe messages each way, each logged with its number and whether it
//! arrived whole, and the server sends one packet to a client it never heard
//! from, which a vanilla client drops with one warning of its own.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::host::Loader;
use crate::locks::Relock;
use crate::pinned;
use crate::wire::{CONTROL, Link, MAX_SEALED, PROBES, open, probe, read_probe, seal};

mod game;

use game::{Game, Taken};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::net::CHANNEL_BASE.id(),
    pinned::net::CLIENT.id(),
    pinned::net::MESSAGE_ABORT.id(),
    pinned::net::MESSAGE_COMMIT.id(),
    pinned::net::MESSAGE_OPEN.id(),
    pinned::net::MESSAGE_RECEIVE.id(),
    pinned::net::MESSAGE_RELEASE.id(),
    pinned::net::SESSION_LIMIT.id(),
    pinned::net::SESSION_MANAGER.id(),
    pinned::net::SOCKET_CHANNEL.id(),
    pinned::net::SOCKET_CREATE.id(),
    pinned::net::SOCKET_DESTROY.id(),
    pinned::net::SOCKET_KIND_LOCAL_MACHINE.id(),
    pinned::net::SOCKET_KIND_MACHINE.id(),
    pinned::net::SOCKET_PEER.id(),
    pinned::server::SESSION.id(),
    pinned::session::HOST_MACHINE.id(),
];

/// The key a client keeps its server under.
const SERVER: u64 = enshwick::abi::NetMessage::SERVER.get();
/// Messages taken from one socket at most per step.
const RECEIVES_PER_STEP: usize = 256;
/// Packets sent on one socket at most per step.
const SENDS_PER_STEP: usize = 64;
/// Packets a peer may send per second, and in one burst: a link sends at
/// most its window per timeout, and its acks.
const PACKET_RATE: f64 = 2048.0;
const PACKET_BURST: f64 = 1024.0;
/// The largest packet taken from the game, and the largest sent: the
/// session's own packet limit wins when smaller.
const LARGEST_PACKET: usize = 1400;
/// How long a side waits before it tries again to open a socket.
const OPEN_AGAIN: Duration = Duration::from_secs(5);
/// Server steps between looks at who is in game.
const MEMBERS_EVERY: u32 = 30;
/// How long the server waits for a client before the developer tools' one
/// packet to it.
const UNHEARD_FOR: Duration = Duration::from_secs(30);
/// The step between the epochs of two links: room for the restarts of one.
const EPOCH_STEP: u32 = 0x1_0000;

/// One peer's socket and link.
#[derive(Debug)]
struct Peer {
    /// The machine handle the socket was opened for; 0 for the server.
    machine: u32,
    socket: usize,
    /// The socket's peer field as the game wrote it, which a slot reused for
    /// another socket would not hold.
    slot: u32,
    link: Link,
    /// Whether the peer sent a packet that read: the server sends nothing
    /// before.
    heard: bool,
    rate: crate::wire::Peer,
    opened: Instant,
    /// The developer tools' one packet to an unheard client went.
    nudged: bool,
    probe: Probe,
}

/// The developer tools' channel check with one peer.
#[derive(Debug, Default)]
struct Probe {
    sent: u8,
    received: u8,
    whole: u8,
    out_of_order: bool,
}

/// What one side of the carrier keeps for one session.
#[derive(Debug)]
struct State {
    session: usize,
    peers: FxHashMap<u64, Peer>,
    /// Peers whose socket did not open, with when it was last tried.
    tried: FxHashMap<u64, Instant>,
    steps: u32,
}

/// The carrier's state: the game's calls, and each side's peers.
struct Carrier {
    game: OnceLock<Game>,
    server: Mutex<Option<State>>,
    client: Mutex<Option<State>>,
    /// Set while the client side holds a socket, so a client frame on a
    /// server without Enshwick takes no lock.
    client_open: AtomicBool,
    /// The epoch the next link starts at.
    epochs: AtomicU32,
    /// Packets dropped: unreadable, over a bound or a rate.
    dropped: AtomicU64,
}

static CARRIER: Carrier = Carrier {
    game: OnceLock::new(),
    server: Mutex::new(None),
    client: Mutex::new(None),
    client_open: AtomicBool::new(false),
    epochs: AtomicU32::new(EPOCH_STEP),
    dropped: AtomicU64::new(0),
};

/// Resolves and checks the game's socket calls; without them the own
/// channel stays off and the log says which part is missing.
pub(crate) fn link(loader: &Loader) {
    match Game::resolve(loader) {
        Ok(game) => {
            let _first = CARRIER.game.set(game);
            crate::log!(INFO, "own channel ready on game channel {}", game.channel());
        }
        Err(missing) => crate::log!(WARN, "own channel off: this build has no {missing}"),
    }
}

/// Pumps the server's sockets after a step, on the game thread: opens one
/// for each player that joined, closes those of players who left, takes
/// what came and sends what is due.
pub(crate) fn after_step(loader: &Loader, now: Instant) {
    let Some(game) = CARRIER.game.get() else {
        return;
    };
    let Ok(server) = crate::world::server() else {
        return;
    };
    let Some(session) = game.server_session(server) else {
        return;
    };
    let Some(mut guard) = CARRIER.server.try_relock() else {
        return;
    };
    let state = fresh(&mut guard, session);
    state.steps = state.steps.wrapping_add(1);
    if state.steps % MEMBERS_EVERY == 1 {
        let Ok(machines) = crate::world::machines(loader) else {
            return;
        };
        members(game, session, state, &machines, now);
    }
    let developer = loader.developer_tools;
    state.peers.retain(|&key, peer| {
        if !game.holds(peer.socket, peer.slot) {
            crate::log!(WARN, "own channel: the socket of {key:016x} went");
            return false;
        }
        receive(game, session, key, peer, now);
        if developer {
            probe_step(peer);
            nudge(game, session, key, peer, now);
        }
        !peer.heard || send(game, session, key, peer, now)
    });
}

/// Opens a socket for each machine in game without one, and closes those
/// of machines that left.
fn members(game: &Game, session: usize, state: &mut State, machines: &[(u64, u32)], now: Instant) {
    let host = game.host_machine(session);
    state.peers.retain(|&key, peer| {
        let live = machines.iter().any(|&(k, m)| k == key && m == peer.machine);
        if !live && game.holds(peer.socket, peer.slot) {
            game.close(session, peer.socket);
        }
        live
    });
    state
        .tried
        .retain(|key, _| machines.iter().any(|&(k, _)| k == *key));
    for &(key, machine) in machines {
        let due = state
            .tried
            .get(&key)
            .is_none_or(|&at| now.saturating_duration_since(at) >= OPEN_AGAIN);
        if machine == 0 || machine == host || state.peers.contains_key(&key) || !due {
            continue;
        }
        match game.open_peer(session, game.machine_kind(), machine, next_epoch(), now) {
            Some(peer) => {
                crate::log!(INFO, "own channel: socket open for {key:016x}");
                state.tried.remove(&key);
                state.peers.insert(key, peer);
            }
            None => {
                state.tried.insert(key, now);
            }
        }
    }
}

/// Pumps a client's socket, where it sends its chat: opens it once the
/// server advertises Enshwick, takes what came and sends what is due.
///
/// # Safety
///
/// - `client` is null or the game client, on the thread that sends its chat.
pub(crate) unsafe fn client_frame(client: *mut u8, now: Instant) {
    let modded = crate::net::server_runs_enshwick() && !crate::world::hosting();
    if !modded && !CARRIER.client_open.load(Ordering::Acquire) {
        return;
    }
    let (Some(game), Some(loader)) = (CARRIER.game.get(), crate::host::loader()) else {
        return;
    };
    if client.is_null() {
        return;
    }
    // SAFETY: `client` is the non-null game client on its chat thread by
    // this function's `# Safety`.
    let Some(session) = (unsafe { game.client_session(client) }) else {
        return;
    };
    let Some(mut guard) = CARRIER.client.try_relock() else {
        return;
    };
    if !modded {
        if let Some(state) = guard.take()
            && state.session == session
        {
            for peer in state.peers.into_values() {
                if game.holds(peer.socket, peer.slot) {
                    game.close(session, peer.socket);
                }
            }
        }
        CARRIER.client_open.store(false, Ordering::Release);
        return;
    }
    let state = fresh(&mut guard, session);
    if let Some(peer) = state.peers.get(&SERVER)
        && !game.holds(peer.socket, peer.slot)
    {
        crate::log!(WARN, "own channel: the socket to the server went");
        state.peers.remove(&SERVER);
    }
    let due = state
        .tried
        .get(&SERVER)
        .is_none_or(|&at| now.saturating_duration_since(at) >= OPEN_AGAIN);
    if !state.peers.contains_key(&SERVER) && due {
        state.tried.insert(SERVER, now);
        if let Some(kind) = game.local_machine_kind()
            && let Some(mut peer) = game.open_peer(session, kind, 0, next_epoch(), now)
        {
            crate::log!(INFO, "own channel: socket open to the server");
            greet(&mut peer);
            state.peers.insert(SERVER, peer);
            CARRIER.client_open.store(true, Ordering::Release);
        }
    }
    let Some(peer) = state.peers.get_mut(&SERVER) else {
        return;
    };
    receive(game, session, SERVER, peer, now);
    if loader.developer_tools {
        probe_step(peer);
    }
    if !send(game, session, SERVER, peer, now) {
        state.peers.remove(&SERVER);
    }
}

/// Forgets the client's socket, when it leaves its server: the socket went
/// with its session.
pub(crate) fn forget() {
    *CARRIER.client.relock() = None;
    CARRIER.client_open.store(false, Ordering::Release);
}

/// The epoch a new link starts at, past every earlier link's restarts.
fn next_epoch() -> u32 {
    CARRIER
        .epochs
        .fetch_add(EPOCH_STEP, Ordering::Relaxed)
        .max(1)
}

/// The state for `session`, anew when the session changed: the sockets of
/// the last one went with it.
fn fresh(guard: &mut Option<State>, session: usize) -> &mut State {
    if guard.as_ref().is_some_and(|s| s.session != session) {
        *guard = None;
    }
    guard.get_or_insert_with(|| State {
        session,
        peers: FxHashMap::default(),
        tried: FxHashMap::default(),
        steps: 0,
    })
}

/// Queues a client's first message, an empty control message, so the
/// server hears it and starts to answer.
fn greet(peer: &mut Peer) {
    let mut sealed = [0u8; MAX_SEALED];
    if let Some(size) = seal(&mut sealed, CONTROL, &[]) {
        let _queued = peer.link.send(&sealed[..size]);
    }
}

/// Takes the packets that came on `peer`'s socket and hands on the
/// messages they complete.
fn receive(game: &Game, session: usize, key: u64, peer: &mut Peer, now: Instant) {
    let Peer {
        socket,
        link,
        heard,
        rate,
        probe,
        ..
    } = peer;
    let mut packet = [0u8; LARGEST_PACKET];
    for _ in 0..RECEIVES_PER_STEP {
        let Some(taken) = game.take(session, *socket, &mut packet) else {
            break;
        };
        let Taken::Bytes(size) = taken else {
            dropped();
            continue;
        };
        let Some(bytes) = packet.get(..size) else {
            dropped();
            continue;
        };
        if !rate.admit_at(now, PACKET_RATE, PACKET_BURST) {
            dropped();
            continue;
        }
        match link.receive(bytes, |message| delivered(key, message, probe)) {
            Ok(()) => *heard = true,
            Err(_) => dropped(),
        }
    }
}

/// Sends what `peer`'s link has due; false once the peer stopped answering.
fn send(game: &Game, session: usize, key: u64, peer: &mut Peer, now: Instant) -> bool {
    let socket = peer.socket;
    let alive = peer.link.poll(now, SENDS_PER_STEP, |packet| {
        game.transmit(session, socket, packet)
    });
    if !alive {
        crate::log!(WARN, "own channel: {key:016x} stopped answering");
        if game.holds(socket, peer.slot) {
            game.close(session, socket);
        }
    }
    alive
}

/// The developer tools' one packet to a client the server never heard
/// from, once it had time to speak.
fn nudge(game: &Game, session: usize, key: u64, peer: &mut Peer, now: Instant) {
    if peer.heard || peer.nudged || now.saturating_duration_since(peer.opened) < UNHEARD_FOR {
        return;
    }
    peer.nudged = true;
    crate::log!(
        INFO,
        "own channel check: one packet to {key:016x}, which never spoke"
    );
    let socket = peer.socket;
    peer.link.nudge(|packet| {
        game.transmit(session, socket, packet);
    });
}

/// A message from `key` came whole.
fn delivered(key: u64, message: &[u8], probe: &mut Probe) {
    let Ok((channel, payload)) = open(message) else {
        dropped();
        return;
    };
    if channel != CONTROL || probe.received >= PROBES {
        return;
    }
    let Some((id, whole)) = read_probe(payload) else {
        return;
    };
    probe.out_of_order |= id != probe.received;
    probe.received += 1;
    probe.whole += u8::from(whole);
    crate::log!(
        INFO,
        "own channel check from {key:016x}: probe {id}, {} bytes, {}",
        payload.len(),
        if whole { "whole" } else { "damaged" }
    );
    if probe.received == PROBES {
        crate::log!(
            INFO,
            "own channel check from {key:016x}: {} of {PROBES} whole, {}",
            probe.whole,
            if probe.out_of_order {
                "out of order"
            } else {
                "in order"
            }
        );
    }
}

/// Queues the developer tools' probes while the link has room, once the
/// peer was heard or, on a client, at once.
fn probe_step(peer: &mut Peer) {
    if !peer.heard && peer.machine != 0 {
        return;
    }
    let mut sealed = [0u8; MAX_SEALED];
    while peer.probe.sent < PROBES {
        let Some(size) = seal(&mut sealed, CONTROL, &probe(peer.probe.sent)) else {
            return;
        };
        if peer.link.send(&sealed[..size]).is_err() {
            return;
        }
        peer.probe.sent += 1;
    }
}

/// Counts a dropped packet or message and logs the first and every
/// thousandth.
#[cold]
fn dropped() {
    let count = CARRIER.dropped.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_multiple_of(1000) {
        crate::log!(
            WARN,
            "own channel: {count} packets dropped, unreadable or over a bound"
        );
    }
}
