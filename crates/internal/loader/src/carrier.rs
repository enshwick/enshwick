//! Enshwick's own messages on the game's connection: one message socket per
//! peer on the game channel no game code uses, voice chat, carrying the
//! reliable link of [`wire`](crate::wire).
//!
//! The server opens a socket for each player's machine in game and sends
//! nothing on it until that client was heard, so a vanilla client never
//! gets a message. A client opens its socket only on a server that
//! advertises Enshwick. Both sides pump their sockets on their own game
//! thread: the server after each step, a client where it sends its chat.
//! The game's per-peer socket lists never hold these sockets, so the
//! game's receive loops never see them.
//!
//! With the developer tools on, each side checks the channel: [`PROBES`]
//! probe messages each way, each logged with its number and whether it
//! arrived whole, and the server sends one packet to a client it never heard
//! from, which a vanilla client drops with one warning of its own.

use std::sync::atomic::{AtomicU64, Ordering};
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
/// How long a client waits before it tries again to open its socket.
const OPEN_AGAIN: Duration = Duration::from_secs(5);
/// How long the server waits for a client before the developer tools' one
/// packet to it.
const UNHEARD_FOR: Duration = Duration::from_secs(30);

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
    in_order: bool,
}

/// What the carrier keeps for one session.
#[derive(Debug)]
struct State {
    session: usize,
    peers: FxHashMap<u64, Peer>,
    /// When a client last tried to open its socket.
    tried: Option<Instant>,
}

static GAME: OnceLock<Game> = OnceLock::new();
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Packets dropped: unreadable, over a bound or a rate.
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Resolves and checks the game's socket calls; without them the own
/// channel stays off and the log says which part is missing.
pub(crate) fn link(loader: &Loader) {
    match Game::resolve(loader) {
        Ok(game) => {
            let _first = GAME.set(game);
            crate::log!(INFO, "own channel ready on game channel {}", game.channel());
        }
        Err(missing) => crate::log!(WARN, "own channel off: this build has no {missing}"),
    }
}

/// Pumps the server's sockets after a step, on the game thread: opens one
/// for each player that joined, closes those of players who left, takes
/// what came and sends what is due.
pub(crate) fn after_step(loader: &Loader, now: Instant) {
    let Some(game) = GAME.get() else {
        return;
    };
    let Ok(server) = crate::world::server() else {
        return;
    };
    let Some(session) = game.server_session(server) else {
        return;
    };
    let Ok(machines) = crate::world::machines(loader) else {
        return;
    };
    let host = game.host_machine(session);
    let Some(mut guard) = STATE.try_relock() else {
        return;
    };
    let state = fresh(&mut guard, session);
    state.peers.retain(|&key, peer| {
        let live = machines.iter().any(|&(k, m)| k == key && m == peer.machine);
        if live && !game.holds(peer.socket, peer.slot) {
            crate::log!(WARN, "own channel: the socket of {key:016x} went");
            return false;
        }
        if !live {
            game.close(session, peer.socket);
        }
        live
    });
    for &(key, machine) in &machines {
        if Some(machine) == host || machine == 0 || state.peers.contains_key(&key) {
            continue;
        }
        if let Some(peer) = game.open_peer(session, game.machine_kind(), machine, now) {
            crate::log!(INFO, "own channel: socket open for {key:016x}");
            state.peers.insert(key, peer);
        }
    }
    let developer = loader.developer_tools;
    state.peers.retain(|&key, peer| {
        receive(game, session, key, peer, now);
        if developer {
            probe_step(peer);
            if !peer.heard
                && !peer.nudged
                && now.saturating_duration_since(peer.opened) >= UNHEARD_FOR
            {
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
        }
        if !peer.heard {
            return true;
        }
        send(game, session, key, peer, now)
    });
}

/// Pumps a client's socket, where it sends its chat: opens it once the
/// server advertises Enshwick, takes what came and sends what is due.
///
/// # Safety
///
/// - `client` is null or the game client, on the thread that sends its chat.
pub(crate) unsafe fn client_frame(client: *mut u8, now: Instant) {
    let (Some(game), Some(loader)) = (GAME.get(), crate::host::loader()) else {
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
    let modded = crate::net::server_runs_enshwick() && !crate::world::hosting();
    let Some(mut guard) = STATE.try_relock() else {
        return;
    };
    if !modded {
        if guard.as_ref().is_some_and(|s| s.session == session) {
            for peer in guard.take().into_iter().flat_map(|s| s.peers.into_values()) {
                game.close(session, peer.socket);
            }
        }
        return;
    }
    let state = fresh(&mut guard, session);
    if let Some(peer) = state.peers.get(&SERVER)
        && !game.holds(peer.socket, peer.slot)
    {
        crate::log!(WARN, "own channel: the socket to the server went");
        state.peers.remove(&SERVER);
    }
    if !state.peers.contains_key(&SERVER)
        && state
            .tried
            .is_none_or(|at| now.saturating_duration_since(at) >= OPEN_AGAIN)
    {
        state.tried = Some(now);
        if let Some(kind) = game.local_machine_kind()
            && let Some(peer) = game.open_peer(session, kind, 0, now)
        {
            crate::log!(INFO, "own channel: socket open to the server");
            state.peers.insert(SERVER, peer);
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

/// Forgets every socket, when the client leaves its server: they went with
/// its session.
pub(crate) fn forget() {
    *STATE.relock() = None;
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
        tried: None,
    })
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
        game.transmit(session, socket, packet);
    });
    if !alive {
        crate::log!(WARN, "own channel: {key:016x} stopped answering");
        game.close(session, socket);
    }
    alive
}

/// A message from `key` came whole.
fn delivered(key: u64, message: &[u8], probe: &mut Probe) {
    let Ok((channel, payload)) = open(message) else {
        dropped();
        return;
    };
    if channel == CONTROL
        && let Some((id, whole)) = read_probe(payload)
    {
        probe.in_order = (probe.received == 0 || probe.in_order) && id == probe.received;
        probe.received = probe.received.saturating_add(1);
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
                if probe.in_order {
                    "in order"
                } else {
                    "out of order"
                }
            );
        }
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
    let count = DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_multiple_of(1000) {
        crate::log!(
            WARN,
            "own channel: {count} packets dropped, unreadable or over a bound"
        );
    }
}
