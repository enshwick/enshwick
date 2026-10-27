//! Transactions: changes of the server's world written as one, with undo.

use crate::abi::{
    BlockPlace, Header, PropPlace, PropRemove, Status, TerrainEdit, TransactionInfo, TransactionOp,
    VoxelRegion, VoxelWrite,
};
use crate::ecs::Entity;
use crate::terrain::{Shape, add_edit, dig_edit, flatten_edit};
use crate::voxels::{VoxelBox, VoxelPos};
use crate::{Host, ItemId};

use super::{BuildRefusal, Player, PropState, World, WorldError, WorldTransform};

/// The id of a committed [`Transaction`], for [`World::undo`] and
/// [`World::transaction_status`]; valid while the server runs.
///
/// # Examples
///
/// See [`Transaction`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransactionId(u64);

impl TransactionId {
    /// The number the loader gave it, for a log line.
    ///
    /// # Examples
    ///
    /// ```
    /// fn note(id: enshwick::world::TransactionId) {
    ///     enshwick::info!("transaction {}", id.get());
    /// }
    /// ```
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Where a committed [`Transaction`] stands, from
/// [`World::transaction_status`].
///
/// # Examples
///
/// ```
/// use enshwick::world::{TransactionId, TransactionStatus, World};
///
/// fn settled(world: World, id: TransactionId) -> bool {
///     !matches!(
///         world.transaction_status(id),
///         Some(TransactionStatus::Pending | TransactionStatus::Undoing)
///     )
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TransactionStatus {
    /// Queued, or written in part: a large transaction spreads over the
    /// server's steps.
    Pending,
    /// Written whole; it can be undone.
    Applied,
    /// Its undo is queued or written in part.
    Undoing,
    /// Undone, or dropped before it wrote anything.
    Undone,
    /// It stopped before or right after a change it could not write or
    /// read back: [`WorldError::NotLoaded`] when part of the world was not
    /// loaded yet (the check starts loading it, so the same transaction
    /// committed again a second or so later can pass), [`WorldError::OutsideWorld`] when a change leaves the world,
    /// [`WorldError::Missing`] when the world ended. The changes written
    /// stay, and [`World::undo`] can take them back while the loader keeps
    /// their undo data; a brick it could not read back after its change is
    /// never written over, so such an undo is refused.
    Failed(WorldError),
    /// The server refused the player the transaction acts for
    /// ([`World::transaction_for`]) before it wrote anything, for this
    /// reason, as [`World::can_build`] gives it.
    NotAllowed(BuildRefusal),
    /// Written whole, or with its undo refused, but the loader dropped what
    /// its undo needs, to stay within its memory cap or because the world
    /// ended; it can no longer be undone.
    Expired,
    /// Its last undo stopped: [`WorldError::HoldsState`] when a prop it
    /// placed now holds state, or one it removed held state it took with
    /// it; [`WorldError::Refused`] when a brick it touched has changed
    /// since, or a prop it placed is gone or one like the prop it removed
    /// stands in its place; [`WorldError::NotLoaded`] when part of it
    /// stayed unloaded for about 3,600 steps. It stopped before writing, or
    /// at that brick with what it had put back kept; the rest stays
    /// written, and [`World::undo`] may be asked again.
    UndoRefused(WorldError),
}

impl TransactionStatus {
    fn from_info(info: &TransactionInfo) -> Self {
        let reason = || {
            if info.holds != 0 {
                return Some(WorldError::HoldsState(PropState::from_bits(info.holds)));
            }
            WorldError::check(info.reason, WorldError::Missing).err()
        };
        match info.state {
            TransactionInfo::APPLIED => Self::Applied,
            TransactionInfo::UNDOING => Self::Undoing,
            TransactionInfo::UNDONE => Self::Undone,
            TransactionInfo::FAILED => match BuildRefusal::from_code(info.refusal) {
                Some(refusal) => Self::NotAllowed(refusal),
                None => Self::Failed(reason().unwrap_or(WorldError::Missing)),
            },
            TransactionInfo::EXPIRED => Self::Expired,
            TransactionInfo::UNDO_REFUSED => {
                Self::UndoRefused(reason().unwrap_or(WorldError::Refused))
            }
            _ => Self::Pending,
        }
    }
}

/// Changes of the server's world that the loader writes as one, and can
/// undo as one; start one with [`World::transaction`].
///
/// The loader writes the changes in order on the server's game thread, a
/// budget of 8³-voxel bricks a step, so a large transaction spreads over
/// steps. Before a change writes, the loader keeps the earlier state of
/// every brick it can touch; [`World::undo`] puts them back in reverse
/// order. A transaction that would touch a part of the world not loaded
/// now fails before it writes anything. Any thread may commit.
///
/// # Examples
///
/// ```
/// use enshwick::terrain::Shape;
/// use enshwick::world::{TransactionId, World, WorldError};
///
/// fn pit_with_rim(world: World, at: [f64; 3], dirt: u8) -> Result<TransactionId, WorldError> {
///     world
///         .transaction()
///         .terrain_dig(at, 4.0, Shape::Round, &[])
///         .terrain_add([at[0] + 6.0, at[1], at[2]], 2.0, 1.0, dirt, Shape::Round)
///         .commit()
/// }
///
/// fn take_back(world: World, id: TransactionId) {
///     if let Err(why) = world.undo(id) {
///         enshwick::warn!("no undo: {why}");
///     }
/// }
/// ```
#[derive(Debug)]
#[must_use = "a transaction changes nothing until it is committed"]
pub struct Transaction {
    host: Host,
    changes: Vec<Change>,
    /// The character of the player it acts for, 0 for the mod's own.
    actor: u32,
}

/// A change as the loader takes it, with what it points at.
#[derive(Debug)]
enum Change {
    Terrain(TerrainEdit),
    Voxels(
        VoxelWrite,
        #[expect(
            dead_code,
            reason = "holds the voxels the write points at until the commit"
        )]
        Vec<crate::abi::Voxel>,
    ),
    Erase(VoxelRegion),
    Block(BlockPlace),
    PlaceProp(PropPlace),
    RemoveProp(PropRemove),
}

impl Change {
    /// The op the loader reads, pointing into this change.
    fn op(&self) -> TransactionOp {
        let (kind, data) = match self {
            Self::Terrain(edit) => (TransactionOp::TERRAIN, (&raw const *edit).cast()),
            Self::Voxels(write, _) => (TransactionOp::VOXELS, (&raw const *write).cast()),
            Self::Erase(region) => (TransactionOp::ERASE, (&raw const *region).cast()),
            Self::Block(block) => (TransactionOp::BLOCK, (&raw const *block).cast()),
            Self::PlaceProp(prop) => (TransactionOp::PLACE_PROP, (&raw const *prop).cast()),
            Self::RemoveProp(prop) => (TransactionOp::REMOVE_PROP, (&raw const *prop).cast()),
        };
        TransactionOp {
            kind,
            reserved: 0,
            data,
        }
    }
}

impl Transaction {
    /// Adds a mound of `material`, as [`Terrain::add`](crate::terrain::Terrain::add)
    /// does, to the changes.
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn terrain_add(
        mut self,
        position: [f64; 3],
        radius: f32,
        height: f32,
        material: u8,
        shape: Shape,
    ) -> Self {
        self.changes.push(Change::Terrain(add_edit(
            position, radius, height, material, shape,
        )));
        self
    }

    /// Digs out the terrain, as [`Terrain::dig`](crate::terrain::Terrain::dig)
    /// does, as one of the changes.
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn terrain_dig(
        mut self,
        position: [f64; 3],
        radius: f32,
        shape: Shape,
        protect: &[u8],
    ) -> Self {
        self.changes
            .push(Change::Terrain(dig_edit(position, radius, shape, protect)));
        self
    }

    /// Flattens the ground, as
    /// [`Terrain::flatten`](crate::terrain::Terrain::flatten) does, as one of
    /// the changes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::terrain::Shape;
    ///
    /// fn pads(world: enshwick::world::World, stone: u8) -> bool {
    ///     world
    ///         .transaction()
    ///         .terrain_flatten([100.0, 40.0, 100.0], 5.0, stone, Shape::Square)
    ///         .terrain_flatten([120.0, 40.0, 100.0], 5.0, stone, Shape::Square)
    ///         .commit()
    ///         .is_ok()
    /// }
    /// ```
    pub fn terrain_flatten(
        mut self,
        position: [f64; 3],
        radius: f32,
        material: u8,
        shape: Shape,
    ) -> Self {
        self.changes.push(Change::Terrain(flatten_edit(
            position, radius, material, shape,
        )));
        self
    }

    /// Writes `voxels` over their box, whatever was there: air, ground and
    /// building blocks alike, each voxel with its fill. A box read with
    /// [`Voxels::read`](crate::voxels::Voxels::read), moved with
    /// [`VoxelBox::moved_to`], pastes it elsewhere.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::{VoxelBox, VoxelPos, Voxels};
    /// use enshwick::world::World;
    ///
    /// fn copy_up(world: World, voxels: Voxels, min: VoxelPos) -> bool {
    ///     let Ok(block) = voxels.read(min, [8, 8, 8]) else {
    ///         return false;
    ///     };
    ///     let [x, y, z] = min.to_array();
    ///     let up = block.moved_to(VoxelPos::new(x, y + 16, z));
    ///     world.transaction().write_voxels(&up).commit().is_ok()
    /// }
    /// ```
    pub fn write_voxels(mut self, voxels: &VoxelBox) -> Self {
        let raw: Vec<crate::abi::Voxel> = voxels.as_slice().iter().map(|v| v.to_abi()).collect();
        let write = VoxelWrite {
            header: Header::of::<VoxelWrite>(),
            min: voxels.min().to_array(),
            size: voxels.size(),
            voxels: raw.as_ptr(),
            count: raw.len() as u64,
        };
        self.changes.push(Change::Voxels(write, raw));
        self
    }

    /// Tears down the building blocks in the box of `size` voxels from
    /// `min`: their voxels turn to air, and the ground stays. The game's
    /// `BuildingTearDownEvent` tells its listeners.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::VoxelPos;
    ///
    /// fn clear(world: enshwick::world::World, corner: VoxelPos) -> bool {
    ///     world.transaction().erase_block(corner, [8, 8, 8]).commit().is_ok()
    /// }
    /// ```
    pub fn erase_block(mut self, min: VoxelPos, size: [u32; 3]) -> Self {
        self.changes.push(Change::Erase(VoxelRegion {
            header: Header::of::<VoxelRegion>(),
            min: min.to_array(),
            size,
        }));
        self
    }

    /// Places a building block as a player's building tool does: the
    /// `VoxelBlueprintItem` of `shape` in the building material `material`
    /// places, centred on `at`'s position and turned by its orientation,
    /// which the game keeps in quarter turns. As a player's building tool
    /// does, it writes over air and over ground and building voxels softer
    /// than `Unbreakable`, and leaves the rest. The game's
    /// `BuildingPlaceEvent` tells its listeners. It takes no cost from
    /// anyone. [`Transaction::commit`] refuses a `shape` with no
    /// `VoxelBlueprintItem` and a `material` whose `ChangeVoxelData` places
    /// no building material.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::world::{World, WorldTransform};
    ///
    /// fn pillar(world: World, shape: ItemId, stone: ItemId, at: [f64; 3]) -> bool {
    ///     let at = WorldTransform::at(at);
    ///     world.transaction().place_block(shape, stone, at).commit().is_ok()
    /// }
    /// ```
    pub fn place_block(mut self, shape: ItemId, material: ItemId, at: WorldTransform) -> Self {
        self.changes.push(Change::Block(BlockPlace {
            header: Header::of::<BlockPlace>(),
            shape: shape.get(),
            material: material.get(),
            position: at.position,
            rotation: at.orientation,
        }));
        self
    }

    /// Places a prop as a player places one from `item`: the template the
    /// item places, with the item it came from, standing at `at`. The game's
    /// `BuildingPlaceEvent` tells its listeners, and its
    /// `LogicEntityPlacedEvent` for a logic prop. It takes no cost from
    /// anyone. The prop is in the world from the step after it is placed,
    /// and the transaction is applied once it stands there;
    /// [`World::transaction_entities`] lists it. [`Transaction::commit`]
    /// refuses an item that places nothing, and one that places a creature
    /// or a flame altar, which founds a base, with
    /// [`WorldError::NotAProp`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::world::{World, WorldTransform};
    ///
    /// fn chest(world: World, chest: ItemId, at: [f64; 3]) -> bool {
    ///     world.transaction().place_prop(chest, WorldTransform::at(at)).commit().is_ok()
    /// }
    /// ```
    pub fn place_prop(mut self, item: ItemId, at: WorldTransform) -> Self {
        self.changes.push(Change::PlaceProp(PropPlace {
            header: Header::of::<PropPlace>(),
            item: item.get(),
            reserved: 0,
            position: at.position,
            orientation: at.orientation,
        }));
        self
    }

    /// Removes the prop `entity` as the game tears one down, without loot,
    /// and raises the game's `BuildingTearDownEvent`. A prop that holds
    /// state a player would lose is refused: items, any plant, a
    /// production queue, assigned survivors, written text, fuel a player
    /// put in a prop that does not refill itself, logic wiring or a logic
    /// trigger (each a
    /// [`HeldState`](super::HeldState)). The transaction fails with
    /// [`WorldError::HoldsState`] before it writes anything; empty it
    /// first, or use [`Transaction::remove_prop_discarding_state`]. The
    /// loader keeps players from the prop for the step before it goes and
    /// checks again then, so nothing put in meanwhile is lost. A creature
    /// or a flame altar fails with [`WorldError::NotAProp`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    ///
    /// fn tidy(world: enshwick::world::World, crate_entity: Entity) -> bool {
    ///     world.transaction().remove_prop(crate_entity).commit().is_ok()
    /// }
    /// ```
    pub fn remove_prop(mut self, entity: Entity) -> Self {
        self.changes.push(Change::RemoveProp(PropRemove {
            header: Header::of::<PropRemove>(),
            entity: entity.0,
            flags: 0,
        }));
        self
    }

    /// Removes the prop `entity` even when it holds state a player would
    /// lose. Destructive: a chest's items, a plant and its growth, a
    /// station's queue, survivors' places, written text, fuel, logic
    /// wiring and a trigger's settings are gone for good, and an undo will
    /// not bring the prop back without them, so it refuses with
    /// [`WorldError::HoldsState`]. Without state, it is
    /// [`Transaction::remove_prop`]. A creature or a flame altar still
    /// fails with [`WorldError::NotAProp`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ecs::Entity;
    ///
    /// fn clear_ruin(world: enshwick::world::World, ruined: Entity) -> bool {
    ///     world.transaction().remove_prop_discarding_state(ruined).commit().is_ok()
    /// }
    /// ```
    pub fn remove_prop_discarding_state(mut self, entity: Entity) -> Self {
        self.changes.push(Change::RemoveProp(PropRemove {
            header: Header::of::<PropRemove>(),
            entity: entity.0,
            flags: PropRemove::DISCARD_STATE,
        }));
        self
    }

    /// The changes so far.
    ///
    /// # Examples
    ///
    /// ```
    /// fn empty(world: enshwick::world::World) -> bool {
    ///     world.transaction().is_empty()
    /// }
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Whether it holds no change yet.
    ///
    /// # Examples
    ///
    /// See [`Transaction::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Hands the changes to the loader, which writes them at the server's
    /// next quiet moments; [`World::transaction_status`] follows them.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for no changes, a change the loader refuses
    /// (as [`Terrain`](crate::terrain::Terrain) does; a block whose shape
    /// item has no `VoxelBlueprintItem` or whose material item places no
    /// building material; voxels whose kind is not the one their material
    /// names), more than the
    /// loader's cap of changes or bricks, or a full queue;
    /// [`WorldError::OutsideWorld`] for a change past the world's edge or
    /// above 1,024 metres, once the loader has met the world (before, the
    /// transaction fails with it instead); [`WorldError::Missing`] while no
    /// world runs or on a build without the edits or the undo. A player's
    /// rights ([`World::transaction_for`]) are checked at the first step,
    /// not here: a refusal shows as [`TransactionStatus::NotAllowed`].
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn commit(self) -> Result<TransactionId, WorldError> {
        let ops: Vec<TransactionOp> = self.changes.iter().map(Change::op).collect();
        let count = u32::try_from(ops.len()).map_err(|_| WorldError::Refused)?;
        let mut id = 0u64;
        let api = self.host.api();
        let status = if self.actor == 0 {
            (api.transaction_commit)(self.host.handle(), ops.as_ptr(), count, &raw mut id)
        } else {
            (api.transaction_commit_for)(
                self.host.handle(),
                self.actor,
                ops.as_ptr(),
                count,
                &raw mut id,
            )
        };
        WorldError::from_status(status).map(|()| TransactionId(id))
    }
}

impl World {
    /// Starts a [`Transaction`] of the server's world.
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn transaction(self) -> Transaction {
        Transaction {
            host: self.0.host,
            changes: Vec::new(),
            actor: 0,
        }
    }

    /// Starts a [`Transaction`] for `player`: before it writes anything,
    /// the loader checks that the server lets the player edit the box of
    /// every change, as [`World::can_build`] does, and a refusal fails it
    /// with [`TransactionStatus::NotAllowed`] and the reason, writing
    /// nothing. The check runs at the transaction's first step on the
    /// server's game thread, so [`Transaction::commit`] does not refuse a
    /// player; [`World::transaction_status`] tells. Building events name the
    /// player as the owner.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::VoxelPos;
    /// use enshwick::world::{Player, World};
    ///
    /// fn clear_for(world: World, player: &Player, corner: VoxelPos) -> bool {
    ///     world.transaction_for(player).erase_block(corner, [8, 8, 8]).commit().is_ok()
    /// }
    /// ```
    pub fn transaction_for(self, player: &Player) -> Transaction {
        Transaction {
            actor: player.entity.0,
            ..self.transaction()
        }
    }

    /// Undoes transaction `id` of this mod: a queued one is dropped, one
    /// written in whole or in part gets every brick it touched back as it
    /// was. The undo runs at the server's next quiet moments. It first
    /// checks that every brick still holds what the transaction left there:
    /// a brick changed since (a player's or another edit) stops it before
    /// it writes anything, so undo transactions that overlap newest first.
    /// It checks each brick again right before writing it back: a brick
    /// changed while it runs stops it at that brick, keeping the bricks
    /// already put back, and the undo may be asked again once the change is
    /// gone. [`World::transaction_status`] tells how it went.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an id this mod did not get, one undone
    /// already, one whose undo data the loader has dropped
    /// ([`TransactionStatus::Expired`]), or while queued work holds the
    /// loader's cap of undo data; ask again once it is written.
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn undo(self, id: TransactionId) -> Result<(), WorldError> {
        let host = self.0.host;
        WorldError::from_status((host.api().transaction_undo)(host.handle(), id.0))
    }

    /// The props transaction `id` of this mod placed and still holds, oldest
    /// first; empty for an id the loader no longer remembers, or not this
    /// mod's. A prop is in the world from the step after the one that
    /// placed it.
    ///
    /// # Examples
    ///
    /// ```
    /// fn placed(world: enshwick::world::World, id: enshwick::world::TransactionId) -> usize {
    ///     world.transaction_entities(id).len()
    /// }
    /// ```
    #[must_use]
    pub fn transaction_entities(self, id: TransactionId) -> Vec<Entity> {
        let host = self.0.host;
        let mut count = 0u32;
        let api = host.api();
        if (api.transaction_entities)(
            host.handle(),
            id.0,
            core::ptr::null_mut(),
            0,
            &raw mut count,
        ) != Status::OK
        {
            return Vec::new();
        }
        let mut raw = vec![0u32; count as usize];
        let capacity = u32::try_from(raw.len()).unwrap_or(u32::MAX);
        if (api.transaction_entities)(
            host.handle(),
            id.0,
            raw.as_mut_ptr(),
            capacity,
            &raw mut count,
        ) != Status::OK
        {
            return Vec::new();
        }
        raw.truncate(count as usize);
        raw.into_iter().map(Entity).collect()
    }

    /// Where transaction `id` of this mod stands; `None` for an id the
    /// loader no longer remembers (it keeps the newest few thousand) or not
    /// this mod's.
    ///
    /// # Examples
    ///
    /// See [`TransactionStatus`].
    #[must_use]
    pub fn transaction_status(self, id: TransactionId) -> Option<TransactionStatus> {
        let host = self.0.host;
        let mut info = TransactionInfo {
            header: Header::of::<TransactionInfo>(),
            state: TransactionInfo::PENDING,
            reason: Status::OK,
            ops: 0,
            applied: 0,
            bricks: 0,
            refusal: 0,
            reserved: 0,
            holds: 0,
            created: 0,
        };
        let status = (host.api().transaction_status)(host.handle(), id.0, &raw mut info);
        status.is_ok().then(|| TransactionStatus::from_info(&info))
    }
}
