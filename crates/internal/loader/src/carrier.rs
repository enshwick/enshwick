//! Enshwick's own messages on the game's connection: one message socket per
//! peer on the game channel no game code uses, voice chat, carrying the
//! reliable link of [`wire`](crate::wire).
//!
//! The server opens a socket for each player's machine in game and sends
//! nothing on it until that client was heard, so a vanilla client never
//! gets a message. A client opens its socket only on a server that
//! advertises Enshwick, and says hello on it. Both sides pump their sockets
//! on their own game thread: the server after each step, a client where it
//! sends its chat. A client that hosts talks to its own server half in
//! process. The game's per-peer socket lists never hold these sockets, so
//! the game's receive loops never see them.
//!
//! Messages are handed to the net module after the carrier lets go of its
//! state, so a handler may send at once.
//!
//! With the developer tools on, each side checks the channel: [`PROBES`]
//! probe messages each way, each logged with its number and whether it
//! arrived whole, and the server sends one packet to a client it never heard
//! from, which a vanilla client drops with one warning of its own.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use enshwick::abi::{HookFlags, Status};
use rustc_hash::FxHashMap;

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::pinned;
use crate::wire::{CONTROL, Link, MAX_SEALED, PROBES, open, probe, read_probe, seal, sealable};

mod game;
#[cfg(test)]
mod tests;

use game::{Game, Taken};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::chat::CLIENT_SEND.id(),
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
/// The bytes of messages one side gathers per step before it hands them
/// on; the rest wait in the game's socket.
const INBOX_BYTES: usize = 1 << 20;
/// Messages a hosting client's two halves hold for each other at most.
const MAX_LOOPED: usize = 1024;
/// How long a side waits before it tries again to open a socket.
const OPEN_AGAIN: Duration = Duration::from_secs(5);
/// Server steps between looks at who is in game.
const MEMBERS_EVERY: u32 = 30;
/// How long the server waits for a client before the developer tools' one
/// packet to it.
const UNHEARD_FOR: Duration = Duration::from_secs(30);
/// The step between the epochs of two links: room for the restarts of one.
const EPOCH_STEP: u32 = 0x1_0000;

type ClientSendFn = unsafe extern "system" fn(client: *mut u8) -> u64;

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

/// Messages gathered in one step, handed on once the state is let go:
/// their bytes back to back, and each one's sender, channel and span.
#[derive(Debug, Default)]
struct Inbox {
    bytes: Vec<u8>,
    items: Vec<Item>,
}

/// One message in an [`Inbox`]: who sent it on which channel, and where its
/// bytes lie.
#[derive(Clone, Copy, Debug)]
struct Item {
    from: u64,
    channel: u32,
    span: (usize, usize),
}

/// A message between a hosting client's two halves.
#[derive(Debug)]
struct Looped {
    channel: u32,
    data: Vec<u8>,
}

/// What one side of the carrier keeps for one session.
#[derive(Debug)]
struct State {
    session: usize,
    peers: FxHashMap<u64, Peer>,
    /// Peers whose socket did not open, with when it was last tried.
    tried: FxHashMap<u64, Instant>,
    steps: u32,
    inbox: Inbox,
}

/// The carrier's state: the game's calls, each side's peers, and the
/// messages a hosting client's halves hold for each other.
struct Carrier {
    game: OnceLock<Game>,
    server: Mutex<Option<State>>,
    client: Mutex<Option<State>>,
    /// Messages a hosting client's client half sent its server half.
    to_server: Mutex<Vec<Looped>>,
    /// Messages a hosting client's server half sent its own player.
    to_client: Mutex<Vec<Looped>>,
    /// Set while `to_server` may hold anything, so an idle step takes no
    /// lock.
    server_waiting: AtomicBool,
    /// Set while `to_client` may hold anything.
    client_waiting: AtomicBool,
    /// Set once a hosting client's client half said hello to its server
    /// half.
    hosted_hello: AtomicBool,
    /// The account key of the player who hosts, on a hosting client; 0
    /// otherwise.
    host_key: AtomicU64,
    /// Set while the client side holds a socket, so a client frame on a
    /// server without Enshwick takes no lock.
    client_open: AtomicBool,
    /// The epoch the next link starts at.
    epochs: AtomicU32,
    /// Packets dropped: unreadable, over a bound or a rate.
    dropped: AtomicU64,
    client_send: Next<ClientSendFn>,
    fuse: crate::fuse::Fuse,
    /// The switch of the lobby a hosting client publishes, apart from the
    /// channel's.
    lobby_fuse: crate::fuse::Fuse,
    /// Server steps of a hosting client, to look for its own player now and
    /// then.
    hosted_steps: AtomicU32,
}

static CARRIER: Carrier = Carrier {
    game: OnceLock::new(),
    server: Mutex::new(None),
    client: Mutex::new(None),
    to_server: Mutex::new(Vec::new()),
    to_client: Mutex::new(Vec::new()),
    server_waiting: AtomicBool::new(false),
    client_waiting: AtomicBool::new(false),
    hosted_hello: AtomicBool::new(false),
    host_key: AtomicU64::new(0),
    client_open: AtomicBool::new(false),
    epochs: AtomicU32::new(EPOCH_STEP),
    dropped: AtomicU64::new(0),
    // SAFETY: `ClientSendFn` is the signature of the game's
    // `chat.client_send`, and `link` uses this slot only as the `next` of
    // `client_send`, its link on that function.
    client_send: unsafe { Next::new() },
    fuse: crate::fuse::Fuse::new("the own channel"),
    lobby_fuse: crate::fuse::Fuse::new("the hosted lobby"),
    hosted_steps: AtomicU32::new(0),
};

/// Resolves and checks the game's socket calls, and on a client links its
/// chat send, where the client pumps its socket; without them the own
/// channel stays off and the log says which part is missing.
pub(crate) fn link(loader: &Loader) {
    let game = Game::resolve(loader);
    if let Ok(game) = game {
        let _first = CARRIER.game.set(game);
    }
    // Linked even without the channel: the lobby a client hosts is published
    // from here too.
    let linked = loader.side != enshwick::abi::Side::CLIENT
        || loader.link_feature(
            "the client's frame step",
            Target::Pinned(pinned::chat::CLIENT_SEND),
            &CARRIER.client_send,
            client_send as ClientSendFn,
            HookFlags::NONE,
        );
    match game {
        Ok(game) if linked => {
            crate::log!(INFO, "own channel ready on game channel {}", game.channel());
        }
        Ok(_) => {}
        Err(missing) => crate::log!(WARN, "own channel off: this build has no {missing}"),
    }
}

/// The loader's link on the client's chat send: pumps the own channel
/// before the game sends.
///
/// # Safety
///
/// - Called only by the game through the chain on `chat.client_send`, on the
///   thread that sends the client's chat, with the game client or null.
unsafe extern "system" fn client_send(client: *mut u8) -> u64 {
    let _published = CARRIER
        .lobby_fuse
        .run("the hosted lobby", crate::lobby::poll);
    let _caught = CARRIER.fuse.run("the client's own channel", || {
        // SAFETY: `client` is the game's own argument, null or the game
        // client, on the thread that sends its chat, by this function's
        // `# Safety`.
        unsafe { client_frame(client, Instant::now()) };
    });
    // SAFETY: `client_send` holds the rest of `chat.client_send`'s chain
    // while this detour runs, and `client` is the game's own argument, passed
    // on unchanged.
    unsafe { CARRIER.client_send.get()(client) }
}

/// Sends `payload` on `channel` to `key`: the server for the server's key,
/// else a client the server heard from. Any thread.
///
/// # Errors
///
/// [`Status::MISSING`] without a link to `key`, [`Status::REFUSED`] when
/// its link is full, [`Status::BAD_ARGS`] for a payload too long or that
/// starts like executable code.
pub(crate) fn send(key: u64, channel: u32, payload: &[u8]) -> Result<(), Status> {
    let mut failed = Ok(());
    send_many(&[key], channel, payload, |_, status| failed = Err(status))?;
    failed
}

/// [`send`] to each of `keys`, sealed once under one lock; `failed` hears
/// each key it did not go to, with why.
///
/// # Errors
///
/// [`Status::BAD_ARGS`] for a payload too long or that starts like
/// executable code, which goes to no one.
pub(crate) fn send_many(
    keys: &[u64],
    channel: u32,
    payload: &[u8],
    mut failed: impl FnMut(u64, Status),
) -> Result<(), Status> {
    if !sealable(payload) {
        return Err(Status::BAD_ARGS);
    }
    let hosting = client_hosts();
    let host = CARRIER.host_key.load(Ordering::Acquire);
    let looped = |key: u64| hosting && (key == SERVER || (host != 0 && key == host));
    for &key in keys.iter().filter(|&&key| looped(key)) {
        if let Err(status) = loop_back(key == SERVER, channel, payload) {
            failed(key, status);
        }
    }
    let mut wired = keys.iter().copied().filter(|&key| !looped(key)).peekable();
    let Some(&first) = wired.peek() else {
        return Ok(());
    };
    let mut sealed = [0u8; MAX_SEALED];
    let size = seal(&mut sealed, channel, payload).ok_or(Status::BAD_ARGS)?;
    let half = if first == SERVER {
        &CARRIER.client
    } else {
        &CARRIER.server
    };
    let mut guard = half.relock();
    for key in wired {
        let sent = match guard.as_mut().and_then(|state| state.peers.get_mut(&key)) {
            Some(peer) if key == SERVER || peer.heard => {
                peer.link.send(&sealed[..size]).map_err(|_| Status::REFUSED)
            }
            _ => Err(Status::MISSING),
        };
        if let Err(status) = sent {
            failed(key, status);
        }
    }
    Ok(())
}

/// Whether this client runs the game's server itself, in single player or
/// a game it hosts: its two halves then talk in process. A dedicated
/// server runs a server too, but no client half.
fn client_hosts() -> bool {
    crate::world::hosting()
        && crate::host::loader().is_some_and(|loader| loader.side == enshwick::abi::Side::CLIENT)
}

/// Notes the account key of the player of a client that hosts; a new one
/// makes the client half say hello to its server half again.
fn set_host(key: u64) {
    let old = CARRIER.host_key.swap(key, Ordering::AcqRel);
    if old != key {
        CARRIER.hosted_hello.store(false, Ordering::Release);
    }
}

/// Holds a message between a hosting client's halves until the other half
/// takes it: to the server from its client half, or to the host's own
/// player from the server.
fn loop_back(to_server: bool, channel: u32, payload: &[u8]) -> Result<(), Status> {
    let (queue, waiting) = looped_queue(to_server);
    let mut looped = queue.relock();
    if looped.len() >= MAX_LOOPED {
        return Err(Status::REFUSED);
    }
    looped.push(Looped {
        channel,
        data: payload.to_vec(),
    });
    waiting.store(true, Ordering::Release);
    Ok(())
}

/// The queue for one half of a hosting client, and its flag.
fn looped_queue(to_server: bool) -> (&'static Mutex<Vec<Looped>>, &'static AtomicBool) {
    if to_server {
        (&CARRIER.to_server, &CARRIER.server_waiting)
    } else {
        (&CARRIER.to_client, &CARRIER.client_waiting)
    }
}

/// The messages held for one half of a hosting client, taken whole; no
/// lock while its queue is empty.
fn take_looped(to_server: bool) -> Vec<Looped> {
    let (queue, waiting) = looped_queue(to_server);
    if !waiting.swap(false, Ordering::AcqRel) {
        return Vec::new();
    }
    core::mem::take(&mut *queue.relock())
}

/// Pumps the server's sockets after a step, on the game thread: hands on
/// what a hosting client's own player sent, opens a socket for each player
/// that joined, closes those of players who left, takes what came and sends
/// what is due, then hands on what came and forgets the peers that went.
pub(crate) fn after_step(loader: &Loader, now: Instant) {
    // In process, so even without the game's socket calls.
    if client_hosts() {
        hosted_step(loader);
    }
    let Some(game) = CARRIER.game.get() else {
        return;
    };
    let Ok(server) = crate::world::server() else {
        return;
    };
    let Some(session) = game.server_session(server) else {
        return;
    };
    let mut gone = Vec::new();
    let mut restarted = Vec::new();
    let inbox = {
        let Some(mut guard) = CARRIER.server.try_relock() else {
            return;
        };
        let state = fresh(&mut guard, session);
        state.steps = state.steps.wrapping_add(1);
        if state.steps % MEMBERS_EVERY == 1
            && let Ok(machines) = crate::world::machines(loader)
        {
            members(game, session, state, &machines, now);
        }
        let developer = loader.developer_tools;
        let State { peers, inbox, .. } = state;
        let share = INBOX_BYTES / peers.len().max(1);
        peers.retain(|&key, peer| {
            let kept = game.holds(peer.socket, peer.slot) && {
                let restarts = peer.link.restarts();
                receive(game, session, key, peer, inbox, share, now);
                if peer.link.restarts() != restarts {
                    restarted.push(key);
                }
                if developer {
                    probe_step(peer);
                    nudge(game, session, key, peer, now);
                }
                !peer.heard || send_due(game, session, key, peer, now)
            };
            if !kept {
                crate::log!(WARN, "own channel: the link to {key:016x} went");
                gone.push(key);
            }
            kept
        });
        core::mem::take(inbox)
    };
    // A client whose link started over says hello again on the new one.
    for &key in &restarted {
        crate::net::forget_peer(key);
    }
    for item in &inbox.items {
        let (start, end) = item.span;
        crate::net::server_carried(item.from, item.channel, &inbox.bytes[start..end]);
    }
    give_back(&CARRIER.server, session, inbox);
    for key in gone {
        crate::net::forget_peer(key);
    }
}

/// A hosting client's server half, after its step: finds its own player
/// while it has none, which needs no network session (single player has
/// one player alone), and takes what its client half sent.
fn hosted_step(loader: &Loader) {
    let step = CARRIER.hosted_steps.fetch_add(1, Ordering::Relaxed);
    if CARRIER.host_key.load(Ordering::Acquire) == 0
        && step.is_multiple_of(MEMBERS_EVERY)
        && let Ok(machines) = crate::world::machines(loader)
        && let [(key, _)] = machines.as_slice()
    {
        set_host(*key);
    }
    let host = CARRIER.host_key.load(Ordering::Acquire);
    if host == 0 {
        return;
    }
    for message in take_looped(true) {
        crate::net::server_carried(host, message.channel, &message.data);
    }
}

/// Opens a socket for each machine in game without one, and closes those
/// of machines that left; notes the hosting player's key.
fn members(game: &Game, session: usize, state: &mut State, machines: &[(u64, u32)], now: Instant) {
    let host = game.host_machine(session);
    // Only a client hosts a player of its own; a dedicated server's host
    // machine is no player's.
    if game.local_machine_kind().is_some()
        && let Some(&(key, _)) = machines
            .iter()
            .find(|&&(_, machine)| machine == host && machine != 0)
    {
        set_host(key);
    }
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
/// server advertises Enshwick, takes what came and sends what is due, then
/// hands on what came and tells the net when the link opened or went. A
/// hosting client takes what its server half sent it.
///
/// # Safety
///
/// - `client` is null or the game client, on the thread that sends its chat.
unsafe fn client_frame(client: *mut u8, now: Instant) {
    if client_hosts() {
        let opened = !CARRIER.hosted_hello.swap(true, Ordering::AcqRel);
        if opened {
            crate::net::link_lost();
        }
        for message in take_looped(false) {
            crate::net::client_carried(message.channel, &message.data);
        }
        crate::net::client_tick(opened);
        return;
    }
    let modded = crate::net::server_runs_enshwick();
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
    let (inbox, change) = {
        let Some(mut guard) = CARRIER.client.try_relock() else {
            return;
        };
        if !modded {
            close_client(game, session, &mut guard);
            drop(guard);
            crate::net::link_lost();
            return;
        }
        let state = fresh(&mut guard, session);
        let change = pump_client(game, loader, session, state, now);
        (core::mem::take(&mut state.inbox), change)
    };
    // The last link's answer goes before the new link's messages are read.
    if change.is_some() {
        crate::net::link_lost();
    }
    for item in &inbox.items {
        let (start, end) = item.span;
        crate::net::client_carried(item.channel, &inbox.bytes[start..end]);
    }
    give_back(&CARRIER.client, session, inbox);
    if CARRIER.client_open.load(Ordering::Acquire) {
        crate::net::client_tick(change == Some(Change::Opened));
    }
}

/// What became of the client's link in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Change {
    /// A new link opened: the client says hello on it.
    Opened,
    /// The link went: the server is no longer ready.
    Lost,
}

/// Closes the client's socket, when its server no longer advertises
/// Enshwick.
fn close_client(game: &Game, session: usize, guard: &mut Option<State>) {
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
}

/// Opens the client's socket when it has none, takes what came on it and
/// sends what is due; whether the link opened or went.
fn pump_client(
    game: &Game,
    loader: &Loader,
    session: usize,
    state: &mut State,
    now: Instant,
) -> Option<Change> {
    let mut change = None;
    if let Some(peer) = state.peers.get(&SERVER)
        && !game.holds(peer.socket, peer.slot)
    {
        crate::log!(WARN, "own channel: the socket to the server went");
        state.peers.remove(&SERVER);
        change = Some(Change::Lost);
    }
    let due = state
        .tried
        .get(&SERVER)
        .is_none_or(|&at| now.saturating_duration_since(at) >= OPEN_AGAIN);
    if !state.peers.contains_key(&SERVER) && due {
        state.tried.insert(SERVER, now);
        if let Some(kind) = game.local_machine_kind()
            && let Some(peer) = game.open_peer(session, kind, 0, next_epoch(), now)
        {
            crate::log!(INFO, "own channel: socket open to the server");
            state.peers.insert(SERVER, peer);
            CARRIER.client_open.store(true, Ordering::Release);
            change = Some(Change::Opened);
        }
    }
    let State { peers, inbox, .. } = state;
    let Some(peer) = peers.get_mut(&SERVER) else {
        if change == Some(Change::Lost) {
            CARRIER.client_open.store(false, Ordering::Release);
        }
        return change;
    };
    let restarts = peer.link.restarts();
    receive(game, session, SERVER, peer, inbox, INBOX_BYTES, now);
    if peer.link.restarts() != restarts {
        // The server started its link over and forgot this client.
        change = Some(Change::Opened);
    }
    if loader.developer_tools {
        probe_step(peer);
    }
    if !send_due(game, session, SERVER, peer, now) {
        peers.remove(&SERVER);
        CARRIER.client_open.store(false, Ordering::Release);
        return Some(Change::Lost);
    }
    change
}

/// Puts `inbox` back, emptied with its room kept, while `side` still serves
/// `session`.
fn give_back(side: &Mutex<Option<State>>, session: usize, mut inbox: Inbox) {
    inbox.bytes.clear();
    inbox.items.clear();
    if let Some(mut guard) = side.try_relock()
        && let Some(state) = guard.as_mut()
        && state.session == session
    {
        state.inbox = inbox;
    }
}

/// Forgets the client's link and what a hosted game's halves held for
/// each other, when the client leaves its server or a hosted game ends.
pub(crate) fn forget() {
    *CARRIER.client.relock() = None;
    CARRIER.client_open.store(false, Ordering::Release);
    CARRIER.server_waiting.store(false, Ordering::Release);
    CARRIER.client_waiting.store(false, Ordering::Release);
    CARRIER.to_server.relock().clear();
    CARRIER.to_client.relock().clear();
    CARRIER.host_key.store(0, Ordering::Release);
    CARRIER.hosted_hello.store(false, Ordering::Release);
}
/// Packets and messages the carrier dropped so far.
pub(crate) fn dropped() -> u64 {
    CARRIER.dropped.load(Ordering::Relaxed)
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
        inbox: Inbox::default(),
    })
}

/// Takes the packets that came on `peer`'s socket and gathers the messages
/// they complete in `inbox`, at most `share` bytes of them this step, so
/// every peer gets its turn; the rest wait in the game's socket.
fn receive(
    game: &Game,
    session: usize,
    key: u64,
    peer: &mut Peer,
    inbox: &mut Inbox,
    share: usize,
    now: Instant,
) {
    let Peer {
        socket,
        link,
        heard,
        rate,
        probe,
        ..
    } = peer;
    let mut packet = [0u8; LARGEST_PACKET];
    let start = inbox.bytes.len();
    for _ in 0..RECEIVES_PER_STEP {
        if inbox.bytes.len() >= INBOX_BYTES || inbox.bytes.len() - start >= share {
            return;
        }
        let Some(taken) = game.take(session, *socket, &mut packet) else {
            return;
        };
        let Taken::Bytes(size) = taken else {
            drop_one();
            continue;
        };
        let Some(bytes) = packet.get(..size) else {
            drop_one();
            continue;
        };
        if !rate.admit_at(now, PACKET_RATE, PACKET_BURST) {
            drop_one();
            continue;
        }
        match link.receive(bytes, |message| delivered(key, message, probe, inbox)) {
            Ok(()) => *heard = true,
            Err(_) => drop_one(),
        }
    }
}

/// Sends what `peer`'s link has due; false once the peer stopped answering.
fn send_due(game: &Game, session: usize, key: u64, peer: &mut Peer, now: Instant) -> bool {
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

/// A message from `key` came whole: a probe is counted, anything else
/// gathered.
fn delivered(key: u64, message: &[u8], probe: &mut Probe, inbox: &mut Inbox) {
    let Ok((channel, payload)) = open(message) else {
        drop_one();
        return;
    };
    if channel == CONTROL
        && let Some((id, whole)) = read_probe(payload)
    {
        probed(key, probe, id, whole, payload.len());
        return;
    }
    let start = inbox.bytes.len();
    inbox.bytes.extend_from_slice(payload);
    inbox.items.push(Item {
        from: key,
        channel,
        span: (start, inbox.bytes.len()),
    });
}

/// Counts and logs probe `id` from `key`, the first [`PROBES`] only.
#[cold]
fn probed(key: u64, probe: &mut Probe, id: u8, whole: bool, size: usize) {
    if probe.received >= PROBES {
        return;
    }
    probe.out_of_order |= id != probe.received;
    probe.received += 1;
    probe.whole += u8::from(whole);
    crate::log!(
        INFO,
        "own channel check from {key:016x}: probe {id}, {size} bytes, {}",
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
fn drop_one() {
    let count = CARRIER.dropped.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_multiple_of(1000) {
        crate::log!(
            WARN,
            "own channel: {count} packets dropped, unreadable or over a bound"
        );
    }
}
