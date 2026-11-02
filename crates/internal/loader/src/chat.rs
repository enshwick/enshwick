//! Chat on the server, the dedicated one or the one a client hosts: lines
//! mods send.
//!
//! The loader links the game's chat post, which every line a player's
//! client sends passes through on the game thread. Lines mods send from any
//! thread wait in a queue that the post-step drains; a line for one player
//! is posted for that player alone, marked as delivered for every other
//! connection; once a line reaches another connection all the same, lines
//! for one player stay off. What players type, `/` lines included, the game posts as it
//! would without the loader.

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

/// Queues `text` for the player with account key `to`, or for everyone.
pub(crate) fn send(to: u64, text: &str) -> Status {
    if LOADED.get().is_none() {
        return Status::MISSING;
    }
    let to = if to == 0 { To::All } else { To::Key(to) };
    queue(to, text);
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

/// Queues the lines of `text`, a mod's message.
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

/// The lines of `text` the chat shows: none empty, at most
/// [`LINES_PER_REPLY`].
fn replied(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
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

fn next() -> Option<PostFn> {
    POST_NEXT.wired()
}

/// The loader's link on the game's chat post: drops what a muted player
/// typed and hands every other post on; it gives the queue's drain the rest
/// of the chain to post through.
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
    let muted = CHAT_FUSE.run("the chat mute check", || {
        // SAFETY: `out` is the game's own argument at the entry of
        // `chat.post`, by this function's `# Safety`.
        unsafe { muted_line(out, kind, hash) }
    });
    if muted == Some(true) {
        // SAFETY: `muted_line` returned true only for a non-null `out`, a
        // result record of at least 8 writable bytes by this function's
        // `# Safety`.
        unsafe { enshwick::raw::write_at(out, 0, 0u64) };
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

/// Whether the game posts a line the player of account `key` typed while
/// muted, which the server then drops before any other player reads it.
///
/// # Safety
///
/// - Called at the entry of the loader's link on `chat.post`, on the game's
///   calling thread.
unsafe fn muted_line(out: *mut u8, kind: u8, key: u64) -> bool {
    let Some((loader, layout)) = LOADED.get() else {
        return false;
    };
    if kind != 0 || out.is_null() {
        return false;
    }
    let frame = (loader.caller_frame)();
    // SAFETY: `frame.rsp` is non-zero, checked first, and `caller_frame`
    // took the frame at the entry of this chain on this thread, linked with
    // `HookFlags::CALLER_FRAME`, so it points at the live return address.
    if frame.rsp == 0 || unsafe { frame.return_address() } != layout.post_return {
        return false;
    }
    crate::chat_mute::drops(loader, key)
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
mod tests {
    use super::*;

    #[test]
    fn a_line_to_one_player_needs_a_positive_byte_mask_and_overhead() {
        let one = OneLine {
            handle_slot: 0x7f,
            line_overhead: 0x17,
        };
        assert_eq!(OneLine::from_rows(Some(0x7f), Some(0x17)), Some(one));
        assert_eq!(OneLine::from_rows(None, Some(0x17)), None, "no mask");
        assert_eq!(OneLine::from_rows(Some(0x7f), None), None, "no overhead");
        assert_eq!(
            OneLine::from_rows(Some(0x80), Some(0x17)),
            None,
            "a negative mask"
        );
        assert_eq!(
            OneLine::from_rows(Some(0x7f), Some(0xe9)),
            None,
            "a negative overhead"
        );
        assert_eq!(OneLine::from_rows(Some(0), Some(0x17)), None, "a zero mask");
        assert_eq!(
            OneLine::from_rows(Some(0x7f), Some(0)),
            None,
            "a zero overhead"
        );
    }

    #[test]
    fn a_line_counts_its_text_and_overhead_rounded_up_to_four() {
        assert_eq!(line_bytes(0, 0x17), 0x18);
        assert_eq!(line_bytes(1, 0x17), 0x18);
        assert_eq!(line_bytes(2, 0x17), 0x1c);
        assert_eq!(line_bytes(0x400, 0x17), 0x418);
        assert_eq!(line_bytes(usize::MAX, 0x17), u32::MAX);
    }

    #[test]
    fn a_reply_keeps_its_lines_that_are_not_empty_up_to_a_bound() {
        let text = "said: hi\n\n  \n/enshwick ok";
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
