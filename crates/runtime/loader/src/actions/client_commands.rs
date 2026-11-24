//! Commands mods give the client's own interface and input: open or close
//! a screen, choose the in-game menu's tab, turn the camera, hold a short
//! move, press a game action, highlight a recipe and act on an item of a
//! carried window, each with an id and an outcome the client's own state
//! confirms.
//!
//! A mod queues from any thread; the interface frame runs the commands,
//! never inside a mod's callback, and hands a look, a move, a press, a
//! recipe to highlight or a window's item to the code that acts on it (see
//! [`Handoff`]). Until a mod queues, each runs nothing but one atomic load.

mod handoff;
mod play;
mod spec;
pub(crate) mod windows;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_base::Relock;
use enshwick_core::RecipeId;
use enshwick_core::abi::Side;
use enshwick_core::abi::{
    ClientCommand as Spec, ClientCommandStatus as Status, ClientSnapshot as Snapshot,
};

pub(crate) use self::handoff::Handoff;
pub(crate) use self::play::{Host, Post, server_str};
#[cfg(test)]
use self::spec::inside_frame;
use self::spec::{expected_screen, kind, text, window_ask};
use crate::actions::client_control::{Mark, MenuTab};
use crate::actions::client_input::{self, Ask};
use crate::actions::client_recipes;
use crate::actions::client_snapshot::{LoaderScreen, RecipeRead, TabRead};
use crate::actions::local_held;
use enshwick_core::__private::rules;

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

/// The feature the commands' fuse names.
const FEATURE: &str = "client commands";
/// Off at the commands' first panic: queueing then answers unavailable.
static FUSE: Fuse = Fuse::new(FEATURE);

/// How long a command may wait for its effect.
const TIMEOUT_MS: u64 = 2000;
/// How many commands' outcomes the loader keeps for their status.
const KEPT: usize = 64;

/// What a queued command does, its arguments checked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Kind {
    /// Opens the screen byte.
    Open(u8),
    /// Closes the open screen, to the world's screen byte.
    Close(u8),
    /// Shows the tab.
    Tab(MenuTab),
    /// Turns the camera by yaw and pitch radians once.
    Look([f32; 2]),
    /// Holds the direction for the milliseconds.
    Move([f32; 2], u32),
    /// Highlights the recipe in the open crafting menu.
    Highlight(RecipeId),
    /// Joins a server from the main menu; its target and password live in
    /// [`play`] from the queue on.
    Join,
    /// Leaves play back to the main menu.
    Leave,
    /// Holds the game action of the digital input's bit for the
    /// milliseconds; `true` for a form of the interact key, whose screen of
    /// what the player faces is the press's own effect.
    Press(u8, u32, bool),
    /// Sets, moves or clears the local player's waypoint, or pings a
    /// position, through its input's waypoint request.
    Mark(Mark),
    /// Hosts a world from the main menu; [`play`] runs it from the queue
    /// on, as a join.
    Host(Host),
    /// Shows a page of the loader's own Mods screen.
    Mods(ModsAsk),
    /// Acts on an item of a carried window; its ask lives in [`windows`]
    /// from the queue on.
    Window(windows::Act),
}

impl Kind {
    const fn raw(self) -> u32 {
        match self {
            Self::Open(_) => Spec::OPEN_SCREEN,
            Self::Close(_) => Spec::CLOSE_SCREEN,
            Self::Tab(_) => Spec::CHOOSE_TAB,
            Self::Look(_) => Spec::LOOK,
            Self::Move(..) => Spec::MOVE,
            Self::Highlight(_) => Spec::HIGHLIGHT_RECIPE,
            Self::Join => Spec::JOIN,
            Self::Host(_) => Spec::HOST,
            Self::Mods(ModsAsk {
                mod_index: None, ..
            }) => Spec::OPEN_MODS,
            Self::Mods(ModsAsk { gear: false, .. }) => Spec::OPEN_MOD,
            Self::Mods(ModsAsk { gear: true, .. }) => Spec::OPEN_MOD_SETTINGS,
            Self::Leave => Spec::LEAVE,
            Self::Press(..) => Spec::PRESS,
            Self::Mark(Mark::Set(_)) => Spec::SET_WAYPOINT,
            Self::Mark(Mark::Clear) => Spec::CLEAR_WAYPOINT,
            Self::Mark(Mark::Ping(_)) => Spec::PING,
            Self::Window(windows::Act::Press) => Spec::WINDOW_PRESS,
            Self::Window(windows::Act::Pick) => Spec::WINDOW_PICK,
            Self::Window(windows::Act::Text) => Spec::WINDOW_TEXT,
        }
    }

    /// Whether a command of this kind waits beside one of `other` from the
    /// same mod: one of each kind, one press of each action, and one
    /// waypoint or ping.
    fn waits_beside(self, other: Self) -> bool {
        match (self, other) {
            (Self::Press(bit, ..), Self::Press(other, ..)) => bit != other,
            (Self::Mark(_), Self::Mark(_)) => false,
            _ => self.raw() != other.raw(),
        }
    }

    /// The longest hold of its kind, which its timeout waits past once its
    /// detour took it, so a move or a press holds its whole hold.
    const fn longest_hold(self) -> u64 {
        match self {
            Self::Move(..) => Spec::MAX_HOLD_MS as u64,
            Self::Press(..) => Spec::MAX_PRESS_MS as u64,
            _ => 0,
        }
    }

    /// Whether it drives the player's input, which only the world's own
    /// screen takes, from the queue to its end.
    const fn drives(self) -> bool {
        matches!(self, Self::Look(_) | Self::Move(..) | Self::Press(..))
    }

    /// The screen change the interface frame applies for it; `None` for a
    /// kind a detour of the game's own code runs through a [`Handoff`].
    const fn screen_change(self) -> Option<ScreenChange> {
        match self {
            Self::Open(screen) => Some(ScreenChange::Open(screen)),
            Self::Close(screen) => Some(ScreenChange::Close(screen)),
            Self::Tab(tab) => Some(ScreenChange::Tab(tab)),
            Self::Mods(ask) => Some(ScreenChange::Mods(ask)),
            _ => None,
        }
    }

    /// Whether a detour of the game's own code runs it, through a
    /// [`Handoff`], rather than the interface frame.
    const fn handed(self) -> bool {
        self.screen_change().is_none()
    }

    /// Whether it joins or leaves play, at the game's own pace once handed:
    /// no session, screen or two-second check ends it then.
    const fn plays(self) -> bool {
        matches!(self, Self::Join | Self::Leave | Self::Host(_))
    }

    /// Its phase when queued: a join waits for the server's answer, a leave
    /// starts in play; 0 for kinds without phases.
    const fn queued_phase(self) -> u32 {
        match self {
            Self::Join => Status::PHASE_QUEUED,
            Self::Host(_) => Status::PHASE_MENU,
            Self::Leave => Status::PHASE_IN_WORLD,
            _ => 0,
        }
    }
}

/// What the interface frame applies for a command, the only kinds it runs
/// itself; every other kind runs in a detour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScreenChange {
    /// Opens the screen byte.
    Open(u8),
    /// Closes the open screen, to the world's screen byte.
    Close(u8),
    /// Shows the tab.
    Tab(MenuTab),
    /// Shows a page of the loader's own Mods screen.
    Mods(ModsAsk),
}

/// A page of the loader's Mods screen a command asks for: the list, or a
/// mod's page by its load-order index, through its gear (`gear`) or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ModsAsk {
    pub(crate) mod_index: Option<usize>,
    pub(crate) gear: bool,
}

impl ModsAsk {
    /// Whether the screen `shown` is what this asks for: a gear may open
    /// the joined server's settings of the mod instead of its page.
    fn shows(self, shown: LoaderScreen) -> bool {
        let Some(index) = self.mod_index else {
            return shown.page == Snapshot::MOD_LIST;
        };
        let page = shown.page == Snapshot::MOD_PAGE
            || (self.gear && shown.page == Snapshot::MOD_SERVER_SETTINGS);
        page && shown.mod_index == Some(index)
    }
}

/// Where a detour stands with a command handed to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Progress {
    /// Handed; the game has not taken it yet.
    Waiting,
    /// The game took it: a move holds, the browser focused the recipe.
    Taken,
    /// The look was written whole, or the move's hold ran out.
    Done,
    /// The detour ended it for this reason.
    Cancelled(u32),
    /// The game's own state lacks its target: the open menu does not list
    /// the recipe.
    Unavailable,
    /// The game ended a join without reaching the world, for this failure.
    Failed(u32),
}

/// One command and where it stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Command {
    id: u64,
    owner: ModIndex,
    pub(crate) kind: Kind,
    session: u64,
    /// The screen byte the client must show, `None` for any.
    expected: Option<u8>,
    /// The held controls when it was queued, or at the first fresh read
    /// after it: a new one is the player's. `None` until one.
    held: Option<u64>,
    queued_ms: u64,
    state: u32,
    reason: u32,
    /// When its detour took it, as [`local_held::now_ms`] counts; `None`
    /// while none holds it. A held one is released when it ends.
    handed: Option<u64>,
    /// A join or leave's last phase, 0 for other kinds.
    phase: u32,
    /// Why a join failed, 0 otherwise.
    failure: u32,
}

impl Command {
    const fn open(&self) -> bool {
        self.state == Status::PENDING || self.state == Status::APPLIED
    }

    /// Where its detour stands with it; `None` before it is handed.
    fn progress(&self) -> Option<Progress> {
        self.handed?;
        let progress = match self.kind {
            Kind::Look(_) | Kind::Move(..) => client_input::progress(self.id),
            Kind::Press(..) => client_input::press_progress(self.id),
            Kind::Highlight(_) => client_recipes::progress(self.id),
            // A handed join or leave runs through `plays_next` instead.
            Kind::Mark(_) => client_input::mark_progress(self.id),
            Kind::Window(_) => windows::progress(self.id),
            Kind::Join
            | Kind::Leave
            | Kind::Host(_)
            | Kind::Open(_)
            | Kind::Close(_)
            | Kind::Tab(_)
            | Kind::Mods(_) => None,
        };
        Some(progress.unwrap_or(Progress::Waiting))
    }

    /// Hands it to its detour; whether the detour took it, false while
    /// another command of its detour runs.
    fn hand(&self) -> bool {
        match self.kind {
            Kind::Look(look) => client_input::hand(self.id, self.owner, Ask::Look(look)),
            Kind::Move(direction, hold) => {
                client_input::hand(self.id, self.owner, Ask::Move(direction, hold))
            }
            Kind::Press(bit, hold, _) => client_input::press(self.id, self.owner, bit, hold),
            Kind::Highlight(recipe) => client_recipes::hand(self.id, recipe),
            Kind::Leave => play::leave(self.id, local_held::now_ms()).is_ok(),
            Kind::Mark(mark) => client_input::mark(self.id, self.owner, mark),
            Kind::Window(_) => windows::hand(self.id),
            // A join and a host are handed when queued, a join with its
            // target and password.
            Kind::Join
            | Kind::Host(_)
            | Kind::Open(_)
            | Kind::Close(_)
            | Kind::Tab(_)
            | Kind::Mods(_) => false,
        }
    }

    /// Takes it back from its detour once it ended; `by_mod` when its mod
    /// cancelled it, so a join the game took goes back to the menu.
    fn release(&self, by_mod: bool) {
        if let Kind::Window(_) = self.kind {
            // Its ask was parked at the queue, handed or not.
            windows::release(self.id);
            return;
        }
        if self.handed.is_none() {
            return;
        }
        match self.kind {
            Kind::Look(_) | Kind::Move(..) => client_input::release(self.id),
            Kind::Press(..) => client_input::unpress(self.id),
            Kind::Highlight(_) => client_recipes::release(self.id),
            Kind::Join | Kind::Leave | Kind::Host(_) => {
                play::release(self.id, local_held::now_ms(), by_mod);
            }
            Kind::Mark(_) => client_input::unmark(self.id),
            Kind::Open(_) | Kind::Close(_) | Kind::Tab(_) | Kind::Mods(_) | Kind::Window(_) => {}
        }
    }
}

/// What one frame shows of the client, for the commands to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) session: u64,
    /// The HUD screen byte; `None` outside a game, as at the main menu.
    pub(crate) screen: Option<u8>,
    pub(crate) tab: TabRead,
    /// The crafting menu's highlighted recipe.
    pub(crate) recipe: RecipeRead,
    /// The player's own held controls of a fresh read, without the bits
    /// presses set; `None` without one.
    pub(crate) held: Option<u64>,
    /// The open screen's host entity, 0 for none.
    pub(crate) host: u32,
    /// What the loader's own Mods screen shows.
    pub(crate) loader: LoaderScreen,
}

impl Seen {
    /// What a frame outside a game shows in `session`: none of the game's
    /// screens, only the loader's own Mods screen, `loader`, which the main
    /// menu opens too.
    pub(crate) const fn menu(session: u64, loader: LoaderScreen) -> Self {
        Self {
            session,
            screen: None,
            tab: TabRead::NoMenu,
            recipe: RecipeRead::Unread,
            held: None,
            host: 0,
            loader,
        }
    }
}

/// What the game answered a command it was handed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Applied {
    /// The game took it.
    Taken,
    /// Not now, such as a full queue: the frame tries again.
    Busy,
    /// The build lacks what it needs.
    Unavailable,
}

/// What a frame does with a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Next {
    Wait,
    /// Hands it to the game.
    Apply,
    /// The game took it, as its detour reports.
    Applied,
    End(u32, u32),
}

/// What the frame does with `command` now, given what it `seen` at `now`
/// and where its detour stands, `progress`, once it is handed; `commanded`
/// while a command opens or closes a screen, so a screen a held press finds
/// is not its own.
fn next(
    command: &Command,
    seen: &Seen,
    progress: Option<Progress>,
    now: u64,
    commanded: bool,
) -> Next {
    let cancel = |reason| Next::End(Status::CANCELLED, reason);
    if let Some(end) = progress.and_then(ended) {
        return end;
    }
    if seen.session != command.session {
        return cancel(Status::SESSION_CHANGED);
    }
    // Its effect showing already, applied or not: an open of the screen
    // shown is not toggled shut.
    let shows = match command.kind {
        Kind::Open(screen) | Kind::Close(screen) => seen.screen == Some(screen),
        Kind::Tab(tab) => seen.tab == TabRead::Shown(tab),
        Kind::Mods(ask) => ask.shows(seen.loader),
        Kind::Highlight(recipe) => seen.recipe == RecipeRead::Shown(Some(recipe)),
        Kind::Look(_)
        | Kind::Move(..)
        | Kind::Press(..)
        | Kind::Join
        | Kind::Leave
        | Kind::Host(_)
        | Kind::Mark(_)
        | Kind::Window(_) => false,
    };
    if shows {
        return Next::End(Status::OBSERVED, 0);
    }
    let drives = command.kind.drives();
    let elsewhere = command
        .expected
        .is_some_and(|screen| seen.screen != Some(screen));
    let pressed = match (command.held, seen.held) {
        (Some(queued), Some(now)) => now & !queued != 0,
        _ => false,
    };
    let applied = command.state == Status::APPLIED || progress == Some(Progress::Taken);
    if applied {
        if drives && pressed {
            return cancel(Status::PLAYER_INPUT);
        }
        if drives && elsewhere {
            // A held interact key opens the screen of what it faces, as a
            // chest's, which has a host: that screen is the press's effect.
            // The map or the pause menu, which keys outside the input open,
            // has none.
            let own =
                matches!(command.kind, Kind::Press(_, _, true)) && seen.host != 0 && !commanded;
            return if own {
                Next::End(Status::OBSERVED, 0)
            } else {
                cancel(Status::OTHER_SCREEN)
            };
        }
        if command.state == Status::PENDING {
            return Next::Applied;
        }
    } else {
        if elsewhere {
            return cancel(Status::OTHER_SCREEN);
        }
        // A waypoint or ping drives none of the player's input to give way.
        if pressed && !matches!(command.kind, Kind::Mark(_)) {
            return cancel(Status::PLAYER_INPUT);
        }
        let ready = match command.kind {
            _ if progress.is_some() => false,
            Kind::Tab(_) => !matches!(seen.tab, TabRead::NoMenu),
            Kind::Highlight(_) => matches!(seen.recipe, RecipeRead::Shown(_)),
            Kind::Open(_)
            | Kind::Close(_)
            | Kind::Mods(_)
            | Kind::Look(_)
            | Kind::Move(..)
            | Kind::Press(..)
            | Kind::Join
            | Kind::Leave
            | Kind::Host(_)
            | Kind::Mark(_)
            | Kind::Window(_) => true,
        };
        // One that waits for its detour, such as for a press's slot, times
        // out as one that waits for its effect.
        if ready && !timed_out(command, now) {
            return Next::Apply;
        }
    }
    if timed_out(command, now) {
        return cancel(Status::TIMED_OUT);
    }
    Next::Wait
}

/// What the frame does with a join or leave the play module holds, its
/// phase and failure taken into `command`: the game's own pace decides,
/// never the two-second limit.
fn plays_next(command: &mut Command) -> Next {
    let Some(report) = play::report_of(command.id) else {
        return Next::End(Status::UNAVAILABLE, 0);
    };
    command.phase = report.phase;
    command.failure = report.failure;
    if let Some(end) = ended(report.progress) {
        return end;
    }
    if report.progress == Progress::Taken && command.state == Status::PENDING {
        Next::Applied
    } else {
        Next::Wait
    }
}

/// How a command ends once its detour reports `progress`; `None` while it
/// runs.
const fn ended(progress: Progress) -> Option<Next> {
    match progress {
        Progress::Done => Some(Next::End(Status::OBSERVED, 0)),
        Progress::Cancelled(reason) => Some(Next::End(Status::CANCELLED, reason)),
        Progress::Unavailable => Some(Next::End(Status::UNAVAILABLE, 0)),
        Progress::Failed(_) => Some(Next::End(Status::FAILED, 0)),
        Progress::Waiting | Progress::Taken => None,
    }
}

/// Whether `command` waited past its timeout at `now`: from its queue while
/// no detour holds it, else from when one took it, its hold beside.
fn timed_out(command: &Command, now: u64) -> bool {
    let (since, limit) = command
        .handed
        .map_or((command.queued_ms, TIMEOUT_MS), |at| {
            (at, TIMEOUT_MS + command.kind.longest_hold())
        });
    now.saturating_sub(since) > limit
}

/// The commands: the ones still open and the last [`KEPT`] that ended.
struct Commands {
    next_id: u64,
    kept: VecDeque<Command>,
}

/// The commands' state, one per process.
struct State {
    /// Whether a command is open: the frame's one check while none is.
    open: AtomicBool,
    commands: Mutex<Commands>,
    /// Whether the loader's Mods screen can open now, once its interface
    /// registered it.
    mods_screen: OnceLock<fn() -> bool>,
}

static STATE: State = State {
    open: AtomicBool::new(false),
    commands: Mutex::new(Commands {
        next_id: 1,
        kept: VecDeque::new(),
    }),
    mods_screen: OnceLock::new(),
};

/// Why a command was not queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CommandError {
    /// Only a client runs them.
    #[error("client commands run on a client")]
    NotClient,
    /// The kind, its target or a bound is wrong.
    #[error("the command's kind or arguments are wrong")]
    BadArgs,
    /// The build lacks what the kind needs.
    #[error("this build lacks what the command needs")]
    Unavailable,
    /// The mod has a command of this kind open, or as many presses as one
    /// mod may hold.
    #[error("a command of this kind waits, or the mod holds its presses")]
    Waiting,
    /// The client is not where the command starts, or a join or leave runs.
    #[error("the client is not where the command starts, or a join or leave runs")]
    NotNow,
}

impl From<play::StartError> for CommandError {
    fn from(error: play::StartError) -> Self {
        match error {
            play::StartError::BadArgs => Self::BadArgs,
            play::StartError::NotNow => Self::NotNow,
        }
    }
}

/// Whether the build runs `kind`: the screen toggle for screens, the tab
/// read for tabs, the builder's detour for looks and moves, the recipe
/// browser's for recipes.
fn runs(kind: Kind) -> bool {
    // Every kind checks itself against the frame's screen reads.
    if FUSE.is_off() || crate::engine::screens::READS_FUSE.is_off() {
        return false;
    }
    let groups = &crate::actions::client_control::GROUPS;
    match kind {
        Kind::Open(_) | Kind::Close(_) => groups.toggle.get().is_some(),
        Kind::Tab(_) => groups.tab.get().is_some(),
        Kind::Look(_) | Kind::Move(..) => client_input::runs(),
        Kind::Press(..) => client_input::presses_run(),
        Kind::Highlight(_) => client_recipes::runs(),
        Kind::Join | Kind::Leave => play::runs(),
        Kind::Host(_) => play::hosts_run(),
        // The screen draws only while its menu shows: an ask no menu takes
        // times out.
        Kind::Mods(_) => mods_screen_opens(),
        Kind::Mark(_) => client_input::marks_run(),
        Kind::Window(_) => windows::runs(),
    }
}

/// Whether the loader's Mods screen can open now, as its interface
/// registered; never before it did.
fn mods_screen_opens() -> bool {
    STATE.mods_screen.get().is_some_and(|opens| opens())
}

/// Registers whether the loader's Mods screen can open for a command, from
/// the interface that draws it, a layer above; a build without one runs no
/// Mods screen command.
pub(crate) fn register_mods_screen(opens: fn() -> bool) {
    let _first = STATE.mods_screen.set(opens);
}

/// Queues mod `owner`'s command `spec`; its id.
///
/// # Errors
///
/// [`CommandError`] off a client, for a wrong spec, on a build without
/// what it needs, and while a command of its kind is open.
pub(crate) fn queue(loader: &Loader, owner: ModIndex, spec: &Spec) -> Result<u64, CommandError> {
    if loader.side != Side::CLIENT {
        return Err(CommandError::NotClient);
    }
    if spec.kind == Spec::CANCEL {
        cancel(owner, spec.cancels);
        return Ok(spec.cancels);
    }
    let kind = kind(loader, spec)?;
    crate::actions::client_snapshot::ask();
    if !runs(kind) {
        return Err(CommandError::Unavailable);
    }
    // SAFETY: the mod's spec, its text borrowed for the call.
    let expected = expected_screen(kind, unsafe { text(spec.expected_screen) }?)?;
    let held = local_held::players_held();
    let mut commands = STATE.commands.relock();
    if !may_queue(&commands.kept, owner, kind) {
        return Err(CommandError::Waiting);
    }
    let id = commands.next_id;
    let now = local_held::now_ms();
    if kind == Kind::Leave {
        // Handed in the frame, after its screen check; refused here where it
        // could not start, as the fake host refuses it.
        play::can_leave(now)?;
    }
    if let Kind::Host(host) = kind {
        play::host(id, host, now)?;
    }
    if kind == Kind::Join {
        // SAFETY: the mod's spec, its target borrowed for the call; the play
        // module copies it.
        let target = unsafe { text(spec.target) }?;
        // SAFETY: the same spec's password, borrowed for the call; the play
        // module keeps a copy only until it is posted.
        let password = unsafe { text(spec.password) }?;
        play::join(id, target, password.as_bytes(), now)?;
    }
    if let Kind::Window(_) = kind {
        // SAFETY: the mod's spec, its target borrowed for the call.
        let window = unsafe { text(spec.target) }?;
        windows::park(id, window_ask(spec, window)?);
    }
    commands.next_id += 1;
    commands.kept.push_back(Command {
        id,
        owner,
        kind,
        session: spec.session,
        expected,
        held,
        queued_ms: now,
        state: Status::PENDING,
        reason: 0,
        handed: matches!(kind, Kind::Join | Kind::Host(_)).then_some(now),
        phase: kind.queued_phase(),
        failure: 0,
    });
    while commands.kept.len() > KEPT && commands.kept.front().is_some_and(|c| !c.open()) {
        commands.kept.pop_front();
    }
    STATE.open.store(true, Ordering::Release);
    Ok(id)
}

/// Whether mod `owner` may queue `kind` beside its open commands in `kept`:
/// one of each kind, one press of each action, and no more presses than
/// one mod may hold.
fn may_queue(kept: &VecDeque<Command>, owner: ModIndex, kind: Kind) -> bool {
    let mine = || kept.iter().filter(|c| c.open() && c.owner == owner);
    let presses = mine().filter(|c| matches!(c.kind, Kind::Press(..))).count();
    mine().all(|c| c.kind.waits_beside(kind))
        && !(matches!(kind, Kind::Press(..)) && presses >= rules::PRESSES_PER_MOD)
}

/// Cancels mod `owner`'s command `id` if it is still open.
fn cancel(owner: ModIndex, id: u64) {
    let mut commands = STATE.commands.relock();
    if let Some(command) = commands
        .kept
        .iter_mut()
        .find(|c| c.id == id && c.owner == owner && c.open())
    {
        command.release(true);
        command.state = Status::CANCELLED;
        command.reason = Status::BY_MOD;
    }
}

/// Where mod `owner`'s command `id` stands; `None` for one the loader no
/// longer keeps or another mod's.
pub(crate) fn status(owner: ModIndex, id: u64) -> Option<Status> {
    let commands = STATE.commands.relock();
    let command = commands
        .kept
        .iter()
        .find(|c| c.id == id && c.owner == owner)?;
    Some(Status {
        state: command.state,
        reason: command.reason,
        kind: command.kind.raw(),
        age_ms: local_held::now_ms().saturating_sub(command.queued_ms),
        phase: command.phase,
        failure: command.failure,
        ..Status::EMPTY
    })
}

/// Runs the open commands in the interface frame with what it read of the
/// `game`: `apply` hands one to the game. One atomic load while none is
/// open. A frame that reads no game, outside a world or without the screen
/// reads, runs the Mods screen's commands with the `menu` read and only
/// ends the others that waited too long.
/// A panic in it turns off only the commands, never the interface frame.
pub(crate) fn frame(
    game: impl FnOnce() -> Option<Seen>,
    menu: impl FnOnce() -> Seen,
    apply: impl FnMut(ScreenChange) -> Applied,
) {
    if !STATE.open.load(Ordering::Acquire) {
        return;
    }
    let _ran = FUSE.run(FEATURE, || {
        let mut commands = STATE.commands.relock();
        let game = game();
        let menu = game.is_none().then(menu);
        let open = run(&mut commands, game, menu, local_held::now_ms(), apply);
        STATE.open.store(open, Ordering::Release);
    });
}

/// Notes the client's app stage `byte` and runs a join or leave in the
/// interface frame: `game` answers whether the frame reads the client's
/// play data, `post` hands a menu command to the game. A panic in it turns
/// off only the commands, never the interface frame.
pub(crate) fn play_frame(
    byte: impl FnOnce() -> u8,
    game: impl FnOnce() -> bool,
    post: impl FnMut(Post<'_>) -> bool,
) {
    let _ran = FUSE.run(FEATURE, || {
        let now = local_held::now_ms();
        let byte = byte();
        let edge = play::note_stage(byte, now);
        #[cfg(windows)]
        if let Some(playing) = edge {
            crate::base::join::play_changed(playing);
        }
        #[cfg(not(windows))]
        let _ = edge;
        play::frame(play::Stage::of(byte), now, game, post);
    });
}

/// Runs the open `commands` at `now` with what the frame read of the
/// `game`, or outside one of the `menu`, which only the Mods screen's
/// commands read; whether one is still open.
fn run(
    commands: &mut Commands,
    game: Option<Seen>,
    menu: Option<Seen>,
    now: u64,
    mut apply: impl FnMut(ScreenChange) -> Applied,
) -> bool {
    let commanded = commanded(commands);
    for command in commands.kept.iter_mut().filter(|c| c.open()) {
        let off = crate::engine::loader::is_off(command.owner)
            || (command.kind.handed() && !runs(command.kind));
        let seen = game.as_ref().or_else(|| {
            menu.as_ref()
                .filter(|_| matches!(command.kind, Kind::Mods(_)))
        });
        let next = if off {
            Next::End(Status::CANCELLED, Status::MOD_OFF)
        } else if command.kind.plays() && command.handed.is_some() {
            plays_next(command)
        } else if let Some(seen) = seen {
            // Session 0, before the client read a game, is the one this
            // frame counts; the first fresh read is the held baseline.
            if command.session == 0 {
                command.session = seen.session;
            }
            if command.held.is_none() {
                command.held = seen.held;
            }
            next(command, seen, command.progress(), now, commanded)
        } else if timed_out(command, now) {
            Next::End(Status::CANCELLED, Status::TIMED_OUT)
        } else {
            Next::Wait
        };
        match next {
            Next::Wait => {}
            Next::Apply => match command.kind.screen_change() {
                None => command.handed = command.hand().then_some(now),
                Some(change) => match apply(change) {
                    Applied::Taken => command.state = Status::APPLIED,
                    Applied::Busy => {}
                    Applied::Unavailable => command.state = Status::UNAVAILABLE,
                },
            },
            Next::Applied => command.state = Status::APPLIED,
            Next::End(state, reason) => {
                command.release(false);
                command.state = state;
                command.reason = reason;
            }
        }
    }
    commands.kept.iter().any(Command::open)
}

/// Whether a command the game took opens or closes a screen, before the
/// frame ends it.
fn commanded(commands: &Commands) -> bool {
    commands
        .kept
        .iter()
        .any(|c| c.state == Status::APPLIED && matches!(c.kind, Kind::Open(_) | Kind::Close(_)))
}

#[cfg(test)]
mod tests;
