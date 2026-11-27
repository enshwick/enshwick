//! Transactions: changes to the world a mod writes together and undoes together.

use core::ffi::c_void;

use crate::abi::Metres;
use crate::abi::{Entity, Header, ItemId, Status, Voxel};

/// One change of a transaction, handed to
/// [`HostApi::transaction_commit`](super::HostApi::transaction_commit) in an
/// array: its kind names the record `data` points at.
///
/// Its layout is frozen: the commit reads an array of it with no entry
/// size. A new kind of change gets a new kind, never a new field.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, TerrainEdit, TransactionOp};
///
/// let dig = enshwick::abi::headed!(TerrainEdit {
///     kind: TerrainEdit::DIG,
///     shape: TerrainEdit::ROUND,
///     flags: 0,
///     material: 0,
///     position: [100.0, 40.0, 100.0],
///     radius: 3.0,
///     height: 0.0,
///     protect: [0; 16],
/// });
/// let op = TransactionOp { kind: TransactionOp::TERRAIN, reserved: 0, data: (&raw const dig).cast() };
/// assert_eq!(op.kind, TransactionOp::TERRAIN);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[expect(
    clippy::exhaustive_structs,
    reason = "an ABI row without a header: a field of its own is a new ABI major"
)]
pub struct TransactionOp {
    /// One of the kinds below; the loader refuses a kind it does not know.
    pub kind: u32,
    /// Zero; the loader refuses another value.
    pub reserved: u32,
    /// The change, borrowed for the call: the record its kind names.
    pub data: *const c_void,
}

impl TransactionOp {
    /// A terrain add, dig or flatten: `data` points at a
    /// [`TerrainEdit`](super::TerrainEdit) without
    /// [`TerrainEdit::DRY_RUN`](super::TerrainEdit::DRY_RUN).
    pub const TERRAIN: u32 = 1;
    /// Voxels written over a box: `data` points at a [`VoxelWrite`].
    pub const VOXELS: u32 = 2;
    /// The building voxels of a box cleared to air: `data` points at a
    /// [`VoxelRegion`].
    pub const ERASE: u32 = 3;
    /// A building block placed: `data` points at a [`BlockPlace`].
    pub const BLOCK: u32 = 4;
    /// A prop placed: `data` points at a [`PropPlace`].
    pub const PLACE_PROP: u32 = 5;
    /// A prop removed: `data` points at a [`PropRemove`].
    pub const REMOVE_PROP: u32 = 6;
}

/// A prop a transaction places, as a player places one from an item, for
/// [`TransactionOp::PLACE_PROP`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PropPlace};
///
/// let chest = enshwick::abi::headed!(PropPlace {
///     item: enshwick::ItemId::new(0x1234),
///     reserved: 0,
///     position: [100.0, 40.0, 100.0],
///     orientation: [0.0, 0.0, 0.0, 1.0],
/// });
/// assert_eq!(chest.item, enshwick::ItemId::new(0x1234));
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PropPlace {
    /// Size and version of this struct.
    pub header: Header,
    /// The item whose placed template the prop is.
    pub item: ItemId,
    /// Zero.
    pub reserved: u32,
    /// Where it stands, in metres.
    pub position: Metres,
    /// Which way it faces, a quaternion x, y, z, w.
    pub orientation: [f32; 4],
}

impl_headed!(PropPlace);

/// A prop a transaction removes, for [`TransactionOp::REMOVE_PROP`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PropRemove};
///
/// let gone = enshwick::abi::headed!(PropRemove { entity: enshwick::Entity::from_raw(77).ok_or("no entity")?, flags: 0 });
/// assert_eq!(gone.flags & PropRemove::DISCARD_STATE, 0);
/// # Ok::<(), &str>(())
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PropRemove {
    /// Size and version of this struct.
    pub header: Header,
    /// The prop's entity.
    pub entity: Entity,
    /// [`PropRemove::DISCARD_STATE`] or none.
    pub flags: u32,
}

impl_headed!(PropRemove);

impl PropRemove {
    /// Removes the prop even when it holds state a player would lose (see
    /// [`TransactionInfo::holds`]); without it, such a prop is refused.
    pub const DISCARD_STATE: u32 = 1;
}

/// Voxels a transaction writes over a box, for
/// [`TransactionOp::VOXELS`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Voxel, VoxelWrite};
///
/// let stone = [Voxel { kind: Voxel::BUILDING, material: 0x83, fill: 0, reserved: 0 }; 8];
/// let write = enshwick::abi::headed!(VoxelWrite {
///     min: [200, 80, 200],
///     size: [2, 2, 2],
///     voxels: stone.as_ptr(),
///     count: 8,
/// });
/// assert_eq!(write.count, 8);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct VoxelWrite {
    /// Size and version of this struct.
    pub header: Header,
    /// The box's lowest voxel, in voxels of half a metre from the world's
    /// corner.
    pub min: [u32; 3],
    /// The box's voxels along each axis.
    pub size: [u32; 3],
    /// The box's voxels, x fastest, then y, then z, borrowed for the call;
    /// each kind must be the one its material names.
    pub voxels: *const Voxel,
    /// The voxels at `voxels`: `size[0] * size[1] * size[2]`.
    pub count: u32,
}

impl_headed!(VoxelWrite);

/// A box of voxels, for [`TransactionOp::ERASE`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, VoxelRegion};
///
/// let region = enshwick::abi::headed!(VoxelRegion { min: [200, 80, 200], size: [8, 4, 8] });
/// assert_eq!(region.size[1], 4);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct VoxelRegion {
    /// Size and version of this struct.
    pub header: Header,
    /// The box's lowest voxel, in voxels of half a metre.
    pub min: [u32; 3],
    /// The box's voxels along each axis.
    pub size: [u32; 3],
}

impl_headed!(VoxelRegion);

/// A building block a transaction places, as a player's building tool
/// does, for [`TransactionOp::BLOCK`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{BlockPlace, Header};
///
/// let block = enshwick::abi::headed!(BlockPlace {
///     shape: enshwick::ItemId::new(0x1234),
///     material: enshwick::ItemId::new(0x5678),
///     position: [100.0, 40.0, 100.0],
///     rotation: [0.0, 0.0, 0.0, 1.0],
/// });
/// assert_eq!(block.rotation[3], 1.0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct BlockPlace {
    /// Size and version of this struct.
    pub header: Header,
    /// The shape's item: one with a `VoxelBlueprintItem`.
    pub shape: ItemId,
    /// The material's item: one whose `ChangeVoxelData` places a building
    /// material.
    pub material: ItemId,
    /// Where the block's middle lies, in metres.
    pub position: Metres,
    /// The block's rotation, a quaternion x, y, z, w; the game keeps the
    /// nearest quarter turns.
    pub rotation: [f32; 4],
}

impl_headed!(BlockPlace);

/// Where a transaction stands, written by
/// [`HostApi::transaction_status`](super::HostApi::transaction_status).
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, Status, TransactionInfo};
///
/// let info = enshwick::abi::headed!(TransactionInfo {
///     state: TransactionInfo::APPLIED,
///     reason: Status::OK,
///     ops: 2,
///     applied: 2,
///     bricks: 16,
///     refusal: 0,
///     reserved: 0,
///     holds: 0,
///     created: 0,
/// });
/// assert_eq!(info.applied, info.ops);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct TransactionInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// One of the states below; a reader treats a state it does not know as
    /// [`TransactionInfo::PENDING`].
    pub state: u32,
    /// Why it failed, or why its last undo was refused:
    /// [`Status::NOT_LOADED`] for a part of the world not loaded,
    /// [`Status::OUTSIDE_WORLD`] for a change that leaves the world,
    /// [`Status::REFUSED`] for a region changed since,
    /// [`Status::UNAVAILABLE`] when the world ended or the build lacks a
    /// piece; [`Status::OK`]
    /// otherwise.
    pub reason: Status,
    /// The changes it holds.
    pub ops: u32,
    /// The changes written to the world and not undone yet.
    pub applied: u32,
    /// The 8³-voxel bricks whose earlier state the loader keeps for its
    /// undo, 1 KiB each.
    pub bricks: u64,
    /// When the server refused the player the transaction acts for, its
    /// reason, one of the [`BuildRights`](super::BuildRights) refusals;
    /// [`BuildRights::ALLOWED`](super::BuildRights::ALLOWED) otherwise.
    pub refusal: u32,
    /// Zero.
    pub reserved: u32,
    /// When a prop's state stopped it, or stopped its last undo, the kinds
    /// of state the prop holds, a set of the `HOLDS_` bits; 0 otherwise.
    pub holds: u32,
    /// The props it placed and did not undo, which
    /// [`HostApi::transaction_entities`](super::HostApi::transaction_entities)
    /// lists; kept when its undo data expires, dropped when the world ends.
    pub created: u32,
}

impl_headed!(TransactionInfo);

impl TransactionInfo {
    /// Queued, or written in part: the loader spreads a large one over the
    /// server's steps.
    pub const PENDING: u32 = 0;
    /// Written whole, each prop it placed standing in the world, or still
    /// absent after the loader's wait limit, or with its start state
    /// unreadable; it can be undone.
    pub const APPLIED: u32 = 1;
    /// An undo is queued or written in part.
    pub const UNDOING: u32 = 2;
    /// Undone, or dropped before it wrote anything.
    pub const UNDONE: u32 = 3;
    /// It stopped before or right after a change it could not write or
    /// read back, for `reason`; the changes written stay and can be undone
    /// while `bricks` is above 0. An undo never fails: it is refused.
    pub const FAILED: u32 = 4;
    /// Written whole, or with its undo refused, but the loader dropped what
    /// its undo needs, to stay within its memory cap or because the world
    /// ended; it can no longer be undone.
    pub const EXPIRED: u32 = 5;
    /// The prop's inventory holds items, in any of its linked slots.
    pub const HOLDS_INVENTORY: u32 = 1;
    /// It is a plant (`Growth`): any plant for a removal; for the undo of
    /// a placement, one whose `Level` differs from the level it first
    /// stood with.
    pub const HOLDS_GROWTH: u32 = 1 << 1;
    /// The `FactoryStation` has a recipe running or queued.
    pub const HOLDS_FACTORY_STATION: u32 = 1 << 2;
    /// A survivor is assigned to the `NpcBed`.
    pub const HOLDS_NPC_BED: u32 = 1 << 3;
    /// Survivors are assigned to it: `StoreAssociatedNpcIds`.
    pub const HOLDS_STORE_ASSOCIATED_NPC_IDS: u32 = 1 << 4;
    /// A player wrote its text: `CustomPlayerString`.
    pub const HOLDS_CUSTOM_PLAYER_STRING: u32 = 1 << 5;
    /// It holds more `Fuel` charges than a fresh one starts with, and does
    /// not refill itself, as a well or a rain barrel does.
    pub const HOLDS_FUEL: u32 = 1 << 6;
    /// It is wired to other logic: `LogicGraphNodeConnected`.
    pub const HOLDS_LOGIC_GRAPH_NODE_CONNECTED: u32 = 1 << 7;
    /// It is a logic trigger, whose settings a player may have changed:
    /// `LogicGraphTrigger` or `LogicTriggerShape`. Any trigger for a
    /// removal; for the undo of a placement, one whose settings differ
    /// from those it first stood with.
    pub const HOLDS_LOGIC_GRAPH_TRIGGER: u32 = 1 << 8;
    /// Its last undo stopped for `reason`: before writing anything, or at a
    /// brick changed since or not loaded for the loader's wait limit, with
    /// what it put back kept. The rest stays written and can be undone
    /// again.
    pub const UNDO_REFUSED: u32 = 6;
}

/// A handler of a transaction's end: the transaction's id, where it stands
/// (borrowed for the call, null once the loader no longer remembers it) and
/// the pointer given at registration. It must not unwind.
///
/// [`HostApi::transaction_settled`](super::HostApi::transaction_settled)
/// calls it once, on the server's game thread after a step.
///
/// # Examples
///
/// The safe layer wraps it in `World::when_settled`.
pub type TransactionSettledFn =
    unsafe extern "system" fn(id: u64, info: *const TransactionInfo, user: *mut c_void);
/// An item's prop template and optional local placement box, copied by
/// [`HostApi::prop_placement_info`](crate::abi::HostApi::prop_placement_info).
/// No entity is placed and no permission, collision or persistence is checked.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{Header, PropPlacementInfo};
/// let info = enshwick::abi::headed!(PropPlacementInfo {
///     template: enshwick::content::Guid::NIL,
///     min: [0.0; 3], max: [0.0; 3], flags: 0,
/// });
/// assert_eq!(info.flags & PropPlacementInfo::HAS_LOCAL_BOX, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
#[non_exhaustive]
pub struct PropPlacementInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The template the item places, never `NIL` on success.
    pub template: crate::content::Guid,
    /// The declared local minimum corner in metres; read only with `HAS_LOCAL_BOX`.
    pub min: [f32; 3],
    /// The declared local maximum corner in metres; read only with `HAS_LOCAL_BOX`.
    pub max: [f32; 3],
    /// `HAS_LOCAL_BOX` when the existing resolver supplied finite local corners.
    pub flags: u32,
}

impl_headed!(PropPlacementInfo);

impl PropPlacementInfo {
    /// The resolver supplied local corners, without normalization or a complete
    /// geometry check. Without it, bounds are unknown,
    /// not an empty box or the loader's fallback permission box.
    pub const HAS_LOCAL_BOX: u32 = 1;

    /// Whether declared corners count as a local box: every value finite and
    /// `max` above `min` on at least one axis. The loader and the fake host
    /// share this rule, so a box inverted on some axes still counts and its
    /// corners stay as declared.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::PropPlacementInfo;
    /// assert!(PropPlacementInfo::declares_box([0.0; 3], [0.0, 1.0, 0.0]));
    /// assert!(!PropPlacementInfo::declares_box([1.0; 3], [0.0; 3]));
    /// assert!(!PropPlacementInfo::declares_box([f32::NAN; 3], [1.0; 3]));
    /// ```
    #[must_use]
    pub fn declares_box(min: [f32; 3], max: [f32; 3]) -> bool {
        min.iter().chain(&max).all(|v| v.is_finite()) && (0..3).any(|axis| max[axis] > min[axis])
    }
}
