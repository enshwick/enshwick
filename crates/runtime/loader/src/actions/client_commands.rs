//! Commands mods give the client's own interface and input: open or close
//! a screen, choose the in-game menu's tab, turn the camera, hold a short
//! move and highlight a recipe, each with an id and an outcome the client's
//! own state confirms.
//!
//! A mod queues from any thread; the interface frame runs the commands,
//! never inside a mod's callback, and hands a look, a move or a recipe to
//! highlight to the detour that writes it (see [`Handoff`]). Until a mod
//! queues, each runs nothing but one atomic load.

mod handoff;

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_base::Relock;
use enshwick_core::RecipeId;
use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Status};
use enshwick_core::abi::{Side, Str};

pub(crate) use self::handoff::Handoff;
use crate::actions::client_control::MenuTab;
use crate::actions::client_input::{self, Ask};
use crate::actions::client_recipes;
use crate::actions::client_snapshot::{RecipeRead, TabRead};
use crate::actions::local_held;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

/// How long a command may wait for its effect.
const TIMEOUT_MS: u64 = 2000;
/// How many commands' outcomes the loader keeps for their status.
const KEPT: usize = 64;

/// The screens a command opens: the ones the game opens from a key in the
/// world (client 26430625), by their `HudScreenType` names.
const OPENS: [&str; 10] = [
    "SystemMenu",
    "InGameMenu",
    "Map",
    "Journal",
    "Character",
    "PlayerCrafting",
    "Backpack",
    "SkillTree",
    "TextChat",
    "Social",
];
/// The screen a close returns to.
const WORLD: &str = "InGame";

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
        }
    }

    /// Whether it drives the player's input, which only the world's own
    /// screen takes, from the queue to its end.
    const fn drives(self) -> bool {
        matches!(self, Self::Look(_) | Self::Move(..))
    }

    /// Whether a detour of the game's own code runs it, through a
    /// [`Handoff`], rather than the interface frame.
    const fn handed(self) -> bool {
        matches!(self, Self::Look(_) | Self::Move(..) | Self::Highlight(_))
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
    /// The held controls when it was queued: a new one is the player's.
    held: u64,
    queued_ms: u64,
    state: u32,
    reason: u32,
    /// Whether its detour holds it, to be released when it ends.
    handed: bool,
}

impl Command {
    const fn open(&self) -> bool {
        self.state == Status::PENDING || self.state == Status::APPLIED
    }

    /// Where its detour stands with it; `None` before it is handed.
    fn progress(&self) -> Option<Progress> {
        if !self.handed {
            return None;
        }
        let progress = match self.kind {
            Kind::Look(_) | Kind::Move(..) => client_input::progress(self.id),
            Kind::Highlight(_) => client_recipes::progress(self.id),
            Kind::Open(_) | Kind::Close(_) | Kind::Tab(_) => None,
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
            Kind::Highlight(recipe) => client_recipes::hand(self.id, recipe),
            Kind::Open(_) | Kind::Close(_) | Kind::Tab(_) => false,
        }
    }

    /// Takes it back from its detour once it ended.
    fn release(&self) {
        if !self.handed {
            return;
        }
        match self.kind {
            Kind::Look(_) | Kind::Move(..) => client_input::release(self.id),
            Kind::Highlight(_) => client_recipes::release(self.id),
            Kind::Open(_) | Kind::Close(_) | Kind::Tab(_) => {}
        }
    }
}

/// What one frame shows of the client, for the commands to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) session: u64,
    pub(crate) screen: u8,
    pub(crate) tab: TabRead,
    /// The crafting menu's highlighted recipe.
    pub(crate) recipe: RecipeRead,
    /// The held controls of a fresh read, 0 without one.
    pub(crate) held: u64,
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
/// and where its detour stands, `progress`, once it is handed.
fn next(command: &Command, seen: &Seen, progress: Option<Progress>, now: u64) -> Next {
    let cancel = |reason| Next::End(Status::CANCELLED, reason);
    match progress {
        Some(Progress::Done) => return Next::End(Status::OBSERVED, 0),
        Some(Progress::Cancelled(reason)) => return cancel(reason),
        Some(Progress::Unavailable) => return Next::End(Status::UNAVAILABLE, 0),
        Some(Progress::Waiting | Progress::Taken) | None => {}
    }
    if seen.session != command.session {
        return cancel(Status::SESSION_CHANGED);
    }
    let drives = command.kind.drives();
    if drives && command.expected.is_some_and(|screen| screen != seen.screen) {
        return cancel(Status::OTHER_SCREEN);
    }
    let pressed = seen.held & !command.held != 0;
    let applied = command.state == Status::APPLIED || progress == Some(Progress::Taken);
    if applied {
        let shows = match command.kind {
            Kind::Open(screen) | Kind::Close(screen) => seen.screen == screen,
            Kind::Tab(tab) => seen.tab == TabRead::Shown(tab),
            Kind::Highlight(recipe) => seen.recipe == RecipeRead::Shown(Some(recipe)),
            Kind::Look(_) | Kind::Move(..) => false,
        };
        if shows {
            return Next::End(Status::OBSERVED, 0);
        }
        if drives && pressed {
            return cancel(Status::PLAYER_INPUT);
        }
        if command.state == Status::PENDING {
            return Next::Applied;
        }
    } else {
        if command.expected.is_some_and(|screen| screen != seen.screen) {
            return cancel(Status::OTHER_SCREEN);
        }
        if pressed {
            return cancel(Status::PLAYER_INPUT);
        }
        let ready = match command.kind {
            _ if progress.is_some() => false,
            Kind::Tab(_) => !matches!(seen.tab, TabRead::NoMenu),
            Kind::Highlight(_) => matches!(seen.recipe, RecipeRead::Shown(_)),
            Kind::Open(_) | Kind::Close(_) | Kind::Look(_) | Kind::Move(..) => true,
        };
        if ready {
            return Next::Apply;
        }
    }
    // A move taken just before the timeout still holds its whole hold.
    let limit = TIMEOUT_MS
        + if drives {
            u64::from(Spec::MAX_HOLD_MS)
        } else {
            0
        };
    if now.saturating_sub(command.queued_ms) > limit {
        return cancel(Status::TIMED_OUT);
    }
    Next::Wait
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
}

static STATE: State = State {
    open: AtomicBool::new(false),
    commands: Mutex::new(Commands {
        next_id: 1,
        kept: VecDeque::new(),
    }),
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
    /// The mod has a command of this kind open.
    #[error("a command of this kind waits")]
    Waiting,
}

/// The screen byte named `name` on this build.
fn screen_byte(name: &str) -> Option<u8> {
    crate::engine::screens::byte(name)
}

/// The text of a spec's name, empty for none, for the call: the command
/// keeps only the screen bytes and tabs found from it.
///
/// # Safety
///
/// - `name` is empty or points at text valid for `'a`, as the mod's call
///   borrows it.
unsafe fn text<'a>(name: Str) -> Result<&'a str, CommandError> {
    // SAFETY: the caller's contract.
    unsafe { name.as_str() }.ok_or(CommandError::BadArgs)
}

/// The kind `spec` asks for, its arguments checked.
fn kind(spec: &Spec) -> Result<Kind, CommandError> {
    // SAFETY: the mod's spec, its text borrowed for the call.
    let target = unsafe { text(spec.target) }?;
    match spec.kind {
        Spec::OPEN_SCREEN => {
            if !OPENS.contains(&target) {
                return Err(CommandError::BadArgs);
            }
            screen_byte(target)
                .map(Kind::Open)
                .ok_or(CommandError::Unavailable)
        }
        Spec::CLOSE_SCREEN => screen_byte(WORLD)
            .map(Kind::Close)
            .ok_or(CommandError::Unavailable),
        Spec::CHOOSE_TAB => MenuTab::ALL
            .into_iter()
            .find(|tab| tab.name() == target)
            .map(Kind::Tab)
            .ok_or(CommandError::BadArgs),
        _ if !spec.in_bounds() => Err(CommandError::BadArgs),
        Spec::LOOK => Ok(Kind::Look(spec.look)),
        Spec::MOVE => Ok(Kind::Move(spec.direction, spec.hold_ms)),
        Spec::HIGHLIGHT_RECIPE => Ok(Kind::Highlight(spec.recipe)),
        _ => Err(CommandError::BadArgs),
    }
}

/// Whether the build runs `kind`: the screen toggle for screens, the tab
/// read for tabs, the builder's detour for looks and moves, the recipe
/// browser's for recipes.
fn runs(kind: Kind) -> bool {
    let groups = &crate::actions::client_control::GROUPS;
    match kind {
        Kind::Open(_) | Kind::Close(_) => groups.toggle.get().is_some(),
        Kind::Tab(_) => groups.tab.get().is_some(),
        Kind::Look(_) | Kind::Move(..) => client_input::runs(),
        Kind::Highlight(_) => client_recipes::runs(),
    }
}

/// The screen byte `name` names, `None` for empty: a look or a move runs
/// only on the world's own screen.
fn expected_screen(kind: Kind, name: &str) -> Result<Option<u8>, CommandError> {
    if kind.drives() {
        if !name.is_empty() && name != WORLD {
            return Err(CommandError::BadArgs);
        }
        return screen_byte(WORLD)
            .map(Some)
            .ok_or(CommandError::Unavailable);
    }
    if name.is_empty() {
        return Ok(None);
    }
    screen_byte(name).map(Some).ok_or(CommandError::BadArgs)
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
    let kind = kind(spec)?;
    crate::actions::client_snapshot::ask();
    if !runs(kind) {
        return Err(CommandError::Unavailable);
    }
    // SAFETY: the mod's spec, its text borrowed for the call.
    let expected = expected_screen(kind, unsafe { text(spec.expected_screen) }?)?;
    let held = match local_held::sample() {
        local_held::Sample::Fresh { bits, .. } => bits,
        _ => 0,
    };
    let mut commands = STATE.commands.relock();
    if commands
        .kept
        .iter()
        .any(|c| c.open() && c.owner == owner && c.kind.raw() == kind.raw())
    {
        return Err(CommandError::Waiting);
    }
    let id = commands.next_id;
    commands.next_id += 1;
    commands.kept.push_back(Command {
        id,
        owner,
        kind,
        session: spec.session,
        expected,
        held,
        queued_ms: local_held::now_ms(),
        state: Status::PENDING,
        reason: 0,
        handed: false,
    });
    while commands.kept.len() > KEPT && commands.kept.front().is_some_and(|c| !c.open()) {
        commands.kept.pop_front();
    }
    STATE.open.store(true, Ordering::Release);
    Ok(id)
}

/// Cancels mod `owner`'s command `id` if it is still open.
fn cancel(owner: ModIndex, id: u64) {
    let mut commands = STATE.commands.relock();
    if let Some(command) = commands
        .kept
        .iter_mut()
        .find(|c| c.id == id && c.owner == owner && c.open())
    {
        command.release();
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
        ..Status::EMPTY
    })
}

/// Runs the open commands in the interface frame with what it `seen`:
/// `apply` hands one to the game. One atomic load while none is open.
pub(crate) fn frame(seen: impl FnOnce() -> Option<Seen>, mut apply: impl FnMut(Kind) -> Applied) {
    if !STATE.open.load(Ordering::Acquire) {
        return;
    }
    let Some(seen) = seen() else {
        return;
    };
    let now = local_held::now_ms();
    let mut commands = STATE.commands.relock();
    for command in commands.kept.iter_mut().filter(|c| c.open()) {
        let off = crate::engine::loader::is_off(command.owner)
            || (command.kind.handed() && !runs(command.kind));
        let next = if off {
            Next::End(Status::CANCELLED, Status::MOD_OFF)
        } else {
            next(command, &seen, command.progress(), now)
        };
        match next {
            Next::Wait => {}
            Next::Apply if command.kind.handed() => command.handed = command.hand(),
            Next::Apply => match apply(command.kind) {
                Applied::Taken => command.state = Status::APPLIED,
                Applied::Busy => {}
                Applied::Unavailable => command.state = Status::UNAVAILABLE,
            },
            Next::Applied => command.state = Status::APPLIED,
            Next::End(state, reason) => {
                command.release();
                command.state = state;
                command.reason = reason;
            }
        }
    }
    let any_open = commands.kept.iter().any(Command::open);
    STATE.open.store(any_open, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(kind: Kind) -> Command {
        Command {
            id: 1,
            owner: ModIndex::new(0),
            kind,
            session: 3,
            expected: Some(1),
            held: 0,
            queued_ms: 100,
            state: Status::PENDING,
            reason: 0,
            handed: false,
        }
    }

    fn seen(screen: u8, tab: TabRead) -> Seen {
        Seen {
            session: 3,
            screen,
            tab,
            recipe: RecipeRead::Unread,
            held: 0,
        }
    }

    /// The world's own screen, 1 on client 26430625.
    fn world() -> Seen {
        seen(1, TabRead::NoMenu)
    }

    #[test]
    fn a_look_is_handed_on_the_world_screen_and_observed_when_written() {
        let look = command(Kind::Look([0.5, 0.0]));
        assert_eq!(next(&look, &world(), None, 150), Next::Apply);
        let handed = Command {
            handed: true,
            ..look
        };
        assert_eq!(
            next(&handed, &world(), Some(Progress::Waiting), 150),
            Next::Wait,
            "handed once"
        );
        assert_eq!(
            next(&handed, &world(), Some(Progress::Done), 160),
            Next::End(Status::OBSERVED, 0)
        );
    }

    #[test]
    fn a_move_is_applied_while_it_holds_and_cancelled_by_a_press_or_another_screen() {
        let walk = Command {
            handed: true,
            ..command(Kind::Move([0.0, 1.0], 250))
        };
        assert_eq!(
            next(&walk, &world(), Some(Progress::Taken), 150),
            Next::Applied
        );
        let moving = Command {
            state: Status::APPLIED,
            ..walk
        };
        let cancelled = |reason| Next::End(Status::CANCELLED, reason);
        let pressed = Seen {
            held: 1 << 3,
            ..world()
        };
        assert_eq!(
            next(&moving, &pressed, Some(Progress::Taken), 160),
            cancelled(Status::PLAYER_INPUT),
            "W pressed during the move"
        );
        assert_eq!(
            next(
                &moving,
                &seen(4, TabRead::NoMenu),
                Some(Progress::Taken),
                160
            ),
            cancelled(Status::OTHER_SCREEN),
            "a menu opened during the move"
        );
        assert_eq!(
            next(
                &moving,
                &world(),
                Some(Progress::Cancelled(Status::INPUT_BLOCKED)),
                160
            ),
            cancelled(Status::INPUT_BLOCKED)
        );
        assert_eq!(
            next(&moving, &world(), Some(Progress::Done), 400),
            Next::End(Status::OBSERVED, 0)
        );
    }

    #[test]
    fn a_move_taken_late_keeps_its_hold_before_the_timeout() {
        let moving = Command {
            handed: true,
            state: Status::APPLIED,
            ..command(Kind::Move([0.0, 1.0], 250))
        };
        let late = 100 + TIMEOUT_MS + 100;
        assert_eq!(
            next(&moving, &world(), Some(Progress::Taken), late),
            Next::Wait
        );
        assert_eq!(
            next(&moving, &world(), Some(Progress::Taken), late + 200),
            Next::End(Status::CANCELLED, Status::TIMED_OUT)
        );
    }

    #[test]
    fn a_recipe_waits_for_the_browser_and_is_observed_once_it_is_highlighted() {
        let torch = RecipeId(0x7cd8_0a04);
        let highlight = Command {
            expected: None,
            ..command(Kind::Highlight(torch))
        };
        let browser = |recipe| Seen {
            recipe: RecipeRead::Shown(recipe),
            ..seen(8, TabRead::NoMenu)
        };
        assert_eq!(next(&highlight, &world(), None, 150), Next::Wait, "no menu");
        assert_eq!(next(&highlight, &browser(None), None, 150), Next::Apply);
        let handed = Command {
            handed: true,
            ..highlight
        };
        assert_eq!(
            next(&handed, &browser(None), Some(Progress::Taken), 160),
            Next::Applied
        );
        let applied = Command {
            state: Status::APPLIED,
            ..handed
        };
        assert_eq!(
            next(&applied, &browser(Some(torch)), Some(Progress::Taken), 170),
            Next::End(Status::OBSERVED, 0)
        );
        assert_eq!(
            next(&handed, &browser(None), Some(Progress::Unavailable), 160),
            Next::End(Status::UNAVAILABLE, 0),
            "a recipe the open menu does not list"
        );
    }

    #[test]
    fn a_look_or_move_past_its_bounds_and_on_another_screen_is_refused() {
        let spec = |kind, look, direction, hold_ms| Spec {
            kind,
            look,
            direction,
            hold_ms,
            ..Spec::EMPTY
        };
        assert_eq!(
            kind(&spec(Spec::LOOK, [Spec::MAX_LOOK, -0.5], [0.0; 2], 0)),
            Ok(Kind::Look([Spec::MAX_LOOK, -0.5]))
        );
        for refused in [
            spec(Spec::LOOK, [Spec::MAX_LOOK + 0.01, 0.0], [0.0; 2], 0),
            spec(Spec::LOOK, [f32::NAN, 0.0], [0.0; 2], 0),
            spec(Spec::MOVE, [0.0; 2], [0.0, 1.0], 251),
            spec(Spec::MOVE, [0.0; 2], [0.0, 1.0], 0),
            spec(Spec::MOVE, [0.0; 2], [1.0, 1.0], 100),
            spec(Spec::MOVE, [0.0; 2], [0.0, 0.0], 100),
            Spec {
                kind: Spec::HIGHLIGHT_RECIPE,
                ..Spec::EMPTY
            },
        ] {
            assert_eq!(kind(&refused), Err(CommandError::BadArgs), "{refused:?}");
        }
        assert_eq!(
            expected_screen(Kind::Move([0.0, 1.0], 100), "Backpack"),
            Err(CommandError::BadArgs)
        );
    }

    #[test]
    fn a_command_applies_on_its_screen_and_is_observed_once_the_screen_shows() {
        let mut open = command(Kind::Open(9));
        assert_eq!(
            next(&open, &seen(1, TabRead::NoMenu), None, 150),
            Next::Apply
        );
        open.state = Status::APPLIED;
        assert_eq!(
            next(&open, &seen(1, TabRead::NoMenu), None, 160),
            Next::Wait
        );
        assert_eq!(
            next(
                &open,
                &seen(9, TabRead::Shown(MenuTab::Backpack)),
                None,
                170
            ),
            Next::End(Status::OBSERVED, 0)
        );
    }

    #[test]
    fn each_cancel_reason_ends_a_pending_command() {
        let open = command(Kind::Open(9));
        let ended = |seen: Seen, now| next(&open, &seen, None, now);
        let cancelled = |reason| Next::End(Status::CANCELLED, reason);
        assert_eq!(
            ended(
                Seen {
                    session: 4,
                    ..seen(1, TabRead::NoMenu)
                },
                150
            ),
            cancelled(Status::SESSION_CHANGED)
        );
        assert_eq!(
            ended(seen(4, TabRead::NoMenu), 150),
            cancelled(Status::OTHER_SCREEN)
        );
        assert_eq!(
            ended(
                Seen {
                    held: 1 << 3,
                    ..seen(1, TabRead::NoMenu)
                },
                150
            ),
            cancelled(Status::PLAYER_INPUT)
        );
        let applied = Command {
            state: Status::APPLIED,
            ..open
        };
        assert_eq!(
            next(
                &applied,
                &seen(1, TabRead::NoMenu),
                None,
                100 + TIMEOUT_MS + 1
            ),
            cancelled(Status::TIMED_OUT)
        );
    }

    #[test]
    fn a_control_held_since_the_queue_is_not_the_player_s_new_input() {
        let held = Command {
            held: 1 << 3,
            ..command(Kind::Close(1))
        };
        let seen = Seen {
            held: 1 << 3,
            ..seen(1, TabRead::NoMenu)
        };
        assert_eq!(next(&held, &seen, None, 150), Next::Apply);
    }

    #[test]
    fn a_tab_waits_for_the_menu_and_is_observed_when_it_shows() {
        let mut tab = Command {
            expected: None,
            ..command(Kind::Tab(MenuTab::Character))
        };
        assert_eq!(next(&tab, &seen(1, TabRead::NoMenu), None, 150), Next::Wait);
        assert_eq!(
            next(&tab, &seen(4, TabRead::Shown(MenuTab::Backpack)), None, 150),
            Next::Apply
        );
        tab.state = Status::APPLIED;
        assert_eq!(
            next(
                &tab,
                &seen(4, TabRead::Shown(MenuTab::Character)),
                None,
                160
            ),
            Next::End(Status::OBSERVED, 0)
        );
    }

    #[test]
    fn the_frame_runs_nothing_while_no_command_is_open() {
        let mut asked = false;
        frame(
            || {
                asked = true;
                None
            },
            |_| Applied::Taken,
        );
        assert!(!asked);
    }

    #[test]
    fn a_server_queues_nothing() {
        let loader = crate::start::tests::bare_loader();
        assert_eq!(
            queue(&loader, ModIndex::new(0), &Spec::EMPTY),
            Err(CommandError::NotClient)
        );
    }
}
