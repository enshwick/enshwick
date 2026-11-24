//! Why a transaction, a change of one, or a call about one failed.

#[cfg(windows)]
use crate::engine::world::WorldError;
use crate::world::rights::RightsError;
#[cfg(windows)]
use crate::world::terrain::TerrainError;
use crate::world::voxels::VoxelError;

/// Why a transaction, a change of one, or a call about one failed; a
/// settled transaction keeps it as its reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TransactionError {
    /// An empty box, one past the world's end or below its corner, or
    /// voxels that do not fill their box.
    #[error("the box is empty, past the world's end or not filled")]
    BadBox,
    /// A terrain edit a transaction does not take.
    #[error("the terrain edit is not one a transaction takes")]
    BadEdit,
    /// No changes, or more changes or bricks than the limits.
    #[error("no changes, or more than the limits")]
    BadCount,
    /// A position, rotation or orientation out of range.
    #[error("the position or rotation is out of range")]
    BadPlace,
    /// An item the game does not have, or one that places nothing a change
    /// of its kind places.
    #[error("the item places nothing of the change's kind")]
    BadItem,
    /// A block shape without a blueprint.
    #[error("no blueprint has the shape")]
    BadShape,
    /// The block's blueprint no longer gives the box the commit checked, as
    /// after the world changed.
    #[error("the block's blueprint changed since the commit")]
    BlueprintChanged,
    /// An item or entity that is no prop.
    #[error("not a prop")]
    NotAProp,
    /// The feature is off, no world runs, or the build lacks a piece the
    /// change writes with.
    #[error("transactions do not run now")]
    Unavailable,
    /// The prop or entity is not in the world, or no longer held.
    #[error("the prop is not in the world")]
    Gone,
    /// A change leaves the world.
    #[error("a change leaves the world")]
    OutsideWorld,
    /// A column of bricks a change touches is not loaded.
    #[error("a column of the change is not loaded")]
    NotLoaded,
    /// The transactions queued and remembered leave no room.
    #[error("no room for another transaction")]
    Full,
    /// An unknown id, or a transaction undone, without undo data, or whose
    /// undo data would pass the cap.
    #[error("the transaction cannot be undone")]
    NotUndoable,
    /// The server refuses the actor the edit.
    #[error("the actor may not edit there")]
    NoRights,
    /// A prop holds state a respawn would lose.
    #[error("a prop holds state")]
    HoldsState,
    /// A brick or prop changed since the transaction, or a prop's place is
    /// taken.
    #[error("the world changed since")]
    Changed,
    /// The server's world, unreadable now.
    #[cfg(windows)]
    #[error(transparent)]
    World(#[from] WorldError),
    /// The world's voxels, as their read answered.
    #[error(transparent)]
    Voxels(VoxelError),
    /// The terrain edit, as the game's editor answered.
    #[cfg(windows)]
    #[error(transparent)]
    Terrain(TerrainError),
    /// The actor's build rights, unreadable now.
    #[error(transparent)]
    Rights(RightsError),
    /// The item the change places, unreadable now.
    #[cfg(windows)]
    #[error(transparent)]
    Item(#[from] crate::inventory::transactions::ItemInfoError),
    /// The prop's spawn, as the server answered.
    #[cfg(windows)]
    #[error(transparent)]
    Spawn(#[from] crate::world::spawns::SpawnError),
    /// The prop's entity, as the entity list or call answered.
    #[cfg(windows)]
    #[error(transparent)]
    Entity(#[from] crate::world::entities::EntityError),
}

/// A prop the hooks cannot hold leaves transactions unable to remove it.
impl From<crate::world::prop_holds::HoldError> for TransactionError {
    fn from(_: crate::world::prop_holds::HoldError) -> Self {
        Self::Unavailable
    }
}

/// A brick not loaded and a box out of the world are the transaction's own
/// reasons; the rest is the read's.
impl From<VoxelError> for TransactionError {
    fn from(error: VoxelError) -> Self {
        match error {
            VoxelError::NotLoaded => Self::NotLoaded,
            VoxelError::OutsideWorld => Self::OutsideWorld,
            #[cfg(windows)]
            VoxelError::World(error) => Self::World(error),
            other => Self::Voxels(other),
        }
    }
}

/// The server's world unreadable for the rights check is the same reason as
/// for any other read; the rest is the rights'.
impl From<RightsError> for TransactionError {
    fn from(error: RightsError) -> Self {
        match error {
            #[cfg(windows)]
            RightsError::World(error) => Self::World(error),
            other => Self::Rights(other),
        }
    }
}

/// As [`From<VoxelError>`], through the edit's own reasons.
#[cfg(windows)]
impl From<TerrainError> for TransactionError {
    fn from(error: TerrainError) -> Self {
        match error {
            TerrainError::OutsideWorld => Self::OutsideWorld,
            TerrainError::Voxels(error) => error.into(),
            TerrainError::World(error) => Self::World(error),
            other => Self::Terrain(other),
        }
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_world_is_one_reason_whichever_read_met_it() {
        let world = WorldError::InStep;
        let reasons = [
            TransactionError::from(RightsError::World(world)),
            TransactionError::from(VoxelError::World(world)),
            TransactionError::from(TerrainError::World(world)),
        ];
        assert_eq!(reasons, [TransactionError::World(world); 3]);
        assert_eq!(
            TransactionError::from(RightsError::NoZones),
            TransactionError::Rights(RightsError::NoZones)
        );
    }
}
