//! What happens on the map, as a server mod hears it: a player's waypoint
//! set, moved or cleared, and a ping; and on a client, fog of war the local
//! player uncovered.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use rustc_hash::FxHashMap;

use crate::engine_ids as ids;
use crate::locks::Relock;
use crate::{Entity, Load, Off, PlayerKey};

/// How often the waypoints are compared: a person sets one far more
/// slowly, so every change is seen.
const WAYPOINT_EVERY: Duration = Duration::from_millis(100);
/// The least time between two pings of one sender a mod hears; the
/// game's own pings come from player input, which a client controls.
const PING_GAP: Duration = Duration::from_millis(100);

/// A player's waypoint set, moved or cleared, from [`Load::on_waypoint`].
/// The name is enshwick's.
///
/// # Examples
///
/// See [`Load::on_waypoint`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct WaypointChanged {
    /// The player's account.
    pub player: PlayerKey,
    /// Their character.
    pub entity: Entity,
    /// Where the waypoint is now, in metres; `None` once cleared.
    #[script(default)]
    pub position: Option<[f64; 3]>,
}

/// A ping on the map and in the world, the game's `UiPingEvent`, from
/// [`Load::on_ping`].
///
/// # Examples
///
/// See [`Load::on_ping`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct MapPinged {
    /// The pinging player's account; `None` for a ping a mod raised for
    /// another entity.
    #[script(default)]
    pub player: Option<PlayerKey>,
    /// The entity the ping comes from.
    pub entity: Entity,
    /// Where, in metres.
    pub position: [f64; 3],
}

/// The least time between two looks at the fog of war; the loader closes
/// a window of uncovering at most once a second.
const FOG_EVERY: Duration = Duration::from_millis(250);

/// Fog of war the local player uncovered since the window before, closed
/// at most once a second, from [`Load::on_fog_uncovered`]. The name is the game's
/// `FogOfWar` with a verb of enshwick's.
///
/// # Examples
///
/// See [`Load::on_fog_uncovered`].
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct FogOfWarUncovered {
    /// The lowest corner of the box around the uncovered ground: world x
    /// and z in metres.
    pub min: [f64; 2],
    /// Its highest corner.
    pub max: [f64; 2],
    /// How many pixels of the fog's image turned from hidden to shown.
    pub pixels: u32,
}

/// The event of `info`'s last window when it is new to `seen`, the session
/// and window a mod saw last, which it moves on. The first look of a
/// session raises none: its last window came before the mod looked.
fn uncovering(
    seen: &mut Option<(u64, u64)>,
    info: &crate::abi::FogInfo,
) -> Option<FogOfWarUncovered> {
    let now = (info.session, info.uncovered);
    let before = seen.replace(now);
    let new = before.is_some_and(|(session, window)| session == now.0 && window != now.1);
    let metres = f64::from(info.metres_per_pixel);
    let size_z = f64::from(info.size_z);
    let [x, row] = info.uncovered_min.map(f64::from);
    let [width, height] = info.uncovered_size.map(f64::from);
    new.then_some(FogOfWarUncovered {
        min: [x * metres, size_z - (row + height) * metres],
        max: [(x + width) * metres, size_z - row * metres],
        pixels: info.uncovered_pixels,
    })
}

/// What [`Load::on_waypoint`] keeps between its looks.
#[derive(Default)]
struct Watched {
    /// Each player's waypoint at the last look.
    seen: FxHashMap<PlayerKey, Option<[f64; 3]>>,
    /// The map's frame, once a world runs.
    frame: Option<super::MapFrame>,
    /// Whether the log told of a waypoint outside the map.
    warned: bool,
}

/// Whether `position` lies inside the map's frame, when the frame is known.
fn inside(frame: Option<&super::MapFrame>, position: [f64; 3]) -> bool {
    frame.is_none_or(|frame| {
        let [x, _, z] = position;
        let [left, top] = frame.origin;
        let [width, height] = frame.size;
        position.iter().all(|c| c.is_finite())
            && (left..=left + width).contains(&x)
            && (top..=top + height).contains(&z)
    })
}

/// Whether a mod hears `sender`'s ping at `now`, given when each sender was
/// last heard; senders quiet for [`PING_GAP`] are forgotten, so the map
/// holds only the last tenth of a second's.
fn heard(last: &mut FxHashMap<Entity, Instant>, sender: Entity, now: Instant) -> bool {
    last.retain(|_, then| now.duration_since(*then) < PING_GAP);
    if last.contains_key(&sender) {
        return false;
    }
    last.insert(sender, now);
    true
}

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
        F: Fn(&WaypointChanged) + Send + Sync + 'static,
    {
        let map = self.map()?;
        let world = self.world()?;
        if map.0.waypoint.is_none() {
            return Err(Off::new("waypoints: this game build has none"));
        }
        let state: Mutex<Watched> = Mutex::default();
        let _period = self.on_tick(WAYPOINT_EVERY, move |_| {
            let mut state = state.relock();
            if state.frame.is_none() {
                state.frame = map.frame().ok();
            }
            let players = world.players();
            state
                .seen
                .retain(|key, _| players.iter().any(|p| p.key == *key));
            for player in &players {
                let Ok(now) = map.waypoint(player.entity) else {
                    continue;
                };
                // Kept even outside the map, so a move back counts as a change.
                let before = state.seen.insert(player.key, now);
                if now.is_some_and(|at| !inside(state.frame.as_ref(), at)) {
                    if !state.warned {
                        state.warned = true;
                        crate::warn!("a waypoint outside the map is not handed on: {now:?}");
                    }
                    continue;
                }
                if before.is_some_and(|before| before != now) {
                    handler(&WaypointChanged {
                        player: player.key,
                        entity: player.entity,
                        position: now,
                    });
                }
            }
        })?;
        Ok(())
    }

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
        F: Fn(&FogOfWarUncovered) + Send + Sync + 'static,
    {
        let fog = self.fog_of_war()?;
        let mut seen = None;
        let mut looked: Option<Instant> = None;
        self.on_frame(move |_| {
            let now = Instant::now();
            if looked.is_some_and(|at| now.duration_since(at) < FOG_EVERY) {
                return;
            }
            looked = Some(now);
            if let Ok(info) = fog.info()
                && let Some(event) = uncovering(&mut seen, &info)
            {
                handler(&event);
            }
        })
    }

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
        F: Fn(&MapPinged) + Send + Sync + 'static,
    {
        let ty = self.game_event_type(ids::UI_PING_EVENT)?;
        let sender = ty.field_by_id(self, ids::UI_PING_EVENT_SENDER_ENTITY_ID)?;
        let at = usize::try_from(self.constant(ids::UI_PING_POSITION)?)
            .map_err(|_| Off::new("pings: the position lies past memory"))?;
        let world = self.world()?;
        let map = self.map().ok();
        let frame: Mutex<Option<super::MapFrame>> = Mutex::default();
        let last: Mutex<FxHashMap<Entity, Instant>> = Mutex::default();
        self.on_raw_game_event(ty, move |event| {
            let Some(entity) = event.get(sender) else {
                return;
            };
            let fixed = [0, 8, 16].map(|axis| event.read::<i64>(at + axis));
            let [Some(x), Some(y), Some(z)] = fixed else {
                return;
            };
            let position = crate::ecs::metres([x, y, z]);
            let mut frame = frame.relock();
            if frame.is_none() {
                *frame = map.and_then(|map| map.frame().ok());
            }
            if !inside(frame.as_ref(), position) {
                return;
            }
            drop(frame);
            let now = Instant::now();
            if !heard(&mut last.relock(), entity, now) {
                return;
            }
            handler(&MapPinged {
                player: world.player(entity).map(|p| p.key),
                entity,
                position,
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_new_window_of_the_same_session_raises_uncovered_fog() {
        let info = |session, window, min, size, pixels| crate::abi::FogInfo {
            metres_per_pixel: 8.0,
            size_z: 1024.0,
            session,
            uncovered: window,
            uncovered_pixels: pixels,
            uncovered_min: min,
            uncovered_size: size,
            ..crate::abi::FogInfo::EMPTY
        };
        let mut seen = None;
        assert_eq!(
            uncovering(&mut seen, &info(1, 4, [0, 0], [1, 1], 1)),
            None,
            "the mod's first look"
        );
        assert_eq!(
            uncovering(&mut seen, &info(1, 4, [0, 0], [1, 1], 1)),
            None,
            "the same window"
        );
        assert_eq!(
            uncovering(&mut seen, &info(1, 5, [2, 10], [3, 2], 6)),
            Some(FogOfWarUncovered {
                min: [16.0, 1024.0 - 12.0 * 8.0],
                max: [40.0, 1024.0 - 10.0 * 8.0],
                pixels: 6,
            })
        );
        assert_eq!(
            uncovering(&mut seen, &info(2, 1, [0, 0], [1, 1], 1)),
            None,
            "a new world's first look"
        );
        assert!(uncovering(&mut seen, &info(2, 2, [0, 0], [1, 1], 1)).is_some());
    }

    #[test]
    fn a_position_counts_as_on_the_map_inside_its_frame_or_without_one() {
        let frame = super::super::MapFrame {
            origin: [-100.0, -50.0],
            size: [200.0, 100.0],
            metres_per_cell: 8.0,
        };
        assert!(inside(Some(&frame), [0.0, 900.0, 0.0]));
        assert!(!inside(Some(&frame), [150.0, 0.0, 0.0]));
        assert!(!inside(Some(&frame), [0.0, 0.0, f64::NAN]));
        assert!(inside(None, [1e9, 0.0, 0.0]));
    }

    #[test]
    fn a_sender_is_heard_once_a_tenth_of_a_second_and_quiet_senders_are_forgotten() {
        let mut last = FxHashMap::default();
        let start = Instant::now();
        assert!(heard(&mut last, Entity(1), start));
        assert!(!heard(&mut last, Entity(1), start + PING_GAP / 2));
        assert!(heard(&mut last, Entity(2), start + PING_GAP / 2));
        assert!(heard(&mut last, Entity(1), start + PING_GAP));
        assert!(heard(&mut last, Entity(3), start + PING_GAP * 3));
        assert_eq!(last.len(), 1, "only the sender heard last stays");
    }
}
