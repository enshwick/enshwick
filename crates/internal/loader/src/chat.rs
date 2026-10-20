//! Chat on the server, the dedicated one or the one a client hosts: commands
//! players type and lines mods send.
//!
//! The loader links the game's chat post, which every line a player types
//! passes through on the game thread. A line from a player's client that
//! starts with `/` is never posted: it goes to the mod that registered its
//! name, or to the loader's own `/enshwick`, and the answer is posted for
//! that player alone, marked as delivered for every other connection. Lines
//! mods send from any thread wait in a queue that the post-step drains.
//!
//! Mods may also run a command line as the server's console, an
//! administrator with no character, on the game thread.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{
    CommandCall, CommandFn, Header, HookFlags, PlayerInfo, Status, Str,
};
use enshwick_loader_api::commands::{is_valid_name, parse_line};

use crate::chains::Next;
use crate::host::Loader;
use crate::settings::LOADER_ID;

/// Lines posted per drain: the game's chat pool drops undelivered lines
/// when it fills.
const LINES_PER_DRAIN: usize = 16;
/// Lines one reply may have.
const LINES_PER_REPLY: usize = 16;
/// Lines waiting at most; more are dropped.
const MAX_WAITING: usize = 256;
/// The result the game's chat post writes on success.
const POSTED: u8 = 0;

/// A registered command.
#[derive(Clone, Debug)]
struct Command {
    owner: usize,
    name: String,
    help: String,
    callback: CommandFn,
    user: usize,
}

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

static LOADED: OnceLock<(&'static Loader, Layout)> = OnceLock::new();
// SAFETY: the slot joins the chain of the chat post, typed `PostFn`.
static POST_NEXT: Next<PostFn> = unsafe { Next::new() };
static COMMANDS: Mutex<Vec<Command>> = Mutex::new(Vec::new());
static SEALED: AtomicBool = AtomicBool::new(false);
static OUTBOX: Mutex<Vec<Line>> = Mutex::new(Vec::new());
/// Set while the outbox holds anything, so an idle step takes no lock.
static WAITING: AtomicBool = AtomicBool::new(false);

/// Registers mod `owner`'s command; refused for a malformed or taken name.
pub(crate) fn register(
    owner: usize,
    name: &str,
    help: &str,
    callback: CommandFn,
    user: *mut c_void,
) -> Status {
    if SEALED.load(Ordering::Acquire) {
        return Status::TOO_LATE;
    }
    if !is_valid_name(name) || name == LOADER_ID {
        return Status::BAD_ARGS;
    }
    let mut commands = COMMANDS.lock().unwrap_or_else(PoisonError::into_inner);
    if commands.iter().any(|c| c.name == name) {
        return Status::REFUSED;
    }
    commands.push(Command {
        owner,
        name: name.to_owned(),
        help: help.to_owned(),
        callback,
        user: user.expose_provenance(),
    });
    Status::OK
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

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn queue(to: To, text: &str) {
    let mut outbox = OUTBOX.lock().unwrap_or_else(PoisonError::into_inner);
    for line in text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(LINES_PER_REPLY)
    {
        if outbox.len() >= MAX_WAITING {
            break;
        }
        outbox.push(Line {
            to,
            text: line.to_owned(),
        });
    }
    WAITING.store(!outbox.is_empty(), Ordering::Release);
}

/// Links the chat post on the server; without it commands and chat lines
/// stay off and the log says so.
pub(crate) fn link(loader: &'static Loader) {
    SEALED.store(true, Ordering::Release);
    let symbol = |name: &str| loader.build.symbol(name).map(|rva| loader.base + rva);
    let constant = |name: &str| {
        loader
            .build
            .constant(name)
            .and_then(|v| usize::try_from(v).ok())
    };
    let layout = (|| {
        let mark = usize::try_from(symbol("chat.mark_delivered")?).ok()?;
        Some(Layout {
            post_return: symbol("chat.post_return")?,
            // SAFETY: the symbol is the game's mark-delivered function on
            // this build.
            mark: unsafe { core::mem::transmute::<usize, MarkFn>(mark) },
            store: constant("server.chat_store")?,
            slots: constant("chat.store_slots")?,
            slot_size: constant("chat.store_slot_size")?,
            slot_count: constant("chat.store_slot_count")?.min(32),
            tail: constant("chat.store_tail")?,
            delivered: constant("chat.node_delivered")?,
            max_text: constant("chat.max_text")?,
            reply_type: u8::try_from(constant("chat.reply_type")?).ok()?,
        })
    })();
    let (Some(layout), Some(target)) = (layout, symbol("chat.post")) else {
        crate::log!(WARN, "chat commands off: this build has no chat layout");
        return;
    };
    if LOADED.set((loader, layout)).is_err() {
        return;
    }
    let status = loader.link(target, &POST_NEXT, post, HookFlags::CALLER_FRAME);
    if !status.is_ok() {
        crate::log!(WARN, "chat commands off: refused ({})", status.0);
        return;
    }
    let count = COMMANDS.lock().map(|c| c.len()).unwrap_or_default();
    crate::log!(INFO, "chat commands on: {count} from mods, and /enshwick");
}

fn next() -> Option<PostFn> {
    POST_NEXT.wired()
}

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
    let caught = std::panic::catch_unwind(|| {
        // SAFETY: the game's arguments, read before anything else is called.
        unsafe { command_line(out, kind, text) }
    });
    if let Ok(Some((loader, line))) = caught {
        // SAFETY: the game's result record holds at least 8 bytes.
        unsafe { out.cast::<u64>().write_unaligned(0) };
        let _outcome =
            std::panic::catch_unwind(|| dispatch(loader, &line, To::Machine(handle, hash)));
        let _outcome = std::panic::catch_unwind(drain);
        return out;
    }
    match next() {
        // SAFETY: forwarded unchanged.
        Some(next) => unsafe { next(out, store, kind, text, loca1, loca2, handle, hash, extra) },
        None => out,
    }
}

/// The line when the game posts a player's `/` line from their client.
///
/// # Safety
///
/// The game's chat post arguments, at its entry.
unsafe fn command_line(
    out: *mut u8,
    kind: u8,
    text: *const [usize; 2],
) -> Option<(&'static Loader, String)> {
    let (loader, layout) = LOADED.get()?;
    if kind != 0 || out.is_null() || text.is_null() {
        return None;
    }
    let frame = (loader.caller_frame)();
    // SAFETY: the frame was taken at this chain's entry, on this thread.
    if frame.rsp == 0 || unsafe { frame.return_address() } != layout.post_return {
        return None;
    }
    // SAFETY: the game passes its text span: pointer and byte length.
    let [ptr, len] = unsafe { text.read() };
    if ptr == 0 || len == 0 || len > layout.max_text {
        return None;
    }
    // SAFETY: the span holds `len` bytes for the call.
    let bytes =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(ptr), len) };
    if bytes.first() != Some(&b'/') {
        return None;
    }
    Some((loader, String::from_utf8_lossy(bytes).into_owned()))
}

/// The player who typed a line: the connected player with the account key,
/// or one known by key alone.
fn sender(loader: &Loader, key: u64) -> PlayerInfo {
    crate::world::players(loader)
        .ok()
        .and_then(|players| players.into_iter().find(|p| p.key == key))
        .unwrap_or(PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: u32::MAX,
            entity: 0,
            key,
            permissions: 0,
            name_len: 0,
            name: [0; 64],
        })
}

/// Runs a command line and queues the reply for `to`.
fn dispatch(loader: &Loader, line: &str, to: To) {
    let Some((name, args)) = parse_line(line) else {
        return;
    };
    let key = match to {
        To::Machine(_, key) | To::Key(key) => key,
        To::All => 0,
    };
    let player = sender(loader, key);
    let who =
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned();
    crate::log!(DEBUG, "{who} ({key:016x}) typed /{name}");
    let reply = if name == LOADER_ID {
        builtin(loader, args, &player)
    } else {
        match find(loader, &name) {
            Some(command) => run(&command, args, &player),
            None => format!("Unknown command /{name}. /enshwick help lists the commands."),
        }
    };
    queue(to, &reply);
}

/// The server's console as a player: an administrator with no character.
fn console() -> PlayerInfo {
    let mut name = [0u8; 64];
    name[..7].copy_from_slice(b"console");
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: 0,
        key: 0,
        permissions: PlayerInfo::CAN_KICK_BAN,
        name_len: 7,
        name,
    }
}

/// Runs `line` as the server's console, on the game thread; the reply.
pub(crate) fn run_console(loader: &Loader, line: &str) -> Result<String, Status> {
    crate::world::server().map_err(|_| Status::WRONG_THREAD)?;
    let (name, args) = parse_line(line).ok_or(Status::BAD_ARGS)?;
    crate::log!(DEBUG, "the console runs /{name}");
    let player = console();
    if name == LOADER_ID {
        return Ok(builtin(loader, args, &player));
    }
    let command = find(loader, &name).ok_or(Status::MISSING)?;
    Ok(run(&command, args, &player))
}

/// The running mod's command `name`.
fn find(loader: &Loader, name: &str) -> Option<Command> {
    let command = COMMANDS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|c| c.name == name)
        .cloned();
    command.filter(|c| {
        loader
            .mods
            .get(c.owner)
            .is_some_and(|m| !m.off.load(Ordering::Acquire))
    })
}

/// Runs a mod's command for `player`; its reply.
fn run(command: &Command, args: &str, player: &PlayerInfo) -> String {
    let call = CommandCall {
        header: Header::of::<CommandCall>(),
        player: *player,
        name: Str::new(&command.name),
        args: Str::new(args),
    };
    let mut reply = Str::EMPTY;
    // SAFETY: the handler lives in the mod's DLL, which stays loaded; the
    // safe layer catches its panics.
    unsafe {
        (command.callback)(
            &raw const call,
            core::ptr::with_exposed_provenance_mut(command.user),
            &raw mut reply,
        );
    };
    // SAFETY: the reply stays valid until the handler runs again.
    unsafe { reply.as_str() }.unwrap_or("").to_owned()
}

/// `/enshwick`: the mods running; `/enshwick help`: every command;
/// `/enshwick status`: the status summary, for administrators.
fn builtin(loader: &Loader, args: &str, player: &PlayerInfo) -> String {
    if args.eq_ignore_ascii_case("status") {
        if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
            return "Only administrators see the status.".to_owned();
        }
        crate::status::request();
        return crate::status::summary(loader).short();
    }
    let commands = COMMANDS.lock().unwrap_or_else(PoisonError::into_inner);
    if args.eq_ignore_ascii_case("help") {
        return help(loader, &commands);
    }
    let mods: Vec<String> = loader
        .mods
        .iter()
        .map(|m| {
            let off = if m.off.load(Ordering::Acquire) {
                " (off)"
            } else {
                ""
            };
            format!("{} {}{off}", m.id, m.version)
        })
        .collect();
    let list = if mods.is_empty() {
        "no mods".to_owned()
    } else {
        mods.join(", ")
    };
    format!(
        "Enshwick {}: {list}. /enshwick help lists the commands.",
        env!("CARGO_PKG_VERSION")
    )
}

fn help(loader: &Loader, commands: &[Command]) -> String {
    let mut lines = vec![
        "/enshwick: the mods running; /enshwick help: this list; /enshwick status: the status (administrators)"
            .to_owned(),
    ];
    lines.extend(commands.iter().filter_map(|c| {
        let record = loader.mods.get(c.owner)?;
        (!record.off.load(Ordering::Acquire))
            .then(|| format!("/{} ({}): {}", c.name, record.id, c.help))
    }));
    lines.join("\n")
}

/// Drops the lines waiting for a hosted game that stops.
pub(crate) fn forget() {
    OUTBOX
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
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
    // SAFETY: the server holds its chat store's pointer; null while it loads
    // or closes.
    let store: usize = unsafe { crate::world::read(server, layout.store) };
    if store == 0 {
        return;
    }
    let lines: Vec<Line> = {
        let mut outbox = OUTBOX.lock().unwrap_or_else(PoisonError::into_inner);
        let count = outbox.len().min(LINES_PER_DRAIN);
        let lines = outbox.drain(..count).collect();
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
            To::Key(key) => match machines.iter().find(|&&(k, _)| k == key) {
                Some(&(_, machine)) => To::Machine(machine, key),
                None => continue,
            },
            other => other,
        };
        // SAFETY: the live chat store, on the game thread outside systems.
        unsafe { post_line(post, layout, store, to, cut(&line.text, layout.max_text)) };
    }
}

/// Posts one line and, for one connection, marks it delivered to the rest.
///
/// # Safety
///
/// `store` is the server's live chat store, used on the game thread.
unsafe fn post_line(post: PostFn, layout: &Layout, store: usize, to: To, text: &str) {
    let (handle, key, target) = match to {
        To::Machine(handle, key) => {
            let target = (handle & 0x7f) as usize;
            if target >= layout.slot_count {
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
    // SAFETY: the rest of the chat post's chain, with the arguments the game
    // passes it.
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
    let Some(target) = target.filter(|_| result[0] == POSTED) else {
        return;
    };
    // SAFETY: the store's list tail is the node just posted.
    let node: usize = unsafe { crate::world::read(store, layout.tail) };
    if node == 0 {
        return;
    }
    let bytes = u32::try_from((text.len() + 0x17 + 3) & !3).unwrap_or(u32::MAX);
    for slot in (0..layout.slot_count).filter(|&s| s != target) {
        // SAFETY: the store holds `slot_count` connection slots.
        let active: u8 =
            unsafe { crate::world::read(store, layout.slots + slot * layout.slot_size) };
        // SAFETY: the node's delivered mask, a bit per slot.
        let mask: u32 = unsafe { crate::world::read(node, layout.delivered) };
        if active != 0 && mask & (1 << slot) == 0 {
            #[expect(clippy::cast_possible_truncation, reason = "slots are under 32")]
            let slot = slot as u32;
            // SAFETY: the game's own step for a line a connection received.
            unsafe {
                (layout.mark)(
                    store_ptr,
                    core::ptr::with_exposed_provenance_mut(node),
                    slot,
                    bytes,
                );
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_cut_at_character_boundaries() {
        assert_eq!(cut("Сейф", 3), "С");
        assert_eq!(cut("safe", 10), "safe");
        assert_eq!(cut("safe", 0), "");
    }
}
