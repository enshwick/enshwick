//! Player actions and renames the loader asks a mod to answer.

use core::ffi::c_void;

use crate::abi::{Entity, Header, PlayerInfo, Status, Str};

/// A question the loader asks a mod about its player actions, registered
/// with [`HostApi::actions`](super::HostApi::actions).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionCall, Header, PlayerInfo, Str};
///
/// let call = ActionCall {
///     header: Header::of::<ActionCall>(),
///     kind: ActionCall::RUN,
///     language: 0,
///     player: PlayerInfo {
///         header: Header::of::<PlayerInfo>(),
///         slot: 0,
///         entity: enshwick::Entity(2),
///         key: enshwick::PlayerKey::new(7),
///         permissions: 0,
///         name_len: 0,
///         name: [0; 64],
///     },
///     action: Str::new("access"),
///     args: Str::new("p0000000000000009\tb1"),
///     target: enshwick::abi::ActionTarget::NONE,
///     reply: core::ptr::null_mut(),
///     word: Str::EMPTY,
/// };
/// assert_eq!(call.kind, ActionCall::RUN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ActionCall {
    /// Size and version of this struct.
    pub header: Header,
    /// What the loader asks: [`ActionCall::SHEET`], [`ActionCall::RUN`],
    /// [`ActionCall::CONSOLE`], [`ActionCall::CHAT`] or
    /// [`ActionCall::REMOTE`]; a mod built for an older minor answers a kind
    /// it does not know with an empty reply.
    pub kind: u32,
    /// The language the player reads the game in, a
    /// [`Language`](crate::i18n::Language) value.
    pub language: u32,
    /// The player the question is about.
    pub player: PlayerInfo,
    /// The action's id, for [`ActionCall::RUN`]; the console line after the
    /// mod's id, for [`ActionCall::CONSOLE`]; the line after the command's
    /// name or alias, for [`ActionCall::CHAT`] and [`ActionCall::REMOTE`];
    /// valid for the call.
    pub action: Str,
    /// The action's arguments in the sheet's encoding, for
    /// [`ActionCall::RUN`]; valid for the call.
    pub args: Str,
    /// The object the player's client named for [`ActionCall::RUN`]; no
    /// flags for none, and for the other questions.
    pub target: ActionTarget,
    /// Where the mod writes its answer to every question but
    /// [`ActionCall::SHEET`], a zeroed [`ActionReply`] the loader owns for
    /// the call; null for [`ActionCall::SHEET`]. A mod writes the whole
    /// struct, its header first, and leaves the callback's `reply` text
    /// untouched. A mod built before ABI 11.19 does not see this field and
    /// answers in the `reply` text as `1` or `0` and then the text, which
    /// the loader reads only while this struct's header stays zero. ABI
    /// 11.19.
    pub reply: *mut ActionReply,
    /// The word that named the mod in the line, in lower case and without
    /// its `/`: the mod's command name, alias or id, for
    /// [`ActionCall::CONSOLE`], [`ActionCall::CHAT`] and
    /// [`ActionCall::REMOTE`], so the mod's help names the command as the
    /// caller typed it; empty for the other questions. Valid for the call.
    /// ABI 11.23.
    pub word: Str,
}

impl_headed!(ActionCall, born = 144);

/// A mod's answer to a run of one of its actions: whether it was done and
/// the text the caller reads.
///
/// A mod writes it through [`ActionCall::reply`], and
/// [`HostApi::run_command`](super::HostApi::run_command) writes one for its
/// caller. ABI 11.19.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionReply, Header, Str};
///
/// let reply = ActionReply { header: Header::of::<ActionReply>(), done: 1, text: Str::new("2 online") };
/// assert_eq!(reply.done, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
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

/// A running mod's command names, from
/// [`HostApi::command_list`](super::HostApi::command_list). ABI 11.19.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{CommandInfo, Header, Str};
///
/// let tools = CommandInfo {
///     header: Header::of::<CommandInfo>(),
///     name: Str::new("admin-tools"),
///     alias: Str::new("adm"),
///     mod_id: Str::new("you.admin-tools"),
/// };
/// assert_eq!(tools.header.size, Header::of::<CommandInfo>().size);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
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
/// [`HostApi::run_action_at`](super::HostApi::run_action_at) and handed back
/// to the mod in [`ActionCall::target`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ActionTarget, Header};
///
/// let chest = ActionTarget {
///     header: Header::of::<ActionTarget>(),
///     flags: ActionTarget::POSITION,
///     entity: enshwick::Entity(0),
///     position: [10.0, 2.0, -4.5],
/// };
/// assert_eq!(chest.flags & ActionTarget::ENTITY, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
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
    pub position: [f64; 3],
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
        entity: Entity(0),
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
    /// let unknown = ActionTarget { flags: ActionTarget::ENTITY | 8, ..ActionTarget::NONE };
    /// assert!(!unknown.is_valid());
    /// let entity = ActionTarget { flags: ActionTarget::ENTITY, position: [f64::NAN; 3], ..ActionTarget::NONE };
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
    /// let mut short = ActionTarget { flags: ActionTarget::ENTITY, ..ActionTarget::NONE };
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
    /// The actions the player may use now, as a sheet: the reply is the
    /// sheet's text.
    pub const SHEET: u32 = 0;
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
    /// [`ActionCall::RUN`]. ABI 11.19.
    pub const CHAT: u32 = 4;
    /// A mod runs a command line for a remote caller, such as a remote
    /// console, as [`ActionCall::CHAT`] does; `player` holds the caller's
    /// label as its name, its permission bits, key 0 and no character. ABI
    /// 11.19.
    pub const REMOTE: u32 = 5;
    /// The window of the calling mod's that `action` names, as `player`
    /// sees it in `language`, after
    /// [`HostApi::action_window`](super::HostApi::action_window) showed it:
    /// the reply is the window's text, empty when the mod keeps no such
    /// window for them or the player may not run an action it names. ABI
    /// 11.22.
    pub const WINDOW: u32 = 6;
}

/// A mod's answer to the loader's questions about its player actions: the
/// call, the pointer given at registration and where to write the reply,
/// which stays valid until the callback runs again.
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
/// # Examples
///
/// The safe layer wraps it in [`Load::add_status`](crate::Load::add_status).
pub type StatusFn = unsafe extern "system" fn(user: *mut c_void, out: *mut Str);

/// A player renaming an entity, handed to the filters registered with
/// [`HostApi::rename_filter`](super::HostApi::rename_filter) before the game applies it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PlayerInfo, RenameCall, Str};
///
/// let call = RenameCall {
///     header: Header::of::<RenameCall>(),
///     entity: enshwick::Entity(4096),
///     known: 0,
///     player: PlayerInfo {
///         header: Header::of::<PlayerInfo>(),
///         slot: 0,
///         entity: enshwick::Entity(0),
///         key: enshwick::PlayerKey::new(0),
///         permissions: 0,
///         name_len: 0,
///         name: [0; 64],
///     },
///     text: Str::new("Tools"),
/// };
/// assert_eq!(call.known, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct RenameCall {
    /// Size and version of this struct.
    pub header: Header,
    /// The entity being renamed.
    pub entity: Entity,
    /// 1 when `player` holds the renaming player, 0 when the loader could
    /// not tell who sent the rename.
    pub known: u8,
    /// The renaming player, when `known` is 1.
    pub player: PlayerInfo,
    /// The new name as the client sent it; valid for the call.
    pub text: Str,
}

impl_headed!(RenameCall);

/// A mod's rename filter: the call and the pointer given at registration;
/// returns 1 to let the rename through, 0 to refuse it.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_rename`, behind the `unstable` feature.
pub type RenameCallFn = unsafe extern "system" fn(call: *const RenameCall, user: *mut c_void) -> u8;
