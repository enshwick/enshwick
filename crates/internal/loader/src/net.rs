//! Messages between a join mod's server part and its client part, carried in
//! chat lines no player sees.
//!
//! A client sends a line `/enshwick-net <frame>` into its chat queue; the
//! server's chat link takes every such line before the game posts it, so the
//! frame never reaches another player. The server sends a frame as a line of
//! the kind its own replies use, posted to one connection alone; the
//! client's link on its chat post takes the line where the client posts what
//! the server sent, before the chat shows it.
//!
//! Only clients that announce themselves get frames: a client with the
//! loader sends a hello with the protocol version and the join mods it runs
//! when it joins a server whose rules name Enshwick, or a game whose host's
//! lobby does (see the lobby module), the server answers unless it keeps
//! the player out (see the admission module), and from then on both sides
//! send. A vanilla client never says hello, so it never receives a frame,
//! and a server that runs join mods disconnects it. A client never says
//! hello to a game that names no Enshwick, whose chat would show the line.
//!
//! The frames and control messages are in [`wire`](crate::wire). Each side keeps
//! a rate per peer and drops what is over it, counting the drops for the
//! status summary.

use crate::mod_index::ModIndex;
use crate::registry::Registry;
use core::ffi::c_void;
use enshwick::ecs::Entity;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use enshwick::abi::{Header, NetMessage, NetMessageFn, Status};
use rustc_hash::FxHashMap;

use crate::host::{Loader, Registered};
use crate::locks::Relock;

use crate::wire::{
    CONTROL, Frame, ITEM_DATA, NOTICE, PART_BYTES, PREFIX, Peer, UI_DEMO, WELCOME, assemble,
    control, demo_position, frames, hello, parse, read_hello, remote_message,
};
pub(crate) use crate::wire::{MAX_CONTROL, MAX_MESSAGE, frame_body, is_frame};

/// Frames waiting to leave at most, per side.
const MAX_WAITING: usize = 512;
/// Frames a client takes from its server per second, and in one burst. The
/// server's chat sends at most 16 lines a step to all its players and holds
/// at most 256 waiting, so its own frames always fit; a server that sends
/// more is dropped from.
const SERVER_FRAME_RATE: f64 = 1024.0;
const SERVER_FRAME_BURST: f64 = 512.0;
/// Seconds between a client's hellos until the server answers.
const HELLO_EVERY: f64 = 5.0;
/// Clients whose hello is on its way in, at most, on the server.
const MAX_GREETING: usize = 64;

/// One channel a mod opened.
#[derive(Debug)]
struct Channel {
    id: u32,
    name: String,
    handler: Registered<NetMessageFn>,
}

/// What the net keeps.
#[derive(Debug, Default)]
struct State {
    /// On the server: peers by account key that said hello. On a client:
    /// the server, [`NetMessage::SERVER`], once it answered.
    peers: FxHashMap<u64, Peer>,
    /// Lines waiting for the client's chat queue.
    outbox: VecDeque<String>,
    /// The id of the next message sent.
    next_message: u16,
    /// When the client last said hello.
    hello_at: Option<Instant>,
    /// Rates of the peers sending, by key, on the server.
    sending: FxHashMap<u64, Peer>,
    /// On a client: the rate of the frames its server sends.
    incoming: Option<Peer>,
    /// On the server: the hellos on their way in from clients that are no
    /// peers yet, by key.
    greeting: FxHashMap<u64, Peer>,
}

static CHANNELS: Registry<Channel> = Registry::new();
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Set while the client's outbox holds anything, so an idle frame takes no
/// lock.
static WAITING: AtomicBool = AtomicBool::new(false);
/// Set on a client while the server it joined advertises Enshwick, which
/// takes frame lines out of the chat; another server would show them.
static SERVER_RUNS_ENSHWICK: AtomicBool = AtomicBool::new(false);
/// Messages dropped: over a rate, too many waiting, or unreadable.
static DROPPED: AtomicU64 = AtomicU64::new(0);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.relock();
    f(guard.get_or_insert_with(State::default))
}

/// [`state`] for a per-frame path: `None`, without waiting, while another
/// thread holds the state.
fn try_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    let mut guard = match STATE.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    Some(f(guard.get_or_insert_with(State::default)))
}

/// The id of mod `id`'s channel `name`.
fn channel_id(id: &str, name: &str) -> u32 {
    let id = enshwick_gamedata::reflect::fnv1a32(format!("{id}/{name}").as_bytes());
    // Zero is the protocol's own.
    id.max(1)
}

/// Opens mod `owner`'s channel `name`; its id.
pub(crate) fn open(
    loader: &Loader,
    owner: ModIndex,
    name: &str,
    callback: NetMessageFn,
    user: *mut c_void,
) -> Result<u32, Status> {
    if CHANNELS.is_sealed() {
        return Err(Status::TOO_LATE);
    }
    let record = loader.mods.get(owner.get()).ok_or(Status::BAD_ARGS)?;
    if !record
        .manifest
        .flags
        .contains(enshwick::abi::ManifestFlags::JOIN)
    {
        return Err(Status::REFUSED);
    }
    if name.is_empty() || name.len() > 64 {
        return Err(Status::BAD_ARGS);
    }
    let id = channel_id(&record.id, name);
    CHANNELS.edit(|pending| {
        if pending.iter().any(|c| c.id == id) {
            return Err(Status::REFUSED);
        }
        pending.push(Channel {
            id,
            name: format!("{}/{name}", record.id),
            handler: Registered::new(owner, callback, user),
        });
        Ok(id)
    })?
}

/// Seals the channels.
pub(crate) fn link() {
    let channels = CHANNELS.seal();
    if !channels.is_empty() {
        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        crate::log!(INFO, "mod messages: channels {}", names.join(", "));
    }
}

/// The frames of a control message, numbered when it takes more than one.
pub(crate) fn control_frames(data: &[u8]) -> Option<Vec<String>> {
    let id = if data.len() > PART_BYTES {
        CONTROL_MESSAGE.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    };
    frames(CONTROL, id, data)
}

/// Hands a message to the mod that opened its channel.
fn deliver(channel: u32, peer: u64, data: &[u8]) {
    let Some(target) = CHANNELS
        .sealed()
        .and_then(|channels| channels.iter().find(|c| c.id == channel))
    else {
        dropped();
        return;
    };
    if !target.handler.live() {
        return;
    }
    let message = NetMessage {
        header: Header::of::<NetMessage>(),
        channel,
        reserved: 0,
        peer,
        data: data.as_ptr(),
        size: u32::try_from(data.len()).unwrap_or(0),
        reserved_tail: 0,
    };
    // SAFETY: the handler lives in the mod's DLL, which stays loaded, and
    // takes the pointer it registered; the safe layer catches its panics.
    crate::profile::call(
        target.handler.owner,
        crate::profile::Feature::Messages,
        || unsafe { (target.handler.callback)(&raw const message, target.handler.user()) },
    );
}

/// Sends the client of the player with account key `key` a notice for the
/// player's entity `entity`; the client shows `text`.
pub(crate) fn notice(key: u64, entity: Entity, text: &str) -> Result<(), Status> {
    if !ready(key) {
        return Err(Status::MISSING);
    }
    let mut data = control(NOTICE);
    data.extend_from_slice(&entity.0.to_le_bytes());
    data.extend_from_slice(text.as_bytes());
    for line in control_frames(&data).ok_or(Status::BAD_ARGS)? {
        crate::chat::send_frame(key, &line);
    }
    Ok(())
}

/// Sends item data records to the client of the player with account key
/// `key`, or to every client that said hello for 0; `records` fits one
/// frame.
pub(crate) fn item_data(key: u64, records: &[u8]) {
    let mut data = control(ITEM_DATA);
    data.extend_from_slice(records);
    let peers: Vec<u64> = if key == 0 {
        state(|s| s.peers.keys().copied().collect())
    } else {
        vec![key]
    };
    let Some(lines) = control_frames(&data) else {
        crate::log!(WARN, "mod messages: item data records too long to send");
        return;
    };
    for peer in peers.into_iter().filter(|&p| p != NetMessage::SERVER) {
        for line in &lines {
            crate::chat::send_frame(peer, line);
        }
    }
}

/// Tells the client of the player with account key `key` to open the
/// interface demo, with where the player stands.
pub(crate) fn ui_demo(key: u64, at: Option<[i64; 3]>) -> Result<(), Status> {
    if !ready(key) {
        return Err(Status::MISSING);
    }
    let mut data = control(UI_DEMO);
    for axis in at.iter().flatten() {
        data.extend_from_slice(&axis.to_le_bytes());
    }
    for line in control_frames(&data).ok_or(Status::BAD_ARGS)? {
        crate::chat::send_frame(key, &line);
    }
    Ok(())
}

/// Numbers the multi-part control messages this side sends.
static CONTROL_MESSAGE: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(1);

/// The frames of a settings message, none for one too long to send.
fn remote_frames(message: &crate::remote::Message) -> Vec<String> {
    let mut data = control(message.kind());
    data.extend_from_slice(&message.body());
    let id = CONTROL_MESSAGE.fetch_add(1, Ordering::Relaxed);
    frames(CONTROL, id, &data).unwrap_or_else(|| {
        crate::log!(WARN, "mod messages: a settings message too long to send");
        Vec::new()
    })
}

/// Sends a settings message to the client of the player with account key
/// `key`, when it takes messages.
pub(crate) fn to_player(key: u64, message: &crate::remote::Message) {
    if !ready(key) {
        return;
    }
    for line in remote_frames(message) {
        crate::chat::send_frame(key, &line);
    }
}

/// Sends a settings message to every client that runs Enshwick.
pub(crate) fn broadcast_control(message: &crate::remote::Message) {
    let keys: Vec<u64> = state(|s| {
        s.peers
            .keys()
            .copied()
            .filter(|&k| k != NetMessage::SERVER)
            .collect()
    });
    let lines = remote_frames(message);
    if keys.is_empty() || lines.is_empty() {
        return;
    }
    for key in keys {
        for line in &lines {
            crate::chat::send_frame(key, line);
        }
    }
}

/// Sends a settings message from the client to its server; false while the
/// server takes no messages.
pub(crate) fn to_server(message: &crate::remote::Message) -> bool {
    if !ready(NetMessage::SERVER) {
        return false;
    }
    let lines = remote_frames(message);
    if lines.is_empty() {
        return false;
    }
    state(|s| {
        if s.outbox.len() + lines.len() > MAX_WAITING {
            return false;
        }
        s.outbox.extend(lines);
        WAITING.store(true, Ordering::Release);
        true
    })
}

/// Makes `key` a peer after its hello; true for a new one.
fn greet(state: &mut State, key: u64) -> bool {
    let pending = state.greeting.remove(&key);
    match state.peers.entry(key) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(pending.unwrap_or_default());
            true
        }
        std::collections::hash_map::Entry::Occupied(_) => false,
    }
}

/// Adds a frame from the client `key` on the server; the whole message once
/// its last part came, with its channel and whether `key` is a peer. A
/// client that is no peer yet may only send control messages, its hello;
/// each message takes from the client's rate.
fn take_frame(state: &mut State, key: u64, frame: Frame) -> Option<(u32, Vec<u8>, bool)> {
    if key == 0 {
        return None;
    }
    let known = state.peers.contains_key(&key);
    let peer = if known {
        state.peers.get_mut(&key)?
    } else {
        if frame.channel() != CONTROL {
            return None;
        }
        let room = state.greeting.len() < MAX_GREETING;
        match state.greeting.entry(key) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) if room => entry.insert(Peer::new()),
            std::collections::hash_map::Entry::Vacant(_) => return None,
        }
    };
    // A peer's message takes one from its rate; a stranger's every frame.
    if (frame.index() == 0 || !known) && !peer.admit() {
        dropped();
        return None;
    }
    let (channel, data) = assemble(peer, frame)?;
    Some((channel, data, known))
}

/// Makes room for the hello of the client `key` in a full store by
/// forgetting the clients whose key holds no live slot in `live`, read only
/// then; nothing is forgotten when the slots do not read.
fn make_greeting_room(state: &mut State, key: u64, live: impl FnOnce() -> Option<Vec<u64>>) {
    if !state.peers.contains_key(&key)
        && !state.greeting.contains_key(&key)
        && state.greeting.len() >= MAX_GREETING
        && let Some(live) = live()
    {
        keep_greeting_in(state, &live);
    }
}

/// Forgets the hellos on their way in from clients whose key holds no live
/// player slot in `live`.
pub(crate) fn keep_greeting(live: &[u64]) {
    state(|s| keep_greeting_in(s, live));
}

fn keep_greeting_in(state: &mut State, live: &[u64]) {
    state.greeting.retain(|key, _| live.contains(key));
}

/// Takes a frame line a client sent, on the server's game thread: `args` is
/// the frame after the line's prefix, `key` the client's account key.
pub(crate) fn server_received(key: u64, args: &str) {
    if key == 0 {
        return;
    }
    let Some(frame) = parse(args) else {
        dropped();
        return;
    };
    let Some((channel, data, known)) = state(|s| {
        make_greeting_room(s, key, crate::world::live_keys);
        take_frame(s, key, frame)
    }) else {
        return;
    };
    if channel != CONTROL {
        deliver(channel, key, &data);
        return;
    }
    if let Some(hello) = read_hello(&data) {
        said_hello(key, &hello);
        return;
    }
    if known
        && let Some(loader) = crate::host::loader()
        && let Some(message) = remote_message(&data)
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
    let fresh = state(|s| greet(s, key));
    if fresh {
        crate::log!(INFO, "mod messages: the client of {key:016x} said hello");
    }
    for line in frames(CONTROL, 0, &control(WELCOME)).unwrap_or_default() {
        crate::chat::send_frame(key, &line);
    }
    if fresh {
        for records in crate::item_data::hello() {
            item_data(key, &records);
        }
        crate::recipes::hello(key);
        crate::remote::sync_to(key);
    }
}

/// Takes a frame line the server sent, on the client; the line's text after
/// the prefix. False when the line is no frame.
pub(crate) fn client_received(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(PREFIX) else {
        return false;
    };
    if !state(|s| admit_from_server(s, Instant::now())) {
        dropped();
        return true;
    }
    let Some(frame) = parse(rest) else {
        dropped();
        return true;
    };
    if frame.channel() == CONTROL {
        // Only the remote settings messages take more than one frame.
        let single = frame.total() == 1;
        if single && frame.part() == control(WELCOME) {
            let fresh = state(|s| s.peers.insert(NetMessage::SERVER, Peer::new()).is_none());
            if fresh {
                crate::log!(INFO, "mod messages: the server answered");
                crate::actions::joined();
            }
        } else if single
            && let Some(records) = frame.part().strip_prefix(control(ITEM_DATA).as_slice())
        {
            crate::item_data::received(records);
        } else if single
            && let Some(rest) = frame.part().strip_prefix(control(NOTICE).as_slice())
            && let Some((entity, text)) = rest.split_first_chunk::<4>()
            && let Ok(text) = core::str::from_utf8(text)
        {
            crate::notices::queue(Entity(u32::from_le_bytes(*entity)), text);
        } else if single && let Some(rest) = frame.part().strip_prefix(control(UI_DEMO).as_slice())
        {
            crate::ui::open_demo(demo_position(rest));
        } else {
            let whole = state(|s| {
                s.peers
                    .get_mut(&NetMessage::SERVER)
                    .and_then(|peer| assemble(peer, frame))
            });
            if let Some((_, data)) = whole
                && let Some(message) = remote_message(&data)
            {
                crate::remote::client(message);
            }
        }
        return true;
    }
    let message = state(|s| {
        s.peers
            .get_mut(&NetMessage::SERVER)
            .and_then(|peer| assemble(peer, frame))
    });
    if let Some((channel, data)) = message {
        deliver(channel, NetMessage::SERVER, &data);
    }
    true
}

/// Takes one frame from the server's rate; false when it is spent.
fn admit_from_server(state: &mut State, now: Instant) -> bool {
    state
        .incoming
        .get_or_insert_with(|| Peer::with_tokens(SERVER_FRAME_BURST))
        .admit_at(now, SERVER_FRAME_RATE, SERVER_FRAME_BURST)
}

/// Notes whether the server the client joins advertises Enshwick, and
/// forgets the last server's state.
pub(crate) fn joining(runs_enshwick: bool) {
    forget();
    crate::recipes::joining();
    SERVER_RUNS_ENSHWICK.store(runs_enshwick, Ordering::Release);
}

/// Forgets every peer, when the client changes servers or a hosted game
/// ends.
pub(crate) fn forget() {
    let mut guard = STATE.relock();
    *guard = None;
    WAITING.store(false, Ordering::Release);
    crate::item_data::forget_client();
}

/// Forgets a player's client, when the player leaves.
pub(crate) fn forget_peer(key: u64) {
    state(|s| {
        s.peers.remove(&key);
        s.sending.remove(&key);
        s.greeting.remove(&key);
    });
}

/// Whether `peer` takes messages: on the server, a player's client that said
/// hello; on a client, the server once it answered ([`NetMessage::SERVER`]).
pub(crate) fn ready(peer: u64) -> bool {
    state(|s| s.peers.contains_key(&peer))
}

/// The account keys of the clients that said hello, on the server.
pub(crate) fn clients() -> Vec<u64> {
    state(|s| {
        s.peers
            .keys()
            .copied()
            .filter(|&k| k != NetMessage::SERVER)
            .collect()
    })
}

/// Sends `data` on `channel` from mod `owner`: to the server for
/// [`NetMessage::SERVER`], else from the server to the player with account
/// key `peer`, or to every client that said hello for 0.
pub(crate) fn send(
    loader: &Loader,
    owner: ModIndex,
    channel: u32,
    peer: u64,
    data: &[u8],
) -> Result<(), Status> {
    let open = CHANNELS
        .sealed()
        .and_then(|channels| channels.iter().find(|c| c.id == channel));
    if open.is_none_or(|c| c.handler.owner != owner) {
        return Err(Status::BAD_ARGS);
    }
    if data.len() > MAX_MESSAGE {
        return Err(Status::BAD_ARGS);
    }
    if peer == NetMessage::SERVER {
        return send_to_server(channel, data);
    }
    let serving = crate::world::hosting() || loader.side == enshwick::abi::Side::SERVER;
    if !serving {
        return Err(Status::WRONG_THREAD);
    }
    send_to_clients(channel, peer, data)
}

fn send_to_clients(channel: u32, peer: u64, data: &[u8]) -> Result<(), Status> {
    let lines = state(|s| {
        let keys: Vec<u64> = if peer == 0 {
            s.peers
                .keys()
                .copied()
                .filter(|&k| k != NetMessage::SERVER)
                .collect()
        } else if s.peers.contains_key(&peer) {
            vec![peer]
        } else {
            return Err(Status::MISSING);
        };
        let message = s.next_message;
        s.next_message = s.next_message.wrapping_add(1);
        let lines = frames(channel, message, data).ok_or(Status::BAD_ARGS)?;
        let mut out = Vec::new();
        for key in keys {
            let rate = s.sending.entry(key).or_insert_with(Peer::new);
            if !rate.admit() {
                dropped();
                continue;
            }
            out.extend(lines.iter().map(|l| (key, l.clone())));
        }
        Ok(out)
    })?;
    for (key, line) in lines {
        crate::chat::send_frame(key, &line);
    }
    Ok(())
}

fn send_to_server(channel: u32, data: &[u8]) -> Result<(), Status> {
    state(|s| {
        if !s.peers.contains_key(&NetMessage::SERVER) {
            return Err(Status::MISSING);
        }
        let rate = s
            .sending
            .entry(NetMessage::SERVER)
            .or_insert_with(Peer::new);
        let lines = frames(channel, s.next_message, data).ok_or(Status::BAD_ARGS)?;
        if !rate.admit() || s.outbox.len() + lines.len() > MAX_WAITING {
            dropped();
            return Err(Status::REFUSED);
        }
        s.next_message = s.next_message.wrapping_add(1);
        s.outbox.extend(lines);
        WAITING.store(true, Ordering::Release);
        Ok(())
    })
}

/// The lines waiting for the client's chat queue, and a hello when the
/// server has not answered yet; on the client's thread that sends chat,
/// every frame, so none while another thread holds the state.
pub(crate) fn client_lines() -> Vec<String> {
    crate::lobby::poll();
    try_state(|s| {
        let mut lines = Vec::new();
        let answered = s.peers.contains_key(&NetMessage::SERVER);
        let due = s
            .hello_at
            .is_none_or(|at| at.elapsed().as_secs_f64() >= HELLO_EVERY);
        let modded = SERVER_RUNS_ENSHWICK.load(Ordering::Acquire)
            || crate::lobby::host_runs_enshwick()
            || crate::world::hosting();
        if !answered && due && modded {
            s.hello_at = Some(Instant::now());
            let mods = crate::advertise::mod_list(&crate::host::join_mods());
            let nonce = if crate::world::hosting() {
                crate::admission::nonce()
            } else {
                0
            };
            lines.extend(control_frames(&hello(&mods, nonce)).unwrap_or_default());
        }
        if WAITING.swap(false, Ordering::AcqRel) {
            lines.extend(s.outbox.drain(..));
        }
        lines
    })
    .unwrap_or_default()
}

/// Counts a dropped message and logs the first and every hundredth.
fn dropped() {
    let count = DROPPED.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_multiple_of(100) {
        crate::log!(
            WARN,
            "mod messages: {count} dropped, over a rate or unreadable"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Partial;

    pub(super) fn parsed(lines: &[String]) -> Vec<Frame> {
        lines
            .iter()
            .map(|l| parse(l.strip_prefix(PREFIX).unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn a_repeated_hello_keeps_the_peer_and_spends_its_rate() {
        let mut state = State::default();
        assert!(greet(&mut state, 7));
        state.peers.get_mut(&7).unwrap().partial.insert(
            (1, 1),
            Partial {
                total: 2,
                parts: Vec::new(),
            },
        );
        let lines = control_frames(&hello("you.trading-post@0.3.0", 0)).unwrap();
        let hellos =
            core::iter::repeat_with(|| take_frame(&mut state, 7, parsed(&lines).remove(0)))
                .take(100)
                .take_while(Option::is_some)
                .count();
        assert!(hellos < 50, "{hellos}");
        assert!(!greet(&mut state, 7));
        assert_eq!(state.peers[&7].partial.len(), 1);
    }

    #[test]
    fn a_new_client_s_hello_in_several_frames_comes_back_whole() {
        let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
        let mods = many.join(",");
        let lines = control_frames(&hello(&mods, 0)).unwrap();
        assert!(lines.len() > 1);
        let mut state = State::default();
        let mut whole = None;
        for frame in parsed(&lines) {
            whole = take_frame(&mut state, 7, frame);
        }
        let (channel, data, known) = whole.unwrap();
        assert_eq!((channel, known), (CONTROL, false));
        assert_eq!(read_hello(&data).map(|h| h.mods), Some(mods));
        assert!(state.peers.is_empty(), "a hello alone makes no peer");
    }

    #[test]
    fn a_stranger_s_frames_on_a_mod_channel_are_not_taken() {
        let mut state = State::default();
        let frame = parsed(&frames(5, 1, b"data").unwrap()).remove(0);
        assert_eq!(take_frame(&mut state, 7, frame), None);
    }

    #[test]
    fn frames_from_an_unknown_sender_are_not_taken() {
        let mut state = State::default();
        let frame = parsed(&control_frames(&hello("", 0)).unwrap()).remove(0);
        assert_eq!(take_frame(&mut state, 0, frame), None);
        assert!(state.greeting.is_empty());
    }

    #[test]
    fn every_frame_of_a_stranger_takes_from_its_rate() {
        let mut state = State::default();
        let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
        let lines = control_frames(&hello(&many.join(","), 0)).unwrap();
        for _ in 0..200 {
            let _taken = take_frame(&mut state, 7, parsed(&lines).remove(1));
        }
        assert!(state.greeting[&7].tokens < 1.0, "the rate ran out");
    }

    #[test]
    fn strangers_waiting_to_say_hello_are_bounded_and_follow_the_live_slots() {
        let mut state = State::default();
        let many: Vec<String> = (0..100).map(|i| format!("mod-{i:04}@1.0.0")).collect();
        let lines = control_frames(&hello(&many.join(","), 0)).unwrap();
        for key in 1..=1000 {
            let _taken = take_frame(&mut state, key, parsed(&lines).remove(0));
        }
        assert!(state.greeting.len() <= MAX_GREETING);
        keep_greeting_in(&mut state, &[3, 4]);
        let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
        kept.sort_unstable();
        assert_eq!(kept, [3, 4]);
    }

    #[test]
    fn a_full_greeting_store_makes_room_from_keys_without_a_live_slot() {
        let mut state = State::default();
        let lines = control_frames(&hello("you.trading-post@0.3.0", 0)).unwrap();
        for key in 1..=MAX_GREETING as u64 {
            let _taken = take_frame(&mut state, key, parsed(&lines).remove(0));
        }
        assert_eq!(state.greeting.len(), MAX_GREETING);
        let newcomer = MAX_GREETING as u64 + 1;
        make_greeting_room(&mut state, 1, || unreachable!("a known key needs no room"));
        make_greeting_room(&mut state, newcomer, || None);
        assert_eq!(
            state.greeting.len(),
            MAX_GREETING,
            "no slots read, none forgotten"
        );
        make_greeting_room(&mut state, newcomer, || Some(vec![2, newcomer]));
        assert!(take_frame(&mut state, newcomer, parsed(&lines).remove(0)).is_some());
        let mut kept: Vec<u64> = state.greeting.keys().copied().collect();
        kept.sort_unstable();
        assert_eq!(kept, [2, newcomer]);
    }

    #[test]
    fn a_client_takes_its_server_s_frames_up_to_a_rate() {
        let mut state = State::default();
        let start = Instant::now();
        let taken = (0..2000)
            .take_while(|_| admit_from_server(&mut state, start))
            .count();
        assert_eq!(taken, 512, "one burst");
        assert!(!admit_from_server(&mut state, start), "a flood is dropped");
        let later = start + std::time::Duration::from_millis(250);
        let refilled = (0..2000)
            .take_while(|_| admit_from_server(&mut state, later))
            .count();
        assert_eq!(refilled, 256, "a quarter second of the rate");
        // The server's own sending fits: 256 lines waiting, then 16 a step.
        let mut honest = State::default();
        assert!((0..256).all(|_| admit_from_server(&mut honest, start)));
        assert!((1..=60).all(|step| {
            let at = start + std::time::Duration::from_secs_f64(f64::from(step) / 60.0);
            (0..16).all(|_| admit_from_server(&mut honest, at))
        }));
    }

    #[test]
    fn channel_ids_are_per_mod_and_never_the_protocol_s() {
        assert_ne!(channel_id("a", "x"), channel_id("b", "x"));
        assert_ne!(channel_id("a", "x"), CONTROL);
    }
}
