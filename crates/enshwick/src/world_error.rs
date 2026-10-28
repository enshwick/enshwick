//! Why the server's world refused a request, shared by every module that
//! acts on it.

use crate::abi::Status;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WorldError {
    /// Called off the server's game thread, inside a step (from a filter
    /// or a hook on a system), or while no server runs.
    #[error("not on the server's game thread")]
    WrongThread,
    /// The entity is not in the world.
    #[error("the entity is not in the world")]
    Gone,
    /// This game build lacks the piece.
    #[error("not available on this loader or game build")]
    Missing,
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
    /// The box asked about leaves the world: past its edge across, or
    /// above the height the loader takes.
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
    #[error("the prop holds {0}")]
    HoldsState(PropState),
    /// The entity is no prop: not placed from an item, or a creature such
    /// as an animal, a pet or a survivor, which prop calls never touch.
    #[error("not a prop")]
    NotAProp,
}

impl WorldError {
    /// The error the loader's `status` stands for, with `missing` for
    /// [`Status::MISSING`]: [`Self::Gone`] where the call names an entity,
    /// [`Self::Lacks`] where it names a component.
    pub(crate) fn check(status: Status, missing: Self) -> Result<(), Self> {
        match status {
            Status::OK => Ok(()),
            Status::WRONG_THREAD => Err(Self::WrongThread),
            Status::NOT_LOADED => Err(Self::NotLoaded),
            Status::OUTSIDE_WORLD => Err(Self::OutsideWorld),
            Status::STATIC_HALF => Err(Self::StaticHalf),
            Status::NOT_A_PROP => Err(Self::NotAProp),
            Status::REFUSED | Status::BAD_ARGS => Err(Self::Refused),
            Status::MISSING => Err(missing),
            _ => Err(Self::Missing),
        }
    }

    /// [`Self::check`] for a call whose missing piece is the feature itself.
    pub(crate) fn from_status(status: Status) -> Result<(), Self> {
        Self::check(status, Self::Missing)
    }
}
