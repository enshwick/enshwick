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

use core::ffi::c_void;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use enshwick::Entity;
use enshwick::abi::{Header, NetMessage, NetMessageFn, Status};
use enshwick::i18n::Language;
use rustc_hash::FxHashMap;

use crate::host::{Loader, Registered};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::registry::Registry;
use crate::wire::{
    CONTROL, ITEM_DATA, NOTICE, PART_BYTES, PREFIX, Peer, UI_DEMO, WELCOME, assemble, control,
    demo_position, frames, hello, parse, read_notice, remote_message,
};
pub(crate) use crate::wire::{MAX_CONTROL, MAX_MESSAGE, frame_body, is_frame};

mod server;
#[cfg(test)]
mod tests;

pub(crate) use server::{keep_greeting, server_received};

/// Frames waiting to leave at most, per side.
const MAX_WAITING: usize = 512;
/// Frames a client puts into its chat queue at most per send, besides its
/// hello. Nothing tells it whether the game's queue dropped one, and the
/// queue's size is not known, so the client sends no more than the server
/// posts in one step.
const LINES_PER_SEND: usize = 16;
/// Frames a client takes from its server per second, and in one burst. The
/// server's chat sends at most 16 lines a step to all its players and holds
/// at most 256 waiting, so its own frames always fit; a server that sends
/// more is dropped from.
const SERVER_FRAME_RATE: f64 = 1024.0;
const SERVER_FRAME_BURST: f64 = 512.0;
/// Seconds between a client's hellos until the server answers.
const HELLO_EVERY: f64 = 5.0;

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
    /// On the server: the clients whose join data was dropped on the way,
    /// by key, with the tries so far; it goes again with the next look at
    /// the live slots.
    owed: Vec<(u64, u8)>,
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
    let mut guard = STATE.try_relock()?;
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
        peer: enshwick::PlayerKey::new(peer),
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
    let data = crate::wire::notice(entity.0, text);
    let lines = control_frames(&data).ok_or(Status::BAD_ARGS)?;
    if !crate::chat::send_frames(key, &lines) {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// Sends item data records to the client of the player with account key
/// `key`, or to every client that said hello for 0; `records` fits one
/// frame. False when the line queue dropped them for a client.
pub(crate) fn item_data(key: u64, records: &[u8]) -> bool {
    let mut data = control(ITEM_DATA);
    data.extend_from_slice(records);
    let peers: Vec<u64> = if key == 0 {
        state(|s| s.peers.keys().copied().collect())
    } else {
        vec![key]
    };
    let Some(lines) = control_frames(&data) else {
        crate::log!(WARN, "mod messages: item data records too long to send");
        return true;
    };
    let dropped_for = peers
        .into_iter()
        .filter(|&p| p != NetMessage::SERVER.get())
        .filter(|&peer| !crate::chat::send_frames(peer, &lines))
        .count();
    dropped_for == 0
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
    let lines = control_frames(&data).ok_or(Status::BAD_ARGS)?;
    if !crate::chat::send_frames(key, &lines) {
        return Err(Status::REFUSED);
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
/// `key`, when it takes messages. False when the line queue dropped it.
pub(crate) fn to_player(key: u64, message: &crate::remote::Message) -> bool {
    if !ready(key) {
        return true;
    }
    let lines = remote_frames(message);
    lines.is_empty() || crate::chat::send_frames(key, &lines)
}

/// Sends a settings message to every client that runs Enshwick.
pub(crate) fn broadcast_control(message: &crate::remote::Message) {
    let keys: Vec<u64> = state(|s| {
        s.peers
            .keys()
            .copied()
            .filter(|&k| k != NetMessage::SERVER.get())
            .collect()
    });
    let lines = remote_frames(message);
    if keys.is_empty() || lines.is_empty() {
        return;
    }
    for key in keys {
        crate::chat::send_frames(key, &lines);
    }
}

/// Sends a settings message from the client to its server; false while the
/// server takes no messages.
pub(crate) fn to_server(message: &crate::remote::Message) -> bool {
    if !ready(NetMessage::SERVER.get()) {
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
            let fresh = state(|s| {
                s.peers
                    .insert(NetMessage::SERVER.get(), Peer::new())
                    .is_none()
            });
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
            && let Some((entity, text)) = read_notice(rest)
        {
            crate::notices::queue(Entity(entity), text);
        } else if single && let Some(rest) = frame.part().strip_prefix(control(UI_DEMO).as_slice())
        {
            if crate::host::loader().is_some_and(|loader| loader.developer_tools) {
                crate::ui::open_demo(demo_position(rest));
            } else {
                crate::log!(
                    WARN,
                    "the server asked for the interface demo; developer tools are off here"
                );
            }
        } else {
            let whole = state(|s| {
                s.peers
                    .get_mut(&NetMessage::SERVER.get())
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
            .get_mut(&NetMessage::SERVER.get())
            .and_then(|peer| assemble(peer, frame))
    });
    if let Some((channel, data)) = message {
        deliver(channel, NetMessage::SERVER.get(), &data);
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

/// Whether the server the client joins advertises Enshwick, itself or in
/// its host's lobby.
pub(crate) fn server_runs_enshwick() -> bool {
    SERVER_RUNS_ENSHWICK.load(Ordering::Acquire) || crate::lobby::host_runs_enshwick()
}

/// Forgets every peer, when the client changes servers or a hosted game
/// ends.
pub(crate) fn forget() {
    crate::carrier::forget();
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

/// The language the player of client `key` reads the game in, as its hello
/// said; `None` before a hello or for a game without Enshwick.
pub(crate) fn language_of(key: u64) -> Option<Language> {
    state(|s| language_in(s, key))
}

/// Keeps the `language` client `key`'s hello named on its peer, or on its
/// hello on the way in, so it goes when they go.
fn remember_language(state: &mut State, key: u64, language: Language) {
    if let Some(peer) = state.peers.get_mut(&key) {
        peer.language = Some(language);
    } else if let Some(peer) = state.greeting.get_mut(&key) {
        peer.language = Some(language);
    }
}

/// [`language_of`] in `state`: a peer's, or a hello still on its way in.
fn language_in(state: &State, key: u64) -> Option<Language> {
    state
        .peers
        .get(&key)
        .or_else(|| state.greeting.get(&key))
        .and_then(|peer| peer.language)
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
            .filter(|&k| k != NetMessage::SERVER.get())
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
    if peer == NetMessage::SERVER.get() {
        return send_to_server(channel, data);
    }
    let serving = crate::world::hosting() || loader.side == enshwick::abi::Side::SERVER;
    if !serving {
        return Err(Status::WRONG_THREAD);
    }
    send_to_clients(channel, peer, data)
}

fn send_to_clients(channel: u32, peer: u64, data: &[u8]) -> Result<(), Status> {
    let (keys, lines) = state(|s| {
        let keys: Vec<u64> = if peer == 0 {
            s.peers
                .keys()
                .copied()
                .filter(|&k| k != NetMessage::SERVER.get())
                .collect()
        } else if s.peers.contains_key(&peer) {
            vec![peer]
        } else {
            return Err(Status::MISSING);
        };
        let message = s.next_message;
        s.next_message = s.next_message.wrapping_add(1);
        let lines = frames(channel, message, data).ok_or(Status::BAD_ARGS)?;
        let keys: Vec<u64> = keys
            .into_iter()
            .filter(|&key| {
                let admitted = s.sending.entry(key).or_insert_with(Peer::new).admit();
                if !admitted {
                    dropped();
                }
                admitted
            })
            .collect();
        if peer != 0 && keys.is_empty() {
            return Err(Status::REFUSED);
        }
        Ok((keys, lines))
    })?;
    let mut queued = true;
    for key in keys {
        queued &= crate::chat::send_frames(key, &lines);
    }
    if peer != 0 && !queued {
        return Err(Status::REFUSED);
    }
    Ok(())
}

fn send_to_server(channel: u32, data: &[u8]) -> Result<(), Status> {
    state(|s| {
        if !s.peers.contains_key(&NetMessage::SERVER.get()) {
            return Err(Status::MISSING);
        }
        let rate = s
            .sending
            .entry(NetMessage::SERVER.get())
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
        let answered = s.peers.contains_key(&NetMessage::SERVER.get());
        let due = s
            .hello_at
            .is_none_or(|at| at.elapsed().as_secs_f64() >= HELLO_EVERY);
        let modded = server_runs_enshwick() || crate::world::hosting();
        if !answered && due && modded {
            s.hello_at = Some(Instant::now());
            let mods = crate::advertise::mod_list(&crate::host::join_mods());
            let nonce = if crate::world::hosting() {
                crate::admission::nonce()
            } else {
                0
            };
            lines.extend(
                control_frames(&hello(&mods, nonce, crate::content::language()))
                    .unwrap_or_default(),
            );
        }
        if WAITING.load(Ordering::Acquire) {
            let count = s.outbox.len().min(LINES_PER_SEND);
            lines.extend(s.outbox.drain(..count));
            WAITING.store(!s.outbox.is_empty(), Ordering::Release);
        }
        lines
    })
    .unwrap_or_default()
}

/// The status summary's line on mod messages lost on the way, once any
/// were.
pub(crate) fn status() -> Option<String> {
    let (lines, strayed) = crate::chat::delivery();
    crate::diag::messages_line(DROPPED.load(Ordering::Relaxed), lines, strayed)
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
