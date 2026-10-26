//! The fake's transactions: what a mod committed, where each stands, and the blocks it knows.

use super::{FakeHost, Installed, table};
use crate::ItemId;
use crate::abi::{BlockPlace, ModHandle, Status, TerrainEdit, TransactionInfo, Voxel};

impl FakeHost {
    /// A building shape the fake knows: `shape`'s `VoxelBlueprintItem` of
    /// `size` voxels. A block of any other shape is refused, as the loader
    /// refuses an item without one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    ///
    /// let loader = enshwick::test::FakeHost::new().blueprint(ItemId(10), [4, 4, 4]).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn blueprint(mut self, shape: ItemId, size: [u32; 3]) -> Self {
        self.blueprints.insert(shape.0, size);
        self
    }

    /// A building material the fake knows: `item` places the voxel
    /// material `material`, 0x80 or more. A block of any other material is
    /// refused, as the loader refuses an item that places no building
    /// material.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    ///
    /// let loader = enshwick::test::FakeHost::new().building_material(ItemId(20), 0x83).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn building_material(mut self, item: ItemId, material: u8) -> Self {
        self.building_materials.insert(item.0, material);
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The transactions the mod committed, oldest first, as the loader
    /// holds them: until [`Installed::run_transactions`] each stays
    /// pending.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.transactions().is_empty());
    /// ```
    #[must_use]
    pub fn transactions(&self) -> Vec<FakeTransaction> {
        table::with(Vec::new(), |s| s.edits.transactions.clone())
    }

    /// Runs the loader's next steps until every queued transaction and undo
    /// is written: pending ones turn applied, undoing ones undone. One for
    /// a player whom the fake's players, zones and building area refuse
    /// for a change's box fails instead, with the refusal, as the loader's
    /// rights check does. The fake keeps the bricks of each change's box as
    /// undo data, and changes nothing it answers reads from.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.run_transactions();
    /// assert!(loader.transactions().is_empty());
    /// ```
    pub fn run_transactions(&self) {
        table::with((), |s| {
            let host = &s.host;
            for transaction in &mut s.edits.transactions {
                let refusal = transaction.refusal(host);
                let bricks = transaction.bricks(host);
                let info = &mut transaction.info;
                match info.state {
                    TransactionInfo::PENDING if refusal != 0 => {
                        info.state = TransactionInfo::FAILED;
                        info.reason = Status::REFUSED;
                        info.refusal = refusal;
                    }
                    TransactionInfo::PENDING => {
                        info.state = TransactionInfo::APPLIED;
                        info.applied = info.ops;
                        info.bricks = bricks;
                    }
                    TransactionInfo::UNDOING => {
                        info.state = TransactionInfo::UNDONE;
                        info.applied = 0;
                        info.bricks = 0;
                    }
                    _ => {}
                }
            }
        });
    }

    /// Makes the loader's next step meet `reason` for transaction `id`, as
    /// the game would where the world is not loaded yet
    /// ([`Status::NOT_LOADED`]) or after a change in its region
    /// ([`Status::REFUSED`]): a pending one fails before writing, an undo is
    /// refused and it stays applied.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.fail_transaction(1, enshwick::abi::Status::NOT_LOADED);
    /// assert!(loader.transactions().is_empty());
    /// ```
    pub fn fail_transaction(&self, id: u64, reason: Status) {
        table::with((), |s| {
            let Some(transaction) = s.edits.transactions.iter_mut().find(|t| t.id == id) else {
                return;
            };
            let info = &mut transaction.info;
            match info.state {
                TransactionInfo::PENDING => {
                    info.state = TransactionInfo::FAILED;
                    info.reason = reason;
                }
                TransactionInfo::UNDOING => {
                    info.state = TransactionInfo::UNDO_REFUSED;
                    info.reason = reason;
                }
                _ => {}
            }
        });
    }
}

/// A change of a [`FakeTransaction`], as the mod handed it over.
///
/// # Examples
///
/// ```
/// use enshwick::test::FakeChange;
///
/// fn blocks(changes: &[FakeChange]) -> usize {
///     changes.iter().filter(|c| matches!(c, FakeChange::Block(_))).count()
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum FakeChange {
    /// A terrain add, dig or flatten.
    Terrain(TerrainEdit),
    /// Voxels written over the box of `size` from `min`.
    Voxels {
        /// The box's lowest voxel.
        min: [u32; 3],
        /// Its voxels along each axis.
        size: [u32; 3],
        /// Its voxels, x fastest, then y, then z.
        voxels: Vec<Voxel>,
    },
    /// The building voxels of the box of `size` from `min` cleared.
    Erase {
        /// The box's lowest voxel.
        min: [u32; 3],
        /// Its voxels along each axis.
        size: [u32; 3],
    },
    /// A building block placed.
    Block(BlockPlace),
}

impl FakeChange {
    /// Its box in metres, lowest and highest corner: a terrain edit's
    /// position ± radius and a block's middle ± half its blueprint, where
    /// the loader takes the game's own boxes.
    fn metres(&self, host: &FakeHost) -> ([f64; 3], [f64; 3]) {
        match self {
            Self::Terrain(edit) => {
                let reach = f64::from(edit.radius);
                (
                    edit.position.map(|at| (f64::from(at) - reach).max(0.0)),
                    edit.position.map(|at| f64::from(at) + reach),
                )
            }
            Self::Voxels { min, size, .. } | Self::Erase { min, size } => (
                min.map(|at| f64::from(at) / 2.0),
                [0, 1, 2].map(|axis| f64::from(min[axis] + size[axis]) / 2.0),
            ),
            Self::Block(block) => {
                let size = host.blueprints.get(&block.shape).copied().unwrap_or([0; 3]);
                let half = size.map(|voxels| f64::from(voxels) / 4.0);
                (
                    [0, 1, 2].map(|axis| (block.position[axis] - half[axis]).max(0.0)),
                    [0, 1, 2].map(|axis| block.position[axis] + half[axis]),
                )
            }
        }
    }

    /// The 8³-voxel bricks of its box.
    pub(crate) fn bricks(&self, host: &FakeHost) -> u64 {
        let (min, max) = self.metres(host);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a checked change's box is finite and at 0 or more; `as` saturates"
        )]
        let brick = |metres: f64| (metres * 2.0) as u64 / 8;
        (0..3)
            .map(|axis| brick(max[axis]).saturating_sub(brick(min[axis])) + 1)
            .product()
    }
}

/// A transaction as the fake loader holds it, from
/// [`Installed::transactions`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::TransactionInfo;
///
/// fn applied(loader: &enshwick::test::Installed) -> usize {
///     loader
///         .transactions()
///         .iter()
///         .filter(|t| t.info.state == TransactionInfo::APPLIED)
///         .count()
/// }
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct FakeTransaction {
    /// The id the mod got.
    pub id: u64,
    /// The mod that committed it.
    pub owner: ModHandle,
    /// The character of the player it acts for; 0 for the mod's own.
    pub actor: u32,
    /// Its changes, in order.
    pub changes: Vec<FakeChange>,
    /// Where it stands, as the mod reads it.
    pub info: TransactionInfo,
}

impl FakeTransaction {
    /// The bricks of all its changes' boxes.
    pub(crate) fn bricks(&self, host: &FakeHost) -> u64 {
        self.changes.iter().map(|change| change.bricks(host)).sum()
    }

    /// The first refusal the fake's rights give its actor for a change's
    /// box; 0 without an actor or a refusal.
    fn refusal(&self, host: &FakeHost) -> u32 {
        if self.actor == 0 {
            return 0;
        }
        let permissions = host
            .players
            .iter()
            .find(|player| player.entity == self.actor)
            .map_or(0, |player| player.permissions);
        self.changes
            .iter()
            .map(|change| {
                let (min, max) = change.metres(host);
                host.zones.refusal(permissions, min, max)
            })
            .find(|&refusal| refusal != 0)
            .unwrap_or(0)
    }
}
