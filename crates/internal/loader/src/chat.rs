//! Chat on the server, the dedicated one or the one a client hosts: lines
//! mods send, and the links that carry mod messages.
//!
//! The loader links the game's chat post, which every line a player's
//! client sends passes through on the game thread. Lines mods send from any
//! thread wait in a queue that the post-step drains; a line for one player
//! is posted for that player alone, marked as delivered for every other
//! connection; once a line reaches another connection all the same, lines
//! for one player stay off. What players type, `/` lines included, the game posts as it
//! would without the loader.
//!
//! The same links carry mod messages (see the net module): the server takes
//! a client's frame lines out of the chat, and a client takes the server's
//! frame lines where it posts what the server sent, before its chat shows
//! them, and puts its own frames into its chat queue before the game sends
//! it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{HookFlags, Status};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::pinned;
use crate::pinned::{Constant, Symbol};
use crate::wire::{Drops, Outbox, strays};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::chat::CLIENT_QUEUE.id(),
    pinned::chat::CLIENT_SEND.id(),
    pinned::chat::CLIENT_STORE.id(),
    pinned::chat::MARK_DELIVERED.id(),
    pinned::chat::MAX_TEXT.id(),
    pinned::chat::NODE_DELIVERED.id(),
    pinned::chat::POST.id(),
    pinned::chat::POST_RETURN.id(),
    pinned::chat::RECEIVE_RETURN_A.id(),
    pinned::chat::RECEIVE_RETURN_B.id(),
    pinned::chat::REPLY_TYPE.id(),
    pinned::chat::STORE_SLOT_COUNT.id(),
    pinned::chat::STORE_SLOT_SIZE.id(),
    pinned::chat::STORE_SLOTS.id(),
    pinned::chat::STORE_TAIL.id(),
    pinned::server::CHAT_STORE.id(),
];

/// Lines posted per drain: the game's chat pool drops undelivered lines
/// when it fills.
const LINES_PER_DRAIN: usize = 16;
/// Lines one reply may have.
const LINES_PER_REPLY: usize = 16;
/// Lines waiting at most; more are dropped.
const MAX_WAITING: usize = 256;
/// The result the game's chat post writes on success.
const POSTED: u8 = 0;

/// Where a line goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum To {
    /// Every player.
    All,
    /// One connection by its machine handle, with the account's key.
    Machine(u32, u64),
    /// The player with this account key.
    Key(u64),
}

/// A line waiting for the game thread.
#[derive(Debug)]
struct Line {
    to: To,
    text: String,
}

/// The game functions and offsets chat uses, from the build table.
#[derive(Debug, Clone, Copy)]
struct Layout {
    post_return: u64,
    /// Where a client posts what the server sent; 0 on the server.
    receive_returns: [u64; 2],
    mark: MarkFn,
    store: usize,
    slots: usize,
    slot_size: usize,
    slot_count: usize,
    tail: usize,
    delivered: usize,
    max_text: usize,
    reply_type: u8,
}

type PostFn = unsafe extern "system" fn(
    out: *mut u8,
    store: *mut u8,
    kind: u8,
    text: *const [usize; 2],
    loca1: u32,
    loca2: u32,
    handle: u32,
    hash: u64,
    extra: *const [u64; 2],
) -> *mut u8;
type MarkFn = unsafe extern "system" fn(store: *mut u8, node: *mut u8, slot: u32, bytes: u32);
type ClientSendFn = unsafe extern "system" fn(client: *mut u8) -> u64;
type ClientQueueFn = unsafe extern "system" fn(
    out: *mut u8,
    store: *mut u8,
    kind: u8,
    text: *const [usize; 2],
    loca1: u32,
    loca2: u32,
) -> *mut u8;

/// The client's own chat queue, from the build table.
#[derive(Debug, Clone, Copy)]
struct ClientLayout {
    queue: ClientQueueFn,
    store: usize,
}

static LOADED: OnceLock<(&'static Loader, Layout)> = OnceLock::new();
// SAFETY: `PostFn` is the signature of the game's `chat.post`, and `link`
// uses this slot only as the `next` of `post`, its link on that function.
static POST_NEXT: Next<PostFn> = unsafe { Next::new() };
// SAFETY: `ClientSendFn` is the signature of the game's `chat.client_send`,
// and `link_client` uses this slot only as the `next` of `client_send`, its
// link on that function.
static CLIENT_SEND_NEXT: Next<ClientSendFn> = unsafe { Next::new() };
static CLIENT: OnceLock<ClientLayout> = OnceLock::new();
static OUTBOX: Mutex<Outbox<Line>> = Mutex::new(Outbox::new(MAX_WAITING));
/// Set while the outbox holds anything, so an idle step takes no lock.
static WAITING: AtomicBool = AtomicBool::new(false);

/// What became of the lines for one player.
#[derive(Debug)]
struct Delivery {
    /// Frame lines dropped: the queue was full, or the player's connection
    /// was gone.
    dropped: Drops,
    /// Lines for one player that reached other players too.
    strayed: Drops,
    /// Set once a line for one player reached others: no line goes to one
    /// player from then on, since this build's chat does not keep it there.
    one_player_off: AtomicBool,
}

static DELIVERY: Delivery = Delivery {
    dropped: Drops::new(),
    strayed: Drops::new(),
    one_player_off: AtomicBool::new(false),
};

/// The lines for one player dropped and those that reached
/// others, for the status summary.
pub(crate) fn delivery() -> (u64, u64) {
    (DELIVERY.dropped.total(), DELIVERY.strayed.total())
}

/// Whether lines for one player still go: no such line has reached
/// another player.
pub(crate) fn one_player_on() -> bool {
    !DELIVERY.one_player_off.load(Ordering::Relaxed)
}

/// Queues `text` for the player with account key `to`, or for everyone.
pub(crate) fn send(to: u64, text: &str) -> Status {
    if LOADED.get().is_none() {
        return Status::MISSING;
    }
    let to = if to == 0 { To::All } else { To::Key(to) };
    queue(to, text);
    Status::OK
}

/// Queues the frame lines of one message, `lines`, for the player with
/// account key `to`, all of them or none: the one way a line a client takes
/// as the server's frame leaves the server. False when none were queued;
/// a full queue, or lines for one player turned off, counts them as
/// dropped.
pub(crate) fn send_frames(to: u64, lines: &[String]) -> bool {
    if LOADED.get().is_none() || to == 0 || lines.is_empty() {
        return false;
    }
    let queued = !DELIVERY.one_player_off.load(Ordering::Relaxed) && {
        let mut outbox = OUTBOX.relock();
        let queued = outbox.push_whole(lines.iter().map(|text| Line {
            to: To::Key(to),
            text: text.clone(),
        }));
        WAITING.store(!outbox.is_empty(), Ordering::Release);
        queued
    };
    if !queued {
        dropped(lines.len());
    }
    queued
}

/// Counts `count` lines for one player dropped, and logs the first and every
/// hundredth.
fn dropped(count: usize) {
    if let Some(total) = DELIVERY.dropped.add(count as u64) {
        crate::log!(
            WARN,
            "mod messages: {total} lines for one player dropped: the line queue was full, the player was gone, or such lines are off"
        );
    }
}

/// A line for one player reached `others` other players as well: counts
/// it and keeps lines for one player off from then on.
#[cold]
fn strayed(others: u32) {
    let _total = DELIVERY.strayed.add(1);
    if !DELIVERY.one_player_off.swap(true, Ordering::Relaxed) {
        crate::log!(
            ERROR,
            "mod messages to one player off: a line for one player reached {others} other players, since this game build's chat did not mark it delivered to them"
        );
    }
}

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Queues the lines of `text`, a mod's message, leaving out any
/// a client would take for a frame of the server's: such a line may carry
/// what a player typed.
fn queue(to: To, text: &str) {
    let mut outbox = OUTBOX.relock();
    for line in replied(text) {
        let line = Line {
            to,
            text: line.to_owned(),
        };
        if !outbox.push_whole([line]) {
            break;
        }
    }
    WAITING.store(!outbox.is_empty(), Ordering::Release);
}

/// The lines of `text` the chat shows: none empty, none a frame, at most
/// [`LINES_PER_REPLY`].
fn replied(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !crate::net::is_frame(l.as_bytes()))
        .take(LINES_PER_REPLY)
}

/// Links the chat post on the server; without it mods' chat lines and the
/// messages clients send stay off and the log says so.
pub(crate) fn link(loader: &'static Loader) {
    let symbol = |name: Symbol| loader.build.symbol(name.id()).map(|rva| loader.base + rva);
    let constant = |name: Constant| {
        loader
            .build
            .constant(name.id())
            .and_then(|v| usize::try_from(v).ok())
    };
    let layout = (|| {
        let mark = usize::try_from(symbol(pinned::chat::MARK_DELIVERED)?).ok()?;
        Some(Layout {
            post_return: symbol(pinned::chat::POST_RETURN)?,
            receive_returns: [
                symbol(pinned::chat::RECEIVE_RETURN_A).unwrap_or(0),
                symbol(pinned::chat::RECEIVE_RETURN_B).unwrap_or(0),
            ],
            // SAFETY: `mark` is the build table's `chat.mark_delivered` in
            // the loaded executable, the game function `MarkFn` types.
            mark: unsafe { core::mem::transmute::<usize, MarkFn>(mark) },
            store: constant(pinned::server::CHAT_STORE)?,
            slots: constant(pinned::chat::STORE_SLOTS)?,
            slot_size: constant(pinned::chat::STORE_SLOT_SIZE)?,
            slot_count: constant(pinned::chat::STORE_SLOT_COUNT)?.min(32),
            tail: constant(pinned::chat::STORE_TAIL)?,
            delivered: constant(pinned::chat::NODE_DELIVERED)?,
            max_text: constant(pinned::chat::MAX_TEXT)?,
            reply_type: u8::try_from(constant(pinned::chat::REPLY_TYPE)?).ok()?,
        })
    })();
    let Some(layout) = layout else {
        crate::log!(WARN, "chat lines off: this build has no chat layout");
        return;
    };
    if LOADED.set((loader, layout)).is_err() {
        return;
    }
    let target = Target::Pinned(pinned::chat::POST);
    let flags = HookFlags::CALLER_FRAME;
    if !loader.link_feature("chat lines", target, &POST_NEXT, post, flags) {
        return;
    }
    crate::log!(INFO, "chat lines and mod messages on");
    if loader.side == enshwick::abi::Side::CLIENT {
        link_client(loader);
    }
}

/// Links the client's chat send, so its own frames go with what it sends.
fn link_client(loader: &'static Loader) {
    let (Some(queue), Some(store)) = (
        loader.address(pinned::chat::CLIENT_QUEUE),
        loader.offset(pinned::chat::CLIENT_STORE),
    ) else {
        return;
    };
    let Ok(queue) = usize::try_from(queue) else {
        return;
    };
    let _first = CLIENT.set(ClientLayout {
        // SAFETY: `queue` is the build table's `chat.client_queue` in the
        // loaded executable, the game function `ClientQueueFn` types.
        queue: unsafe { core::mem::transmute::<usize, ClientQueueFn>(queue) },
        store,
    });
    loader.link_feature(
        "mod messages to the server",
        Target::Pinned(pinned::chat::CLIENT_SEND),
        &CLIENT_SEND_NEXT,
        client_send as ClientSendFn,
        HookFlags::NONE,
    );
}

/// The switch of the chat lines and mod messages.
static CHAT_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat lines and mod messages");

/// The loader's link on the client's chat send: queues the client's frame
/// lines before the game sends.
///
/// # Safety
///
/// - Called only by the game through the chain on `chat.client_send`, on the
///   thread that sends the client's chat, with the game client or null.
unsafe extern "system" fn client_send(client: *mut u8) -> u64 {
    let _caught = CHAT_FUSE.run("the client send", || {
        // SAFETY: `client` is the game's own argument, null or the game
        // client, on the thread that sends its chat, by this function's
        // `# Safety`.
        unsafe { queue_frames(client) };
    });
    // SAFETY: `CLIENT_SEND_NEXT` holds the rest of `chat.client_send`'s chain
    // while this detour runs, and `client` is the game's own argument, passed
    // on unchanged.
    unsafe { CLIENT_SEND_NEXT.get()(client) }
}

/// Puts the client's waiting frame lines into its chat queue.
///
/// # Safety
///
/// - `client` is null or the game client, whose chat store pointer sits at
///   `chat.client_store`.
/// - Called on the thread that sends the client's chat.
unsafe fn queue_frames(client: *mut u8) {
    let Some(layout) = CLIENT.get() else {
        return;
    };
    if client.is_null() {
        return;
    }
    let lines = crate::net::client_lines();
    if lines.is_empty() {
        return;
    }
    // SAFETY: `client` is non-null, checked by the guard before it, and is the
    // game client by this function's `# Safety`; its chat store pointer sits at
    // the table's `chat.client_store`.
    let store: usize = unsafe { crate::memory::read(client.expose_provenance(), layout.store) };
    if store == 0 {
        return;
    }
    for line in lines {
        let span = [line.as_ptr().expose_provenance(), line.len()];
        let mut result = [0u8; 16];
        // SAFETY: `layout.queue` is the table's `chat.client_queue`, on the
        // non-zero chat store checked by the guard before it, on the thread
        // that sends the client's chat by this function's `# Safety`; `result`
        // is 16 writable bytes, and `span` names the live `line`, which the
        // game copies before the call returns.
        unsafe {
            (layout.queue)(
                result.as_mut_ptr(),
                core::ptr::with_exposed_provenance_mut(store),
                0,
                &raw const span,
                0,
                0,
            );
        };
    }
}

fn next() -> Option<PostFn> {
    POST_NEXT.wired()
}

/// The loader's link on the game's chat post: takes frame lines out of the
/// chat and hands them to the net.
///
/// # Safety
///
/// - Called only by the game through the chain on `chat.post`, with its own
///   arguments: `out` null or a result record of at least 8 writable bytes,
///   `text` null or a span of pointer and byte length readable for the call.
unsafe extern "system" fn post(
    out: *mut u8,
    store: *mut u8,
    kind: u8,
    text: *const [usize; 2],
    loca1: u32,
    loca2: u32,
    handle: u32,
    hash: u64,
    extra: *const [u64; 2],
) -> *mut u8 {
    let frame = CHAT_FUSE.run("the frame check", || {
        // SAFETY: `out` and `text` are the game's own arguments at the entry
        // of `chat.post`, by this function's `# Safety`.
        unsafe { server_frame(out, kind, text) }
    });
    if frame.unwrap_or(false) {
        // SAFETY: `server_frame` returned true only for a non-null `out`, a
        // result record of at least 8 writable bytes by this function's
        // `# Safety`.
        unsafe { enshwick::raw::write_at(out, 0, 0u64) };
        return out;
    }
    let caught = CHAT_FUSE.run("the client frame check", || {
        // SAFETY: `out` and `text` are the game's own arguments at the entry
        // of `chat.post`, by this function's `# Safety`.
        unsafe { posted_frame(out, kind, text) }
    });
    if let Some(Some(frame)) = caught {
        // SAFETY: `posted_frame` returned a frame only for a non-null `out`, a
        // result record of at least 8 writable bytes by this function's
        // `# Safety`.
        unsafe { enshwick::raw::write_at(out, 0, 0u64) };
        let _outcome = CHAT_FUSE.run("a client frame", || {
            crate::net::server_received(hash, &frame);
        });
        let _outcome = CHAT_FUSE.run("the chat queue", drain);
        return out;
    }
    match next() {
        // SAFETY: `next` is the rest of `chat.post`'s chain, wired while this
        // detour runs, and the arguments are the game's own, passed on
        // unchanged.
        Some(next) => unsafe { next(out, store, kind, text, loca1, loca2, handle, hash, extra) },
        None => out,
    }
}

/// Whether the client is posting a frame line the server sent, which it
/// then hands to the net instead of the chat.
///
/// # Safety
///
/// - Called at the entry of the loader's link on `chat.post`, on the game's
///   calling thread.
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn server_frame(out: *mut u8, kind: u8, text: *const [usize; 2]) -> bool {
    let Some((loader, layout)) = LOADED.get() else {
        return false;
    };
    if kind != layout.reply_type
        || out.is_null()
        || text.is_null()
        || layout.receive_returns[0] == 0
    {
        return false;
    }
    let frame = (loader.caller_frame)();
    if frame.rsp == 0 {
        return false;
    }
    // SAFETY: `frame.rsp` is non-zero, checked by the guard before it, and
    // `caller_frame` took the frame at the entry of this chain on this thread,
    // linked with `HookFlags::CALLER_FRAME`, so it points at the live return
    // address.
    let caller = unsafe { frame.return_address() };
    if !layout.receive_returns.contains(&caller) {
        return false;
    }
    // SAFETY: `text` is non-null, checked by the guard before it, and is the
    // game's span of two words, readable for the call by this function's
    // `# Safety`.
    let [ptr, len]: [usize; 2] = unsafe { enshwick::raw::read_at(text.cast(), 0) };
    if ptr == 0 || len == 0 || len > layout.max_text {
        return false;
    }
    let mut bytes = vec![0; len];
    // SAFETY: `ptr` is non-null, checked by the guard before it, and the
    // span's `len` bytes are readable and unchanged for the call by this
    // function's `# Safety`.
    unsafe { enshwick::raw::read_bytes_at(core::ptr::with_exposed_provenance(ptr), 0, &mut bytes) };
    if !crate::net::is_frame(&bytes) {
        return false;
    }
    core::str::from_utf8(&bytes).is_ok_and(crate::net::client_received)
}

/// The frame when the game posts a frame line from a player's client.
///
/// # Safety
///
/// - Called at the entry of the loader's link on `chat.post`, on the game's
///   calling thread.
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn posted_frame(out: *mut u8, kind: u8, text: *const [usize; 2]) -> Option<String> {
    let (loader, layout) = LOADED.get()?;
    if kind != 0 || out.is_null() || text.is_null() {
        return None;
    }
    let frame = (loader.caller_frame)();
    // SAFETY: `frame.rsp` is non-zero, checked first, and `caller_frame`
    // took the frame at the entry of this chain on this thread, linked with
    // `HookFlags::CALLER_FRAME`, so it points at the live return address.
    if frame.rsp == 0 || unsafe { frame.return_address() } != layout.post_return {
        return None;
    }
    // SAFETY: `text` is non-null, checked by the guard before it, and is the
    // game's span of two words, readable for the call by this function's
    // `# Safety`.
    let [ptr, len]: [usize; 2] = unsafe { enshwick::raw::read_at(text.cast(), 0) };
    if ptr == 0 || len == 0 || len > layout.max_text {
        return None;
    }
    let mut bytes = vec![0; len];
    // SAFETY: `ptr` is non-null, checked by the guard before it, and the
    // span's `len` bytes are readable and unchanged for the call by this
    // function's `# Safety`.
    unsafe { enshwick::raw::read_bytes_at(core::ptr::with_exposed_provenance(ptr), 0, &mut bytes) };
    client_frame(&bytes).map(str::to_owned)
}

/// The frame a line from a player's client carries, which the server takes
/// out of the chat; `None` for every other line, `/` lines included.
fn client_frame(line: &[u8]) -> Option<&str> {
    core::str::from_utf8(line)
        .ok()
        .and_then(crate::net::frame_body)
}

/// Drops the lines waiting for a hosted game that stops.
pub(crate) fn forget() {
    OUTBOX.relock().clear();
    WAITING.store(false, Ordering::Release);
}

/// Posts waiting lines; on the game thread, outside the game's systems.
pub(crate) fn drain() {
    if !WAITING.load(Ordering::Acquire) {
        return;
    }
    let (Some((loader, layout)), Some(post)) = (LOADED.get(), next()) else {
        return;
    };
    let Ok(server) = crate::world::server() else {
        return;
    };
    // SAFETY: `world::server` returned the live server on its game thread
    // between steps; its chat store pointer, null while it loads or closes,
    // sits at the table's `server.chat_store`.
    let store: usize = unsafe { crate::memory::read(server, layout.store) };
    if store == 0 {
        return;
    }
    let lines: Vec<Line> = {
        let mut outbox = OUTBOX.relock();
        let lines = outbox.take(LINES_PER_DRAIN).collect();
        WAITING.store(!outbox.is_empty(), Ordering::Release);
        lines
    };
    let machines = if lines.iter().any(|l| matches!(l.to, To::Key(_))) {
        crate::world::machines(loader).unwrap_or_default()
    } else {
        Vec::new()
    };
    for line in lines {
        let to = match line.to {
            To::Key(_) if DELIVERY.one_player_off.load(Ordering::Relaxed) => {
                dropped(1);
                continue;
            }
            To::Key(key) => {
                let Some(&(_, machine)) = machines.iter().find(|&&(k, _)| k == key) else {
                    dropped(1);
                    continue;
                };
                To::Machine(machine, key)
            }
            other => other,
        };
        // SAFETY: `store` is the server's non-zero chat store, checked by the
        // guard before it, read on its game thread between steps
        // (`world::server`), and `post` is the rest of `chat.post`'s chain.
        unsafe { post_line(post, layout, store, to, cut(&line.text, layout.max_text)) };
    }
}

/// Posts one line and, for one connection, marks it delivered to the rest.
///
/// # Safety
///
/// - `store` is the server's live, non-zero chat store.
/// - Called on the server's game thread between steps, where no system
///   touches the store.
/// - `post` is the rest of the chain on the game's `chat.post`.
unsafe fn post_line(post: PostFn, layout: &Layout, store: usize, to: To, text: &str) {
    let (handle, key, target) = match to {
        To::Machine(handle, key) => {
            let target = (handle & 0x7f) as usize;
            if target >= layout.slot_count {
                dropped(1);
                return;
            }
            (handle, key, Some(target))
        }
        _ => (0, 0, None),
    };
    let mut result = [0u8; 16];
    let span = [text.as_ptr().expose_provenance(), text.len()];
    let extra = [0u64; 2];
    let store_ptr = core::ptr::with_exposed_provenance_mut::<u8>(store);
    // SAFETY: `post` is the rest of `chat.post`'s chain and `store` the live
    // chat store on the game thread, by this function's `# Safety`; `result`
    // is 16 writable bytes, `span` names the live `text` and `extra` two live
    // words, as the game passes them.
    unsafe {
        post(
            result.as_mut_ptr(),
            store_ptr,
            layout.reply_type,
            &raw const span,
            0,
            0,
            handle,
            key,
            &raw const extra,
        );
    };
    let Some(target) = target else {
        return;
    };
    if result[0] != POSTED {
        dropped(1);
        return;
    }
    let active = (0..layout.slot_count)
        .filter(|&slot| {
            // SAFETY: `store` is the live chat store on the game thread, by
            // this function's `# Safety`, and holds the table's
            // `chat.store_slot_count` connection slots, `slot` being under it.
            let active: u8 =
                unsafe { crate::memory::read(store, layout.slots + slot * layout.slot_size) };
            active != 0
        })
        .fold(0u32, |mask, slot| mask | 1 << slot);
    // SAFETY: `store` is the live chat store on the game thread, by this
    // function's `# Safety`; its list tail, the node just posted, sits at the
    // table's `chat.store_tail`.
    let node: usize = unsafe { crate::memory::read(store, layout.tail) };
    #[expect(clippy::cast_possible_truncation, reason = "slots are under 32")]
    let target = target as u32;
    let delivered = (node != 0).then(|| {
        // SAFETY: `node` is non-zero, checked by `then`, and is the store's
        // tail node just posted; its delivered mask sits at the table's
        // `chat.node_delivered`. It stays live across the `mark` calls
        // below, which run between the two reads: the store frees a node
        // only when a later `chat.post` needs pool space and the node is the
        // oldest, or when the whole store is cleared, and `mark_delivered`
        // only sets a bit and counts down pending bytes (server build 399
        // and client build 426). Neither free happens here: both reads run
        // on the game thread with only `mark` calls between them. The
        // target's bit stays unset until the game sends the line to the
        // target on a later step.
        let read_mask = || unsafe { crate::memory::read::<u32>(node, layout.delivered) };
        let bytes = u32::try_from((text.len() + 0x17 + 3) & !3).unwrap_or(u32::MAX);
        let unmarked = strays(active, target, Some(read_mask()));
        for slot in (0..32).filter(|&slot| unmarked & 1 << slot != 0) {
            // SAFETY: `layout.mark` is the table's `chat.mark_delivered`, the
            // game's own step for a line a connection received, on the live
            // store and its non-zero tail node, on the game thread by this
            // function's `# Safety`; `slot` is an active one of the store's
            // slots.
            unsafe {
                (layout.mark)(
                    store_ptr,
                    core::ptr::with_exposed_provenance_mut(node),
                    slot,
                    bytes,
                );
            };
        }
        read_mask()
    });
    let others = strays(active, target, delivered);
    if others != 0 {
        strayed(others.count_ones());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_slash_line_stays_in_the_chat_and_only_frames_are_taken() {
        assert_eq!(client_frame(b"/safe list"), None);
        assert_eq!(client_frame(b"/enshwick status"), None);
        assert_eq!(client_frame(b"hello"), None);
        assert_eq!(client_frame(b"/enshwick-net AQAB"), Some("AQAB"));
    }

    #[test]
    fn no_reply_or_mod_line_passes_for_a_frame() {
        let text = "said: hi\n/enshwick-net AQAAAAAAAAAAAQE\n\n/enshwick ok";
        assert_eq!(
            replied(text).collect::<Vec<_>>(),
            ["said: hi", "/enshwick ok"]
        );
        assert_eq!(replied(&"x\n".repeat(40)).count(), LINES_PER_REPLY);
    }

    #[test]
    fn lines_are_cut_at_character_boundaries() {
        assert_eq!(cut("Сейф", 3), "С");
        assert_eq!(cut("safe", 10), "safe");
        assert_eq!(cut("safe", 0), "");
    }
}
