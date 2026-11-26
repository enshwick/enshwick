//! The one watcher of the waypoints all players see, which the loader and the fake both run.
//!
//! The loader runs it for `Event::WAYPOINT_CHANGED` and the maps of carried
//! windows, and the fake as the loader does: each player's waypoint at the
//! last look, and the rule for one outside the map's frame.

use crate::abi::{MapFrameInfo, Metres, PlayerInfo, WaypointData};
use crate::{Entity, PlayerKey};

/// How often the waypoints are looked at, in milliseconds: a person sets
/// one far more slowly, so every change is seen.
pub const WAYPOINT_EVERY_MS: u64 = 100;

/// The bytes of a waypoint's place: three `i64` in 32.32 fixed point.
const POSITION_BYTES: usize = 24;

/// Whether a `PlayerWaypoint` of `size` bytes holds the byte that shows it
/// at `enabled` and its place at `position`; a reader trusts no layout
/// that fails this.
#[must_use]
pub fn fields_inside(size: usize, enabled: usize, position: usize) -> bool {
    enabled < size
        && position
            .checked_add(POSITION_BYTES)
            .is_some_and(|end| end <= size)
}

/// Whether a look is due at `now`, after the last at `looked`, both in
/// milliseconds; `looked` is 0 before the first.
#[must_use]
pub const fn due(looked: u64, now: u64) -> bool {
    looked == 0 || now.saturating_sub(looked) >= WAYPOINT_EVERY_MS
}

/// Whether `at` lies inside the map's frame of `origin` and `size`, world x
/// and z in metres; the one rule for waypoints and pings.
#[must_use]
pub fn inside(origin: [f64; 2], size: [f64; 2], at: Metres) -> bool {
    let [x, _, z] = at;
    let [left, top] = origin;
    let [width, height] = size;
    at.iter().all(|c| c.is_finite())
        && (left..=left + width).contains(&x)
        && (top..=top + height).contains(&z)
}

/// The waypoint a `PlayerWaypoint` holds, from the byte that shows it and
/// its place in 32.32 fixed point: `None` while the byte is 0.
#[must_use]
pub fn position(enabled: u8, fixed: [i64; 3]) -> Option<Metres> {
    (enabled != 0).then(|| crate::ecs::metres(fixed))
}

/// The data of player `key`'s waypoint `now`, on character `entity`.
#[must_use]
pub fn data(key: PlayerKey, entity: Entity, now: Option<Metres>) -> WaypointData {
    enshwick_abi::headed!(WaypointData {
        key,
        entity,
        position: now.unwrap_or_default(),
        set: u32::from(now.is_some()),
        reserved_tail: 0,
    })
}

/// Each online player's waypoint at the last look.
#[derive(Debug, Default)]
pub struct Waypoints {
    seen: Vec<(PlayerKey, Option<Metres>)>,
    warned: bool,
}

impl Waypoints {
    /// No waypoint seen yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            seen: Vec::new(),
            warned: false,
        }
    }

    /// Looks at the waypoint of each of `players` through `read`, `None`
    /// when it cannot, and hands `changed` each one that changed since the
    /// last look; the first waypoint outside `frame` comes back once, for
    /// the log.
    ///
    /// A player's first look only notes the waypoint, and an unread one
    /// changes nothing. One outside the frame is noted but not handed on,
    /// so a move back counts as a change; a cleared one is handed on.
    pub fn look(
        &mut self,
        players: &[PlayerInfo],
        frame: Option<&MapFrameInfo>,
        mut read: impl FnMut(Entity) -> Option<Option<Metres>>,
        mut changed: impl FnMut(&WaypointData),
    ) -> Option<Metres> {
        self.seen
            .retain(|(key, _)| players.iter().any(|p| p.key == *key));
        let mut outside = None;
        for player in players {
            let Some(now) = read(player.entity) else {
                continue;
            };
            if !self.knows(player.key) {
                self.seen.push((player.key, now));
                continue;
            }
            outside = outside.or(self.moved(player, now, frame, &mut changed));
        }
        outside
    }

    /// `player` sets their waypoint to `now` between two looks, as the
    /// next look sees it: a player not seen yet had none, and the rest is
    /// as in [`look`](Self::look).
    pub fn set(
        &mut self,
        player: &PlayerInfo,
        now: Option<Metres>,
        frame: Option<&MapFrameInfo>,
        mut changed: impl FnMut(&WaypointData),
    ) -> Option<Metres> {
        if !self.knows(player.key) {
            self.seen.push((player.key, None));
        }
        self.moved(player, now, frame, &mut changed)
    }

    /// Whether a look saw player `key`.
    fn knows(&self, key: PlayerKey) -> bool {
        self.seen.iter().any(|(seen, _)| *seen == key)
    }

    /// Moves the waypoint seen of `player`, already seen, to `now` and
    /// hands `changed` a change inside `frame`; one outside, the first time,
    /// comes back for the log.
    fn moved(
        &mut self,
        player: &PlayerInfo,
        now: Option<Metres>,
        frame: Option<&MapFrameInfo>,
        changed: &mut impl FnMut(&WaypointData),
    ) -> Option<Metres> {
        let (_, before) = self.seen.iter_mut().find(|(key, _)| *key == player.key)?;
        if core::mem::replace(before, now) == now {
            return None;
        }
        if let Some(at) = now
            && frame.is_some_and(|frame| !inside(frame.origin, frame.size, at))
        {
            return (!core::mem::replace(&mut self.warned, true)).then_some(at);
        }
        changed(&data(player.key, player.entity, now));
        None
    }

    /// Forgets every waypoint seen, so the next look only notes them: for a
    /// watcher that stops looking for a while.
    pub fn forget(&mut self) {
        self.seen.clear();
    }

    /// Forgets player `key`'s waypoint, as they leave.
    pub fn left(&mut self, key: PlayerKey) {
        self.seen.retain(|(seen, _)| *seen != key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANN: Entity = Entity::from_engine(40);

    fn ann() -> PlayerInfo {
        enshwick_abi::headed!(PlayerInfo {
            key: PlayerKey::new(7),
            entity: ANN,
            ..bytemuck::Zeroable::zeroed()
        })
    }

    fn frame() -> MapFrameInfo {
        enshwick_abi::headed!(MapFrameInfo {
            origin: [0.0, 0.0],
            size: [1000.0, 1000.0],
            metres_per_cell: 8.0,
            ..MapFrameInfo::EMPTY
        })
    }

    /// The positions `waypoints` hands on as Ann's waypoint reads `now`.
    #[expect(
        clippy::option_option,
        reason = "the watcher's read: unread, cleared or set"
    )]
    fn look(waypoints: &mut Waypoints, now: Option<Option<Metres>>) -> Vec<Option<Metres>> {
        let mut changes = Vec::new();
        waypoints.look(
            &[ann()],
            Some(&frame()),
            |_| now,
            |data| {
                changes.push((data.set == 1).then_some(data.position));
            },
        );
        changes
    }

    #[test]
    fn a_first_look_notes_and_each_set_move_and_clear_after_it_is_handed_on_once() {
        let mut waypoints = Waypoints::new();
        let near = Some([101.0, 0.0, 100.0]);
        let far = Some([400.0, 2.0, 100.0]);
        assert!(
            look(&mut waypoints, Some(near)).is_empty(),
            "the first look"
        );
        assert!(look(&mut waypoints, Some(near)).is_empty(), "unchanged");
        assert_eq!(look(&mut waypoints, Some(far)), [far]);
        assert!(look(&mut waypoints, None).is_empty(), "an unread waypoint");
        assert_eq!(look(&mut waypoints, Some(None)), [None]);
        assert_eq!(look(&mut waypoints, Some(near)), [near]);
    }

    #[test]
    fn a_waypoint_outside_the_frame_is_noted_but_not_handed_on_and_a_move_back_is() {
        let mut waypoints = Waypoints::new();
        let inner = Some([500.0, 0.0, 500.0]);
        let outside = [-50.0, 0.0, 500.0];
        look(&mut waypoints, Some(inner));
        let mut out = None;
        waypoints.look(
            &[ann()],
            Some(&frame()),
            |_| Some(Some(outside)),
            |_| {
                out = Some(());
            },
        );
        assert_eq!(out, None);
        assert_eq!(look(&mut waypoints, Some(inner)), [inner]);
        assert!(look(&mut waypoints, Some(Some([f64::NAN, 0.0, 1.0]))).is_empty());
    }

    #[test]
    fn the_first_waypoint_outside_the_frame_comes_back_once_for_the_log() {
        let mut waypoints = Waypoints::new();
        let mut look =
            |at: Metres| waypoints.look(&[ann()], Some(&frame()), |_| Some(Some(at)), |_| {});
        assert_eq!(look([1.0, 0.0, 1.0]), None);
        assert_eq!(look([-1.0, 0.0, 1.0]), Some([-1.0, 0.0, 1.0]));
        assert_eq!(look([1.0, 0.0, 1.0]), None);
        assert_eq!(look([-2.0, 0.0, 1.0]), None, "once");
    }

    #[test]
    fn a_player_gone_or_forgotten_is_noted_afresh_at_the_next_look() {
        let mut waypoints = Waypoints::new();
        look(&mut waypoints, Some(Some([1.0, 0.0, 1.0])));
        waypoints.look(&[], None, |_| None, |_| {});
        assert!(
            look(&mut waypoints, Some(Some([2.0, 0.0, 1.0]))).is_empty(),
            "left"
        );
        waypoints.forget();
        assert!(
            look(&mut waypoints, Some(Some([3.0, 0.0, 1.0]))).is_empty(),
            "forgot"
        );
    }

    #[test]
    fn a_position_counts_as_inside_the_frame_only_when_finite_and_within_it() {
        let ([x, z], size) = ([-100.0, -50.0], [200.0, 100.0]);
        assert!(inside([x, z], size, [0.0, 900.0, 0.0]));
        assert!(!inside([x, z], size, [150.0, 0.0, 0.0]));
        assert!(!inside([x, z], size, [0.0, 0.0, f64::NAN]));
    }

    #[test]
    fn a_set_between_looks_runs_from_no_waypoint_and_keeps_the_frame_rule() {
        let mut waypoints = Waypoints::new();
        let mut set = |at: Metres| {
            let mut handed = 0;
            waypoints.set(&ann(), Some(at), Some(&frame()), |_| handed += 1);
            handed
        };
        assert_eq!(set([5.0, 0.0, 5.0]), 1, "a player not seen had none");
        assert_eq!(set([5.0, 0.0, 5.0]), 0, "set where it already was");
        assert_eq!(set([5000.0, 0.0, 5.0]), 0, "outside the frame");
        assert_eq!(set([6.0, 0.0, 5.0]), 1, "a move back in");
        waypoints.left(PlayerKey::new(7));
        assert!(look(&mut waypoints, Some(Some([6.0, 0.0, 5.0]))).is_empty());
    }

    #[test]
    fn a_layout_counts_only_when_both_fields_lie_inside_the_type() {
        assert!(fields_inside(32, 0, 8));
        assert!(!fields_inside(32, 32, 0), "the byte past the end");
        assert!(!fields_inside(32, 0, 9), "the place runs past the end");
        assert!(!fields_inside(32, 0, usize::MAX), "no overflow");
    }

    #[test]
    fn a_look_is_due_first_and_then_once_every_period() {
        assert!(due(0, 1), "before the first look, even at boot");
        assert!(!due(1_000, 1_000 + WAYPOINT_EVERY_MS - 1));
        assert!(due(1_000, 1_000 + WAYPOINT_EVERY_MS));
        assert!(!due(1_000, 999), "a clock that goes back waits");
    }

    #[test]
    fn a_waypoint_reads_only_while_its_byte_shows_it() {
        assert_eq!(position(0, [1 << 32; 3]), None);
        assert_eq!(
            position(1, [1 << 32, 0, -(1 << 31)]),
            Some([1.0, 0.0, -0.5])
        );
        let cleared = data(PlayerKey::new(7), ANN, None);
        assert_eq!((cleared.set, cleared.position), (0, [0.0; 3]));
    }
}
