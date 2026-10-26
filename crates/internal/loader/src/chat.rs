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
//!
//! The same links carry mod messages (see the net module): the server takes
//! a client's frame lines as it takes commands, and a client takes the
//! server's frame lines where it posts what the server sent, before its chat
//! shows them, and puts its own frames into its chat queue before the game
//! sends it.

use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick::abi::{
    CommandCall, CommandCallFn, Header, HookFlags, Level, PlayerInfo, Status, Str,
};
use enshwick::commands::{is_valid_name, parse_line};

use crate::chains::Next;
use crate::host::Loader;
use crate::settings::LOADER_ID;
use crate::texts;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "chat.client_queue",
    "chat.client_send",
    "chat.client_store",
    "chat.mark_delivered",
    "chat.max_text",
    "chat.node_delivered",
    "chat.post",
    "chat.post_return",
    "chat.receive_return_a",
    "chat.receive_return_b",
    "chat.reply_type",
    "chat.store_slot_count",
    "chat.store_slot_size",
    "chat.store_slots",
    "chat.store_tail",
    "server.chat_store",
];

/// Lines posted per drain: the game's chat pool drops undelivered lines
/// when it fills.
const LINES_PER_DRAIN: usize = 16;
/// Lines one reply may have.
const LINES_PER_REPLY: usize = 16;
/// Lines waiting at most; more are dropped.
const MAX_WAITING: usize = 256;
/// Commands one player may type per second, and in one burst; more are
/// dropped unanswered, so no player fills the lines every other player
/// waits for.
const COMMAND_RATE: f64 = 4.0;
const COMMAND_BURST: f64 = 12.0;
/// The result the game's chat post writes on success.
const POSTED: u8 = 0;

/// A registered command.
#[derive(Clone, Debug)]
struct Command {
    owner: usize,
    name: String,
    help: String,
    callback: CommandCallFn,
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
// SAFETY: the slot joins the chain of the chat post, typed `PostFn`.
static POST_NEXT: Next<PostFn> = unsafe { Next::new() };
// SAFETY: the slot joins the chain of the client's chat send, typed
// `ClientSendFn`.
static CLIENT_SEND_NEXT: Next<ClientSendFn> = unsafe { Next::new() };
static CLIENT: OnceLock<ClientLayout> = OnceLock::new();
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
    callback: CommandCallFn,
    user: *mut c_void,
) -> Status {
    if SEALED.load(Ordering::Acquire) {
        return Status::TOO_LATE;
    }
    if !is_valid_name(name)
        || name == LOADER_ID
        || name == crate::configure::COMMAND
        || name == crate::net::COMMAND
    {
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

/// Queues the frame line `line` for the player with account key `to`: the
/// one way a line a client takes as the server's frame leaves the server.
pub(crate) fn send_frame(to: u64, line: &str) {
    if LOADED.get().is_none() || to == 0 {
        return;
    }
    let mut outbox = OUTBOX.lock().unwrap_or_else(PoisonError::into_inner);
    if outbox.len() < MAX_WAITING {
        outbox.push(Line {
            to: To::Key(to),
            text: line.to_owned(),
        });
    }
    WAITING.store(!outbox.is_empty(), Ordering::Release);
}

/// `text` cut to at most `max` bytes at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Queues the lines of `text`, a reply or a mod's message, leaving out any
/// a client would take for a frame of the server's: such a line may carry
/// what a player typed.
fn queue(to: To, text: &str) {
    let mut outbox = OUTBOX.lock().unwrap_or_else(PoisonError::into_inner);
    for line in replied(text) {
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

/// The lines of `text` the chat shows: none empty, none a frame, at most
/// [`LINES_PER_REPLY`].
fn replied(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !crate::net::is_frame(l.as_bytes()))
        .take(LINES_PER_REPLY)
}

/// A player's command rate: tokens left and when it last typed.
#[derive(Clone, Copy, Debug)]
struct Rate {
    tokens: f64,
    seen: std::time::Instant,
}

impl Rate {
    /// Takes one command at `now`; false when the rate is spent.
    fn admit(&mut self, now: std::time::Instant) -> bool {
        let gained = now.duration_since(self.seen).as_secs_f64() * COMMAND_RATE;
        self.seen = now;
        self.tokens = (self.tokens + gained).min(COMMAND_BURST);
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

/// The command rates of the players who typed, by account key.
static RATES: Mutex<Option<std::collections::HashMap<u64, Rate>>> = Mutex::new(None);
/// Commands dropped over a player's rate.
static OVER_RATE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Takes one command from the rate of the player with account key `key`;
/// false, counted and now and then logged, when it is spent.
fn admit(key: u64) -> bool {
    let now = std::time::Instant::now();
    let admitted = {
        let mut rates = RATES.lock().unwrap_or_else(PoisonError::into_inner);
        let rates = rates.get_or_insert_with(Default::default);
        if rates.len() > 256 {
            // Players idle long enough to have a full rate again lose
            // nothing by being forgotten.
            let full = COMMAND_BURST / COMMAND_RATE;
            rates.retain(|_, rate| now.duration_since(rate.seen).as_secs_f64() < full);
        }
        rates
            .entry(key)
            .or_insert(Rate {
                tokens: COMMAND_BURST,
                seen: now,
            })
            .admit(now)
    };
    if !admitted {
        let count = OVER_RATE.fetch_add(1, Ordering::Relaxed) + 1;
        if count == 1 || count.is_multiple_of(100) {
            crate::log!(WARN, "chat commands: {count} dropped over a player's rate");
        }
    }
    admitted
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
            receive_returns: [
                symbol("chat.receive_return_a").unwrap_or(0),
                symbol("chat.receive_return_b").unwrap_or(0),
            ],
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
    link_client(loader, symbol("chat.client_send"));
}

/// Links the client's chat send, so its own frames go with what it sends.
fn link_client(loader: &'static Loader, target: Option<u64>) {
    let (Some(target), Some(queue), Some(store)) = (
        target,
        loader.address("chat.client_queue"),
        loader.offset("chat.client_store"),
    ) else {
        return;
    };
    let Ok(queue) = usize::try_from(queue) else {
        return;
    };
    let _first = CLIENT.set(ClientLayout {
        // SAFETY: the symbol is the client's chat queue with this signature
        // on this build.
        queue: unsafe { core::mem::transmute::<usize, ClientQueueFn>(queue) },
        store,
    });
    loader.link_feature(
        "mod messages to the server",
        Some(target),
        &CLIENT_SEND_NEXT,
        client_send as ClientSendFn,
        HookFlags::NONE,
    );
}

/// The switch of the chat commands and mod messages.
static CHAT_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat commands and mod messages");

unsafe extern "system" fn client_send(client: *mut u8) -> u64 {
    let _caught = CHAT_FUSE.run("the client send", || {
        // SAFETY: the game client, on the thread that sends its chat.
        unsafe { queue_frames(client) };
    });
    // SAFETY: forwarded unchanged.
    unsafe { CLIENT_SEND_NEXT.get()(client) }
}

/// Puts the client's waiting frame lines into its chat queue.
///
/// # Safety
///
/// `client` is the game client, on the thread that sends its chat.
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
    // SAFETY: the game client holds its chat store here.
    let store: usize = unsafe { crate::world::read(client.addr(), layout.store) };
    if store == 0 {
        return;
    }
    for line in lines {
        let span = [line.as_ptr().expose_provenance(), line.len()];
        let mut result = [0u8; 16];
        // SAFETY: the client's chat store, on its own thread, with a line
        // the game copies before the call returns.
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
        // SAFETY: the game's arguments, read before anything else is called.
        unsafe { server_frame(out, kind, text) }
    });
    if frame.unwrap_or(false) {
        // SAFETY: the game's result record holds at least 8 bytes.
        unsafe { out.cast::<u64>().write_unaligned(0) };
        return out;
    }
    let caught = CHAT_FUSE.run("the command check", || {
        // SAFETY: the game's arguments, read before anything else is called.
        unsafe { command_line(out, kind, text) }
    });
    if let Some(Some((loader, line))) = caught {
        // SAFETY: the game's result record holds at least 8 bytes.
        unsafe { out.cast::<u64>().write_unaligned(0) };
        let _outcome = CHAT_FUSE.run("a command", || {
            dispatch(loader, &line, To::Machine(handle, hash));
        });
        let _outcome = CHAT_FUSE.run("the command queue", drain);
        return out;
    }
    match next() {
        // SAFETY: forwarded unchanged.
        Some(next) => unsafe { next(out, store, kind, text, loca1, loca2, handle, hash, extra) },
        None => out,
    }
}

/// Whether the client is posting a frame line the server sent, which it
/// then hands to the net instead of the chat.
///
/// # Safety
///
/// The game's chat post arguments, at its entry.
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
    // SAFETY: the frame was taken at this chain's entry, on this thread.
    let caller = unsafe { frame.return_address() };
    if !layout.receive_returns.contains(&caller) {
        return false;
    }
    // SAFETY: the game passes its text span: pointer and byte length.
    let [ptr, len] = unsafe { text.read() };
    if ptr == 0 || len == 0 || len > layout.max_text {
        return false;
    }
    // SAFETY: the span holds `len` bytes for the call.
    let bytes =
        unsafe { core::slice::from_raw_parts(core::ptr::with_exposed_provenance::<u8>(ptr), len) };
    if !crate::net::is_frame(bytes) {
        return false;
    }
    core::str::from_utf8(bytes).is_ok_and(crate::net::client_received)
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
    if name == crate::net::COMMAND {
        crate::net::server_received(key, args);
        return;
    }
    if !admit(key) {
        return;
    }
    let player = sender(loader, key);
    let who =
        String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)]).into_owned();
    crate::log!(DEBUG, "{who} ({key:016x}) typed /{name}");
    let reply = if name == LOADER_ID {
        builtin(loader, args, &player)
    } else if name == crate::configure::COMMAND {
        config(loader, args, &player)
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
    if name == crate::configure::COMMAND {
        return Ok(config(loader, args, &player));
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
    crate::profile::call(
        command.owner,
        crate::profile::Feature::Commands,
        || unsafe {
            (command.callback)(
                &raw const call,
                core::ptr::with_exposed_provenance_mut(command.user),
                &raw mut reply,
            );
        },
    );
    // SAFETY: the reply stays valid until the handler runs again.
    unsafe { reply.as_str() }.unwrap_or("").to_owned()
}

/// `/enshwick`: the mods running; `/enshwick help`: every command;
/// `/enshwick <mod id>`: what the mod is; `/enshwick status`: the status
/// summary, for administrators.
fn builtin(loader: &Loader, args: &str, player: &PlayerInfo) -> String {
    if args.eq_ignore_ascii_case("status") {
        if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
            return texts::fill("enshwick.status.admins", &[]);
        }
        crate::status::request();
        return crate::status::summary(loader).short();
    }
    if let Some(rest) = word(args, "profile") {
        if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
            return texts::fill("enshwick.profile.admins", &[]);
        }
        if rest.eq_ignore_ascii_case("reset") {
            crate::status::reset_profile(loader);
            return texts::fill("enshwick.profile.reset", &[]);
        }
        let mut lines = crate::status::cost_lines(loader, 5, 2);
        lines.push(texts::fill("enshwick.profile.full", &[]));
        return lines.join(
            "
",
        );
    }
    if args.eq_ignore_ascii_case("conflicts") {
        if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
            return texts::fill("enshwick.conflicts.admins", &[]);
        }
        let lines = crate::status::conflicts(loader);
        if lines.is_empty() {
            return texts::fill("enshwick.conflicts.none", &[]);
        }
        return lines.join(
            "
",
        );
    }
    if args.eq_ignore_ascii_case("ui-demo") {
        return ui_demo(loader, player);
    }
    if let Some(rest) = word(args, "recipes") {
        return crate::recipes::command(loader, rest, player);
    }
    if let Some(rest) = word(args, "log") {
        if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
            return texts::fill("enshwick.log.admins", &[]);
        }
        return log_lines(loader, rest);
    }
    let commands = COMMANDS.lock().unwrap_or_else(PoisonError::into_inner);
    if args.eq_ignore_ascii_case("help") {
        return help(loader, &commands);
    }
    if !args.is_empty() {
        return loader
            .mods
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(args))
            .map_or_else(|| texts::fill("enshwick.unknown", &[("id", args)]), about);
    }
    let mods: Vec<String> = loader.mods.iter().map(name).collect();
    let version = env!("CARGO_PKG_VERSION");
    let mut sentences = vec![if mods.is_empty() {
        texts::fill("enshwick.mods.none", &[("version", version)])
    } else {
        texts::fill(
            "enshwick.mods",
            &[("version", version), ("mods", &mods.join(", "))],
        )
    }];
    let disabled = crate::modlist::disabled();
    if !disabled.is_empty() {
        sentences.push(texts::fill(
            "enshwick.off-in",
            &[
                ("file", crate::modlist::LIST_PATH),
                ("mods", &disabled.join(", ")),
            ],
        ));
    }
    sentences.extend(crate::modlist::replaced().iter().map(|old| {
        texts::fill(
            "enshwick.replaced",
            &[("id", &old.id), ("by", &old.by), ("file", &old.file)],
        )
    }));
    sentences.push(texts::fill("enshwick.more", &[]));
    sentences.join(" ")
}

/// A mod's name and version, marked when it is off.
fn name(record: &crate::host::ModRecord) -> String {
    let label = record.label();
    if record.off.load(Ordering::Acquire) {
        texts::fill(
            "mods.off",
            &[("name", &label), ("version", &record.version)],
        )
    } else {
        format!("{label} {}", record.version)
    }
}

/// `/config`, for administrators: read and change mods' settings.
fn config(loader: &Loader, args: &str, player: &PlayerInfo) -> String {
    use crate::configure::{self, Request};

    if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
        return texts::fill("config.admins", &[]);
    }
    let request = configure::parse(args);
    let id = match request {
        Request::Mods => {
            let with: Vec<&str> = loader
                .mods
                .iter()
                .filter(|m| m.schema.lock().is_ok_and(|s| s.is_some()))
                .map(|m| m.id.as_str())
                .collect();
            return if with.is_empty() {
                texts::fill("config.none", &[])
            } else {
                texts::fill("config.mods", &[("mods", &with.join(", "))])
            };
        }
        Request::List(id) | Request::Get(id, _) | Request::Set(id, _, _) => id,
    };
    let Some((index, record)) = loader.mods.iter().enumerate().find(|(_, m)| m.id == id) else {
        return texts::fill("config.unknown", &[("id", id)]);
    };
    let Some(schema) = record.schema.lock().ok().and_then(|s| s.clone()) else {
        return texts::fill("config.no-schema", &[("id", id)]);
    };
    let path = loader.configs_dir.join(format!("{id}.toml"));
    if !path.is_file() {
        return texts::fill("config.missing", &[("id", id)]);
    }
    let text = crate::worlds::show_text(loader, record);
    match request {
        Request::Set(_, key, value) => {
            let who = String::from_utf8_lossy(&player.name[..usize::from(player.name_len).min(64)])
                .into_owned();
            match configure::apply(loader, index, key, value, &who) {
                Ok(change) => configure::changed(id, key, &change),
                Err(reply) => reply,
            }
        }
        Request::Get(_, key) => configure::get(id, &schema, &text, key).unwrap_or_else(|e| e),
        _ => configure::list(id, &schema, &text),
    }
}

/// What `record` is, for `/enshwick <mod id>`.
fn about(record: &crate::host::ModRecord) -> String {
    let mut lines = vec![name(record)];
    if !record.meta.description.is_empty() {
        lines.push(record.meta.description.clone());
    }
    let credits = record.meta.credits();
    if !credits.is_empty() {
        lines.push(credits);
    }
    lines.extend(record.notes.iter().cloned());
    lines.join("\n")
}

/// The rest of `args` after the word `name`, or `None` when it starts with
/// another.
fn word<'a>(args: &'a str, name: &str) -> Option<&'a str> {
    let (first, rest) = args.split_once(' ').unwrap_or((args, ""));
    first.eq_ignore_ascii_case(name).then(|| rest.trim())
}

/// `/enshwick log [mod] [level] [n]`: the latest `n` lines (10 by default,
/// at most a reply's) the loader kept, from one mod or the loader (`enshwick`)
/// or all, at `level` or more severe.
fn log_lines(loader: &Loader, args: &str) -> String {
    match log_query(args, |id| loader.mods.iter().any(|m| m.id == id)) {
        Ok((source, level, count)) => {
            let lines = crate::log::recent(source.as_deref(), level, count);
            if lines.is_empty() {
                texts::fill("enshwick.log.none", &[])
            } else {
                lines.join("\n")
            }
        }
        Err(why) => why,
    }
}

/// The source, level and count `/enshwick log` asks for, in any order; a
/// word that is no number, level or known source (`known`, or the loader's
/// own id) is an error naming it.
fn log_query(
    args: &str,
    known: impl Fn(&str) -> bool,
) -> Result<(Option<String>, Level, usize), String> {
    let mut source = None;
    let mut level = Level::DEBUG;
    let mut count = 10;
    for word in args.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            count = n.clamp(1, LINES_PER_REPLY);
        } else if let Ok(named) = word.to_ascii_lowercase().parse::<Level>() {
            level = named;
        } else if word.eq_ignore_ascii_case(LOADER_ID) || known(word) {
            source = Some(word.to_ascii_lowercase());
        } else {
            return Err(texts::fill("enshwick.log.usage", &[("word", word)]));
        }
    }
    Ok((source, level, count))
}

/// `/enshwick ui-demo`: opens the interface demo on the administrator's
/// own client.
fn ui_demo(loader: &Loader, player: &PlayerInfo) -> String {
    if player.permissions & PlayerInfo::CAN_KICK_BAN == 0 {
        return texts::fill("ui-demo.admins", &[]);
    }
    if player.key == 0 {
        return texts::fill("ui-demo.player", &[]);
    }
    match crate::net::ui_demo(
        player.key,
        crate::world::fixed_position(loader, player.entity),
    ) {
        Ok(()) => texts::fill("ui-demo.opening", &[]),
        Err(_) => texts::fill("ui-demo.no-client", &[]),
    }
}

fn help(loader: &Loader, commands: &[Command]) -> String {
    let mut lines = vec![texts::fill("enshwick.help", &[]), crate::configure::help()];
    lines.extend(commands.iter().filter_map(|c| {
        let record = loader.mods.get(c.owner)?;
        (!record.off.load(Ordering::Acquire))
            .then(|| format!("/{} ({}): {}", c.name, record.id, c.help))
    }));
    lines.join("\n")
}

/// Drops the lines waiting for a hosted game that stops.
pub(crate) fn forget() {
    *RATES.lock().unwrap_or_else(PoisonError::into_inner) = None;
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
    fn a_log_query_takes_its_words_in_any_order() {
        let known = |id: &str| id == "safes";
        assert_eq!(log_query("", known), Ok((None, Level::DEBUG, 10)));
        assert_eq!(
            log_query("warn 5 safes", known),
            Ok((Some("safes".to_owned()), Level::WARN, 5))
        );
        assert_eq!(
            log_query("Enshwick 999", known),
            Ok((Some("enshwick".to_owned()), Level::DEBUG, LINES_PER_REPLY))
        );
        assert!(log_query("nobody", known).unwrap_err().contains("nobody"));
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
    fn a_player_s_commands_run_out_after_a_burst_and_come_back() {
        let start = std::time::Instant::now();
        let mut rate = Rate {
            tokens: COMMAND_BURST,
            seen: start,
        };
        let burst = (0..100).filter(|_| rate.admit(start)).count();
        assert_eq!(burst, 12);
        assert!(!rate.admit(start));
        assert!(rate.admit(start + std::time::Duration::from_millis(300)));
        assert!(!rate.admit(start + std::time::Duration::from_millis(300)));
    }

    #[test]
    fn lines_are_cut_at_character_boundaries() {
        assert_eq!(cut("Сейф", 3), "С");
        assert_eq!(cut("safe", 10), "safe");
        assert_eq!(cut("safe", 0), "");
    }
}
