//! Writes mods ask the server for: a player's level and experience, and items dropped on the ground.

use crate::abi::{Header, ItemId};

/// A request to drop a stack of new items on the ground, for
/// [`HostApi::drop_items`](super::HostApi::drop_items). ABI 11.24.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{DropRequest, Header};
///
/// let logs = DropRequest {
///     header: Header::of::<DropRequest>(),
///     item: enshwick::ItemId(0x2a),
///     count: 5,
///     position: [100 << 32, 40 << 32, 0],
///     level: 0,
///     rarity: 0,
/// };
/// assert_eq!(logs.count, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct DropRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// The item.
    pub item: ItemId,
    /// How many, 1 to 65535: one stack lies as one pickup.
    pub count: u32,
    /// Where, in 32.32 fixed-point metres.
    pub position: [i64; 3],
    /// The item's level, as the game levels weapons and armour; 0 for none.
    pub level: u32,
    /// The game's `ItemRarity` of the item, 0 for common.
    pub rarity: u32,
}

impl_headed!(DropRequest);

/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that asks for the writes, during `init`. ABI 11.24.
pub const PROGRESS_OPEN: u32 = 0;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that sets the level. ABI 11.24.
pub const PROGRESS_SET_LEVEL: u32 = 1;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that sets the experience inside the level. ABI 11.24.
pub const PROGRESS_SET_EXPERIENCE: u32 = 2;
/// The `what` of [`HostApi::progress_write`](super::HostApi::progress_write)
/// that adds experience for the game to apply. ABI 11.24.
pub const PROGRESS_ADD_EXPERIENCE: u32 = 3;
