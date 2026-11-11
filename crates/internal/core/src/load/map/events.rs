//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::engine_ids as ids;
use crate::locks::Relock;
use crate::{Entity, Load, Off, PlayerKey};
use enshwick_world::__load::*;
use rustc_hash::FxHashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[enshwick_macros::forward(base)]
#[crate::script(map_events)]
impl Load<'_> {
    /// Calls `handler` on the server's game thread when a player sets,
    /// moves or clears the waypoint all players see. The waypoints are
    /// compared on a tick every tenth of a second, so a change shows within
    /// that; a
    /// player's waypoint when they join counts as no change. A waypoint
    /// outside the map is not handed on, and the log says so once. On a
    /// client it hears only a game that client hosts.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init` or on a game build without waypoints.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_waypoint(|change| match change.position {
    ///         Some(at) => enshwick::info!("{:?} marks {at:?}", change.player),
    ///         None => enshwick::info!("{:?} clears the waypoint", change.player),
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_waypoint<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&WaypointChanged) + Send + Sync + 'static;

    /// Calls `handler` on the thread that draws the interface when the
    /// local player uncovered fog of war, at most once a second, with the
    /// box around the new ground and how many pixels of the fog it was.
    /// Client only: the fog of war is the client's own. The fog is looked
    /// at four times a second inside the interface's frames, so a window
    /// shows within that; a world's first copy of its saved fog counts as
    /// none.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, after `init` or on a game build without the
    /// fog of war.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_fog_uncovered(|new| {
    ///         enshwick::info!("{} pixels of new ground near {:?}", new.pixels, new.min);
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_fog_uncovered<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&FogOfWarUncovered) + Send + Sync + 'static;

    /// Calls `handler` on the server's game thread for each ping a player
    /// makes on the map or in the world, and each one a mod raises with
    /// [`Effects::ping`](crate::effects::Effects::ping). A sender is heard
    /// at most once a tenth of a second, and a ping outside the map is not
    /// handed on.
    ///
    /// # Errors
    ///
    /// [`Off`] after `init` or on a game build without pings.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_ping(|ping| enshwick::info!("{:?} pings {:?}", ping.player, ping.position))
    /// }
    /// ```
    #[script]
    pub fn on_ping<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&MapPinged) + Send + Sync + 'static;
}
