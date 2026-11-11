//! Chat on the server, the dedicated one or the one a client hosts: lines
//! mods send.
//!
//! The loader links the game's chat post, which every line a player's
//! client sends passes through on the game thread. Lines mods send from any
//! thread wait in a queue that the post-step drains; a line for one player
//! is posted for that player alone, marked as delivered for every other
//! connection; once a line reaches another connection all the same, lines
//! for one player stay off. What players type the game posts as it would
//! without the loader, except a `/` line that names a mod's commands, which
//! leaves the chat and runs that command ([`crate::commands`]), even for a
//! player muted in chat.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

mod said;

pub(crate) use said::{Rate, admit, register};

use enshwick_core::abi::{ChatLine, HookFlags, Status};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::pinned;
use crate::pinned::{Constant, Symbol};
use crate::wire::{Drops, Outbox, strays};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::chat::HANDLE_SLOT.id(),
    pinned::chat::LINE_OVERHEAD.id(),
    pinned::chat::MARK_DELIVERED.id(),
    pinned::chat::MAX_TEXT.id(),
    pinned::chat::NODE_DELIVERED.id(),
    pinned::chat::POST.id(),
    pinned::chat::POST_RETURN.id(),
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
/// Lines waiting past which replies to typed commands queue no more, so
/// players typing commands leave room for the mods' own lines.
const MAX_REPLY_WAITING: usize = MAX_WAITING / 2;
/// The result the game's chat post writes on success.
const POSTED: u8 = 0;
/// The chat type of a line a player typed, which the game's own client
/// sends for typed text (server build 387); the server copies the type a
/// client sends unchecked.
const TYPED: u8 = 0;
/// The chat type of an emote line (server build 387, the loca tag
/// `Emote.chatMessage`), which a vanilla client sends for its player's emotes.
const EMOTE: u8 = 1;

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
    /// Where the server posts what a player typed.
    post_return: u64,
    mark: MarkFn,
    store: usize,
    slots: usize,
    slot_size: usize,
    slot_count: usize,
    tail: usize,
    delivered: usize,
    max_text: usize,
    reply_type: u8,
    /// What lines to one connection need; without it only those lines are
    /// off.
    one: Option<OneLine>,
}

/// What a line to one connection needs from the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OneLine {
    /// The bits of a connection handle that name its slot in the store.
    handle_slot: u32,
    /// The bytes a line counts beyond its text, before rounding to 4.
    line_overhead: usize,
}

/// The largest row value taken for a mask or an overhead: the game uses
/// both as a sign-extended byte, so a larger byte would be a negative.
const MOST_BYTE: u64 = 0x7f; // bound, not a game offset: the largest positive imm8

impl OneLine {
    /// The handle mask and line overhead from the table's rows; `None`
    /// without either or with one that is zero or past a positive byte.
    fn from_rows(handle_slot: Option<u64>, line_overhead: Option<u64>) -> Option<Self> {
        let byte = |value: Option<u64>| value.filter(|&value| (1..=MOST_BYTE).contains(&value));
        Some(Self {
            handle_slot: u32::try_from(byte(handle_slot)?).ok()?,
            line_overhead: usize::try_from(byte(line_overhead)?).ok()?,
        })
    }
}

/// The bytes the store counts for a line of `text_len` bytes, `overhead`
/// bytes beyond its text, rounded up to 4 as the game rounds them.
fn line_bytes(text_len: usize, overhead: usize) -> u32 {
    let bytes = text_len.saturating_add(overhead).saturating_add(3) & !3;
    u32::try_from(bytes).unwrap_or(u32::MAX)
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

static LOADED: OnceLock<(&'static Loader, Layout)> = OnceLock::new();
// SAFETY: `PostFn` is the signature of the game's `chat.post`, and `link`
// uses this slot only as the `next` of `post`, its link on that function.
static POST_NEXT: Next<PostFn> = unsafe { Next::new() };
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
    /// Set when this build has no usable handle mask or line overhead, or
    /// once a line for one player reached others: no line goes to one player
    /// from then on, since this build's chat does not keep it there.
    one_player_off: AtomicBool,
    /// Players' lines whose client sent a chat type other than typed text
    /// or an emote, which were dropped.
    foreign: Drops,
}

static DELIVERY: Delivery = Delivery {
    dropped: Drops::new(),
    strayed: Drops::new(),
    one_player_off: AtomicBool::new(false),
    foreign: Drops::new(),
};

/// The lines for one player dropped and those that reached
/// others, for the status summary.
pub(crate) fn delivery() -> (u64, u64) {
    (DELIVERY.dropped.total(), DELIVERY.strayed.total())
}

/// Whether lines for one player go out: off once one reached others.
pub(crate) fn to_one_player() -> bool {
    !DELIVERY.one_player_off.load(Ordering::Relaxed)
}

/// Queues `text` for the player with account key `to`, or for everyone.
pub(crate) fn send(to: u64, text: &str) -> Status {
    if LOADED.get().is_none() {
        return Status::MISSING;
    }
    let to = if to == 0 { To::All } else { To::Key(to) };
    let _left_out = queue(to, replied(text), MAX_WAITING);
    Status::OK
}

/// Queues every line of `text` for the player with account key `to` alone:
/// the reply to a command they typed, which may be longer than the one
/// message a mod sends, so a long help arrives whole, while fewer than
/// [`MAX_REPLY_WAITING`] lines wait.
pub(crate) fn reply(to: u64, text: &str) -> Status {
    if LOADED.get().is_none() {
        return Status::MISSING;
    }
    let left_out = queue(To::Key(to), shown(text), MAX_REPLY_WAITING);
    if left_out > 0 {
        dropped(left_out);
    }
    Status::OK
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

/// Queues `lines`, each a chat line, while fewer than `room` lines wait;
/// how many it left out.
fn queue<'a>(to: To, lines: impl Iterator<Item = &'a str>, room: usize) -> usize {
    let mut outbox = OUTBOX.relock();
    let mut left_out = 0;
    for line in lines {
        let line = Line {
            to,
            text: line.to_owned(),
        };
        if left_out > 0 || outbox.len() >= room || !outbox.push_whole([line]) {
            left_out += 1;
        }
    }
    WAITING.store(!outbox.is_empty(), Ordering::Release);
    left_out
}

/// The lines of `text` the chat shows: none empty.
fn shown(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|l| !l.trim().is_empty())
}

/// The lines of a mod's message `text` the chat shows, at most
/// [`LINES_PER_REPLY`].
fn replied(text: &str) -> impl Iterator<Item = &str> {
    shown(text).take(LINES_PER_REPLY)
}

/// Links the chat post on the server; without it mods' chat lines and the
/// messages clients send stay off and the log says so.
pub(crate) fn link(loader: &'static Loader) {
    said::seal();
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
            one: OneLine::from_rows(
                loader.build.constant(pinned::chat::HANDLE_SLOT.id()),
                loader.build.constant(pinned::chat::LINE_OVERHEAD.id()),
            ),
        })
    })();
    let Some(layout) = layout else {
        crate::log!(WARN, "chat lines off: this build has no chat layout");
        return;
    };
    if LOADED.get().is_some() {
        return;
    }
    if layout.one.is_none() {
        // Refused at the queue, so callers and the status see them off.
        DELIVERY.one_player_off.store(true, Ordering::Relaxed);
        crate::log!(
            WARN,
            "chat lines to one player off: this build has no usable handle mask or line overhead"
        );
    }
    // Linked first: without the post, mods' lines must stay refused.
    let target = Target::Pinned(pinned::chat::POST);
    if !loader.link_feature(
        "chat lines",
        target,
        &POST_NEXT,
        post,
        HookFlags::CALLER_FRAME,
    ) || LOADED.set((loader, layout)).is_err()
    {
        return;
    }
    crate::log!(INFO, "chat lines on");
}

/// The switch of the chat lines and the chat mute.
static CHAT_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat lines");
/// The switch of the chat lines mods filter and hear, apart from the mute.
static MODS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat lines for mods");
/// The switch of the mods' commands typed in chat.
static COMMANDS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat commands");

/// Takes a line the player of account `key` typed out of the chat when it
/// is a mod's command, queued to run after the step; false for any other
/// line. Reads the text only when it starts with `/`.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn command(loader: &'static Loader, text: *const [usize; 2], key: u64) -> bool {
    let Some(&(_, layout)) = LOADED.get() else {
        return false;
    };
    let mut first = [0u8; 1];
    // SAFETY: the caller guarantees `text` null or readable for the call.
    if unsafe { read_start(text, &mut first) } != Some(1) || first != *b"/" {
        return false;
    }
    // SAFETY: the caller guarantees `text` null or readable for the call.
    let line = unsafe { read_text(text, layout.max_text) };
    crate::commands::queue(loader, key, &line)
}

/// Copies the first bytes of the game's span `text` into `start`, at most
/// its length; how many, or `None` for a null span.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn read_start(text: *const [usize; 2], start: &mut [u8]) -> Option<usize> {
    if text.is_null() {
        return None;
    }
    // SAFETY: `text` is non-null, checked by the guard before it, and is the
    // game's span of two words, readable for the call by this function's
    // `# Safety`.
    let [ptr, len]: [usize; 2] = unsafe { enshwick_core::raw::read_at(text.cast(), 0) };
    if ptr == 0 {
        return None;
    }
    let count = len.min(start.len());
    // SAFETY: `ptr` is non-null and the span holds `len` bytes, at least
    // `count`, readable and unchanged for the call by this function's
    // `# Safety`.
    unsafe {
        enshwick_core::raw::read_bytes_at(
            core::ptr::with_exposed_provenance(ptr),
            0,
            &mut start[..count],
        );
    };
    Some(count)
}

fn next() -> Option<PostFn> {
    POST_NEXT.wired()
}

/// The loader's link on the game's chat post: drops what a muted player
/// sent and a client's line of a type no client sends, passes every other
/// line a player's client sent through the chat filters, and hands the
/// server's own lines on unchanged; it gives the queue's drain the
/// rest of the chain to post through.
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
    // SAFETY: this is the entry of the loader's link on `chat.post`, on the
    // game's calling thread, by this function's `# Safety`.
    let typed = unsafe { typed_line(out) };
    if let Some(loader) = typed.filter(|_| crate::commands::any()) {
        let taken = COMMANDS_FUSE
            .run("the chat command check", || {
                // SAFETY: `text` is the game's own argument at the entry of
                // `chat.post`, by this function's `# Safety`.
                unsafe { command(loader, text, hash) }
            })
            .unwrap_or(false);
        if taken {
            // SAFETY: `typed` is set only for a non-null `out`, a result
            // record of at least 8 writable bytes by this function's
            // `# Safety`.
            unsafe { enshwick_core::raw::write_at(out, 0, 0u64) };
            return out;
        }
    }
    let judged = match typed {
        None => Verdict::Pass,
        // A check that panicked turns its fuse off: the mute is gone for the
        // process, but a player's line still keeps only a type a client may send.
        Some(loader) => CHAT_FUSE
            .run("the chat line check", || {
                // SAFETY: `text` is the game's own argument at the entry of
                // `chat.post`, by this function's `# Safety`.
                unsafe { judge(loader, kind, text, hash) }
            })
            .unwrap_or_else(|| verdict(true, kind, false, false)),
    };
    if judged == Verdict::Drop {
        if !sent_by_clients(kind) && DELIVERY.foreign.add(1).is_some() {
            // shortcut: logs the first drop and every 100th, not once per player; a per-player set when the console needs it.
            crate::log!(
                DEBUG,
                "chat: the client of {hash:016x} sent a line of chat type {kind}; it is dropped"
            );
        }
        // SAFETY: only a player's line is dropped, and it is judged only
        // for a non-null `out`, a result record of at least 8 writable
        // bytes by this function's `# Safety`.
        unsafe { enshwick_core::raw::write_at(out, 0, 0u64) };
        return out;
    }
    let heard = MODS_FUSE
        .run("the chat line for mods", || {
            // SAFETY: `text` is the game's own argument at the entry of
            // `chat.post`, by this function's `# Safety`.
            unsafe { for_mods(typed.is_some(), kind, text, hash, [loca1, loca2]) }
        })
        .unwrap_or(Heard::Keep);
    let rewritten = match heard {
        Heard::Keep => None,
        Heard::Drop => {
            // SAFETY: a line is dropped only for a non-null `out` (a typed
            // line), a result record of at least 8 writable bytes by this
            // function's `# Safety`.
            unsafe { enshwick_core::raw::write_at(out, 0, 0u64) };
            return out;
        }
        Heard::Rewrite(new) => Some(new),
    };
    let span = rewritten
        .as_ref()
        .map(|new| [new.as_ptr().expose_provenance(), new.len()]);
    let text = span.as_ref().map_or(text, |span| &raw const *span);
    match next() {
        // SAFETY: `next` is the rest of `chat.post`'s chain, wired while this
        // detour runs, and the arguments are the game's own, the chat type
        // bounded to typed text or an emote for a line a player's client
        // sent; a text a
        // filter rewrote is a span over `rewritten`, which lives past the
        // call and holds at most the table's `chat.max_text` bytes.
        Some(next) => unsafe { next(out, store, kind, text, loca1, loca2, handle, hash, extra) },
        None => out,
    }
}

/// What mods made of a line the game posts.
#[derive(Debug, PartialEq, Eq)]
enum Heard {
    /// It goes on as it is.
    Keep,
    /// A chat filter dropped it.
    Drop,
    /// The chat filters left this text instead.
    Rewrite(String),
}

/// The kind mods hear a line of chat type `kind` as: a player's own text
/// when a client sent it (`typed`) as typed text, else what the type says,
/// so a client's emote is heard as an emote.
const fn heard_as(typed: bool, kind: u8) -> u32 {
    if typed && kind == TYPED {
        ChatLine::PLAYER
    } else {
        said::kind_of(kind)
    }
}

/// Hands a line the game posts to the mods: every line a player's client
/// sent (`typed`), an emote included, to the chat filters, so no type byte
/// carries text past them; then every line that goes on to the lines mods
/// hear after the step. Nothing happens while no mod filters or hears the
/// chat.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn for_mods(
    typed: bool,
    kind: u8,
    text: *const [usize; 2],
    key: u64,
    loca: [u32; 2],
) -> Heard {
    let Some(&(loader, layout)) = LOADED.get() else {
        return Heard::Keep;
    };
    if said::idle(loader) {
        return Heard::Keep;
    }
    // SAFETY: the caller guarantees `text` null or readable for the call.
    let original = unsafe { read_text(text, layout.max_text) };
    let kind = heard_as(typed, kind);
    let (line, heard) = if typed {
        match said::filter(key, loca, original.clone(), layout.max_text) {
            None => return Heard::Drop,
            Some(line) if line == original => (line, Heard::Keep),
            Some(line) => (line.clone(), Heard::Rewrite(line)),
        }
    } else {
        (original, Heard::Keep)
    };
    said::heard(
        loader,
        said::Said {
            player: key,
            kind,
            loca,
            text: line,
        },
    );
    heard
}

/// The text of the game's span `text`, at most `max` bytes, with bytes that
/// are not UTF-8 replaced.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn read_text(text: *const [usize; 2], max: usize) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: `text` is non-null, checked by the guard before it, and is the
    // game's span of two words, readable for the call by this function's
    // `# Safety`.
    let [ptr, len]: [usize; 2] = unsafe { enshwick_core::raw::read_at(text.cast(), 0) };
    if ptr == 0 || len == 0 {
        return String::new();
    }
    let mut bytes = vec![0u8; len.min(max)];
    // SAFETY: `ptr` is non-null and the span holds `len` bytes, at least
    // `bytes`' length, readable and unchanged for the call by this
    // function's `# Safety`.
    unsafe {
        enshwick_core::raw::read_bytes_at(core::ptr::with_exposed_provenance(ptr), 0, &mut bytes);
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    // A replaced byte takes three, so a peer's line could pass the limit.
    text.truncate(text.floor_char_boundary(max));
    text
}

/// The start of the hidden lines an older loader's client sends its server.
const OLD_FRAME: &[u8] = b"/enshwick-net ";

/// Whether `text` starts like an older loader's hidden frame line, which no
/// other player should read.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn old_frame(text: *const [usize; 2]) -> bool {
    let mut start = [0u8; OLD_FRAME.len()];
    // SAFETY: the caller guarantees `text` null or readable for the call.
    let read = unsafe { read_start(text, &mut start) };
    read == Some(OLD_FRAME.len()) && start == OLD_FRAME
}

/// The verdict on a line of chat type `kind` the player of account `key`
/// sent: dropped when the player is muted, the line an old frame line or of
/// a type no client sends, else it keeps its type.
///
/// # Safety
///
/// - `text` is null or the game's span of pointer and byte length, its bytes
///   readable and unchanged for the call.
unsafe fn judge(loader: &'static Loader, kind: u8, text: *const [usize; 2], key: u64) -> Verdict {
    // SAFETY: the caller guarantees `text` null or readable for the call.
    let old = unsafe { old_frame(text) };
    let muted = !old && crate::chat_mute::drops(loader, key);
    verdict(true, kind, muted, old)
}

/// The loader when the game posts a line a player's client sent, told by
/// where the game posts it, the dispatcher of what clients send, never by
/// its type byte; `None` for every other line and a null `out`. It holds no
/// call that can panic, so it decides even while the chat fuse is off.
///
/// # Safety
///
/// - Called at the entry of the loader's link on `chat.post`, on the game's
///   calling thread.
unsafe fn typed_line(out: *mut u8) -> Option<&'static Loader> {
    let (loader, layout) = LOADED.get()?;
    if out.is_null() {
        return None;
    }
    let frame = (loader.caller_frame)();
    // SAFETY: `frame.rsp` is non-zero, checked first, and `caller_frame`
    // took the frame at the entry of this chain on this thread, linked with
    // `HookFlags::CALLER_FRAME`, so it points at the live return address.
    let typed = frame.rsp != 0 && unsafe { frame.return_address() } == layout.post_return;
    typed.then_some(*loader)
}

/// What the loader's link on `chat.post` does with a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// The line goes no further.
    Drop,
    /// The line goes on with its chat type.
    Pass,
}

/// The verdict on a line of chat type `kind`: a line a player's client sent
/// (`typed`) is dropped when its player is `muted`, it starts like an old
/// frame line or its type is one no client sends for its player, since the
/// server copies that byte unchecked and posts the line under the player's
/// name (server build 387); otherwise it keeps its type. A line the server
/// or the loader posts itself keeps its type.
fn verdict(typed: bool, kind: u8, muted: bool, old_frame: bool) -> Verdict {
    if !typed {
        return Verdict::Pass;
    }
    if muted || old_frame || !sent_by_clients(kind) {
        Verdict::Drop
    } else {
        Verdict::Pass
    }
}

/// Whether a client sends lines of chat type `kind` for its player: typed
/// text and emotes. A line of another type (a client sent type 2 on a fall,
/// client 26430625) carries no text the chat shows under the player's name.
const fn sent_by_clients(kind: u8) -> bool {
    kind == TYPED || kind == EMOTE
}

/// Drops the lines waiting for a hosted game that stops.
pub(crate) fn forget() {
    OUTBOX.relock().clear();
    WAITING.store(false, Ordering::Release);
    said::forget();
}

/// Raises the lines mods hear, then posts waiting lines; on the game
/// thread, outside the game's systems.
pub(crate) fn drain() {
    if let Some(&(loader, _)) = LOADED.get() {
        said::raise(loader);
    }
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
            let Some(one) = layout.one else {
                dropped(1);
                return;
            };
            let target = (handle & one.handle_slot) as usize;
            if target >= layout.slot_count {
                dropped(1);
                return;
            }
            (handle, key, Some((target, one)))
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
    let Some((target, one)) = target else {
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
        let bytes = line_bytes(text.len(), one.line_overhead);
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
mod tests;
