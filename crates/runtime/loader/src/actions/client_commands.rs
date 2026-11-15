//! Commands mods give the client's own interface and input: open or close
//! a screen, choose the in-game menu's tab, turn the camera, hold a short
//! move and highlight a recipe, each with an id and an outcome the client's
//! own state confirms.
//!
//! A mod queues from any thread; the interface frame runs the screen and
//! tab commands and the input builder the look and move, never inside a
//! mod's callback. Until a mod queues, both run nothing but one atomic load.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_base::Relock;
use enshwick_core::abi::{ClientCommand as Spec, ClientCommandStatus as Status};
use enshwick_core::abi::{Side, Str};

use crate::actions::client_control::MenuTab;
use crate::actions::client_snapshot::TabRead;
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
}

impl Kind {
    const fn raw(self) -> u32 {
        match self {
            Self::Open(_) => Spec::OPEN_SCREEN,
            Self::Close(_) => Spec::CLOSE_SCREEN,
            Self::Tab(_) => Spec::CHOOSE_TAB,
        }
    }
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
}

impl Command {
    const fn open(&self) -> bool {
        self.state == Status::PENDING || self.state == Status::APPLIED
    }
}

/// What one frame shows of the client, for the commands to check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Seen {
    pub(crate) session: u64,
    pub(crate) screen: u8,
    pub(crate) tab: TabRead,
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
    Apply,
    End(u32, u32),
}

/// What the frame does with `command` now, given what it `seen` at `now`.
fn next(command: &Command, seen: &Seen, now: u64) -> Next {
    let cancel = |reason| Next::End(Status::CANCELLED, reason);
    if seen.session != command.session {
        return cancel(Status::SESSION_CHANGED);
    }
    if command.state == Status::APPLIED {
        let shows = match command.kind {
            Kind::Open(screen) | Kind::Close(screen) => seen.screen == screen,
            Kind::Tab(tab) => seen.tab == TabRead::Shown(tab),
        };
        if shows {
            return Next::End(Status::OBSERVED, 0);
        }
    } else {
        if command.expected.is_some_and(|screen| screen != seen.screen) {
            return cancel(Status::OTHER_SCREEN);
        }
        if seen.held & !command.held != 0 {
            return cancel(Status::PLAYER_INPUT);
        }
        let ready = match command.kind {
            Kind::Tab(_) => !matches!(seen.tab, TabRead::NoMenu),
            Kind::Open(_) | Kind::Close(_) => true,
        };
        if ready {
            return Next::Apply;
        }
    }
    if now.saturating_sub(command.queued_ms) > TIMEOUT_MS {
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
    (0..=u8::MAX).find(|screen| crate::ui::interface::screen_name(*screen) == name)
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
        Spec::LOOK | Spec::MOVE | Spec::HIGHLIGHT_RECIPE => Err(CommandError::Unavailable),
        _ => Err(CommandError::BadArgs),
    }
}

/// Whether the build runs `kind`: the screen toggle for screens, the tab
/// read for tabs.
fn runs(kind: Kind) -> bool {
    let groups = &crate::actions::client_control::GROUPS;
    match kind {
        Kind::Open(_) | Kind::Close(_) => groups.toggle.get().is_some(),
        Kind::Tab(_) => groups.tab.get().is_some(),
    }
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
    let expected = unsafe { text(spec.expected_screen) }?;
    let expected = if expected.is_empty() {
        None
    } else {
        Some(screen_byte(expected).ok_or(CommandError::BadArgs)?)
    };
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
        let next = if crate::engine::loader::is_off(command.owner) {
            Next::End(Status::CANCELLED, Status::MOD_OFF)
        } else {
            next(command, &seen, now)
        };
        match next {
            Next::Wait => {}
            Next::Apply => match apply(command.kind) {
                Applied::Taken => command.state = Status::APPLIED,
                Applied::Busy => {}
                Applied::Unavailable => command.state = Status::UNAVAILABLE,
            },
            Next::End(state, reason) => {
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
        }
    }

    fn seen(screen: u8, tab: TabRead) -> Seen {
        Seen {
            session: 3,
            screen,
            tab,
            held: 0,
        }
    }

    #[test]
    fn a_command_applies_on_its_screen_and_is_observed_once_the_screen_shows() {
        let mut open = command(Kind::Open(9));
        assert_eq!(next(&open, &seen(1, TabRead::NoMenu), 150), Next::Apply);
        open.state = Status::APPLIED;
        assert_eq!(next(&open, &seen(1, TabRead::NoMenu), 160), Next::Wait);
        assert_eq!(
            next(&open, &seen(9, TabRead::Shown(MenuTab::Backpack)), 170),
            Next::End(Status::OBSERVED, 0)
        );
    }

    #[test]
    fn each_cancel_reason_ends_a_pending_command() {
        let open = command(Kind::Open(9));
        let ended = |seen: Seen, now| next(&open, &seen, now);
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
            next(&applied, &seen(1, TabRead::NoMenu), 100 + TIMEOUT_MS + 1),
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
        assert_eq!(next(&held, &seen, 150), Next::Apply);
    }

    #[test]
    fn a_tab_waits_for_the_menu_and_is_observed_when_it_shows() {
        let mut tab = Command {
            expected: None,
            ..command(Kind::Tab(MenuTab::Character))
        };
        assert_eq!(next(&tab, &seen(1, TabRead::NoMenu), 150), Next::Wait);
        assert_eq!(
            next(&tab, &seen(4, TabRead::Shown(MenuTab::Backpack)), 150),
            Next::Apply
        );
        tab.state = Status::APPLIED;
        assert_eq!(
            next(&tab, &seen(4, TabRead::Shown(MenuTab::Character)), 160),
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
