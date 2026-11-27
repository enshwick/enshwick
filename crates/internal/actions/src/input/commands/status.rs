//! What came of a client command: where it stands, why it was cancelled
//! or a join failed, and a join's phase, each with the name Luau gives it.

use crate::abi::ClientCommandStatus as Raw;

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
    /// Another screen than the expected one shows, one the command did not
    /// open itself.
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
    /// The window draws the item off, as a Run button while a field is
    /// empty, or the item takes no such option or text.
    ItemOff,
    /// The Mods screen refused a
    /// [`CommandAction::Rebind`](super::CommandAction::Rebind) or a
    /// [`CommandAction::SetSetting`](super::CommandAction::SetSetting): the
    /// setting's schema or its mod
    /// refuses the value, the setting is not open to change here, or the
    /// file did not write.
    ChangeRefused,
    /// A reason a newer loader gives that this build of the API does not
    /// name, as its raw value.
    Other(u32),
}

impl CancelReason {
    const ALL: [(u32, Self, &'static str); 9] = [
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
        (Raw::ITEM_OFF, Self::ItemOff, "item_off"),
        (Raw::CHANGE_REFUSED, Self::ChangeRefused, "change_refused"),
    ];

    pub(super) fn from_raw(reason: u32) -> Self {
        Self::ALL
            .iter()
            .find(|(raw, ..)| *raw == reason)
            .map_or(Self::Other(reason), |(_, reason, _)| *reason)
    }

    /// The reason's name as the Luau API and the book give it, such as
    /// `"other_screen"`; `"other"` for every [`CancelReason::Other`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::CancelReason;
    ///
    /// assert_eq!(CancelReason::OtherScreen.name(), "other_screen");
    /// assert_eq!(CancelReason::Other(99).name(), "other");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, reason, _)| *reason == self)
            .map_or("other", |(.., name)| name)
    }
}

/// Where a command stands, from [`Input::command_status`](crate::input::Input::command_status).
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
    /// The game ended a join or a host without reaching the world.
    Failed(JoinFailure),
}

impl CommandState {
    pub(super) fn from_raw(raw: &Raw) -> Self {
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

/// Why a join or a host failed, as far as the client can tell.
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

    pub(super) fn from_raw(failure: u32) -> Self {
        Self::ALL
            .iter()
            .find(|(raw, ..)| *raw == failure)
            .map_or(Self::Other(failure), |(_, failure, _)| *failure)
    }

    /// The failure's name as the Luau API and the book give it, such as
    /// `"wrong_password"`; `"other"` for every [`JoinFailure::Other`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::JoinFailure;
    ///
    /// assert_eq!(JoinFailure::TimedOut.name(), "timed_out");
    /// assert_eq!(JoinFailure::Other(99).name(), "other");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, failure, _)| *failure == self)
            .map_or("other", |(.., name)| name)
    }
}

/// Where the client stands on a [`CommandAction::Join`](super::CommandAction::Join),
/// [`CommandAction::Host`](super::CommandAction::Host) or [`CommandAction::Leave`](super::CommandAction::Leave).
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
    pub(super) fn from_raw(raw: u32) -> Option<Self> {
        (raw != 0).then(|| {
            Self::ALL
                .iter()
                .find(|(value, ..)| *value == raw)
                .map_or(Self::Other(raw), |(_, phase, _)| *phase)
        })
    }

    /// The phase's name as the Luau API and the book give it, such as
    /// `"in_world"`; `"other"` for every [`JoinPhase::Other`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::JoinPhase;
    ///
    /// assert_eq!(JoinPhase::InWorld.name(), "in_world");
    /// assert_eq!(JoinPhase::Other(99).name(), "other");
    /// ```
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, phase, _)| *phase == self)
            .map_or("other", |(.., name)| name)
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
    /// A join, host or leave's phase: `"queued"`, `"menu"`, `"connecting"`,
    /// `"loading"`, `"in_world"` or `"left"`.
    pub phase: Option<String>,
}

/// Where a command stands and how long ago it was queued, from
/// [`Input::command_status`](crate::input::Input::command_status).
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
    /// Where the client stands on a join, host or leave; `None` for other
    /// kinds.
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
