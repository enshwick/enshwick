//! Commands for the client's own interface and input: open or close a
//! screen, choose the in-game menu's tab, each with an id and an outcome
//! the client's own state confirms.

use super::Input;
use crate::abi::{ClientCommand as RawCommand, ClientCommandStatus as Raw, Status, Str};
use crate::controls::GameAction;
use crate::{RecipeId, WorldError};
use enshwick_world::__internal::world_status;

/// What a [`ClientCommand`] does.
///
/// # Examples
///
/// ```
/// use enshwick::input::CommandAction;
///
/// let open = CommandAction::OpenScreen("Backpack".to_owned());
/// assert_ne!(open, CommandAction::CloseScreen);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CommandAction {
    /// Opens a screen the game opens from a key in the world, by its
    /// `HudScreenType` name: `SystemMenu`, `InGameMenu`, `Map`, `Journal`,
    /// `Character`, `PlayerCrafting`, `Backpack`, `SkillTree`, `TextChat` or
    /// `Social`.
    OpenScreen(String),
    /// Closes the open screen, back to the world.
    CloseScreen,
    /// Shows the in-game menu's tab, named as the screen it stands for, as
    /// [`ClientSnapshot::menu_tab`](super::ClientSnapshot::menu_tab) names
    /// it.
    ChooseTab(String),
    /// Turns the camera once by `yaw` and `pitch` radians, before the
    /// player's camera sensitivity; each at most a quarter turn. A positive
    /// yaw turns right.
    Look {
        /// Radians of yaw.
        yaw: f32,
        /// Radians of pitch.
        pitch: f32,
    },
    /// Holds `direction` (x right, y forward, length at most 1) for
    /// `hold_ms` milliseconds, at most 250.
    Move {
        /// The direction.
        direction: [f32; 2],
        /// The hold in milliseconds.
        hold_ms: u32,
    },
    /// Highlights `recipe` in the open crafting menu, crafting nothing.
    HighlightRecipe(RecipeId),
    /// Joins the dedicated server `server`, `host:query_port`, from the main
    /// menu with a character selected, giving `password` when it asks for
    /// one. Its [`CommandStatus::phase`] follows the join at the game's own
    /// pace. Its command's session is 0.
    Join {
        /// The server, as `host:query_port`.
        server: String,
        /// The server's password, `None` for none.
        password: Option<Password>,
    },
    /// Leaves the world back to the main menu.
    Leave,
    /// Holds `action` in the local player's input for `hold_ms`
    /// milliseconds, 1 to 2000, as its key would; one step is a tap.
    ///
    /// The game acts on the form its key sets: an attack is
    /// [`GameAction::MainhandActionTap`] with
    /// [`GameAction::MainhandActionRelease`], an interaction
    /// [`GameAction::ContextualActionTap`]; a plain action such as
    /// [`GameAction::Jump`] or [`GameAction::Sprint`] acts on its own bit.
    /// Several presses hold at once, beside a look or a move.
    Press {
        /// The action.
        action: GameAction,
        /// The hold in milliseconds.
        hold_ms: u32,
    },
}

/// A server's password for [`CommandAction::Join`]: the loader hands it to
/// the game's own join and keeps, logs and returns none of it, and its
/// `Debug` shows none of it either.
///
/// # Examples
///
/// ```
/// use enshwick::input::Password;
///
/// let password = Password::new("hunter2");
/// assert!(!format!("{password:?}").contains("hunter2"));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    /// The password `text`; the game takes at most 1024 bytes.
    ///
    /// # Examples
    ///
    /// See [`Password`].
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl core::fmt::Debug for Password {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Password(..)")
    }
}

/// A command for the client, from [`Input::client_command`]: what it does,
/// the session it belongs to and the screen the client must show.
///
/// # Examples
///
/// ```
/// use enshwick::input::{ClientCommand, CommandAction};
///
/// fn open_backpack(input: enshwick::input::Input) -> Option<enshwick::input::CommandId> {
///     let snapshot = input.client_snapshot().ok()?;
///     let command = ClientCommand::new(CommandAction::OpenScreen("Backpack".to_owned()), snapshot.session())
///         .on_screen("InGame");
///     input.client_command(&command).ok()
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ClientCommand {
    action: CommandAction,
    session: u64,
    expected_screen: Option<String>,
}

impl ClientCommand {
    /// `action` for the client's session `session`, as
    /// [`ClientSnapshot::session`](super::ClientSnapshot::session) names it,
    /// on any screen; 0 takes the session of the first frame that runs it.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[must_use]
    pub const fn new(action: CommandAction, session: u64) -> Self {
        Self {
            action,
            session,
            expected_screen: None,
        }
    }

    /// The same command, run only while the HUD screen `screen` shows: one
    /// that finds another is cancelled.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[must_use]
    pub fn on_screen(mut self, screen: &str) -> Self {
        self.expected_screen = Some(screen.to_owned());
        self
    }

    fn raw(&self) -> RawCommand {
        let mut raw = RawCommand {
            session: self.session,
            expected_screen: self.expected_screen.as_deref().map_or(Str::EMPTY, Str::new),
            ..RawCommand::EMPTY
        };
        match &self.action {
            CommandAction::OpenScreen(screen) => {
                raw.kind = RawCommand::OPEN_SCREEN;
                raw.target = Str::new(screen);
            }
            CommandAction::CloseScreen => raw.kind = RawCommand::CLOSE_SCREEN,
            CommandAction::ChooseTab(tab) => {
                raw.kind = RawCommand::CHOOSE_TAB;
                raw.target = Str::new(tab);
            }
            CommandAction::Look { yaw, pitch } => {
                raw.kind = RawCommand::LOOK;
                raw.look = [*yaw, *pitch];
            }
            CommandAction::Move { direction, hold_ms } => {
                raw.kind = RawCommand::MOVE;
                raw.direction = *direction;
                raw.hold_ms = *hold_ms;
            }
            CommandAction::HighlightRecipe(recipe) => {
                raw.kind = RawCommand::HIGHLIGHT_RECIPE;
                raw.recipe = *recipe;
            }
            CommandAction::Join { server, password } => {
                raw.kind = RawCommand::JOIN;
                raw.target = Str::new(server);
                raw.password = password.as_ref().map_or(Str::EMPTY, |p| Str::new(&p.0));
            }
            CommandAction::Leave => raw.kind = RawCommand::LEAVE,
            CommandAction::Press { action, hold_ms } => {
                raw.kind = RawCommand::PRESS;
                raw.target = Str::new(action.name());
                raw.hold_ms = *hold_ms;
            }
        }
        raw
    }
}

/// A queued command's id, for [`Input::command_status`].
///
/// # Examples
///
/// ```
/// use enshwick::input::CommandId;
///
/// assert_ne!(CommandId::new(1), CommandId::new(2));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct CommandId(u64);

impl CommandId {
    /// The id `id`, as the loader numbered it.
    ///
    /// # Examples
    ///
    /// See [`CommandId`].
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Why a command ended without its effect.
///
/// # Examples
///
/// ```
/// use enshwick::input::CancelReason;
///
/// assert_ne!(CancelReason::TimedOut, CancelReason::PlayerInput);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CancelReason {
    /// The client's session changed.
    SessionChanged,
    /// Another screen than the expected one shows.
    OtherScreen,
    /// The player pressed a control or moved the stick or mouse.
    PlayerInput,
    /// The mod cancelled it.
    ByMod,
    /// Two seconds passed without its effect.
    TimedOut,
    /// The game blocks the player's input now.
    InputBlocked,
    /// The mod or the feature turned off.
    ModOff,
    /// A reason a newer loader gives that this build of the API does not
    /// name, as its raw value.
    Other(u32),
}

impl CancelReason {
    const ALL: [(u32, Self, &'static str); 7] = [
        (
            Raw::SESSION_CHANGED,
            Self::SessionChanged,
            "session_changed",
        ),
        (Raw::OTHER_SCREEN, Self::OtherScreen, "other_screen"),
        (Raw::PLAYER_INPUT, Self::PlayerInput, "player_input"),
        (Raw::BY_MOD, Self::ByMod, "by_mod"),
        (Raw::TIMED_OUT, Self::TimedOut, "timed_out"),
        (Raw::INPUT_BLOCKED, Self::InputBlocked, "input_blocked"),
        (Raw::MOD_OFF, Self::ModOff, "mod_off"),
    ];

    fn from_raw(reason: u32) -> Self {
        Self::ALL
            .iter()
            .find(|(raw, ..)| *raw == reason)
            .map_or(Self::Other(reason), |(_, reason, _)| *reason)
    }

    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, reason, _)| *reason == self)
            .map_or("other", |(.., name)| name)
    }
}

/// Where a command stands, from [`Input::command_status`].
///
/// # Examples
///
/// ```
/// use enshwick::input::CommandState;
///
/// assert_ne!(CommandState::Applied, CommandState::Observed);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CommandState {
    /// Queued; the game has not taken it yet.
    Pending,
    /// The game took it.
    Applied,
    /// The client's own state shows its effect.
    Observed,
    /// It ended without its effect.
    Cancelled(CancelReason),
    /// The running build lacks what it needs, or the target is not there.
    Unavailable,
    /// The game ended a join without reaching the world.
    Failed(JoinFailure),
}

impl CommandState {
    fn from_raw(raw: &Raw) -> Self {
        match raw.state {
            Raw::APPLIED => Self::Applied,
            Raw::OBSERVED => Self::Observed,
            Raw::CANCELLED => Self::Cancelled(CancelReason::from_raw(raw.reason)),
            Raw::UNAVAILABLE => Self::Unavailable,
            Raw::FAILED => Self::Failed(JoinFailure::from_raw(raw.failure)),
            _ => Self::Pending,
        }
    }
}

/// Why a join failed, as far as the client can tell.
///
/// # Examples
///
/// ```
/// use enshwick::input::JoinFailure;
///
/// assert_ne!(JoinFailure::Full, JoinFailure::Unknown);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JoinFailure {
    /// The server refused the password, or asks for one and none was given.
    WrongPassword,
    /// The server is full.
    Full,
    /// The server was not found.
    NotFound,
    /// The server is reserved for others.
    Reserved,
    /// The server runs another game version.
    VersionMismatch,
    /// The loader's join check or the server refused the client.
    Refused,
    /// The game did not start or finish the join in time.
    TimedOut,
    /// The server's query port did not answer with its address.
    Unreachable,
    /// The client does not show why.
    Unknown,
    /// A failure a newer loader gives that this build of the API does not
    /// name, as its raw value.
    Other(u32),
}

impl JoinFailure {
    const ALL: [(u32, Self, &'static str); 9] = [
        (Raw::WRONG_PASSWORD, Self::WrongPassword, "wrong_password"),
        (Raw::FULL, Self::Full, "full"),
        (Raw::NOT_FOUND, Self::NotFound, "not_found"),
        (Raw::RESERVED, Self::Reserved, "reserved"),
        (
            Raw::VERSION_MISMATCH,
            Self::VersionMismatch,
            "version_mismatch",
        ),
        (Raw::REFUSED, Self::Refused, "refused"),
        (Raw::JOIN_TIMED_OUT, Self::TimedOut, "timed_out"),
        (Raw::UNREACHABLE, Self::Unreachable, "unreachable"),
        (Raw::UNKNOWN, Self::Unknown, "unknown"),
    ];

    fn from_raw(failure: u32) -> Self {
        Self::ALL
            .iter()
            .find(|(raw, ..)| *raw == failure)
            .map_or(Self::Other(failure), |(_, failure, _)| *failure)
    }

    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, failure, _)| *failure == self)
            .map_or("other", |(.., name)| name)
    }
}

/// Where the client stands on a [`CommandAction::Join`] or
/// [`CommandAction::Leave`].
///
/// # Examples
///
/// ```
/// use enshwick::input::JoinPhase;
///
/// assert_ne!(JoinPhase::Loading, JoinPhase::InWorld);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JoinPhase {
    /// Waiting to start: the loader asks the server for its address.
    Queued,
    /// The main menu took the join and starts play.
    Menu,
    /// Play started; the client connects to the server.
    Connecting,
    /// Connected; the client loads the world.
    Loading,
    /// The client is in the server's world.
    InWorld,
    /// The client left play, back to the main menu.
    Left,
    /// A phase a newer loader gives that this build of the API does not
    /// name, as its raw value.
    Other(u32),
}

impl JoinPhase {
    const ALL: [(u32, Self, &'static str); 6] = [
        (Raw::PHASE_QUEUED, Self::Queued, "queued"),
        (Raw::PHASE_MENU, Self::Menu, "menu"),
        (Raw::PHASE_CONNECTING, Self::Connecting, "connecting"),
        (Raw::PHASE_LOADING, Self::Loading, "loading"),
        (Raw::PHASE_IN_WORLD, Self::InWorld, "in_world"),
        (Raw::PHASE_LEFT, Self::Left, "left"),
    ];

    /// The phase of `raw`, `None` for 0: no join or leave.
    fn from_raw(raw: u32) -> Option<Self> {
        (raw != 0).then(|| {
            Self::ALL
                .iter()
                .find(|(value, ..)| *value == raw)
                .map_or(Self::Other(raw), |(_, phase, _)| *phase)
        })
    }

    fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, phase, _)| *phase == self)
            .map_or("other", |(.., name)| name)
    }
}

/// [`ClientCommand`] as Luau gives it; its `Debug` shows none of the
/// password.
#[derive(Clone, PartialEq, crate::Script)]
#[script(name = "ClientCommand")]
struct LuauCommand {
    /// `"open_screen"`, `"close_screen"`, `"choose_tab"`, `"look"`,
    /// `"move"`, `"highlight_recipe"`, `"join"`, `"leave"` or `"press"`.
    pub kind: String,
    /// The session, as `client_snapshot().session` gives it.
    pub session: u64,
    /// The HUD screen the client must show.
    pub expected_screen: Option<String>,
    /// The screen to open, the tab to choose or the server to join.
    pub target: Option<String>,
    /// A look's yaw and pitch in radians.
    pub look: Option<[f32; 2]>,
    /// A move's direction.
    pub direction: Option<[f32; 2]>,
    /// A move's or a press's hold in milliseconds.
    pub hold_ms: Option<u32>,
    /// The recipe to highlight.
    pub recipe: Option<RecipeId>,
    /// A join's server password.
    pub password: Option<String>,
    /// The game action a press holds, by its game name, as a control's
    /// `link` names it.
    pub action: Option<String>,
}

impl core::fmt::Debug for LuauCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LuauCommand")
            .field("kind", &self.kind)
            .field("session", &self.session)
            .field("expected_screen", &self.expected_screen)
            .field("target", &self.target)
            .field("look", &self.look)
            .field("direction", &self.direction)
            .field("hold_ms", &self.hold_ms)
            .field("recipe", &self.recipe)
            .field("password", &self.password.as_ref().map(|_| "<secret>"))
            .field("action", &self.action)
            .finish()
    }
}

impl TryFrom<LuauCommand> for ClientCommand {
    type Error = crate::script::Error;

    fn try_from(luau: LuauCommand) -> Result<Self, Self::Error> {
        let missing =
            |what: &str| crate::script::Error::new(format!("a {} command needs {what}", luau.kind));
        let action = match luau.kind.as_str() {
            "open_screen" => {
                CommandAction::OpenScreen(luau.target.clone().ok_or_else(|| missing("a target"))?)
            }
            "close_screen" => CommandAction::CloseScreen,
            "choose_tab" => {
                CommandAction::ChooseTab(luau.target.clone().ok_or_else(|| missing("a target"))?)
            }
            "look" => {
                let [yaw, pitch] = luau.look.ok_or_else(|| missing("a look"))?;
                CommandAction::Look { yaw, pitch }
            }
            "move" => CommandAction::Move {
                direction: luau.direction.ok_or_else(|| missing("a direction"))?,
                hold_ms: luau.hold_ms.ok_or_else(|| missing("a hold"))?,
            },
            "highlight_recipe" => {
                CommandAction::HighlightRecipe(luau.recipe.ok_or_else(|| missing("a recipe"))?)
            }
            "join" => CommandAction::Join {
                server: luau.target.clone().ok_or_else(|| missing("a target"))?,
                password: luau.password.clone().map(Password),
            },
            "leave" => CommandAction::Leave,
            "press" => {
                let name = luau.action.as_deref().ok_or_else(|| missing("an action"))?;
                let action = GameAction::ALL
                    .iter()
                    .copied()
                    .find(|action| action.name() == name)
                    .ok_or_else(|| crate::script::Error::new(format!("no game action {name:?}")))?;
                CommandAction::Press {
                    action,
                    hold_ms: luau.hold_ms.ok_or_else(|| missing("a hold"))?,
                }
            }
            other => {
                return Err(crate::script::Error::new(format!(
                    "no client command kind {other:?}"
                )));
            }
        };
        Ok(Self {
            action,
            session: luau.session,
            expected_screen: luau.expected_screen,
        })
    }
}

impl crate::script::Typed for ClientCommand {
    const TY: &'static crate::script::Ty = <LuauCommand as crate::script::Typed>::TY;
}

impl crate::script::FromScript for ClientCommand {
    fn from_value(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        LuauCommand::from_value(value)?.try_into()
    }
}

/// [`CommandStatus`] as Luau sees it.
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[script(name = "CommandStatus")]
struct LuauStatus {
    /// `"pending"`, `"applied"`, `"observed"`, `"cancelled"`,
    /// `"unavailable"` or `"failed"`.
    pub state: String,
    /// Why a cancelled command ended, such as `"timed_out"`, or why a
    /// `"failed"` join failed, such as `"full"`; `"other"` for one a newer
    /// loader gives.
    pub reason: Option<String>,
    /// Milliseconds since the command was queued.
    pub age_ms: u64,
    /// A join or leave's phase: `"queued"`, `"menu"`, `"connecting"`,
    /// `"loading"`, `"in_world"` or `"left"`.
    pub phase: Option<String>,
}

/// Where a command stands and how long ago it was queued, from
/// [`Input::command_status`].
///
/// # Examples
///
/// ```
/// use enshwick::input::{CommandId, CommandState};
///
/// fn done(input: enshwick::input::Input, id: CommandId) -> bool {
///     input.command_status(id).is_ok_and(|status| status.state == CommandState::Observed)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommandStatus {
    /// Where it stands.
    pub state: CommandState,
    /// Milliseconds since it was queued.
    pub age_ms: u64,
    /// Where the client stands on a join or leave; `None` for other kinds.
    pub phase: Option<JoinPhase>,
}

impl crate::script::Typed for CommandStatus {
    const TY: &'static crate::script::Ty = <LuauStatus as crate::script::Typed>::TY;
}

impl crate::script::IntoScript for CommandStatus {
    fn into_value(self) -> crate::script::Value {
        let (state, reason) = match self.state {
            CommandState::Pending => ("pending", None),
            CommandState::Applied => ("applied", None),
            CommandState::Observed => ("observed", None),
            CommandState::Cancelled(reason) => ("cancelled", Some(reason.name().to_owned())),
            CommandState::Unavailable => ("unavailable", None),
            CommandState::Failed(failure) => ("failed", Some(failure.name().to_owned())),
        };
        LuauStatus {
            state: state.to_owned(),
            reason,
            age_ms: self.age_ms,
            phase: self.phase.map(|phase| phase.name().to_owned()),
        }
        .into_value()
    }
}

#[crate::script(client_commands)]
impl Input {
    /// Queues `command` for the client's interface or input; its id. The
    /// call answers only that it is queued: [`Input::command_status`]
    /// tells what came of it. It runs later on the client's game thread,
    /// only while the client is still in the command's session on its
    /// screen. One command of each kind waits per mod, and one press of each
    /// game action, up to four presses.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] while one of its kind waits or the mod holds
    /// four presses, a bad argument
    /// for a wrong screen, tab or bound, [`WorldError::Unavailable`] on a
    /// server and on a build without what it needs.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[script]
    pub fn client_command(self, command: &ClientCommand) -> Result<CommandId, WorldError> {
        let raw = command.raw();
        let mut id = 0u64;
        let host = self.0.host;
        world_status((host.api().client_command)(
            host.handle(),
            &raw const raw,
            &raw mut id,
        ))?;
        Ok(CommandId(id))
    }

    /// Cancels the mod's command `id` if it still waits.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server.
    ///
    /// # Examples
    ///
    /// ```
    /// fn give_up(input: enshwick::input::Input, id: enshwick::input::CommandId) {
    ///     let _cancelled = input.cancel_command(id);
    /// }
    /// ```
    #[script]
    pub fn cancel_command(self, id: CommandId) -> Result<(), WorldError> {
        let raw = RawCommand {
            kind: RawCommand::CANCEL,
            cancels: id.0,
            ..RawCommand::EMPTY
        };
        let mut out = 0u64;
        let host = self.0.host;
        world_status((host.api().client_command)(
            host.handle(),
            &raw const raw,
            &raw mut out,
        ))
    }

    /// Where the mod's command `id` stands.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server and for an id the loader no
    /// longer keeps: it keeps the last 64 commands.
    ///
    /// # Examples
    ///
    /// See [`CommandStatus`].
    #[script]
    pub fn command_status(self, id: CommandId) -> Result<CommandStatus, WorldError> {
        let mut raw = Raw::EMPTY;
        let host = self.0.host;
        let status = (host.api().client_command_status)(host.handle(), id.0, &raw mut raw);
        if status != Status::OK {
            world_status(status)?;
        }
        Ok(CommandStatus {
            state: CommandState::from_raw(&raw),
            age_ms: raw.age_ms,
            phase: JoinPhase::from_raw(raw.phase),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_action_fills_its_own_fields_of_the_raw_command() {
        let open =
            ClientCommand::new(CommandAction::OpenScreen("Map".to_owned()), 2).on_screen("InGame");
        let raw = open.raw();
        assert_eq!((raw.kind, raw.session), (RawCommand::OPEN_SCREEN, 2));
        // SAFETY: the view points at `open`'s target, alive here.
        let target = unsafe { raw.target.as_str() };
        // SAFETY: the view points at `open`'s screen, alive here.
        let screen = unsafe { raw.expected_screen.as_str() };
        assert_eq!((target, screen), (Some("Map"), Some("InGame")));
        let walk = ClientCommand::new(
            CommandAction::Move {
                direction: [0.0, 1.0],
                hold_ms: 250,
            },
            2,
        )
        .raw();
        assert_eq!(
            (walk.kind, walk.direction, walk.hold_ms),
            (RawCommand::MOVE, [0.0, 1.0], 250)
        );
    }

    #[test]
    fn a_luau_command_needs_what_its_kind_takes() {
        let luau = |kind: &str, target: Option<&str>| LuauCommand {
            kind: kind.to_owned(),
            session: 1,
            expected_screen: None,
            target: target.map(str::to_owned),
            look: None,
            direction: None,
            hold_ms: None,
            recipe: None,
            password: None,
            action: None,
        };
        assert!(ClientCommand::try_from(luau("open_screen", Some("Map"))).is_ok());
        assert!(ClientCommand::try_from(luau("open_screen", None)).is_err());
        assert!(ClientCommand::try_from(luau("dance", None)).is_err());
        assert!(
            ClientCommand::try_from(luau("close_screen", None))
                .is_ok_and(|c| c.action == CommandAction::CloseScreen)
        );
    }

    #[test]
    fn a_press_carries_its_action_s_game_name_and_its_hold() {
        let press = ClientCommand::new(
            CommandAction::Press {
                action: GameAction::ContextualActionTap,
                hold_ms: 60,
            },
            3,
        )
        .raw();
        // SAFETY: the view points at the action's static name.
        let target = unsafe { press.target.as_str() };
        assert_eq!(
            (press.kind, target, press.hold_ms, press.session),
            (RawCommand::PRESS, Some("ContextualAction_Tap"), 60, 3)
        );
    }

    #[test]
    fn a_luau_press_needs_a_game_action_by_its_game_name_and_a_hold() {
        let luau = |action: Option<&str>, hold_ms: Option<u32>| LuauCommand {
            kind: "press".to_owned(),
            session: 1,
            expected_screen: None,
            target: None,
            look: None,
            direction: None,
            hold_ms,
            recipe: None,
            password: None,
            action: action.map(str::to_owned),
        };
        assert!(
            ClientCommand::try_from(luau(Some("MainhandAction_Tap"), Some(16))).is_ok_and(|c| {
                c.action
                    == CommandAction::Press {
                        action: GameAction::MainhandActionTap,
                        hold_ms: 16,
                    }
            })
        );
        assert!(ClientCommand::try_from(luau(Some("Dance"), Some(16))).is_err());
        assert!(ClientCommand::try_from(luau(None, Some(16))).is_err());
        assert!(ClientCommand::try_from(luau(Some("Jump"), None)).is_err());
    }

    #[test]
    fn a_cancelled_status_names_its_reason() {
        let raw = Raw {
            state: Raw::CANCELLED,
            reason: Raw::PLAYER_INPUT,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&raw),
            CommandState::Cancelled(CancelReason::PlayerInput)
        );
        assert_eq!(CancelReason::PlayerInput.name(), "player_input");
    }

    #[test]
    fn a_luau_join_never_shows_its_password_in_a_debug_print() {
        let luau = LuauCommand {
            kind: "join".to_owned(),
            session: 0,
            expected_screen: None,
            target: Some("127.0.0.1:15637".to_owned()),
            look: None,
            direction: None,
            hold_ms: None,
            recipe: None,
            password: Some("hunter2".to_owned()),
            action: None,
        };
        let shown = format!("{luau:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("127.0.0.1:15637"));
    }

    #[test]
    fn a_join_carries_its_server_and_password_and_a_failed_one_names_why() {
        let join = ClientCommand::new(
            CommandAction::Join {
                server: "127.0.0.1:15637".to_owned(),
                password: Some(Password::new("hunter2")),
            },
            0,
        );
        let raw = join.raw();
        assert_eq!(raw.kind, RawCommand::JOIN);
        // SAFETY: the view points at `join`'s server, alive here.
        let target = unsafe { raw.target.as_str() };
        // SAFETY: the view points at `join`'s password, alive here.
        let password = unsafe { raw.password.as_str() };
        assert_eq!(
            (target, password),
            (Some("127.0.0.1:15637"), Some("hunter2"))
        );
        assert!(
            !format!("{join:?}").contains("hunter2"),
            "the password never shows"
        );
        let failed = Raw {
            state: Raw::FAILED,
            failure: Raw::FULL,
            phase: Raw::PHASE_MENU,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&failed),
            CommandState::Failed(JoinFailure::Full)
        );
        assert_eq!(JoinPhase::from_raw(failed.phase), Some(JoinPhase::Menu));
        assert_eq!(JoinPhase::from_raw(0), None, "no phase for other kinds");
        assert_eq!(JoinFailure::from_raw(99), JoinFailure::Other(99));
    }

    #[test]
    fn a_reason_a_newer_loader_adds_reads_as_other_not_as_a_known_one() {
        let raw = Raw {
            state: Raw::CANCELLED,
            reason: 99,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&raw),
            CommandState::Cancelled(CancelReason::Other(99))
        );
        assert_eq!(CancelReason::Other(99).name(), "other");
    }
}
