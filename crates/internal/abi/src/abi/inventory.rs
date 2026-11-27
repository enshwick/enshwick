//! Inventory requests and their results, and item data.

use core::ffi::c_void;

use crate::abi::{Entity, Header, ItemId};
use crate::content::Guid;

/// One operation of an inventory transaction, in [`InventoryRequest::ops`].
///
/// Frozen, with no header: [`InventoryRequest`] reads the operations as an
/// array; a kind that needs more than these fields is a new entry.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::abi::InventoryOp;
///
/// // Everything from a chest (entity 40) to a player (entity 2).
/// let op = InventoryOp {
///     kind: InventoryOp::MOVE_ALL,
///     target: Entity::from_raw(40).ok_or("no entity")?,
///     other: Entity::from_raw(2).ok_or("no entity")?,
///     ..InventoryOp::default()
/// };
/// assert_eq!(op.kind, 5);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
pub struct InventoryOp {
    /// What to do: one of the constants on this type.
    pub kind: u32,
    /// [`InventoryOp::INCLUDE_SPECIAL`], [`InventoryOp::FILLED`] or none.
    pub flags: u32,
    /// The inventory entity; for slot operations, the sub-inventory that
    /// holds the slot.
    pub target: Entity,
    /// For slot operations, the slot's index in `target`; 0 otherwise.
    pub slot: u32,
    /// The destination inventory of a move, or the inventory a renew gives
    /// to; entity 0 otherwise.
    pub other: Entity,
    /// The fill, 0 to 1, each new item's meter starts with, for a give with
    /// [`InventoryOp::FILLED`] and for a renew; 0 otherwise.
    pub fill: f32,
    /// The item id of a give, take or renew.
    pub item: ItemId,
    /// How many to give or take; for a slot move, how many to move, 0 for the
    /// whole stack.
    pub count: u32,
}

// Entity 0, the ABI's "none", in the fields an op of this kind leaves unread.
impl Default for InventoryOp {
    fn default() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

impl InventoryOp {
    /// Adds `count` new items `item` to the inventory `target`.
    pub const GIVE: u32 = 1;
    /// Removes `count` items `item` from the inventory `target`.
    pub const TAKE: u32 = 2;
    /// Destroys the stack in slot `slot` of `target`.
    pub const DESTROY_SLOT: u32 = 3;
    /// Moves the stack in slot `slot` of `target`, or `count` of it, into the
    /// inventory `other`.
    pub const MOVE_SLOT: u32 = 4;
    /// Moves every stack of the inventory `target` into the inventory `other`.
    pub const MOVE_ALL: u32 = 5;
    /// For [`InventoryOp::MOVE_ALL`]: also move the source's action bar,
    /// equipment and locked slots.
    pub const INCLUDE_SPECIAL: u32 = 1;
    /// Replaces the stack in slot `slot` of `target` with `count` new items
    /// `item`, each with its own entity and its meter at `fill`, given to
    /// the inventory `other` and asked for the same slot.
    pub const RENEW: u32 = 6;
    /// For [`InventoryOp::GIVE`]: each new item's meter starts at `fill`,
    /// the game's own per-item fuel fill.
    pub const FILLED: u32 = 2;
}

/// Inventory operations to run in one transaction, for
/// [`HostApi::inventory_submit`](super::HostApi::inventory_submit).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, InventoryOp, InventoryRequest};
///
/// let ops = [InventoryOp { kind: InventoryOp::GIVE, target: enshwick::Entity::from_raw(2).ok_or("no entity")?, item: enshwick::ItemId::new(0x1234), count: 5, ..Default::default() }];
/// let request = enshwick::abi::headed!(InventoryRequest {
///     flags: InventoryRequest::NOTIFY,
///     actor: enshwick::Entity::from_raw(2).ok_or("no entity")?,
///     ops: ops.as_ptr(),
///     op_count: 1,
///     reserved: 0,
/// });
/// assert_eq!(request.op_count, 1);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct InventoryRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// [`InventoryRequest::ALLOW_PARTIAL`], [`InventoryRequest::NOTIFY`].
    pub flags: u32,
    /// The player the game credits with the change, for pickup and
    /// inventory-full messages; 0 for whichever player the game is serving.
    pub actor: Entity,
    /// The operations, read during the call.
    pub ops: *const InventoryOp,
    /// How many operations `ops` holds.
    pub op_count: u32,
    /// Zero.
    pub reserved: u32,
}

impl_headed!(InventoryRequest);

impl InventoryRequest {
    /// Commit even when a give or take leaves some of its count undone.
    pub const ALLOW_PARTIAL: u32 = 1;
    /// Show the actor the pickup and inventory-full messages.
    pub const NOTIFY: u32 = 2;
    /// With [`NOTIFY`](Self::NOTIFY), raise the pickup event marked hidden,
    /// so no pickup message shows while the event's readers still hear it.
    pub const HIDE_IN_UI: u32 = 4;
}

/// The outcome of one operation in an [`InventoryResult`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::InventoryOpResult;
///
/// let done = InventoryOpResult { error: 0, remaining: 0 };
/// assert_ne!(done.error, InventoryOpResult::NOT_RUN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
pub struct InventoryOpResult {
    /// The game's error code, 0 for none, or [`InventoryOpResult::NOT_RUN`].
    pub error: u32,
    /// How much of the count the operation could not do.
    pub remaining: u32,
}

impl InventoryOpResult {
    /// The operation did not run: one before it failed.
    pub const NOT_RUN: u32 = 0x100;
}

/// What became of an [`InventoryRequest`], passed to its
/// [`InventoryResultFn`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::InventoryResult;
///
/// assert_ne!(InventoryResult::COMMITTED, InventoryResult::ROLLED_BACK);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct InventoryResult {
    /// Size and version of this struct.
    pub header: Header,
    /// One of the constants on this type.
    pub outcome: u32,
    /// How many results `ops` holds: one per operation, or none when the
    /// request never ran.
    pub op_count: u32,
    /// The operations' results, valid for the call.
    pub ops: *const InventoryOpResult,
}

impl_headed!(InventoryResult);

impl InventoryResult {
    /// Every operation succeeded and the transaction was committed.
    pub const COMMITTED: u32 = 0;
    /// An operation failed or fell short; the transaction was rolled back
    /// and nothing changed.
    pub const ROLLED_BACK: u32 = 1;
    /// The request could not run: this game build lacks a piece, or its hook
    /// was not armed. Nothing changed.
    pub const UNAVAILABLE: u32 = 2;
    /// The game did not run inventories in time, as with nobody online.
    /// Nothing changed.
    pub const TIMED_OUT: u32 = 3;
}

/// Receives the [`InventoryResult`] of a request, on the server's game thread
/// after a simulation step; it must not unwind.
///
/// Null is never a callback: the loader refuses a null one with
/// [`Status::BAD_ARGS`](crate::abi::Status::BAD_ARGS) and keeps nothing.
///
/// # Examples
///
/// The safe layer wraps it in
/// [`Inventories::submit`](../inventory/struct.Inventories.html#method.submit).
pub type InventoryResultFn =
    unsafe extern "system" fn(result: *const InventoryResult, user: *mut c_void);

/// An item's static data, from [`HostApi::item_info`](super::HostApi::item_info).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, ItemTypeInfo};
///
/// let sword = enshwick::abi::headed!(ItemTypeInfo {
///     id: enshwick::ItemId::new(7),
///     max_stack: 1,
///     category: 2,
///     rarity: 0,
///     flags: ItemTypeInfo::HAS_PIDE,
///     placed_entity: enshwick::abi::Guid::NIL,
/// });
/// assert_ne!(sword.flags & ItemTypeInfo::HAS_PIDE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct ItemTypeInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The item id: FNV-1a of the item's GUID.
    pub id: ItemId,
    /// How many fit one stack.
    pub max_stack: u16,
    /// The game's `ItemCategory`: 1 equipment, 2 weapons, 3 tools and so
    /// on.
    pub category: u8,
    /// The game's `ItemRarity` of the type.
    pub rarity: u8,
    /// `ItemInfo` flags: [`ItemTypeInfo::HAS_PIDE`] and the others on this type.
    pub flags: u32,
    /// The GUID of the template a player places from the item, its
    /// `equipment.placedEntity`; all zero for an item that
    /// places nothing.
    pub placed_entity: Guid,
}

impl_headed!(ItemTypeInfo);

impl ItemTypeInfo {
    /// Each item has an entity of its own, with durability or a level.
    pub const HAS_PIDE: u32 = 1 << 1;
    /// The item is a container.
    pub const IS_CONTAINER: u32 = 1 << 2;
    /// The item is a blueprint.
    pub const IS_BLUEPRINT: u32 = 1 << 3;
    /// Each item keeps a count the game shows as a bar on its slot, as the
    /// water pouch keeps its water (the game's fuel).
    pub const USES_FUEL: u32 = 1 << 13;
}
