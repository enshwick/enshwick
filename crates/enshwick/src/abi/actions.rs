//! Player actions and renames the loader asks a mod to answer.

use core::ffi::c_void;

use crate::abi::{Entity, Header, PlayerInfo, Str};

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
/// };
/// assert_eq!(call.kind, ActionCall::RUN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ActionCall {
    /// Size and version of this struct.
    pub header: Header,
    /// What the loader asks: [`ActionCall::SHEET`], [`ActionCall::RUN`],
    /// [`ActionCall::HINTS`] or [`ActionCall::CONSOLE`].
    pub kind: u32,
    /// The language the player reads the game in, a
    /// [`Language`](crate::i18n::Language) value.
    pub language: u32,
    /// The player the question is about.
    pub player: PlayerInfo,
    /// The action's id, for [`ActionCall::RUN`]; the console line after the
    /// mod's id, for [`ActionCall::CONSOLE`]; valid for the call.
    pub action: Str,
    /// The action's arguments in the sheet's encoding, for
    /// [`ActionCall::RUN`]; valid for the call.
    pub args: Str,
}

impl_headed!(ActionCall);

impl ActionCall {
    /// The actions the player may use now, as a sheet: the reply is the
    /// sheet's text.
    pub const SHEET: u32 = 0;
    /// The player runs an action from the interface: the reply is `1` or
    /// `0`, whether it was done, and the text the player reads.
    pub const RUN: u32 = 1;
    /// The actions with a hint that the player has at hand now: a line
    /// `<id><TAB><label><TAB><subject>` each.
    pub const HINTS: u32 = 2;
    /// The server's console runs an action from its text form, such as
    /// `access Ann on`, in `action`: the action's id and its values as
    /// words; `help` or nothing lists the actions. `player` is the console,
    /// an administrator with no character. The reply is as for
    /// [`ActionCall::RUN`].
    pub const CONSOLE: u32 = 3;
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
