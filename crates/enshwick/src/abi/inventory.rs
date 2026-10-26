//! Inventory requests and their results, and item data.

use core::ffi::c_void;

use crate::abi::Header;

/// One operation of an inventory transaction, in [`InventoryRequest::ops`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::InventoryOp;
///
/// // Everything from a chest (entity 40) to a player (entity 2).
/// let op = InventoryOp {
///     kind: InventoryOp::MOVE_ALL,
///     flags: 0,
///     target: 40,
///     other: 2,
///     item: 0,
///     count: 0,
/// };
/// assert_eq!(op.kind, 5);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InventoryOp {
    /// What to do: one of the constants on this type.
    pub kind: u32,
    /// [`InventoryOp::INCLUDE_SPECIAL`] or none.
    pub flags: u32,
    /// The inventory entity, or for slot operations the slot reference
    /// (`sub-inventory | index << 32`).
    pub target: u64,
    /// The destination inventory entity of a move; for a give with
    /// [`InventoryOp::FILLED`], the bits of the `f32` fill each new item's
    /// meter starts with; 0 otherwise.
    pub other: u64,
    /// The item id of a give or take.
    pub item: u32,
    /// How many to give or take; for a slot move, how many to move, 0 for the
    /// whole stack.
    pub count: u32,
}

impl InventoryOp {
    /// Adds `count` new items `item` to the inventory `target`.
    pub const GIVE: u32 = 1;
    /// Removes `count` items `item` from the inventory `target`.
    pub const TAKE: u32 = 2;
    /// Destroys the stack in slot `target`.
    pub const DESTROY_SLOT: u32 = 3;
    /// Moves the stack in slot `target`, or `count` of it, into the inventory
    /// `other`.
    pub const MOVE_SLOT: u32 = 4;
    /// Moves every stack of the inventory `target` into the inventory `other`.
    pub const MOVE_ALL: u32 = 5;
    /// For [`InventoryOp::MOVE_ALL`]: also move the source's action bar,
    /// equipment and locked slots.
    pub const INCLUDE_SPECIAL: u32 = 1;
    /// Replaces the stack in slot `target` with `count` new items `item`,
    /// each with its own entity, given to the inventory in `other`'s low
    /// half and asked for the same slot; `other`'s high half holds the
    /// `f32` fill of their meter. Since minor 42.
    pub const RENEW: u32 = 6;
    /// For [`InventoryOp::GIVE`]: `other` holds the fill of each new item's
    /// meter, the game's own per-item fuel fill. Since minor 37.
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
/// let ops = [InventoryOp { kind: InventoryOp::GIVE, target: 2, item: 0x1234, count: 5, ..Default::default() }];
/// let request = InventoryRequest {
///     header: Header::of::<InventoryRequest>(),
///     flags: InventoryRequest::NOTIFY,
///     actor: 2,
///     ops: ops.as_ptr(),
///     op_count: 1,
///     reserved: 0,
/// };
/// assert_eq!(request.op_count, 1);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct InventoryRequest {
    /// Size and version of this struct.
    pub header: Header,
    /// [`InventoryRequest::ALLOW_PARTIAL`], [`InventoryRequest::NOTIFY`].
    pub flags: u32,
    /// The player the game credits with the change, for pickup and
    /// inventory-full messages; 0 for whichever player the game is serving.
    pub actor: u32,
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
pub type InventoryResultFn =
    unsafe extern "system" fn(result: *const InventoryResult, user: *mut c_void);

/// An item's static data, from [`HostApi::item_info`](super::HostApi::item_info).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, ItemData};
///
/// let sword = ItemData {
///     header: Header::of::<ItemData>(),
///     id: 7,
///     max_stack: 1,
///     category: 2,
///     rarity: 0,
///     flags: ItemData::HAS_PIDE,
///     placed_entity: [0; 16],
/// };
/// assert_ne!(sword.flags & ItemData::HAS_PIDE, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ItemData {
    /// Size and version of this struct.
    pub header: Header,
    /// The item id: FNV-1a of the item's GUID.
    pub id: u32,
    /// How many fit one stack.
    pub max_stack: u16,
    /// The game's `ItemCategory`: 1 equipment, 2 weapons, 3 tools and so
    /// on. Since minor 48; zero before.
    pub category: u8,
    /// The game's `ItemRarity` of the type. Since minor 48; zero before.
    pub rarity: u8,
    /// `ItemInfo` flags: [`ItemData::HAS_PIDE`] and the others on this type.
    pub flags: u32,
    /// The GUID of the template a player places from the item, its
    /// `equipment.placedEntity`, in file order; all zero for an item that
    /// places nothing. Since minor 88; zero before.
    pub placed_entity: [u8; 16],
}

impl_headed!(ItemData);

impl ItemData {
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
