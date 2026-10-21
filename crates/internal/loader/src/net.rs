//! Messages between a join mod's server part and its client part, carried in
//! chat lines no player sees.
//!
//! A client sends a line `/enshwick-net <frame>` into its chat queue; the
//! server's chat link takes every `/` line before the game posts it, so the
//! frame never reaches another player. The server sends a frame as a line of
//! the kind its own replies use, posted to one connection alone; the
//! client's link on its chat post takes the line where the client posts what
//! the server sent, before the chat shows it.
//!
//! Only clients that announce themselves get frames: a client with the
//! loader sends a hello with the protocol version when it joins, the server
//! answers, and from then on both sides send. A vanilla client never says
//! hello, so it never receives a frame; the join check makes sure a client
//! that said hello runs the server's join mods.
//!
//! A frame is base64 of a small header and up to one line's worth of the
//! message; a message of up to [`MAX_MESSAGE`] bytes takes up to
//! [`MAX_PARTS`] frames. Each side keeps a rate per peer and drops what is
//! over it, counting the drops for the status summary.

use core::ffi::c_void;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use enshwick_loader_api::abi::{Header, NetMessage, NetMessageFn, Status};

use crate::host::{Loader, Registered, take};

/// The chat command that carries frames from clients.
pub(crate) const COMMAND: &str = "enshwick-net";
/// The prefix of a frame line, both ways.
const PREFIX: &str = "/enshwick-net ";
/// The version of the frame protocol.
const PROTOCOL: u8 = 1;
/// The largest message a mod may send, in bytes.
pub(crate) const MAX_MESSAGE: usize = 4096;
/// The most frames one message takes.
const MAX_PARTS: usize = 8;
/// Message bytes in one frame: a line holds 1024 bytes, less the prefix,
/// as base64 of the header and the part.
const PART_BYTES: usize = 720;
/// The channel of the protocol's own frames.
const CONTROL: u32 = 0;
/// Control frames: a client's hello and the server's answer.
const HELLO: u8 = 1;
const WELCOME: u8 = 2;
/// A control frame from the server: a notice for the client to show.
const NOTICE: u8 = 3;
/// A control frame from the server: item data records.
const ITEM_DATA: u8 = 4;
/// A control frame from the server: open the interface demo.
const UI_DEMO: u8 = 5;
/// Messages each peer may send per second, and in one burst.
const RATE: f64 = 20.0;
const BURST: f64 = 40.0;
/// Frames waiting to leave at most, per side.
const MAX_WAITING: usize = 512;
/// Seconds between a client's hellos until the server answers.
const HELLO_EVERY: f64 = 5.0;

/// A frame's header: protocol, channel, message id, part and part count.
const HEADER: usize = 1 + 4 + 2 + 1 + 1;

/// One channel a mod opened.
#[derive(Debug)]
struct Channel {
    id: u32,
    name: String,
    handler: Registered<NetMessageFn>,
}

/// Parts of a message on their way in.
#[derive(Debug, Default)]
struct Partial {
    total: u8,
    parts: Vec<Option<Vec<u8>>>,
}

/// A peer's messages on their way in, and its rate.
#[derive(Debug)]
struct Peer {
    partial: HashMap<(u32, u16), Partial>,
    tokens: f64,
    seen: Instant,
}

impl Peer {
    fn new() -> Self {
        Self {
            partial: HashMap::new(),
            tokens: BURST,
            seen: Instant::now(),
        }
    }

    /// Takes one message from the peer's rate; false when it is spent.
    fn admit(&mut self) -> bool {
        let now = Instant::now();
        let gained = now.duration_since(self.seen).as_secs_f64() * RATE;
        self.seen = now;
        self.tokens = (self.tokens + gained).min(BURST);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

/// What the net keeps.
#[derive(Debug, Default)]
struct State {
    /// On the server: peers by account key that said hello. On a client:
    /// the server, [`NetMessage::SERVER`], once it answered.
    peers: HashMap<u64, Peer>,
    /// Lines waiting for the client's chat queue.
    outbox: VecDeque<String>,
    /// The id of the next message sent.
    next_message: u16,
    /// When the client last said hello.
    hello_at: Option<Instant>,
    /// Rates of the peers sending, by key, on the server.
    sending: HashMap<u64, Peer>,
}

static PENDING: Mutex<Vec<Channel>> = Mutex::new(Vec::new());
static CHANNELS: OnceLock<Box<[Channel]>> = OnceLock::new();
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
    let mut guard = STATE.lock().unwrap_or_else(PoisonError::into_inner);
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
    let id = enshwick_loader_reflect::fnv1a32(format!("{id}/{name}").as_bytes());
    // Zero is the protocol's own.
    id.max(1)
}

/// Opens mod `owner`'s channel `name`; its id.
pub(crate) fn open(
    loader: &Loader,
    owner: usize,
    name: &str,
    callback: NetMessageFn,
    user: *mut c_void,
) -> Result<u32, Status> {
    if CHANNELS.get().is_some() {
        return Err(Status::TOO_LATE);
    }
    let record = loader.mods.get(owner).ok_or(Status::BAD_ARGS)?;
    if !record
        .manifest
        .flags
        .contains(enshwick_loader_api::abi::ManifestFlags::JOIN)
    {
        return Err(Status::REFUSED);
    }
    if name.is_empty() || name.len() > 64 {
        return Err(Status::BAD_ARGS);
    }
    let id = channel_id(&record.id, name);
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    if pending.iter().any(|c| c.id == id) {
        return Err(Status::REFUSED);
    }
    pending.push(Channel {
        id,
        name: format!("{}/{name}", record.id),
        handler: Registered::new(owner, callback, user),
    });
    Ok(id)
}

/// Seals the channels.
pub(crate) fn link() {
    let channels = take(&PENDING);
    if !channels.is_empty() {
        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        crate::log!(INFO, "mod messages: channels {}", names.join(", "));
    }
    let _first = CHANNELS.set(channels.into_boxed_slice());
}

/// The frames of one message: base64 lines with the prefix; `None` for a
/// message longer than [`MAX_PARTS`] frames, which no peer takes.
fn frames(channel: u32, message: u16, data: &[u8]) -> Option<Vec<String>> {
    let parts: Vec<&[u8]> = if data.is_empty() {
        vec![&[]]
    } else {
        data.chunks(PART_BYTES).collect()
    };
    if parts.len() > MAX_PARTS {
        return None;
    }
    let total = u8::try_from(parts.len()).ok()?;
    let lines = parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let mut bytes = Vec::with_capacity(HEADER + part.len());
            bytes.push(PROTOCOL);
            bytes.extend_from_slice(&channel.to_le_bytes());
            bytes.extend_from_slice(&message.to_le_bytes());
            bytes.push(u8::try_from(index).unwrap_or(u8::MAX));
            bytes.push(total);
            bytes.extend_from_slice(part);
            format!("{PREFIX}{}", URL_SAFE_NO_PAD.encode(bytes))
        })
        .collect();
    Some(lines)
}

/// The frames of a control message, numbered when it takes more than one.
fn control_frames(data: &[u8]) -> Option<Vec<String>> {
    let id = if data.len() > PART_BYTES {
        CONTROL_MESSAGE.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    };
    frames(CONTROL, id, data)
}

/// A frame's header and part.
#[derive(Debug, PartialEq, Eq)]
struct Frame {
    channel: u32,
    message: u16,
    index: u8,
    total: u8,
    part: Vec<u8>,
}

/// The frame a line carries, after the prefix.
fn parse(text: &str) -> Option<Frame> {
    let bytes = URL_SAFE_NO_PAD.decode(text.trim()).ok()?;
    if bytes.len() < HEADER || bytes[0] != PROTOCOL {
        return None;
    }
    let frame = Frame {
        channel: u32::from_le_bytes(bytes[1..5].try_into().ok()?),
        message: u16::from_le_bytes(bytes[5..7].try_into().ok()?),
        index: bytes[7],
        total: bytes[8],
        part: bytes[HEADER..].to_vec(),
    };
    let fits = frame.total >= 1
        && usize::from(frame.total) <= MAX_PARTS
        && frame.index < frame.total
        && frame.part.len() <= PART_BYTES;
    fits.then_some(frame)
}

/// Adds `frame` from `peer`; the whole message once its last part came.
fn assemble(peer: &mut Peer, frame: Frame) -> Option<(u32, Vec<u8>)> {
    let key = (frame.channel, frame.message);
    if frame.total == 1 {
        peer.partial.remove(&key);
        return Some((frame.channel, frame.part));
    }
    let partial = peer.partial.entry(key).or_default();
    if partial.total != frame.total {
        *partial = Partial {
            total: frame.total,
            parts: vec![None; usize::from(frame.total)],
        };
    }
    partial.parts[usize::from(frame.index)] = Some(frame.part);
    if partial.parts.iter().any(Option::is_none) {
        if peer.partial.len() > MAX_PARTS * 4 {
            peer.partial.clear();
        }
        return None;
    }
    let message = peer.partial.remove(&key)?;
    Some((
        frame.channel,
        message.parts.into_iter().flatten().flatten().collect(),
    ))
}

/// Hands a message to the mod that opened its channel.
fn deliver(channel: u32, peer: u64, data: &[u8]) {
    let Some(target) = CHANNELS
        .get()
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

/// A control frame's message: its kind and the protocol version.
fn control(kind: u8) -> Vec<u8> {
    vec![kind, PROTOCOL]
}

/// Sends the client of the player with account key `key` a notice for the
/// player's entity `entity`; the client shows `text`.
pub(crate) fn notice(key: u64, entity: u32, text: &str) -> Result<(), Status> {
    if !ready(key) {
        return Err(Status::MISSING);
    }
    let mut data = control(NOTICE);
    data.extend_from_slice(&entity.to_le_bytes());
    data.extend_from_slice(text.as_bytes());
    for line in control_frames(&data).ok_or(Status::BAD_ARGS)? {
        crate::chat::send(key, &line);
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
            crate::chat::send(peer, line);
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
        crate::chat::send(key, &line);
    }
    Ok(())
}

/// The position a demo frame carries: three little-endian `i64`.
fn demo_position(bytes: &[u8]) -> Option<[i64; 3]> {
    let (axes, _) = bytes.as_chunks::<8>();
    match axes {
        [x, y, z, ..] => Some([x, y, z].map(|a| i64::from_le_bytes(*a))),
        _ => None,
    }
}

/// The longest control message, in bytes, with its kind and version.
pub(crate) const MAX_CONTROL: usize = PART_BYTES * MAX_PARTS - 2;
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
        crate::chat::send(key, &line);
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
            crate::chat::send(key, line);
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

/// A control message's kind and body, when it is a settings message.
fn remote_message(data: &[u8]) -> Option<crate::remote::Message> {
    let (&kind, rest) = data.split_first()?;
    let (&version, body) = rest.split_first()?;
    if version != PROTOCOL {
        return None;
    }
    crate::remote::Message::read(kind, body)
}

/// Records a hello from `key`: `Some(true)` for a new peer, `Some(false)`
/// for a known one, whose hello takes from its rate and keeps its state, and
/// `None` when that rate is spent.
fn greet(peers: &mut HashMap<u64, Peer>, key: u64) -> Option<bool> {
    match peers.entry(key) {
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(Peer::new());
            Some(true)
        }
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.get_mut().admit().then_some(false)
        }
    }
}

/// Takes a frame line a client sent, on the server's game thread: `args` is
/// the line after the command name, `key` the client's account key.
pub(crate) fn server_received(key: u64, args: &str) {
    if key == 0 {
        return;
    }
    let Some(frame) = parse(args) else {
        dropped();
        return;
    };
    if frame.channel == CONTROL {
        if frame.part == control(HELLO) {
            let Some(fresh) = state(|s| greet(&mut s.peers, key)) else {
                dropped();
                return;
            };
            if fresh {
                crate::log!(INFO, "mod messages: the client of {key:016x} said hello");
            }
            for line in frames(CONTROL, 0, &control(WELCOME)).unwrap_or_default() {
                crate::chat::send(key, &line);
            }
            if fresh {
                for records in crate::item_data::hello() {
                    item_data(key, &records);
                }
            }
            return;
        }
        let whole = state(|s| {
            let peer = s.peers.get_mut(&key)?;
            if frame.index == 0 && !peer.admit() {
                dropped();
                return None;
            }
            assemble(peer, frame)
        });
        if let (Some((_, data)), Some(loader)) = (whole, crate::host::loader())
            && let Some(message) = remote_message(&data)
        {
            crate::remote::server(loader, key, message);
        }
        return;
    }
    let message = state(|s| {
        let peer = s.peers.get_mut(&key)?;
        if frame.index == 0 && !peer.admit() {
            dropped();
            return None;
        }
        assemble(peer, frame)
    });
    if let Some((channel, data)) = message {
        deliver(channel, key, &data);
    }
}

/// Takes a frame line the server sent, on the client; the line's text after
/// the prefix. False when the line is no frame.
pub(crate) fn client_received(text: &str) -> bool {
    let Some(rest) = text.strip_prefix(PREFIX) else {
        return false;
    };
    let Some(frame) = parse(rest) else {
        dropped();
        return true;
    };
    if frame.channel == CONTROL {
        // Only the remote settings messages take more than one frame.
        let single = frame.total == 1;
        if single && frame.part == control(WELCOME) {
            let fresh = state(|s| s.peers.insert(NetMessage::SERVER, Peer::new()).is_none());
            if fresh {
                crate::log!(INFO, "mod messages: the server answered");
            }
        } else if single
            && let Some(records) = frame.part.strip_prefix(control(ITEM_DATA).as_slice())
        {
            crate::item_data::received(records);
        } else if single
            && let Some(rest) = frame.part.strip_prefix(control(NOTICE).as_slice())
            && let Some((entity, text)) = rest.split_first_chunk::<4>()
            && let Ok(text) = core::str::from_utf8(text)
        {
            crate::notices::queue(u32::from_le_bytes(*entity), text);
        } else if single && let Some(rest) = frame.part.strip_prefix(control(UI_DEMO).as_slice()) {
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

/// Whether `text` is a frame line, which no chat shows.
pub(crate) fn is_frame(text: &[u8]) -> bool {
    text.starts_with(PREFIX.as_bytes())
}

/// Notes whether the server the client joins advertises Enshwick, and
/// forgets the last server's state.
pub(crate) fn joining(runs_enshwick: bool) {
    forget();
    SERVER_RUNS_ENSHWICK.store(runs_enshwick, Ordering::Release);
}

/// Forgets every peer, when the client changes servers or a hosted game
/// ends.
pub(crate) fn forget() {
    let mut guard = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    *guard = None;
    WAITING.store(false, Ordering::Release);
    crate::item_data::forget_client();
}

/// Forgets a player's client, when the player leaves.
pub(crate) fn forget_peer(key: u64) {
    state(|s| {
        s.peers.remove(&key);
        s.sending.remove(&key);
    });
}

/// Whether `peer` takes messages: on the server, a player's client that said
/// hello; on a client, the server once it answered ([`NetMessage::SERVER`]).
pub(crate) fn ready(peer: u64) -> bool {
    state(|s| s.peers.contains_key(&peer))
}

/// Sends `data` on `channel` from mod `owner`: to the server for
/// [`NetMessage::SERVER`], else from the server to the player with account
/// key `peer`, or to every client that said hello for 0.
pub(crate) fn send(
    loader: &Loader,
    owner: usize,
    channel: u32,
    peer: u64,
    data: &[u8],
) -> Result<(), Status> {
    let open = CHANNELS
        .get()
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
    let serving = crate::world::hosting() || loader.side == enshwick_loader_api::abi::Side::SERVER;
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
        crate::chat::send(key, &line);
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
    try_state(|s| {
        let mut lines = Vec::new();
        let answered = s.peers.contains_key(&NetMessage::SERVER);
        let due = s
            .hello_at
            .is_none_or(|at| at.elapsed().as_secs_f64() >= HELLO_EVERY);
        let modded = SERVER_RUNS_ENSHWICK.load(Ordering::Acquire) || crate::world::hosting();
        if !answered && due && modded {
            s.hello_at = Some(Instant::now());
            lines.extend(frames(CONTROL, 0, &control(HELLO)).unwrap_or_default());
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

    #[test]
    fn a_repeated_hello_keeps_the_peer_and_spends_its_rate() {
        let mut peers = HashMap::new();
        assert_eq!(greet(&mut peers, 7), Some(true));
        peers.get_mut(&7).unwrap().partial.insert(
            (1, 1),
            Partial {
                total: 2,
                parts: Vec::new(),
            },
        );
        let hellos = core::iter::repeat_with(|| greet(&mut peers, 7))
            .take(100)
            .take_while(Option::is_some)
            .count();
        assert!(hellos < 50, "{hellos}");
        assert_eq!(greet(&mut peers, 7), None);
        assert_eq!(peers[&7].partial.len(), 1);
    }

    #[test]
    fn a_demo_frame_carries_a_position_or_none() {
        let mut data = Vec::new();
        for axis in [1i64 << 32, -5, 7] {
            data.extend_from_slice(&axis.to_le_bytes());
        }
        assert_eq!(demo_position(&data), Some([1 << 32, -5, 7]));
        assert_eq!(demo_position(&data[..16]), None);
        assert_eq!(demo_position(&[]), None);
    }

    #[test]
    fn a_message_goes_out_in_frames_and_comes_back_whole() {
        let data: Vec<u8> = (0..2000u32).map(|i| (i % 251) as u8).collect();
        let lines = frames(7, 3, &data).unwrap();
        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .all(|l| l.len() <= 1024 && is_frame(l.as_bytes()))
        );
        let mut peer = Peer::new();
        let mut whole = None;
        for line in lines.iter().rev() {
            let frame = parse(line.strip_prefix(PREFIX).unwrap()).unwrap();
            whole = assemble(&mut peer, frame);
        }
        assert_eq!(whole, Some((7, data)));
        assert!(peer.partial.is_empty());
    }

    #[test]
    fn the_largest_message_fits_its_frames_and_malformed_frames_are_dropped() {
        assert!(frames(1, 0, &[0; MAX_MESSAGE]).unwrap().len() <= MAX_PARTS);
        assert_eq!(
            frames(1, 0, &[0; PART_BYTES * MAX_PARTS]).unwrap().len(),
            MAX_PARTS
        );
        assert!(frames(1, 0, &[0; PART_BYTES * MAX_PARTS + 1]).is_none());
        let mut long = control(NOTICE);
        long.extend_from_slice(&[b'a'; MAX_CONTROL + 1]);
        assert!(control_frames(&long).is_none());
        assert!(parse("not base64 !").is_none());
        assert!(parse(&URL_SAFE_NO_PAD.encode([9u8; HEADER])).is_none());
        let mut bad = vec![PROTOCOL, 1, 0, 0, 0, 0, 0, 3, 2];
        bad.push(0);
        assert!(parse(&URL_SAFE_NO_PAD.encode(&bad)).is_none());
    }

    #[test]
    fn a_peer_s_rate_runs_out_after_its_burst() {
        let mut peer = Peer::new();
        let admitted = (0..100).filter(|_| peer.admit()).count();
        assert!((40..=42).contains(&admitted), "{admitted}");
    }

    #[test]
    fn channel_ids_are_per_mod_and_never_the_protocol_s() {
        assert_ne!(channel_id("a", "x"), channel_id("b", "x"));
        assert_ne!(channel_id("a", "x"), CONTROL);
    }
}
