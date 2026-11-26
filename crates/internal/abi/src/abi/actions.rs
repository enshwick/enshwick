//! Player actions and renames the loader asks a mod to answer.

use core::ffi::c_void;

use crate::abi::Metres;
use crate::abi::{Entity, Header, PlayerInfo, Status, Str};

/// A question the loader asks a mod about its player actions, registered
/// with [`HostApi::actions`](super::HostApi::actions).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionCall, Header, PlayerInfo, Str};
///
/// let player = PlayerInfo::with_name(enshwick::Entity::from_engine(2), enshwick::PlayerKey::new(7), 0, "Ann");
/// let call = enshwick::abi::headed!(ActionCall {
///     kind: ActionCall::RUN,
///     language: 0,
///     player: &raw const player,
///     action: Str::new("access"),
///     args: Str::new("p0000000000000009\tb1"),
///     target: core::ptr::null(),
///     reply: core::ptr::null_mut(),
///     word: Str::EMPTY,
///     via: 0,
///     reason: 0,
/// });
/// assert_eq!(call.kind, ActionCall::RUN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ActionCall {
    /// Size and version of this struct.
    pub header: Header,
    /// What the loader asks or tells: [`ActionCall::ALLOWED`],
    /// [`ActionCall::RUN`], [`ActionCall::CONSOLE`], [`ActionCall::CHAT`],
    /// [`ActionCall::REMOTE`], [`ActionCall::WINDOW`] or
    /// [`ActionCall::WINDOW_CLOSED`]; a mod built for an older minor answers a kind
    /// it does not know with an empty reply.
    pub kind: u32,
    /// The language the player reads the game in, a
    /// [`Language`](../i18n/struct.Language.html) value.
    pub language: u32,
    /// The player the question is about, never null, valid for the call.
    pub player: *const PlayerInfo,
    /// The action's id, for [`ActionCall::RUN`]; the console line after the
    /// mod's id, for [`ActionCall::CONSOLE`]; the line after the command's
    /// name or alias, for [`ActionCall::CHAT`] and [`ActionCall::REMOTE`];
    /// valid for the call.
    pub action: Str,
    /// The action's arguments in the sheet's encoding, for
    /// [`ActionCall::RUN`]; valid for the call.
    pub args: Str,
    /// The object the player's client named for [`ActionCall::RUN`], valid
    /// for the call; null for none and for the other questions.
    pub target: *const ActionTarget,
    /// Where the mod writes its answer to a run, a zeroed [`ActionReply`]
    /// the loader owns for the call; null for [`ActionCall::ALLOWED`] and
    /// [`ActionCall::WINDOW`], whose answer is the callback's `reply` text. A mod writes the whole
    /// struct, its header first, and leaves the callback's `reply` text
    /// untouched; a run whose reply it leaves zero is not done.
    pub reply: *mut ActionReply,
    /// The word that named the mod in the line, in lower case and without
    /// its `/`: the mod's command name, alias or id, for
    /// [`ActionCall::CONSOLE`], [`ActionCall::CHAT`] and
    /// [`ActionCall::REMOTE`], so the mod's help names the command as the
    /// caller typed it; empty for the other questions. Valid for the call.
    pub word: Str,
    /// For [`ActionCall::REFUSED`], the question the run would have been:
    /// [`ActionCall::RUN`], [`ActionCall::CONSOLE`], [`ActionCall::CHAT`] or
    /// [`ActionCall::REMOTE`]; 0 for the other questions.
    pub via: u32,
    /// For [`ActionCall::REFUSED`], why the loader refused the run, one of
    /// the `ActionCall::REFUSED_` reasons; 0 for the other questions.
    pub reason: u32,
}

impl_headed!(ActionCall);

/// A mod's answer to a run of one of its actions: whether it was done and
/// the text the caller reads.
///
/// A mod writes it through [`ActionCall::reply`], and
/// [`HostApi::run_command`](super::HostApi::run_command) writes one for its
/// caller.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionReply, Header, Str};
///
/// let reply = enshwick::abi::headed!(ActionReply { done: 1, text: Str::new("2 online") });
/// assert_eq!(reply.done, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ActionReply {
    /// Size and version of this struct; zero until the writer fills it.
    pub header: Header,
    /// 1 when the action was done, 0 when it was refused or failed.
    pub done: u8,
    /// The text the caller reads, empty for none; valid until the writer
    /// answers again on the same thread.
    pub text: Str,
}

impl_headed!(ActionReply);

/// The `close` value of
/// [`HostApi::action_window`](super::HostApi::action_window) that shows a
/// window: it opens, or updates where it shows.
pub const WINDOW_SHOW: u32 = 0;
/// The `close` value of
/// [`HostApi::action_window`](super::HostApi::action_window) that closes a
/// window.
pub const WINDOW_CLOSE: u32 = 1;
/// The `close` value of
/// [`HostApi::action_window`](super::HostApi::action_window) that updates a
/// window only on the clients that still show it.
pub const WINDOW_UPDATE: u32 = 2;

/// A running mod's command names, from
/// [`HostApi::command_list`](super::HostApi::command_list).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{CommandInfo, Header, Str};
///
/// let tools = enshwick::abi::headed!(CommandInfo {
///     name: Str::new("admin-tools"),
///     alias: Str::new("adm"),
///     mod_id: Str::new("you.admin-tools"),
/// });
/// assert_eq!(tools.header.size, Header::of::<CommandInfo>().size);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct CommandInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The full name, `admin-tools` in `/admin-tools online`.
    pub name: Str,
    /// The shorter alias, `adm` in `/adm online`; empty when another mod
    /// took the word first.
    pub alias: Str,
    /// The id of the mod that named them.
    pub mod_id: Str,
}

impl_headed!(CommandInfo);

/// The object a run of an action names, sent with
/// [`HostApi::run_action`](super::HostApi::run_action) and handed back
/// to the mod in [`ActionCall::target`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionTarget, Header};
///
/// let chest = enshwick::abi::headed!(ActionTarget {
///     flags: ActionTarget::POSITION,
///     entity: enshwick::Entity::from_engine(0),
///     position: [10.0, 2.0, -4.5],
/// });
/// assert_eq!(chest.flags & ActionTarget::ENTITY, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ActionTarget {
    /// Size and version of this struct.
    pub header: Header,
    /// Which fields hold a value: [`ActionTarget::ENTITY`],
    /// [`ActionTarget::POSITION`], both or none.
    pub flags: u32,
    /// The object's entity in the server's world, with
    /// [`ActionTarget::ENTITY`].
    pub entity: Entity,
    /// Where it stands, in metres, with [`ActionTarget::POSITION`].
    pub position: Metres,
}

impl_headed!(ActionTarget);

impl ActionTarget {
    /// `entity` holds the object's entity.
    pub const ENTITY: u32 = 1;
    /// `position` holds where it stands.
    pub const POSITION: u32 = 2;
    /// No object.
    pub const NONE: Self = Self {
        header: Header::of::<Self>(),
        flags: 0,
        entity: Entity::from_engine(0),
        position: [0.0; 3],
    };

    /// Whether a loader takes this target: only the flags above, and a
    /// finite position where [`ActionTarget::POSITION`] is set. The client,
    /// the server and the fake loader hold every target to it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ActionTarget;
    ///
    /// let unknown = enshwick::abi::headed!(ActionTarget { flags: ActionTarget::ENTITY | 8, ..ActionTarget::NONE });
    /// assert!(!unknown.is_valid());
    /// let entity = enshwick::abi::headed!(ActionTarget { flags: ActionTarget::ENTITY, position: [f64::NAN; 3], ..ActionTarget::NONE });
    /// assert!(entity.is_valid(), "an unset position is not read");
    /// ```
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.flags & !(Self::ENTITY | Self::POSITION) == 0
            && (self.flags & Self::POSITION == 0 || self.position.iter().all(|c| c.is_finite()))
    }

    /// Reads the target a mod passes with a run, as the client loader and the
    /// fake loader take it: `Ok(None)` for null or no flags, and
    /// [`Status::BAD_ARGS`] for a header too short or a target that is not
    /// [valid](Self::is_valid).
    ///
    /// # Errors
    ///
    /// [`Status::BAD_ARGS`] for a target the loader refuses.
    ///
    /// # Safety
    ///
    /// - `raw` is null, or points at an `ActionTarget` readable for the size
    ///   its header states, which no other thread writes during the call.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ActionTarget, Status};
    ///
    /// let mut short = enshwick::abi::headed!(ActionTarget { flags: ActionTarget::ENTITY, ..ActionTarget::NONE });
    /// short.header.size = 8;
    /// // SAFETY: `short` is a whole local `ActionTarget`.
    /// assert_eq!(unsafe { ActionTarget::read(&raw const short) }, Err(Status::BAD_ARGS));
    /// // SAFETY: null is allowed.
    /// assert_eq!(unsafe { ActionTarget::read(core::ptr::null()) }, Ok(None));
    /// ```
    pub unsafe fn read(raw: *const Self) -> Result<Option<Self>, Status> {
        if raw.is_null() {
            return Ok(None);
        }
        // SAFETY: the caller's contract, for the size the header states.
        let target = unsafe { super::read_headed(raw, <Self as super::Headed>::BORN) }
            .filter(Self::is_valid)
            .ok_or(Status::BAD_ARGS)?;
        Ok(Some(target).filter(|t| t.flags != 0))
    }
}

impl ActionCall {
    /// The ids of the actions the player may run now, as the mod's rights
    /// for them say, one per line in the reply text; the mod looks for no
    /// subject and builds no label or list. The loader asks it to choose
    /// the carried controls a player gets, at their join and when their
    /// permissions or the mod's rules change.
    pub const ALLOWED: u32 = 0;
    /// The player runs an action from the interface: the answer, whether it
    /// was done and the text the player reads, goes in
    /// [`ActionCall::reply`].
    pub const RUN: u32 = 1;
    /// The server's console runs an action from its text form, such as
    /// `access Ann on`, in `action`: the action's id and its values as
    /// words; `help` or nothing lists the actions. `player` is the console,
    /// an administrator with no character. The reply is as for
    /// [`ActionCall::RUN`].
    pub const CONSOLE: u32 = 3;
    /// A player runs a command from chat, `/<name> ` and then what
    /// [`ActionCall::CONSOLE`] takes in `action`; `player` is the sender and
    /// `language` the language they read. The reply is as for
    /// [`ActionCall::RUN`].
    pub const CHAT: u32 = 4;
    /// A mod runs a command line for a remote caller, such as a remote
    /// console, as [`ActionCall::CHAT`] does; `player` holds the caller's
    /// label as its name, its permission bits, key 0 and no character.
    pub const REMOTE: u32 = 5;
    /// The window of the calling mod's that `action` names, as `player`
    /// sees it in `language`, after
    /// [`HostApi::action_window`](super::HostApi::action_window) showed it:
    /// the reply is the window's text, empty when the mod keeps no such
    /// window for them or the player may not run an action it names.
    pub const WINDOW: u32 = 6;
    /// The client of the player whose account key `player` holds closed
    /// the calling mod's window `action`, let it go for a newer one, or
    /// closed it for an empty or oversized answer to
    /// [`ActionCall::WINDOW`]; or, with an empty `action`, that
    /// player left, and holds none of the mod's windows any more. The mod
    /// forgets what it kept for them. `player` holds the account key alone:
    /// no character, name or permission. No reply is read.
    pub const WINDOW_CLOSED: u32 = 7;
    /// The loader refused a run of one of the mod's actions before asking
    /// the mod, and answered the caller itself: `via` names the question the
    /// run would have been, `reason` why, and `action`, `args` and `word`
    /// hold what that question would have held. `player` is the caller, or
    /// the account key alone for one no longer in the world. No reply is
    /// read.
    pub const REFUSED: u32 = 8;
    /// [`ActionCall::reason`]: the caller is over the loader's rate of runs.
    pub const REFUSED_RATE: u32 = 1;
    /// [`ActionCall::reason`]: the run's arguments are out of the loader's
    /// bounds, which only a changed client sends.
    pub const REFUSED_BOUNDS: u32 = 2;
    /// [`ActionCall::reason`]: the runs waiting for the end of the step fill
    /// the loader's queue.
    pub const REFUSED_BUSY: u32 = 3;
    /// [`ActionCall::reason`]: the object the run names stands out of the
    /// caller's reach.
    pub const REFUSED_REACH: u32 = 4;
}

/// A mod's answer to the loader's questions about its player actions: the
/// call, the pointer given at registration and where to write the reply,
/// which stays valid until the callback runs again.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the server's game thread for a player, and on the thread that runs
/// the line for the console or a remote caller.
///
/// # Examples
///
/// The safe layer wraps it in `Load::actions`, behind the `unstable` feature.
pub type ActionFn =
    unsafe extern "system" fn(call: *const ActionCall, user: *mut c_void, reply: *mut Str);

/// A mod's status source: the pointer given at registration and where to
/// write the mod's lines for the loader's status summary. The text stays
/// valid until the source runs again; empty for none.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread that asks for the loader's status summary, such as the
/// one running a console line.
///
/// # Examples
///
/// The safe layer wraps it in [`Load::add_status`](../struct.Load.html#method.add_status).
pub type StatusFn = unsafe extern "system" fn(user: *mut c_void, out: *mut Str);

/// A player renaming an entity, handed to the filters registered with
/// [`HostApi::rename_filter`](super::HostApi::rename_filter) before the game applies it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, RenameCall, Str};
///
/// let call = enshwick::abi::headed!(RenameCall {
///     entity: enshwick::Entity::from_engine(4096),
///     player: core::ptr::null(),
///     text: Str::new("Tools"),
/// });
/// assert!(call.player.is_null());
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct RenameCall {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity being renamed.
    pub entity: Entity,
    /// The renaming player, valid for the call; null when the loader could
    /// not tell who sent the rename.
    pub player: *const PlayerInfo,
    /// The new name as the client sent it; valid for the call.
    pub text: Str,
}

impl_headed!(RenameCall);

/// A mod's rename filter: the call and the pointer given at registration;
/// returns [`FILTER_REFUSE`](super::FILTER_REFUSE) to refuse the rename, else [`FILTER_PASS`](super::FILTER_PASS).
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
/// Runs on the thread the game changes the name on.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_rename`, behind the `unstable` feature.
pub type RenameFilterFn =
    unsafe extern "system" fn(call: *const RenameCall, user: *mut c_void) -> u8;
