//! Joining a dedicated server from the main menu and leaving play: the
//! game's own menu commands the interface frame posts, the phases the
//! interface's stage and play reads show, and why a join failed as far as
//! the client can tell.
//!
//! One join or leave runs at a time. A join first asks the server's query
//! port for its Steam id and game port on a thread of its own, then posts
//! the password (menu command 0x25, which keeps only its hash) and the
//! address (0x26) at the main menu, and the start of play (0x29) until play
//! starts; 0x29 does nothing in the main menu or a world (client 26430625,
//! `0x140873710`, `0x14085e780`). Cancel and leave post the back command
//! (5), which the start-play state takes as back and play as
//! `Play::BackToMenu` (`0x14087c220`, `0x14085e780`). Never posted: 4,
//! which quits the game from the main menu.
//!
//! A host posts the character (0x11), the world (0x16), the session's
//! kind (0x27, which also sets the session to a hosted one) and the play
//! of the chosen world (0x28, which starts play as 0x26 does), then the
//! start of play as a join does. The phases follow as for a join.
//!
//! A slot id names a place in one of the save system's two storages, 0 the
//! local folder and 1 Steam's cloud, which the game's storage window picks
//! when no save was played yet (client 26430625, `0x140c6a050`). The main
//! menu takes a character or world command only for a slot that holds a
//! save and leaves its choice otherwise (`0x140e23800`, `0x140e23a50`), so
//! a host posts each place in storage 0, then in storage 1: the storage
//! that holds it is taken, storage 1 when both do.
//!
//! The host's own assumption, also static: a place no storage holds leaves
//! the menu's last choice, so play then starts with that one.
//!
//! Assumptions the in-game check settles (static reads of client
//! 26430625): the poster reaches the main menu's queue from the interface
//! frame, a joined target's bytes between its ports and its has-address
//! flag may stay zero, the back command cancels a join that loads, and an
//! app stage byte of 0 (no play state yet) is a menu, as the interface
//! frame takes it.

use std::net::{SocketAddr, SocketAddrV4, ToSocketAddrs};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use enshwick_base::Relock;
use enshwick_core::abi::{ClientCommandStatus as Status, Str};

use super::Progress;
use crate::base::join::{Layout, ServerInfo};
use crate::engine::kept_text::KeptText;

/// How long a posted join may take to start play before it fails.
const START_MS: u64 = 20_000;
/// How often the start of play or the back command is posted again while
/// the game has not taken it.
const REPOST_MS: u64 = 250;
/// How long a cancelled join keeps going back before the loader lets it be:
/// the start-play command posted just before may still start play.
const BACK_MS: u64 = 30_000;
/// How long a back command keeps going at least, so play a start command
/// started after it is caught too.
const BACK_SETTLE_MS: u64 = 1_000;
/// How long a leave may take to reach the menu.
const LEAVE_MS: u64 = 60_000;
/// How old the interface's stage read may be for a join or leave to start.
const FRESH_MS: u64 = 1_000;
/// How many bytes of joined servers' texts the process keeps for the
/// snapshot; a join past it runs without naming its server.
const SERVERS_BUDGET: usize = 64 * 1024;

/// Where the client stands, from the interface's app stage byte, as the
/// interface frame names its screens (`ui::interface::screen`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// No read fresh enough.
    Unread,
    /// A menu: the title, the main menu, character select, and any byte the
    /// interface frame does not name otherwise.
    Menu,
    /// Play started and loads.
    Loading,
    /// In a world.
    InGame,
}

impl Stage {
    /// The stage of the interface's app stage byte: 4 to 6 loading, 7 in
    /// game, every other a menu (client 26430625, read by the interface
    /// frame, which takes 0 for a menu too).
    pub(crate) const fn of(byte: u8) -> Self {
        match byte {
            4..=6 => Self::Loading,
            7 => Self::InGame,
            _ => Self::Menu,
        }
    }

    const fn playing(self) -> bool {
        matches!(self, Self::Loading | Self::InGame)
    }
}

/// A menu command the interface frame posts for a join or leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Post<'a> {
    /// The password for the join (0x25), the game hashing it at once.
    Password(&'a [u8]),
    /// The server's address (0x26): the join target's first bytes.
    Join(&'a [u8]),
    /// The start of play (0x29).
    StartPlay,
    /// Back (5): out of the start-play state, or out of play.
    Back,
    /// The character to play as (0x11), by its slot id.
    Character(u16),
    /// The world to host (0x16), by its slot id.
    World(u16),
    /// A hosted session's kind (0x27): offline, or open online.
    Session { online: bool },
    /// The play of the chosen world (0x28), which starts play as the
    /// join's address does.
    PlayWorld,
}

/// What a host plays: a world and a character by their places in the
/// main menu's lists, from 0, offline or open online.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Host {
    pub(crate) world: u16,
    pub(crate) character: u16,
    pub(crate) online: bool,
}

/// The save system's storages a host posts a place in, the one taken when
/// both hold it last.
const STORAGES: [u16; 2] = [0, 1];

/// The game's slot id of list place `at` in `storage`: the place plus 1 in
/// its low byte, 0 being no slot, and the storage in its high byte (client
/// 26430625, `0x140ea52c0`, `0x140e92870`, `0x140e95ce0`); the command's
/// bounds keep `at` below `ClientCommand::MAX_SLOTS`.
const fn slot(at: u16, storage: u16) -> u16 {
    storage << 8 | (at + 1)
}

/// The list place of the game's slot id `slot`, whatever its storage, as a
/// host posts it in [`slot`]; `None` for no slot.
pub(crate) const fn place(slot: u16) -> Option<u16> {
    (slot & 0xff).checked_sub(1)
}

/// A password held only until the interface frame posts it, wiped then.
struct Secret(Vec<u8>);

impl Drop for Secret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a live, aligned `u8` of this vector, written
            // in place so the wipe is not elided.
            unsafe { core::ptr::write_volatile(byte, 0) };
        }
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
    }
}

impl core::fmt::Debug for Secret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("<secret>")
    }
}

/// What the server's query port answered a join.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The target and what the server said of itself.
    Server(Box<[u8]>, ServerInfo),
    /// No answer with a Steam id and game port.
    Unreachable,
}

/// Where a join or leave is.
#[derive(Debug)]
enum Step {
    /// Waiting for the query port's answer.
    Asking { password: Secret },
    /// The answer is in: post the password and the address at the menu.
    Post {
        password: Option<Secret>,
        address: Box<[u8]>,
    },
    /// A host: its menu commands go one after another at the menu, `posted`
    /// of them taken so far.
    Hosting { host: Host, posted: u8 },
    /// Posted: the start of play goes again until play starts. `clear` once
    /// the game took a start of play with none of its popups showing, so
    /// only a popup that start made ends it.
    Starting {
        posted_ms: u64,
        last_ms: u64,
        clear: bool,
    },
    /// Play started: the phases follow the game's own pace.
    Playing,
    /// A leave: back goes again until the menu shows.
    Leaving { last_ms: Option<u64> },
    /// Cancelled after the join was posted: back goes again until the menu
    /// shows.
    Backing { since_ms: u64, last_ms: Option<u64> },
}

/// One join or leave and what it reports.
#[derive(Debug)]
struct Op {
    id: u64,
    step: Step,
    started_ms: u64,
    /// The server's answer, kept to tell why a join failed.
    info: Option<ServerInfo>,
    password_given: bool,
    /// The join check's refusals when the join was posted.
    refusals: u64,
    report: Report,
}

/// What a join or leave reports to its command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Report {
    pub(crate) progress: Progress,
    pub(crate) phase: u32,
    pub(crate) failure: u32,
}

/// What the frame sees for a join or leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Seen {
    stage: Stage,
    /// Whether the interface read the client's play data this frame: the
    /// client game exists.
    game: bool,
    /// The join check's refusals so far.
    refusals: u64,
    /// Whether the game's own popup shows: at the menu, its error that it
    /// could not start the play a host or join asked for.
    popup: bool,
}

/// The one join or leave and the server it joined.
#[derive(Debug)]
struct Play {
    op: Option<Op>,
    /// The joined server's `host:query_port`, kept for the process so the
    /// snapshot can name it.
    server: Option<&'static str>,
    /// Every server text kept, so a join of the same server twice keeps one.
    servers: KeptText,
    /// The answer for the op of the id, from its query thread.
    answer: Option<(u64, Answer)>,
}

impl Play {
    /// Names `server` as the joined one, or none.
    fn name(&mut self, server: Option<&'static str>) {
        self.server = server;
        STATE.named.store(server.is_some(), Ordering::Release);
    }
}

/// The joins' and leaves' state, one per process.
struct State {
    play: Mutex<Play>,
    /// Whether an op runs: the frame's only check while none does.
    running: AtomicBool,
    /// Whether a server is named: the idle frame's check before it locks.
    named: AtomicBool,
    /// The last stage byte the interface read and when.
    stage: AtomicU64,
}

static STATE: State = State {
    play: Mutex::new(Play {
        op: None,
        server: None,
        servers: KeptText::new(SERVERS_BUDGET),
        answer: None,
    }),
    running: AtomicBool::new(false),
    named: AtomicBool::new(false),
    stage: AtomicU64::new(0),
};

/// Why a join or leave does not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartError {
    /// The target is no `host:port`, or the password too long.
    BadArgs,
    /// The client is not where it starts, or another join or leave runs.
    NotNow,
}

/// Notes the stage byte the interface read at `now`; one swap.
/// `Some(playing)` when play started or ended since the last read.
pub(crate) fn note_stage(byte: u8, now: u64) -> Option<bool> {
    let was = STATE
        .stage
        .swap(now << 8 | u64::from(byte), Ordering::AcqRel);
    let playing = Stage::of(byte).playing();
    (Stage::of((was & 0xff) as u8).playing() != playing).then_some(playing)
}

/// The stage the interface last read, if at most [`FRESH_MS`] old at `now`.
fn stage(now: u64) -> Stage {
    let word = STATE.stage.load(Ordering::Acquire);
    if now.saturating_sub(word >> 8) > FRESH_MS {
        return Stage::Unread;
    }
    Stage::of((word & 0xff) as u8)
}

/// Starts join `id` of `target` with `password` at `now`, its query on a
/// thread of its own; the password is kept only until it is posted.
///
/// # Errors
///
/// [`StartError::BadArgs`] for a target that is no `host:port` or a password
/// past the game's room, [`StartError::NotNow`] off the main menu and while
/// another join or leave runs.
pub(crate) fn join(id: u64, target: &str, password: &[u8], now: u64) -> Result<(), StartError> {
    let (host, port) =
        enshwick_core::__private::rules::join_target(target).ok_or(StartError::BadArgs)?;
    if password.len() > enshwick_core::abi::ClientCommand::MAX_PASSWORD {
        return Err(StartError::BadArgs);
    }
    let layout = crate::base::join::layout().ok_or(StartError::NotNow)?;
    if stage(now) != Stage::Menu {
        return Err(StartError::NotNow);
    }
    let mut play = STATE.play.relock();
    if play.op.is_some() {
        return Err(StartError::NotNow);
    }
    let server = play.servers.keep(target);
    play.name(server);
    play.answer = None;
    play.op = Some(Op {
        id,
        step: Step::Asking {
            password: Secret(password.to_vec()),
        },
        started_ms: now,
        info: None,
        password_given: !password.is_empty(),
        refusals: 0,
        report: report(Progress::Waiting, Status::PHASE_QUEUED, 0),
    });
    STATE.running.store(true, Ordering::Release);
    drop(play);
    let host = host.to_owned();
    let asked = crate::engine::off_thread::start("join query", move || {
        let answer = ask(&layout, &host, port);
        let mut play = STATE.play.relock();
        if play.op.as_ref().is_some_and(|op| op.id == id) {
            play.answer = Some((id, answer));
        }
    });
    if !asked {
        STATE.play.relock().answer = Some((id, Answer::Unreachable));
    }
    Ok(())
}

/// Starts host `id` of `host` at `now`.
///
/// # Errors
///
/// [`StartError::NotNow`] off the main menu and while another join, host
/// or leave runs.
pub(crate) fn host(id: u64, host: Host, now: u64) -> Result<(), StartError> {
    if stage(now) != Stage::Menu {
        return Err(StartError::NotNow);
    }
    let mut play = STATE.play.relock();
    if play.op.is_some() {
        return Err(StartError::NotNow);
    }
    play.name(None);
    play.answer = None;
    play.op = Some(Op {
        id,
        step: Step::Hosting { host, posted: 0 },
        started_ms: now,
        info: None,
        password_given: false,
        refusals: crate::base::join::refusals(),
        report: report(Progress::Waiting, Status::PHASE_MENU, 0),
    });
    STATE.running.store(true, Ordering::Release);
    Ok(())
}

/// The menu commands a host posts, in order.
const fn host_posts(host: Host) -> [Post<'static>; 6] {
    let [local, cloud] = STORAGES;
    [
        Post::Character(slot(host.character, local)),
        Post::Character(slot(host.character, cloud)),
        Post::World(slot(host.world, local)),
        Post::World(slot(host.world, cloud)),
        Post::Session {
            online: host.online,
        },
        Post::PlayWorld,
    ]
}

/// Asks `host`'s query port `port` for the join's target: its first IPv4,
/// Steam id and game port.
fn ask(layout: &Layout, host: &str, port: u16) -> Answer {
    let Some(query) = (host, port).to_socket_addrs().ok().and_then(|mut all| {
        all.find_map(|a| match a {
            SocketAddr::V4(v4) => Some(v4),
            SocketAddr::V6(_) => None,
        })
    }) else {
        crate::log!(
            INFO,
            "joining {host}:{port}: the name does not resolve to an IPv4 address"
        );
        return Answer::Unreachable;
    };
    let info = match crate::base::join::query_info(SocketAddr::V4(query)) {
        Ok(info) => info,
        Err(error) => {
            crate::log!(
                INFO,
                "joining {query}: no answer at its query port ({error})"
            );
            return Answer::Unreachable;
        }
    };
    let (Some(host_id), Some(game)) = (info.steam_id, info.game_port) else {
        crate::log!(
            INFO,
            "joining {query}: its answer names no Steam id or game port"
        );
        return Answer::Unreachable;
    };
    let target = layout.server(host_id, SocketAddrV4::new(*query.ip(), game), port);
    Answer::Server(target[..layout.address_len()].into(), info)
}

/// Whether a leave may start at `now`, so a mod's queue refuses one that
/// could not.
///
/// # Errors
///
/// [`StartError::NotNow`] outside play and while a join or leave runs.
pub(crate) fn can_leave(now: u64) -> Result<(), StartError> {
    if stage(now).playing() && !STATE.running.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err(StartError::NotNow)
    }
}

/// Starts leave `id` at `now`.
///
/// # Errors
///
/// [`StartError::NotNow`] outside play and while another join or leave
/// runs.
pub(crate) fn leave(id: u64, now: u64) -> Result<(), StartError> {
    can_leave(now)?;
    let mut play = STATE.play.relock();
    if play.op.is_some() {
        return Err(StartError::NotNow);
    }
    play.op = Some(Op {
        id,
        step: Step::Leaving { last_ms: None },
        started_ms: now,
        info: None,
        password_given: false,
        refusals: 0,
        report: report(Progress::Waiting, Status::PHASE_IN_WORLD, 0),
    });
    STATE.running.store(true, Ordering::Release);
    Ok(())
}

/// What op `id` reports; `None` once it is gone.
pub(crate) fn report_of(id: u64) -> Option<Report> {
    let play = STATE.play.relock();
    play.op
        .as_ref()
        .filter(|op| op.id == id)
        .map(|op| op.report)
}

/// Lets op `id` go at `now`; `back` when its mod cancelled it, so a join
/// the game took goes back to the menu.
pub(crate) fn release(id: u64, now: u64, back: bool) {
    let mut play = STATE.play.relock();
    let Some(op) = play.op.as_mut().filter(|op| op.id == id) else {
        return;
    };
    if !let_go(op, now, back) {
        play.op = None;
        STATE.running.store(false, Ordering::Release);
    }
}

/// Lets `op` go at `now`; whether it stays, going back to the menu. A join
/// the game took goes back only when `back`, as its mod cancelled it: a mod
/// or feature turning off leaves the player where the game takes them.
fn let_go(op: &mut Op, now: u64, back: bool) -> bool {
    let ended = !matches!(op.report.progress, Progress::Waiting | Progress::Taken);
    match op.step {
        Step::Starting { .. } | Step::Playing if back && !ended => {
            op.step = Step::Backing {
                since_ms: now,
                last_ms: None,
            };
            true
        }
        Step::Backing { .. } => true,
        _ => false,
    }
}

/// Whether the build has what a join and a leave post, with room for the
/// build's join address in the join command.
pub(crate) fn runs() -> bool {
    let room = crate::actions::client_control::GROUPS
        .menu
        .get()
        .map(crate::actions::client_control::MenuPosts::join_room);
    fits(room, crate::base::join::layout().map(|l| l.address_len()))
}

/// Whether an address of `address_len` bytes fits the join command's
/// `room`; false without either.
fn fits(room: Option<usize>, address_len: Option<usize>) -> bool {
    room.zip(address_len).is_some_and(|(room, len)| len <= room)
}

/// Whether the build has what a host posts: its own commands and the
/// join's start of play and back.
pub(crate) fn hosts_run() -> bool {
    let groups = &crate::actions::client_control::GROUPS;
    groups.host.get().is_some() && groups.menu.get().is_some()
}

/// The server the client joined through a join, while it is on its way or
/// in play; `None` in the menu.
pub(crate) fn server() -> Option<&'static str> {
    STATE.play.relock().server
}

/// Runs the join or leave in the interface frame at `now`, the client at
/// `stage` with the game's own popup showing or not: `game` answers whether
/// the frame reads the client's play data, and `post` hands a command to the
/// game and answers whether its queue took it. Two atomic loads while none
/// runs and no server is named; the joined server is forgotten once the
/// menu shows.
pub(crate) fn frame(
    stage: Stage,
    popup: bool,
    now: u64,
    game: impl FnOnce() -> bool,
    mut post: impl FnMut(Post<'_>) -> bool,
) {
    if !STATE.running.load(Ordering::Acquire) {
        if stage == Stage::Menu && STATE.named.load(Ordering::Acquire) {
            let mut play = STATE.play.relock();
            // A join started since the load names its server: keep it.
            if play.op.is_none() {
                play.name(None);
            }
        }
        return;
    }
    let seen = Seen {
        stage,
        game: game(),
        refusals: crate::base::join::refusals(),
        popup,
    };
    let mut play = STATE.play.relock();
    let answer = play.answer.take();
    let Some(op) = play.op.as_mut() else {
        STATE.running.store(false, Ordering::Release);
        return;
    };
    if let Some((id, answer)) = answer
        && id == op.id
    {
        take_answer(op, answer, seen.refusals);
    }
    let done = step(op, seen, now, &mut post);
    if done {
        let back = matches!(op.step, Step::Backing { .. });
        if back || forgets_server(op, seen.stage) {
            play.name(None);
        }
        // A finished op stays for its command's last read; one going back
        // has no command left.
        if back {
            play.op = None;
            STATE.running.store(false, Ordering::Release);
        }
    }
}

/// Whether `op`, ended with the client at `stage`, leaves no joined server:
/// it failed or the menu shows. A leave that timed out in a world keeps it.
fn forgets_server(op: &Op, stage: Stage) -> bool {
    stage == Stage::Menu || matches!(op.report.progress, Progress::Failed(_))
}

/// Takes the query port's `answer` into `op`, while it asks.
fn take_answer(op: &mut Op, answer: Answer, refusals: u64) {
    let Step::Asking { password } = &mut op.step else {
        return;
    };
    let password = core::mem::replace(password, Secret(Vec::new()));
    match answer {
        Answer::Server(address, info) => {
            op.info = Some(info);
            op.refusals = refusals;
            op.step = Step::Post {
                password: Some(password),
                address,
            };
        }
        Answer::Unreachable => {
            drop(password);
            op.report = report(
                Progress::Failed(Status::UNREACHABLE),
                Status::PHASE_QUEUED,
                Status::UNREACHABLE,
            );
        }
    }
}

const fn report(progress: Progress, phase: u32, failure: u32) -> Report {
    Report {
        progress,
        phase,
        failure,
    }
}

/// Moves `op` on with what the frame `seen` at `now`, posting through
/// `post`; whether it ended, its report final.
fn step(op: &mut Op, seen: Seen, now: u64, post: &mut impl FnMut(Post<'_>) -> bool) -> bool {
    if matches!(
        op.report.progress,
        Progress::Failed(_) | Progress::Done | Progress::Cancelled(_)
    ) {
        return !matches!(op.step, Step::Backing { .. }) || back(op, seen, now, post);
    }
    match &mut op.step {
        Step::Asking { .. } => false,
        Step::Post { password, address } => {
            // At the menu too: a queue that never takes the posts ends it.
            if now.saturating_sub(op.started_ms) > START_MS {
                *password = None;
                op.report = report(
                    Progress::Failed(Status::JOIN_TIMED_OUT),
                    Status::PHASE_QUEUED,
                    Status::JOIN_TIMED_OUT,
                );
                return true;
            }
            if seen.stage != Stage::Menu {
                return false;
            }
            if let Some(secret) = password {
                if !post(Post::Password(&secret.0)) {
                    return false;
                }
                // Dropped here: the game kept only its hash.
                *password = None;
            }
            if !post(Post::Join(address)) {
                return false;
            }
            op.step = Step::Starting {
                posted_ms: now,
                last_ms: 0,
                clear: false,
            };
            op.report = report(Progress::Taken, Status::PHASE_MENU, 0);
            false
        }
        Step::Hosting { .. } => hosting(op, seen, now, post),
        Step::Starting { .. } => {
            if seen.stage.playing() {
                op.step = Step::Playing;
                return step(op, seen, now, post);
            }
            starting(op, seen, now, post)
        }
        Step::Playing => match seen.stage {
            Stage::InGame if seen.game => {
                op.report = report(Progress::Done, Status::PHASE_IN_WORLD, 0);
                true
            }
            Stage::Loading | Stage::InGame => {
                let phase = if seen.game {
                    Status::PHASE_LOADING
                } else {
                    Status::PHASE_CONNECTING
                };
                op.report = report(Progress::Taken, phase, 0);
                false
            }
            Stage::Menu => {
                let failure = failure(op, seen.refusals);
                op.report = report(Progress::Failed(failure), Status::PHASE_MENU, failure);
                true
            }
            Stage::Unread => false,
        },
        Step::Leaving { last_ms } => {
            if seen.stage == Stage::Menu {
                op.report = report(Progress::Done, Status::PHASE_LEFT, 0);
                return true;
            }
            if now.saturating_sub(op.started_ms) > LEAVE_MS {
                op.report = report(Progress::Cancelled(Status::TIMED_OUT), op.report.phase, 0);
                return true;
            }
            if seen.stage.playing()
                && last_ms.is_none_or(|last| now.saturating_sub(last) >= REPOST_MS)
                && post(Post::Back)
            {
                *last_ms = Some(now);
                op.report = report(Progress::Taken, Status::PHASE_IN_WORLD, 0);
            }
            false
        }
        Step::Backing { .. } => back(op, seen, now, post),
    }
}

/// Posts a host's menu commands for `op` at the menu, each once its queue
/// takes the one before, then waits for play as a join; whether it ended,
/// timed out.
fn hosting(op: &mut Op, seen: Seen, now: u64, post: &mut impl FnMut(Post<'_>) -> bool) -> bool {
    let Step::Hosting { host, posted } = &mut op.step else {
        return false;
    };
    if now.saturating_sub(op.started_ms) > START_MS {
        op.report = report(
            Progress::Failed(Status::JOIN_TIMED_OUT),
            Status::PHASE_MENU,
            Status::JOIN_TIMED_OUT,
        );
        return true;
    }
    if seen.stage != Stage::Menu {
        return false;
    }
    let posts = host_posts(*host);
    while let Some(&next) = posts.get(usize::from(*posted)) {
        if !post(next) {
            return false;
        }
        *posted += 1;
    }
    // The host's 20 seconds run from its queue, its posts' time included.
    op.step = Step::Starting {
        posted_ms: op.started_ms,
        last_ms: 0,
        clear: false,
    };
    op.report = report(Progress::Taken, Status::PHASE_MENU, 0);
    false
}

/// Posts the start of play for `op` at the menu until play starts; whether
/// it ended: refused by the join check, or failed by the game's popup or
/// timed out and going back.
///
/// A play the game cannot create (a world place no storage holds, a host
/// setting the save refuses) passes the loading stage between two frames
/// and comes back to the menu with the game's error popup (client 26430625:
/// `Play::Failed` from the creation step `Session`, `0x140854250`), so a
/// popup at the menu after the game took a start of play is the failure.
fn starting(op: &mut Op, seen: Seen, now: u64, post: &mut impl FnMut(Post<'_>) -> bool) -> bool {
    let Step::Starting {
        posted_ms,
        last_ms,
        clear,
    } = op.step
    else {
        return false;
    };
    if seen.refusals > op.refusals {
        op.report = report(
            Progress::Failed(Status::REFUSED),
            Status::PHASE_MENU,
            Status::REFUSED,
        );
        return true;
    }
    let failed = if seen.stage == Stage::Menu && seen.popup && clear {
        Some(failure(op, seen.refusals))
    } else if now.saturating_sub(posted_ms) > START_MS {
        Some(Status::JOIN_TIMED_OUT)
    } else {
        None
    };
    if let Some(failure) = failed {
        op.report = report(Progress::Failed(failure), Status::PHASE_MENU, failure);
        // A start of play posted just before may still be taken: back goes
        // on as for a cancel.
        op.step = Step::Backing {
            since_ms: now,
            last_ms: None,
        };
        return back(op, seen, now, post);
    }
    if seen.stage == Stage::Menu
        && !seen.popup
        && now.saturating_sub(last_ms) >= REPOST_MS
        && post(Post::StartPlay)
    {
        op.step = Step::Starting {
            posted_ms,
            last_ms: now,
            clear: true,
        };
    }
    false
}

/// Posts back for a cancelled join until the menu shows; whether it is done.
fn back(op: &mut Op, seen: Seen, now: u64, post: &mut impl FnMut(Post<'_>) -> bool) -> bool {
    let Step::Backing { since_ms, last_ms } = &mut op.step else {
        return true;
    };
    let waited = now.saturating_sub(*since_ms);
    if waited > BACK_MS || (seen.stage == Stage::Menu && waited >= BACK_SETTLE_MS) {
        return true;
    }
    if seen.stage != Stage::Unread
        && last_ms.is_none_or(|last| now.saturating_sub(last) >= REPOST_MS)
        && post(Post::Back)
    {
        *last_ms = Some(now);
    }
    false
}

/// Why `op`'s join failed, as far as the client tells: the loader's join
/// check refused it, or the server's answer showed it full or asking for a
/// password the mod did not give.
fn failure(op: &Op, refusals: u64) -> u32 {
    let info = op.info.clone().unwrap_or_default();
    if refusals > op.refusals {
        Status::REFUSED
    } else if info.full() {
        Status::FULL
    } else if info.password && !op.password_given {
        Status::WRONG_PASSWORD
    } else {
        Status::UNKNOWN
    }
}

/// The server's text for the snapshot, empty for none.
pub(crate) fn server_str() -> Str {
    server().map_or(Str::EMPTY, Str::new)
}

#[cfg(test)]
mod tests;
