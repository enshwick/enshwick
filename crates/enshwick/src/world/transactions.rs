//! Transactions: changes of the server's world written as one, with undo.

use crate::Host;
use crate::abi::{Header, Status, TerrainEdit, TransactionInfo, TransactionOp};
use crate::terrain::{Shape, add_edit, dig_edit, flatten_edit};

use super::{World, WorldError};

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
    /// It stopped before a change it could not write:
    /// [`WorldError::NotLoaded`] when part of the world was not loaded,
    /// [`WorldError::Missing`] when the world ended. The changes written
    /// before it stay, and [`World::undo`] can still take them back.
    Failed(WorldError),
    /// Written whole, but the loader dropped what its undo needs to stay
    /// within its memory cap; it can no longer be undone.
    Expired,
    /// Its last undo wrote nothing and was refused:
    /// [`WorldError::Refused`] when a brick it touched has changed since,
    /// [`WorldError::NotLoaded`] when part of it was not loaded. It stays
    /// written, and [`World::undo`] may be asked again.
    UndoRefused(WorldError),
}

impl TransactionStatus {
    fn from_info(info: &TransactionInfo) -> Self {
        let reason = || WorldError::check(info.reason, WorldError::Missing).err();
        match info.state {
            TransactionInfo::APPLIED => Self::Applied,
            TransactionInfo::UNDOING => Self::Undoing,
            TransactionInfo::UNDONE => Self::Undone,
            TransactionInfo::FAILED => Self::Failed(reason().unwrap_or(WorldError::Missing)),
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
    terrain: Vec<TerrainEdit>,
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
        self.terrain
            .push(add_edit(position, radius, height, material, shape));
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
        self.terrain
            .push(dig_edit(position, radius, shape, protect));
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
        self.terrain
            .push(flatten_edit(position, radius, material, shape));
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
        self.terrain.len()
    }

    /// Whether it holds no change yet.
    ///
    /// # Examples
    ///
    /// See [`Transaction::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terrain.is_empty()
    }

    /// Hands the changes to the loader, which writes them at the server's
    /// next quiet moments; [`World::transaction_status`] follows them.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for no changes, a change the loader refuses
    /// (as [`Terrain`](crate::terrain::Terrain) does), more than the
    /// loader's cap of changes or bricks, or a full queue;
    /// [`WorldError::Missing`] while no world runs or on a build without the
    /// edits or the undo.
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn commit(self) -> Result<TransactionId, WorldError> {
        let ops: Vec<TransactionOp> = self
            .terrain
            .iter()
            .map(|edit| TransactionOp {
                kind: TransactionOp::TERRAIN,
                reserved: 0,
                data: (&raw const *edit).cast(),
            })
            .collect();
        let count = u32::try_from(ops.len()).map_err(|_| WorldError::Refused)?;
        let mut id = 0u64;
        let status = (self.host.api().transaction_commit)(
            self.host.handle(),
            ops.as_ptr(),
            count,
            &raw mut id,
        );
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
            terrain: Vec::new(),
        }
    }

    /// Undoes transaction `id` of this mod: a queued one is dropped, one
    /// written in whole or in part gets every brick it touched back as it
    /// was. The undo runs at the server's next quiet moments; it first checks
    /// that every brick still holds what the transaction left there, and
    /// writes nothing when one has changed since (a player's or another
    /// edit), so undo transactions that overlap newest first.
    /// [`World::transaction_status`] tells how it went.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an id this mod did not get, one undone
    /// already, or one whose undo data the loader has dropped
    /// ([`TransactionStatus::Expired`]).
    ///
    /// # Examples
    ///
    /// See [`Transaction`].
    pub fn undo(self, id: TransactionId) -> Result<(), WorldError> {
        let host = self.0.host;
        WorldError::from_status((host.api().transaction_undo)(host.handle(), id.0))
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
        };
        let status = (host.api().transaction_status)(host.handle(), id.0, &raw mut info);
        status.is_ok().then(|| TransactionStatus::from_info(&info))
    }
}
