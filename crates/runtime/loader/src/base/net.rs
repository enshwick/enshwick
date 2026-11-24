//! Messages between a join mod's server part and its client part, and the
//! protocol's own, on the own channel of the game's connection (see the
//! carrier module), reliable and in order.
//!
//! Only clients that announce themselves get messages: a client with the
//! loader sends a hello with the protocol version and the join mods it runs
//! when it joins a server whose rules name Enshwick, or a game whose host's
//! lobby does (see the lobby module), the server answers unless it keeps
//! the player out (see the admission module), and from then on both sides
//! send; what the server sends a client before its answer reached it waits
//! for that (see the outbox module), so no feature keeps its own. A vanilla client never says hello, so it never receives a message,
//! and a server that runs join mods disconnects it. A client never opens
//! the channel to a game that names no Enshwick.
//!
//! The control messages are in [`wire`](crate::base::wire). Each side keeps a
//! rate per peer and drops what is over it, counting the drops for the
//! status summary.

use core::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use enshwick_core::Entity;
use enshwick_core::abi::{Header, NetMessage, NetMessageFn};
use enshwick_core::i18n::Language;
use rustc_hash::FxHashMap;

use crate::base::wire::{
    CONTROL, ITEM_DATA, NOTICE, Peer, SOUND, STAT_VALUES, UI_DEMO, WELCOME, control, hello,
    read_notice, read_stat_values, remote_message,
};
pub(crate) use crate::base::wire::{MAX_CONTROL, MAX_MESSAGE};
use crate::engine::loader::{Loader, Registered};
use crate::engine::mod_index::ModIndex;
use crate::engine::parts::ServerLink;
use crate::engine::registry::{JoinFeatureError, Registry, sealed_is_too_late};
use enshwick_base::Relock;

mod outbox;
mod own_stats;
mod server;
#[cfg(test)]
mod tests;

pub(crate) use own_stats::own_stat;
pub(crate) use server::{keep_greeting, link_restarted, server_carried};

/// How long a client waits for the server's answer before it says hello
/// again on the same link.
const HELLO_AGAIN: Duration = Duration::from_secs(5);

/// One channel a mod opened.
#[derive(Debug)]
struct Channel {
    id: u32,
    name: String,
    handler: Registered<NetMessageFn>,
}

/// The peers the net talks to and their rates.
#[derive(Debug, Default)]
struct Links {
    /// On the server: peers by account key that said hello. On a client:
    /// the server, [`NetMessage::SERVER`], once it answered.
    peers: FxHashMap<u64, Peer>,
    /// When the client last said hello.
    hello_at: Option<Instant>,
    /// Rates of the peers sending, by key.
    sending: FxHashMap<u64, Peer>,
    /// On the server: the hellos on their way in from clients that are no
    /// peers yet, by key.
    greeting: FxHashMap<u64, Peer>,
    /// On the server: the clients whose join data did not fit their link,
    /// by key, with the tries so far; it goes again with the next look at
    /// the live slots.
    owed: Vec<(u64, u8)>,
    /// On the server: the clients that said hello but hold no welcome yet,
    /// by key, with what waits for it.
    unwelcomed: FxHashMap<u64, outbox::Outbox>,
}

/// What the net keeps while a game runs.
#[derive(Debug)]
struct State {
    links: Mutex<Option<Links>>,
    /// Set on a client while the server it joined advertises Enshwick,
    /// which opens the own channel to it.
    server_runs_enshwick: AtomicBool,
    /// Set on a client while its server's answer holds, so a frame takes no
    /// lock once it is welcomed.
    answered: AtomicBool,
    /// Messages dropped this run, over a rate or unreadable, for the status
    /// summary; no game's end resets it.
    dropped: AtomicU64,
    /// On a client: the stat values its server last sent for its player's
    /// own character, by stat key.
    own_stats: arc_swap::ArcSwapOption<FxHashMap<u32, f32>>,
    /// On a client: what takes each kind of control message the server
    /// sends that a part above base owns, by its kind, as that part named
    /// it.
    takes: [std::sync::OnceLock<TakeControl>; CONTROL_KINDS],
}

/// What takes a control message of one kind on the client: the message
/// after its kind.
pub(crate) type TakeControl = fn(&Loader, &[u8]);

/// The control kinds a part may take, every kind the protocol names below
/// it.
const CONTROL_KINDS: usize = 32;

impl State {
    const fn new() -> Self {
        Self {
            links: Mutex::new(None),
            server_runs_enshwick: AtomicBool::new(false),
            answered: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            own_stats: arc_swap::ArcSwapOption::const_empty(),
            takes: [const { std::sync::OnceLock::new() }; CONTROL_KINDS],
        }
    }

    /// Forgets the game that went: its peers and its answer. Whether the
    /// server runs Enshwick stays, since the next join check sets it, maybe
    /// before a hosted game's end reaches this on another thread.
    fn forget(&self) {
        self.answered.store(false, Ordering::Release);
        self.own_stats.store(None);
        *self.links.relock() = None;
    }
}

static CHANNELS: Registry<Channel> = Registry::new();
static STATE: State = State::new();

fn state<R>(f: impl FnOnce(&mut Links) -> R) -> R {
    let mut guard = STATE.links.relock();
    f(guard.get_or_insert_with(Links::default))
}

/// [`state`] for a per-frame path: `None`, without waiting, while another
/// thread holds the state.
fn try_state<R>(f: impl FnOnce(&mut Links) -> R) -> Option<R> {
    let mut guard = STATE.links.try_relock()?;
    Some(f(guard.get_or_insert_with(Links::default)))
}

/// The id of mod `id`'s channel `name`.
fn channel_id(id: &str, name: &str) -> u32 {
    let id = enshwick_gamedata::reflect::fnv1a32(format!("{id}/{name}").as_bytes());
    // Zero is the protocol's own.
    id.max(1)
}

/// Why a mod's channel did not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum OpenChannelError {
    /// The registration itself was refused: after init.
    #[error(transparent)]
    Register(#[from] JoinFeatureError),
    /// No mod has the index.
    #[error("no such mod")]
    NoMod,
    /// The mod does not run on both sides, so no peer can answer.
    #[error("the mod does not run on both sides")]
    OneSided,
    /// The name is empty or longer than [`MAX_CHANNEL_NAME`] bytes.
    #[error("a channel name of 1 to {MAX_CHANNEL_NAME} bytes")]
    BadName,
    /// The mod opened a channel of that name, or one of the same id.
    #[error("the channel is open")]
    Twice,
}

sealed_is_too_late!(OpenChannelError);

/// The longest channel name, in bytes.
const MAX_CHANNEL_NAME: usize = 64;

/// Opens mod `owner`'s channel `name`; its id.
///
/// # Errors
///
/// [`OpenChannelError`] after init, for a mod not on both sides, a bad name
/// or one open already.
pub(crate) fn open(
    loader: &Loader,
    owner: ModIndex,
    name: &str,
    callback: NetMessageFn,
    user: *mut c_void,
) -> Result<u32, OpenChannelError> {
    if CHANNELS.is_sealed() {
        return Err(JoinFeatureError::TooLate.into());
    }
    let record = loader
        .mods
        .get(owner.get())
        .ok_or(OpenChannelError::NoMod)?;
    if !record.joins_when_on() {
        return Err(OpenChannelError::OneSided);
    }
    if name.is_empty() || name.len() > MAX_CHANNEL_NAME {
        return Err(OpenChannelError::BadName);
    }
    let id = channel_id(&record.id, name);
    CHANNELS.edit(|pending| {
        if pending.iter().any(|c| c.id == id) {
            return Err(OpenChannelError::Twice);
        }
        pending.push(Channel {
            id,
            name: format!("{}/{name}", record.id),
            handler: Registered::new(owner, callback, user),
        });
        Ok(id)
    })?
}

/// Seals the channels, before any link.
pub(crate) fn seal() {
    let channels = CHANNELS.seal();
    if !channels.is_empty() {
        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        crate::log!(INFO, "mod messages: channels {}", names.join(", "));
    }
}

/// Sends `data` on `channel` to `key` over the own channel; false when it
/// did not go: no link to the peer, or its link is full.
fn carry(key: u64, channel: u32, data: &[u8]) -> bool {
    crate::base::carrier::send(key, channel, data).is_ok()
}

/// Sends `data` on `channel` to the clients `keys` from the server: now to
/// a welcomed one, else kept for its welcome (see the outbox module);
/// `failed` hears each that took nothing, a full outbox as a full link.
fn post(
    keys: &[u64],
    channel: u32,
    data: &[u8],
    mut failed: impl FnMut(u64, crate::base::carrier::CarrierError),
) -> Result<(), crate::base::carrier::CarrierError> {
    use crate::base::carrier::CarrierError;
    let mut refused = Vec::new();
    let ready: Vec<u64> = state(|s| {
        keys.iter()
            .copied()
            .filter(|&key| match outbox::hold(s, key, channel, data) {
                outbox::Hold::Ready => true,
                outbox::Hold::Held => false,
                outbox::Hold::Full => {
                    refused.push((key, CarrierError::Full));
                    false
                }
                outbox::Hold::Unknown => {
                    refused.push((key, CarrierError::NoLink));
                    false
                }
            })
            .collect()
    });
    for (key, error) in refused {
        if error == CarrierError::Full {
            dropped();
        }
        failed(key, error);
    }
    crate::base::carrier::send_many(&ready, channel, data, failed)
}

/// [`post`] to one client, with one look at the outbox and no list: `None`
/// when no client of `key` said hello, else whether the message went or
/// waits for the client's welcome.
fn post_one(key: u64, channel: u32, data: &[u8]) -> Option<bool> {
    match state(|s| outbox::hold(s, key, channel, data)) {
        outbox::Hold::Unknown => None,
        outbox::Hold::Ready => Some(carry(key, channel, data)),
        outbox::Hold::Held => Some(true),
        outbox::Hold::Full => {
            dropped();
            Some(false)
        }
    }
}

/// Says hello to the server, on the thread that sends the client's chat: at
/// once when its link `opened` (the caller forgot the last link's answer
/// with [`link_lost`] first), and
/// every few seconds until this one is answered, since the server may drop
/// a hello it has no room for. Waits on no lock but for a new link.
pub(crate) fn client_tick(opened: bool) {
    let server = NetMessage::SERVER.get();
    if STATE.answered.load(Ordering::Acquire) {
        return;
    }
    let due = |s: &mut Links| {
        let due = !s.peers.contains_key(&server)
            && (opened || s.hello_at.is_none_or(|at| at.elapsed() >= HELLO_AGAIN));
        if due {
            s.hello_at = Some(Instant::now());
        }
        due
    };
    let due = if opened {
        state(due)
    } else {
        try_state(due).unwrap_or(false)
    };
    if !due {
        return;
    }
    let mods = crate::base::advertise::mod_list(&crate::base::join::client::join_mods());
    let nonce = if crate::engine::world::hosting() {
        crate::base::admission::nonce()
    } else {
        0
    };
    carry(
        NetMessage::SERVER.get(),
        CONTROL,
        &hello(&mods, nonce, crate::engine::texts::game_language()),
    );
}

/// Forgets the server's answer and the windows it showed when the
/// client's link went.
pub(crate) fn link_lost() {
    STATE.answered.store(false, Ordering::Release);
    STATE.own_stats.store(None);
    state(|s| {
        s.peers.remove(&NetMessage::SERVER.get());
        s.sending.remove(&NetMessage::SERVER.get());
    });
    server_link(ServerLink::Lost);
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
        peer: enshwick_core::PlayerKey::new(peer),
        data: data.as_ptr(),
        size: u32::try_from(data.len()).unwrap_or(0),
        reserved_tail: 0,
    };
    // SAFETY: the handler lives in the mod's DLL, which stays loaded, and
    // takes the pointer it registered; the safe layer catches its panics.
    crate::engine::profile::call(
        target.handler.owner,
        crate::engine::profile::Feature::Messages,
        || unsafe { (target.handler.callback)(&raw const message, target.handler.user()) },
    );
}

/// Why a message did not go out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NetError {
    /// No ready link to the peer, or no peer has the key.
    #[error("no ready link to the peer")]
    NoPeer,
    /// A channel the mod did not open.
    #[error("the mod did not open the channel")]
    BadChannel,
    /// A message longer than a frame carries.
    #[error("the message is too long")]
    TooLong,
    /// Only a server sends to clients.
    #[error("only a server sends to clients")]
    NotServing,
    /// The peer's sending rate is spent.
    #[error("the peer's sending rate is spent")]
    Throttled,
    /// The control message was not carried.
    #[error("the control message was not carried")]
    NotCarried,
    /// The carrier did not take the payload.
    #[error(transparent)]
    Carrier(#[from] crate::base::carrier::CarrierError),
}

/// Sends the client of the player with account key `key` a notice for the
/// player's entity `entity`; the client shows `text`.
pub(crate) fn notice(key: u64, entity: Entity, text: &str) -> Result<(), NetError> {
    let data = crate::base::wire::notice(entity.0, text);
    if data.len() > MAX_CONTROL {
        return Err(NetError::TooLong);
    }
    posted(post_one(key, CONTROL, &data))
}

/// The answer of a [`post_one`] to a single client.
fn posted(sent: Option<bool>) -> Result<(), NetError> {
    match sent {
        None => Err(NetError::NoPeer),
        Some(false) => Err(NetError::NotCarried),
        Some(true) => Ok(()),
    }
}

/// Sends item data records to the client of the player with account key
/// `key`, or to every client that said hello for 0. False when a client's
/// link had no room for them.
pub(crate) fn item_data(key: u64, records: &[u8]) -> bool {
    let mut data = control(ITEM_DATA);
    data.extend_from_slice(records);
    if data.len() > MAX_CONTROL {
        crate::log!(WARN, "mod messages: item data records too long to send");
        return false;
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
    let mut whole = true;
    let sent = post(&peers, CONTROL, &data, |_, _| whole = false);
    sent.is_ok() && whole
}

/// Sends the client of the player with account key `key` the values of its
/// own character's shown stat kinds; false when it took no message.
pub(crate) fn stat_values(key: u64, values: &[(u32, f32)]) -> bool {
    post_one(key, CONTROL, &crate::base::wire::stat_values(values)) == Some(true)
}

/// Names `take` as what takes the control messages of `kind` the server
/// sends this client, after their kind; the first one named stays.
pub(crate) fn on_control(kind: u8, take: TakeControl) {
    crate::base::remote::name(&STATE.takes, kind, take);
}

/// Hands `rest`, a control message of `kind` after its kind, to the part
/// that takes that kind, once the loader runs.
fn take(kind: u8, rest: &[u8]) {
    let Some(loader) = crate::engine::loader::loader() else {
        return;
    };
    match crate::base::remote::named(&STATE.takes, kind) {
        Some(take) => take(loader, rest),
        None => crate::base::remote::untaken("control message", kind),
    }
}

/// Tells the parts what became of the client's link to its server, once
/// the loader runs.
fn server_link(link: ServerLink) {
    if let Some(loader) = crate::engine::loader::loader() {
        loader.parts.server_link(link);
    }
}

/// Sends a mod's sound message `data` to the clients `keys` from the
/// server; a client whose link or outbox is full misses it, counted as
/// dropped.
pub(crate) fn sound(keys: &[u64], data: &[u8]) {
    let _sent = post(keys, CONTROL, data, |_, _| {});
}

/// Sends the client of the player with account key `key`, whose hello named
/// other mods, `mods`, the server's mods of side both, so it lists the
/// difference once it is back in its menu. A link with no room drops it:
/// the chat note still names the mods.
pub(crate) fn refused_mods(key: u64, mods: &str) {
    if !carry(key, CONTROL, &crate::base::wire::mods(mods)) {
        crate::log!(
            INFO,
            "join check: the list of mods for {key:016x} did not go; the chat note names them"
        );
    }
}

/// Tells the client of the player with account key `key` to open the
/// interface demo, with where the player stands.
pub(crate) fn ui_demo(key: u64, at: Option<[i64; 3]>) -> Result<(), NetError> {
    let mut data = control(UI_DEMO);
    for axis in at.iter().flatten() {
        data.extend_from_slice(&axis.to_le_bytes());
    }
    posted(post_one(key, CONTROL, &data))
}

/// A settings message as a control message; `None` for one too long to
/// send.
fn remote_data(message: &crate::base::remote::Message) -> Option<Vec<u8>> {
    let mut data = control(message.kind());
    data.extend_from_slice(&message.body());
    if data.len() > MAX_CONTROL {
        crate::log!(WARN, "mod messages: a settings message too long to send");
        return None;
    }
    Some(data)
}

/// Sends a settings message to the client of the player with account key
/// `key`, when it runs Enshwick, kept for its welcome until then. False
/// when its link or its outbox had no room.
pub(crate) fn to_player(key: u64, message: &crate::base::remote::Message) -> bool {
    remote_data(message).is_none_or(|data| post_one(key, CONTROL, &data) != Some(false))
}

/// Sends a settings message to every client that runs Enshwick.
pub(crate) fn broadcast_control(message: &crate::base::remote::Message) {
    let keys = clients();
    let Some(data) = remote_data(message) else {
        return;
    };
    let _sent = post(&keys, CONTROL, &data, |_, _| {});
}

/// Sends a settings message from the client to its server; false while the
/// server takes no messages.
pub(crate) fn to_server(message: &crate::base::remote::Message) -> bool {
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
        STATE.answered.store(true, Ordering::Release);
        if fresh {
            crate::log!(INFO, "mod messages: the server answered");
            server_link(ServerLink::Answered);
            crate::base::join::client::admitted();
        }
    } else if let Some((list, complete)) = crate::base::wire::read_mods(data) {
        crate::base::join::client::kicked_for_mods(&list, complete);
    } else if let Some(records) = data.strip_prefix(control(ITEM_DATA).as_slice()) {
        take(ITEM_DATA, records);
    } else if let Some(values) = data.strip_prefix(control(STAT_VALUES).as_slice()) {
        own_stats::received(read_stat_values(values));
    } else if let Some(rest) = data.strip_prefix(control(SOUND).as_slice()) {
        take(SOUND, rest);
    } else if let Some(rest) = data.strip_prefix(control(NOTICE).as_slice())
        && let Some((entity, text)) = read_notice(rest)
    {
        crate::base::notices::queue(Entity(entity), text);
    } else if let Some(rest) = data.strip_prefix(control(UI_DEMO).as_slice()) {
        if crate::engine::loader::loader().is_some_and(|loader| loader.developer_tools) {
            take(UI_DEMO, rest);
        } else {
            crate::log!(
                WARN,
                "the server asked for the interface demo; developer tools are off here"
            );
        }
    } else if ready(NetMessage::SERVER.get())
        && let Some(message) = remote_message(data)
    {
        crate::base::remote::client(message);
    }
}

/// Notes whether the server the client joins advertises Enshwick, and
/// forgets the last server's state.
pub(crate) fn joining(runs_enshwick: bool) {
    forget();
    server_link(ServerLink::Joining);
    STATE
        .server_runs_enshwick
        .store(runs_enshwick, Ordering::Release);
}

/// Whether the server the client joins advertises Enshwick, itself or in
/// its host's lobby.
pub(crate) fn server_runs_enshwick() -> bool {
    STATE.server_runs_enshwick.load(Ordering::Acquire) || crate::base::lobby::host_runs_enshwick()
}

/// Forgets every peer, when the client changes servers or a hosted game
/// ends.
pub(crate) fn forget() {
    crate::base::carrier::forget();
    STATE.forget();
    crate::base::notices::forget();
    server_link(ServerLink::Forgotten);
}

/// Forgets a player's client, when the player leaves.
pub(crate) fn forget_peer(key: u64) {
    state(|s| {
        s.peers.remove(&key);
        s.sending.remove(&key);
        s.greeting.remove(&key);
        s.unwelcomed.remove(&key);
    });
}

/// The language the player of client `key` reads the game in, as its hello
/// said, or the game's own for the player of this client while it hosts the
/// game; `None` before a hello, for a game without Enshwick or for a game
/// language the ABI does not list.
pub(crate) fn language_of(key: u64) -> Option<Language> {
    if crate::base::carrier::is_host(key) {
        return known(crate::engine::texts::game_language());
    }
    state(|s| language_in(s, key))
}

/// `language` if the ABI lists it; a number read from the game or a peer
/// that it does not list reads as unknown.
fn known(language: Language) -> Option<Language> {
    Language::ALL.contains(&language).then_some(language)
}

/// The language the player of client `key` reads, as the ABI numbers it:
/// [`language_of`], else 0, the fallback.
pub(crate) fn language_number(key: u64) -> u32 {
    language_of(key).map_or(0, |language| language.0)
}

/// Keeps `language`, which client `key` named after its hello for its
/// action replies, as the peer's language where its hello named none, so
/// every reader of [`language_of`] sees the one the actions use. A number
/// the game does not know is dropped, as the hello's is.
pub(crate) fn language_named(key: u64, language: u32) {
    let Some(language) = known(Language(language)) else {
        return;
    };
    state(|s| {
        if let Some(peer) = s.peers.get_mut(&key) {
            peer.language.get_or_insert(language);
        }
    });
}

/// Keeps the `language` client `key`'s hello named on its peer, or on its
/// hello on the way in, so it goes when they go.
fn remember_language(state: &mut Links, key: u64, language: Language) {
    if let Some(peer) = state.peers.get_mut(&key) {
        peer.language = Some(language);
    } else if let Some(peer) = state.greeting.get_mut(&key) {
        peer.language = Some(language);
    }
}

/// [`language_of`] in `state`: a peer's, or a hello still on its way in.
fn language_in(state: &Links, key: u64) -> Option<Language> {
    state
        .peers
        .get(&key)
        .or_else(|| state.greeting.get(&key))
        .and_then(|peer| peer.language)
}

/// Whether `peer` takes messages: on the server, a player's client that said
/// hello, its messages kept until its welcome went; on a client, the server
/// once it answered ([`NetMessage::SERVER`]).
pub(crate) fn ready(peer: u64) -> bool {
    state(|s| s.peers.contains_key(&peer))
}

/// Whether the client `key` holds the server's welcome and nothing waits
/// for it, so what goes now arrives on its current link, on the server.
pub(crate) fn welcomed(key: u64) -> bool {
    state(|s| s.peers.contains_key(&key) && !s.unwelcomed.contains_key(&key))
}

/// Makes client `peer` welcomed, as after its hello and the welcome, or
/// unknown again, for a test of a part that sends to clients.
#[cfg(test)]
pub(crate) fn set_ready(peer: u64, ready: bool) {
    state(|s| {
        s.unwelcomed.remove(&peer);
        if ready {
            s.peers.entry(peer).or_insert_with(Peer::new);
        } else {
            s.peers.remove(&peer);
        }
    });
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
) -> Result<(), NetError> {
    let open = CHANNELS
        .sealed()
        .and_then(|channels| channels.iter().find(|c| c.id == channel));
    if open.is_none_or(|c| c.handler.owner != owner) {
        return Err(NetError::BadChannel);
    }
    if data.len() > MAX_MESSAGE {
        return Err(NetError::TooLong);
    }
    if peer == NetMessage::SERVER.get() {
        return send_to_server(channel, data);
    }
    let serving =
        crate::engine::world::hosting() || loader.side == enshwick_core::abi::Side::SERVER;
    if !serving {
        return Err(NetError::NotServing);
    }
    send_to_clients(channel, peer, data)
}

fn send_to_clients(channel: u32, peer: u64, data: &[u8]) -> Result<(), NetError> {
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
            return Err(NetError::NoPeer);
        };
        let keys: Vec<u64> = keys
            .into_iter()
            .filter(|&key| admit_sending(s, key))
            .collect();
        if peer != 0 && keys.is_empty() {
            return Err(NetError::Throttled);
        }
        Ok(keys)
    })?;
    let mut failed = Vec::new();
    post(&keys, channel, data, |key, status| {
        failed.push((key, status));
    })
    .inspect_err(|_| refund(&keys))?;
    if !failed.is_empty() {
        refund(&failed.iter().map(|&(key, _)| key).collect::<Vec<_>>());
    }
    match failed.first() {
        Some(&(_, error)) if peer != 0 => Err(error.into()),
        _ => Ok(()),
    }
}

fn send_to_server(channel: u32, data: &[u8]) -> Result<(), NetError> {
    let server = NetMessage::SERVER.get();
    state(|s| {
        if !s.peers.contains_key(&server) {
            return Err(NetError::NoPeer);
        }
        if !admit_sending(s, server) {
            return Err(NetError::Throttled);
        }
        Ok(())
    })?;
    crate::base::carrier::send(server, channel, data)
        .inspect_err(|_| refund(&[server]))
        .map_err(Into::into)
}

/// Takes one message to `key` from its sending rate, in process too;
/// counts one over it as dropped.
fn admit_sending(s: &mut Links, key: u64) -> bool {
    let admitted = s.sending.entry(key).or_default().admit();
    if !admitted {
        dropped();
    }
    admitted
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
    let (lines, strayed) = crate::base::tell::delivery();
    let dropped = STATE.dropped.load(Ordering::Relaxed) + crate::base::carrier::dropped();
    crate::engine::diag::messages_line(dropped, lines, strayed)
}

/// Counts a dropped message and logs the first and every hundredth.
fn dropped() {
    let count = STATE.dropped.fetch_add(1, Ordering::Relaxed) + 1;
    if count == 1 || count.is_multiple_of(100) {
        crate::log!(
            WARN,
            "mod messages: {count} dropped, over a rate, unreadable or past a client's outbox bound"
        );
    }
}
