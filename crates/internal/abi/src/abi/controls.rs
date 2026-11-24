//! Controls a mod declares: keys the player presses and the game actions
//! they follow.

use core::ffi::c_void;

use crate::abi::{Header, PlayerInfo, Str};

/// A control a mod declares with [`HostApi::control`](super::HostApi::control)
/// during `enshwick_mod_init`.
///
/// A key is the key's scan code in
/// bits 0 to 9 (bit 8 for an extended key, bit 9 for a mouse button) and the
/// modifiers that must be down with it in bits 16 to 18.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ControlSpec, Header, Str};
///
/// let spec = ControlSpec {
///     header: Header::of::<ControlSpec>(),
///     name: Str::new("horn"),
///     label: Str::new("Sound the horn"),
///     key: 0x23,
///     second: 0,
///     link: Str::EMPTY,
///     fires: ControlSpec::PRESS,
///     hold_ms: 0,
///     action: Str::EMPTY,
/// };
/// assert_eq!(spec.fires, ControlSpec::PRESS);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ControlSpec {
    /// Size and version of this struct.
    pub header: Header,
    /// The control's name in its mod, 1 to 32 of `[a-z0-9-]`; the player's
    /// override names it `<mod id>:<name>`. Valid for the call.
    pub name: Str,
    /// What the mod's key bindings show for it, in the player's language. Valid
    /// for the call; the loader copies it.
    pub label: Str,
    /// The default key, or 0 for none.
    pub key: u32,
    /// A second default key, or 0 for none.
    pub second: u32,
    /// The game action the control follows, by the name the game's input
    /// gives it, such as `ContextualAction`; empty for none. Valid for the
    /// call.
    pub link: Str,
    /// When the control fires: [`ControlSpec::PRESS`],
    /// [`ControlSpec::RELEASE`] and [`ControlSpec::HOLD`], at least one of
    /// them and no other bit.
    pub fires: u32,
    /// How long a key stays down before [`ControlSpec::HOLD`] fires, in
    /// milliseconds.
    pub hold_ms: u32,
    /// The id of the mod's own action the server runs, for the player, each
    /// time the control fires on a client; empty for none. A mod of side
    /// server needs it, since its controls reach the clients without its
    /// code; then [`label`](Self::label) is a key of its texts. Valid for the
    /// call.
    pub action: Str,
}

impl_headed!(ControlSpec);

impl ControlSpec {
    /// The control fires once when its key goes down.
    pub const PRESS: u32 = 1;
    /// The control fires once when its key comes up.
    pub const RELEASE: u32 = 2;
    /// The control fires once when its key stayed down for
    /// [`ControlSpec::hold_ms`].
    pub const HOLD: u32 = 4;
}

/// A control firing, handed to the mod's [`ControlFn`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ControlEvent, ControlSpec, Header, Str};
///
/// let event = ControlEvent {
///     header: Header::of::<ControlEvent>(),
///     name: Str::new("horn"),
///     kind: ControlSpec::PRESS,
///     server: 0,
///     reserved: [0; 3],
///     player: core::ptr::null(),
/// };
/// assert_eq!(event.server, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ControlEvent {
    /// Size and version of this struct.
    pub header: Header,
    /// The control's name in its mod; valid for the call.
    pub name: Str,
    /// What happened: [`ControlSpec::PRESS`], [`ControlSpec::RELEASE`] or
    /// [`ControlSpec::HOLD`].
    pub kind: u32,
    /// 1 when the server fires a control that follows a game action, for
    /// `player`; 0 on the client, for its own player.
    pub server: u8,
    /// Zero.
    pub reserved: [u8; 3],
    /// The player whose game action fired the control, valid for the call,
    /// when `server` is 1; null on the client.
    pub player: *const PlayerInfo,
}

impl_headed!(ControlEvent);

/// A mod's control callback: the event and the pointer given with
/// [`HostApi::control`](super::HostApi::control).
///
/// The client calls it on its interface thread for a key, the server on its
/// game thread for a game action; a client hosting a game does both, so a
/// control with a key and a game action may run on the two threads at once.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
///
/// # Examples
///
/// The safe layer wraps it in `Load::control`, behind the `unstable`
/// feature.
pub type ControlFn = unsafe extern "system" fn(event: *const ControlEvent, user: *mut c_void);

/// The keys a game control holds in the client's keyboard and mouse
/// bindings, as [`HostApi::game_binding`](super::HostApi::game_binding)
/// writes them. ABI 12.20.
///
/// Each key is in the form of [`ControlSpec::key`], 0 for none. A key a
/// mod control may not take, such as Shift alone or Escape, keeps its scan
/// code; input that is no key, such as the mouse wheel, is
/// [`GameBinding::OTHER`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::GameBinding;
///
/// let unbound = GameBinding::EMPTY;
/// assert_eq!((unbound.first, unbound.second), (0, 0));
/// assert_ne!(GameBinding::OTHER, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct GameBinding {
    /// Size and version of this struct.
    pub header: Header,
    /// The first key.
    pub first: u32,
    /// The second key.
    pub second: u32,
}

impl_headed!(GameBinding);

impl GameBinding {
    /// A slot that holds input the key form does not name: the mouse
    /// wheel, a gamepad, or a key without a name.
    pub const OTHER: u32 = u32::MAX;

    /// Its header set and no key, the value a read starts from.
    ///
    /// # Examples
    ///
    /// See [`GameBinding`].
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        first: 0,
        second: 0,
    };
}
