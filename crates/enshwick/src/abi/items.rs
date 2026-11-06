//! What a player does with an item, handed to item filters.

use core::ffi::c_void;

use crate::abi::{Entity, Header, ItemId};

/// A player's action with an item, handed to the filters registered with
/// [`HostApi::item_filter`](super::HostApi::item_filter) in the server's
/// world.
///
/// The name is the game's `ItemAction`; the kinds are its `ItemActionType`,
/// `ChangeEquipment` and the events that follow a pickup or a placement,
/// and [`CONSUMED`](Self::CONSUMED) the loader's word that the game took the
/// item of a use every filter allowed.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, ItemAction};
///
/// let call = ItemAction {
///     header: Header::of::<ItemAction>(),
///     kind: ItemAction::CONSUME,
///     player: enshwick::Entity(4096),
///     item: enshwick::ItemId::new(0x5e01),
///     count: 1,
///     inventory: enshwick::Entity(4100),
///     slot: 3,
/// };
/// assert_eq!(call.kind, ItemAction::CONSUME);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct ItemAction {
    /// Size and version of this struct.
    pub header: Header,
    /// What the player does; one of the constants on this type.
    pub kind: u32,
    /// The player's character entity.
    pub player: Entity,
    /// The item.
    pub item: ItemId,
    /// How many: 1 for a use, the stack for the others.
    pub count: u32,
    /// The inventory that holds the item's slot, 0 when none does.
    pub inventory: Entity,
    /// The slot's index in `inventory`.
    pub slot: u32,
}

impl_headed!(ItemAction);

impl ItemAction {
    /// The player eats, drinks or uses the item from the inventory; a
    /// refusal leaves it in the slot.
    pub const CONSUME: u32 = 1;
    /// The player throws the stack away from the inventory; a refusal
    /// keeps it.
    pub const TRASH: u32 = 2;
    /// The player equips the item; a refusal keeps the equipment as it was.
    pub const EQUIP: u32 = 3;
    /// The player takes the item off; a refusal keeps it on.
    pub const UNEQUIP: u32 = 4;
    /// The player picked the item up, after the game moved it; the answer
    /// is ignored.
    pub const PICKUP: u32 = 5;
    /// The player placed the item as a building piece or a prop, after the
    /// game placed it; the answer is ignored.
    pub const PLACE: u32 = 6;
    /// The game took the item of a [`CONSUME`](Self::CONSUME) every filter
    /// allowed from its slot, at a later step; a use the game refuses, or
    /// whose player moves a stack first, raises none. The answer is ignored.
    /// ABI 11.7.
    pub const CONSUMED: u32 = 7;
}

/// A mod's item filter: the action and the pointer given at registration;
/// returns 0 to refuse an action that can be refused.
///
/// The game handles a refused action as one it already ran, so nothing
/// moves and nothing is lost.
///
/// # Examples
///
/// The safe layer wraps it in `Load::on_item_action`, with the `unstable`
/// feature.
pub type ItemActionFn = unsafe extern "system" fn(call: *const ItemAction, user: *mut c_void) -> u8;
