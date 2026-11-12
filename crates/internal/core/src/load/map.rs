//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::map::{FogOfWar, Map};
use crate::{Load, Off};
use enshwick_world::__load::*;

mod events;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The map's markers and the players' waypoints in the game server,
    /// dedicated or hosted by the client.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks the marker reads.
    ///
    /// # Examples
    ///
    /// See [`Map`].
    pub fn map(&mut self) -> Result<Map, Off>;

    /// The client's fog of war, which the loader copies for the mod from
    /// now on; a mod asks for it during `init`.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] on a dedicated server, which has no fog, or after `init`.
    ///
    /// # Examples
    ///
    /// See [`FogOfWar`].
    pub fn fog_of_war(&mut self) -> Result<FogOfWar, Off>;
}
