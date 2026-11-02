//! Messages between a join mod's server part and its client part, and the
//! protocol's own, on the own channel of the game's connection (see the
//! carrier module), reliable and in order.
//!
//! Only clients that announce themselves get messages: a client with the
//! loader sends a hello with the protocol version and the join mods it runs
//! when it joins a server whose rules name Enshwick, or a game whose host's
//! lobby does (see the lobby module), the server answers unless it keeps
//! the player out (see the admission module), and from then on both sides
//! send. A vanilla client never says hello, so it never receives a message,
//! and a server that runs join mods disconnects it. A client never opens
//! the channel to a game that names no Enshwick.
//!
//! The control messages are in [`wire`](crate::wire). Each side keeps a
//! rate per peer and drops what is over it, counting the drops for the
//! status summary.

use core::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick::Entity;
use enshwick::abi::{Header, NetMessage, NetMessageFn, Status};
use enshwick::i18n::Language;
use rustc_hash::FxHashMap;

use crate::host::{Loader, Registered};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::registry::Registry;
use crate::wire::{
    CONTROL, ITEM_DATA, NOTICE, Peer, UI_DEMO, WELCOME, control, demo_position, hello, read_notice,
    remote_message,
};
pub(crate) use crate::wire::{MAX_CONTROL, MAX_MESSAGE};

mod server;
#[cfg(test)]
mod tests;

pub(crate) use server::{keep_greeting, server_carried};

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
    /// Rates of the peers sending, by key.
    sending: FxHashMap<u64, Peer>,
    /// On the server: the hellos on their way in from clients that are no
    /// peers yet, by key.
    greeting: FxHashMap<u64, Peer>,
    /// On the server: the clients whose join data did not fit their link,
    /// by key, with the tries so far; it goes again with the next look at
    /// the live slots.
    owed: Vec<(u64, u8)>,
}

static CHANNELS: Registry<Channel> = Registry::new();
static STATE: Mutex<Option<State>> = Mutex::new(None);
/// Set on a client while the server it joined advertises Enshwick, which
/// opens the own channel to it.
static SERVER_RUNS_ENSHWICK: AtomicBool = AtomicBool::new(false);
/// Messages dropped: over a rate, or unreadable.
static DROPPED: AtomicU64 = AtomicU64::new(0);

fn state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut guard = STATE.relock();
    f(guard.get_or_insert_with(State::default))
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

/// Sends `data` on `channel` to `key` over the own channel; false when it
/// did not go: no link to the peer, or its link is full.
fn carry(key: u64, channel: u32, data: &[u8]) -> bool {
    crate::carrier::send(key, channel, data).is_ok()
}

/// Says hello on a client link that just opened, and forgets the last
/// link's answer: the server is ready again once it answers this one. On
/// the thread that sends the client's chat.
pub(crate) fn link_opened() {
    link_lost();
    let mods = crate::advertise::mod_list(&crate::host::join_mods());
    let nonce = if crate::world::hosting() {
        crate::admission::nonce()
    } else {
        0
    };
    carry(
        NetMessage::SERVER.get(),
        CONTROL,
        &hello(&mods, nonce, crate::content::language()),
    );
}

/// Forgets the server's answer when the client's link went.
pub(crate) fn link_lost() {
    state(|s| {
        s.peers.remove(&NetMessage::SERVER.get());
        s.sending.remove(&NetMessage::SERVER.get());
    });
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
    if data.len() > MAX_CONTROL {
        return Err(Status::BAD_ARGS);
    }
    if !carry(key, CONTROL, &data) {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// Sends item data records to the client of the player with account key
/// `key`, or to every client that said hello for 0. False when a client's
/// link had no room for them.
pub(crate) fn item_data(key: u64, records: &[u8]) -> bool {
    let mut data = control(ITEM_DATA);
    data.extend_from_slice(records);
    if data.len() > MAX_CONTROL {
        crate::log!(WARN, "mod messages: item data records too long to send");
        return true;
    }
    let peers: Vec<u64> = if key == 0 {
        state(|s| s.peers.keys().copied().collect())
    } else {
        vec![key]
    };
    let peers: Vec<u64> = peers
        .into_iter()
        .filter(|&p| p != NetMessage::SERVER.get())
        .collect();
    crate::carrier::send_many(&peers, CONTROL, &data).is_ok()
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
    if !carry(key, CONTROL, &data) {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// A settings message as a control message; `None` for one too long to
/// send.
fn remote_data(message: &crate::remote::Message) -> Option<Vec<u8>> {
    let mut data = control(message.kind());
    data.extend_from_slice(&message.body());
    if data.len() > MAX_CONTROL {
        crate::log!(WARN, "mod messages: a settings message too long to send");
        return None;
    }
    Some(data)
}

/// Sends a settings message to the client of the player with account key
/// `key`, when it takes messages. False when its link had no room.
pub(crate) fn to_player(key: u64, message: &crate::remote::Message) -> bool {
    if !ready(key) {
        return true;
    }
    remote_data(message).is_none_or(|data| carry(key, CONTROL, &data))
}

/// Sends a settings message to every client that runs Enshwick.
pub(crate) fn broadcast_control(message: &crate::remote::Message) {
    let keys = clients();
    let Some(data) = remote_data(message) else {
        return;
    };
    let _sent = crate::carrier::send_many(&keys, CONTROL, &data);
}

/// Sends a settings message from the client to its server; false while the
/// server takes no messages.
pub(crate) fn to_server(message: &crate::remote::Message) -> bool {
    if !ready(NetMessage::SERVER.get()) {
        return false;
    }
    remote_data(message).is_some_and(|data| carry(NetMessage::SERVER.get(), CONTROL, &data))
}

/// Takes a message the server sent on `channel`, on the client.
pub(crate) fn client_carried(channel: u32, data: &[u8]) {
    if channel != CONTROL {
        if state(|s| s.peers.contains_key(&NetMessage::SERVER.get())) {
            deliver(channel, NetMessage::SERVER.get(), data);
        }
        return;
    }
    if data == control(WELCOME).as_slice() {
        let fresh = state(|s| {
            s.peers
                .insert(NetMessage::SERVER.get(), Peer::new())
                .is_none()
        });
        if fresh {
            crate::log!(INFO, "mod messages: the server answered");
            crate::actions::joined();
        }
    } else if let Some(records) = data.strip_prefix(control(ITEM_DATA).as_slice()) {
        crate::item_data::received(records);
    } else if let Some(rest) = data.strip_prefix(control(NOTICE).as_slice())
        && let Some((entity, text)) = read_notice(rest)
    {
        crate::notices::queue(Entity(entity), text);
    } else if let Some(rest) = data.strip_prefix(control(UI_DEMO).as_slice()) {
        if crate::host::loader().is_some_and(|loader| loader.developer_tools) {
            crate::ui::open_demo(demo_position(rest));
        } else {
            crate::log!(
                WARN,
                "the server asked for the interface demo; developer tools are off here"
            );
        }
    } else if ready(NetMessage::SERVER.get())
        && let Some(message) = remote_message(data)
    {
        crate::remote::client(message);
    }
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
    let keys = state(|s| {
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
        Ok(keys)
    })?;
    let sent = crate::carrier::send_many(&keys, channel, data);
    if sent.is_err() {
        refund(&keys);
    }
    if peer == 0 { Ok(()) } else { sent }
}

fn send_to_server(channel: u32, data: &[u8]) -> Result<(), Status> {
    let server = NetMessage::SERVER.get();
    state(|s| {
        if !s.peers.contains_key(&server) {
            return Err(Status::MISSING);
        }
        if !s.sending.entry(server).or_insert_with(Peer::new).admit() {
            dropped();
            return Err(Status::REFUSED);
        }
        Ok(())
    })?;
    crate::carrier::send(server, channel, data).inspect_err(|_| refund(&[server]))
}

/// Gives the rate tokens of a send that did not go back to `keys`.
fn refund(keys: &[u64]) {
    state(|s| {
        for key in keys {
            if let Some(rate) = s.sending.get_mut(key) {
                rate.refund();
            }
        }
    });
}

/// The status summary's line on mod messages lost on the way, once any
/// were.
pub(crate) fn status() -> Option<String> {
    let (lines, strayed) = crate::chat::delivery();
    let dropped = DROPPED.load(Ordering::Relaxed) + crate::carrier::dropped();
    crate::diag::messages_line(dropped, lines, strayed)
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
