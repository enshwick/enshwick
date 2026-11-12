//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Entity;
use crate::PlayerKey;
use crate::WorldError;
use crate::abi::{Header, PlayerInfo, SpawnRequest, TeleportRequest};
use crate::ecs::ComponentType;
use crate::engine_ids as ids;
use crate::game_events::{EventType, Field, Scalar};
use crate::{Host, Load, Off};
use enshwick_inventory::__load::*;
use enshwick_world::__internal::{world_check, world_status};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The mod's [`Effects`] in the server's world: game events it raises
    /// and spawns; on a client, the built-in server's while it hosts a
    /// game. An action whose event this build lacks answers
    /// [`WorldError::Missing`].
    ///
    /// # Examples
    ///
    /// See the [module](crate::effects).
    #[must_use]
    pub fn effects(&mut self) -> Effects;
}
