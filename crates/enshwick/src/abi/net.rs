//! Messages between a mod's server and client parts.

use core::ffi::c_void;

use crate::abi::{Header, PlayerKey};

/// A message on a mod's channel, handed to the handler registered with
/// [`HostApi::net_channel`](super::HostApi::net_channel).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, NetMessage};
///
/// let bytes = [1u8, 2, 3];
/// let message = NetMessage {
///     header: Header::of::<NetMessage>(),
///     channel: 0x1234,
///     reserved: 0,
///     peer: NetMessage::SERVER,
///     data: bytes.as_ptr(),
///     size: 3,
///     reserved_tail: 0,
/// };
/// assert_eq!(message.peer, NetMessage::SERVER);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct NetMessage {
    /// Size and version of this struct.
    pub header: Header,
    /// The channel, as [`HostApi::net_channel`](super::HostApi::net_channel) wrote it.
    pub channel: u32,
    /// Zero.
    pub reserved: u32,
    /// Who sent it: a player's account key on the server,
    /// [`NetMessage::SERVER`] on a client.
    pub peer: PlayerKey,
    /// The message, valid for the call.
    pub data: *const u8,
    /// How many bytes `data` holds.
    pub size: u32,
    /// Zero.
    pub reserved_tail: u32,
}

impl_headed!(NetMessage);

impl NetMessage {
    /// The server, as a peer: what a client hears from and sends to.
    pub const SERVER: PlayerKey = PlayerKey(u64::MAX);
}

/// A mod's handler for messages on its channel: the message and the pointer
/// given at registration. It must not unwind.
pub type NetMessageFn = unsafe extern "system" fn(message: *const NetMessage, user: *mut c_void);
