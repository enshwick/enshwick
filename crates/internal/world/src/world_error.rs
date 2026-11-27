//! Why the server's world refused a request, shared by every module that
//! acts on it.

use crate::abi::Status;
#[cfg(feature = "unstable")]
use crate::world::PropState;

/// Why the server's world refused a request.
///
/// # Examples
///
/// ```
/// use enshwick::WorldError;
///
/// assert_eq!(WorldError::WrongThread.to_string(), "not on the server's game thread");
/// ```
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, thiserror::Error, crate::script::ErrorKind, crate::Script,
)]
#[non_exhaustive]
pub enum WorldError {
    /// Called off the server's game thread or inside a step (from a filter
    /// or a hook on a system); some reads answer it while no server runs,
    /// where the entity calls answer [`WorldError::Unavailable`].
    #[error("not on the server's game thread")]
    WrongThread,
    /// The entity is not in the world.
    #[error("the entity is not in the world")]
    Gone,
    /// The call cannot run here: no server runs in this process, as on a
    /// client in another's game, or this loader or game build lacks the
    /// piece.
    #[error("not available here, or on this loader or game build")]
    Unavailable,
    /// A queue of the world is full: the same request may pass after a
    /// later step.
    #[error("the world is busy; try again later")]
    Busy,
    /// The loader failed inside its own code, or answered with an outcome
    /// this build of the mod does not know.
    #[error("failed in the loader")]
    Failed,
    /// The world refused: a player's character, a save before the world
    /// has loaded, a write that would kill, or a copy the game keeps for
    /// clients.
    #[error("refused by the world")]
    Refused,
    /// The entity is in the world but lacks the component, such as a chest
    /// asked for its stamina.
    #[error("the entity lacks the component")]
    Lacks,
    /// The part of the world asked about is not loaded yet; asking starts
    /// loading it, so try again in a second or so.
    #[error("that part of the world is not loaded")]
    NotLoaded,
    /// The box or place asked about leaves the world: past its edge across,
    /// above the height the loader takes, or, for a teleport, past the box
    /// the world's scene keeps entities in.
    #[error("outside the world")]
    OutsideWorld,
    /// The component type is the static half of its ECS type, such as
    /// `FuelSetup`: the set-up values every entity of the template shares,
    /// which the world reads but never writes.
    #[error("the template's set-up data, which is never written")]
    StaticHalf,
    /// The prop holds state a player would lose: items, growth, a
    /// production queue, assigned survivors, written text, fuel, logic
    /// wiring or a logic trigger's settings;
    /// [`HeldState`](crate::world::HeldState) names each kind.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    #[error("the prop holds {0}")]
    #[cfg(feature = "unstable")]
    HoldsState(PropState),
    /// The entity is no prop: not placed from an item, or a creature such
    /// as an animal, a pet or a survivor, which prop calls never touch.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    #[error("not a prop")]
    #[cfg(feature = "unstable")]
    NotAProp,
    /// The entity is no spawn point a player respawns at.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    #[error("not a spawn point players respawn at")]
    #[cfg(feature = "unstable")]
    NotASpawnPoint,
}

crate::load::off_from!(WorldError);

impl enshwick_base::load::StatusError for WorldError {
    fn of(status: Status) -> Self {
        match status {
            Status::WRONG_THREAD => Self::WrongThread,
            Status::NOT_LOADED => Self::NotLoaded,
            Status::OUTSIDE_WORLD => Self::OutsideWorld,
            Status::STATIC_HALF => Self::StaticHalf,
            #[cfg(feature = "unstable")]
            Status::NOT_A_PROP => Self::NotAProp,
            Status::REFUSED | Status::BAD_ARGS => Self::Refused,
            Status::UNAVAILABLE | Status::MISSING => Self::Unavailable,
            Status::BUSY => Self::Busy,
            _ => Self::Failed,
        }
    }
}

impl WorldError {
    /// The error the loader's `status` stands for, with `missing` for
    /// [`Status::MISSING`]: [`Self::Gone`] where the call names an entity,
    /// [`Self::Lacks`] where it names a component.
    pub(crate) fn check(status: Status, missing: Self) -> Result<(), Self> {
        if status == Status::MISSING {
            return Err(missing);
        }
        enshwick_base::load::outcome(status)
    }

    /// [`Self::check`] for a call whose missing piece is the feature itself.
    pub(crate) fn from_status(status: Status) -> Result<(), Self> {
        enshwick_base::load::outcome(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unavailable_call_reads_as_unavailable_and_an_unknown_status_as_a_failure() {
        assert_eq!(
            WorldError::check(Status::UNAVAILABLE, WorldError::Gone),
            Err(WorldError::Unavailable)
        );
        assert_eq!(
            WorldError::check(Status::BUSY, WorldError::Gone),
            Err(WorldError::Busy)
        );
        for status in [Status::LOADER_FAULT, Status(u32::MAX)] {
            assert_eq!(
                WorldError::check(status, WorldError::Gone),
                Err(WorldError::Failed)
            );
        }
    }
}
