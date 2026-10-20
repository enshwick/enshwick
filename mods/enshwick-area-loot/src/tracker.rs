//! What the mod remembers of the loot containers it saw: whether each was
//! empty, who last opened it and since when, where it stands, and which ones
//! a take in flight holds.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use enshwick_api::ecs::Entity;
use enshwick_api::loot::{Container, near};

/// How long a take holds a container: no one else takes from it, and its
/// emptying starts nothing.
pub(crate) const HOLD: Duration = Duration::from_secs(30);
/// A container unseen this long is forgotten; enemy loot despawns sooner.
pub(crate) const FORGET: Duration = Duration::from_secs(3600);

/// One container, as last seen.
#[derive(Clone, Copy, Debug)]
struct Seen {
    position: [f64; 3],
    empty: bool,
    guest: Entity,
    accepted_at: i64,
    /// When the mod first saw the current guest and time.
    since: Instant,
    last: Instant,
}

/// The containers seen and the ones held.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    seen: HashMap<Entity, Seen>,
    held: HashMap<Entity, Instant>,
}

impl Tracker {
    /// Records `container` as seen at `now`. Returns who emptied it when it
    /// just turned empty and a take does not hold it: the entity that last
    /// opened or looted it.
    pub(crate) fn observe(&mut self, container: &Container, now: Instant) -> Option<Entity> {
        let (guest, accepted_at) = container
            .offer
            .map_or((Entity::NONE, 0), |o| (o.guest, o.accepted_at));
        let fresh = Seen {
            position: container.position,
            empty: container.empty,
            guest,
            accepted_at,
            since: now,
            last: now,
        };
        let was = self.seen.insert(container.entity, fresh)?;
        if let Some(seen) = self.seen.get_mut(&container.entity)
            && (was.guest, was.accepted_at) == (guest, accepted_at)
        {
            seen.since = was.since;
        }
        let turned = !was.empty && container.empty;
        (turned && !guest.is_none() && !self.holds(container.entity, now)).then_some(guest)
    }

    /// Whether an entity other than `player` opened or looted `container`
    /// within `window` before `now`. A change the mod did not see happen
    /// counts as just now.
    pub(crate) fn busy(
        &self,
        container: &Container,
        player: Entity,
        now: Instant,
        window: Duration,
    ) -> bool {
        let Some(offer) = container.offer else {
            return false;
        };
        if offer.guest.is_none() || offer.guest == player {
            return false;
        }
        let since = self
            .seen
            .get(&container.entity)
            .filter(|s| (s.guest, s.accepted_at) == (offer.guest, offer.accepted_at))
            .map_or(now, |s| s.since);
        now.saturating_duration_since(since) < window
    }

    /// The containers seen within `radius` across and `height` up or down
    /// of `center`, other than `except` and the held ones, nearest first.
    pub(crate) fn around(
        &self,
        center: [f64; 3],
        radius: f64,
        height: f64,
        except: Entity,
        now: Instant,
    ) -> Vec<Entity> {
        let mut found: Vec<(f64, Entity)> = self
            .seen
            .iter()
            .filter(|&(&e, s)| {
                e != except
                    && !s.empty
                    && !self.holds(e, now)
                    && near(center, s.position, radius, height)
            })
            .map(|(&e, s)| {
                let (dx, dz) = (s.position[0] - center[0], s.position[2] - center[2]);
                (dx.mul_add(dx, dz * dz), e)
            })
            .collect();
        found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        found.into_iter().map(|(_, e)| e).collect()
    }

    /// Holds `container` for a take until `now` plus [`HOLD`].
    pub(crate) fn hold(&mut self, container: Entity, now: Instant) {
        self.held.insert(container, now + HOLD);
    }

    /// Lets go of `container`.
    pub(crate) fn release(&mut self, container: Entity) {
        self.held.remove(&container);
    }

    /// Forgets `container`, which is gone or no longer loot.
    pub(crate) fn forget(&mut self, container: Entity) {
        self.seen.remove(&container);
    }

    fn holds(&self, container: Entity, now: Instant) -> bool {
        self.held.get(&container).is_some_and(|&until| now < until)
    }

    /// Drops containers unseen for [`FORGET`] and holds that ran out.
    pub(crate) fn prune(&mut self, now: Instant) {
        self.seen
            .retain(|_, s| now.saturating_duration_since(s.last) < FORGET);
        self.held.retain(|_, &mut until| now < until);
    }

    /// Forgets everything, for a new game.
    pub(crate) fn clear(&mut self) {
        self.seen.clear();
        self.held.clear();
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.seen.len()
    }
}

#[cfg(test)]
mod tests {
    use enshwick_api::loot::Offer;

    use super::*;

    const PLAYER: Entity = Entity(2);
    const OTHER: Entity = Entity(3);

    fn corpse(id: u32, x: f64, empty: bool, guest: Entity, at: i64) -> Container {
        Container {
            entity: Entity(id),
            position: [x, 0.0, 0.0],
            empty,
            expires: true,
            area_loot: Some(true),
            offer: Some(Offer {
                guest,
                accepted_at: at,
                disabled: false,
            }),
        }
    }

    #[test]
    fn emptying_reports_the_last_guest_once() {
        let mut t = Tracker::default();
        let now = Instant::now();
        assert_eq!(
            t.observe(&corpse(9, 0.0, false, Entity::NONE, 0), now),
            None
        );
        assert_eq!(t.observe(&corpse(9, 0.0, false, PLAYER, 5), now), None);
        assert_eq!(
            t.observe(&corpse(9, 0.0, true, PLAYER, 5), now),
            Some(PLAYER)
        );
        assert_eq!(t.observe(&corpse(9, 0.0, true, PLAYER, 5), now), None);
        // Seen empty first: nothing to tell.
        assert_eq!(t.observe(&corpse(10, 0.0, true, PLAYER, 5), now), None);
        // Emptied by nobody (the mod's own take of a corpse never opened).
        t.observe(&corpse(11, 0.0, false, Entity::NONE, 0), now);
        assert_eq!(
            t.observe(&corpse(11, 0.0, true, Entity::NONE, 0), now),
            None
        );
    }

    #[test]
    fn a_held_container_starts_nothing_until_the_hold_runs_out() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.observe(&corpse(9, 0.0, false, OTHER, 5), now);
        t.hold(Entity(9), now);
        assert_eq!(t.observe(&corpse(9, 0.0, true, OTHER, 5), now), None);
        let later = now + HOLD;
        t.observe(&corpse(9, 0.0, false, OTHER, 5), later);
        assert_eq!(
            t.observe(&corpse(9, 0.0, true, OTHER, 5), later),
            Some(OTHER)
        );
        t.hold(Entity(9), later);
        t.release(Entity(9));
        t.observe(&corpse(9, 0.0, false, OTHER, 5), later);
        assert_eq!(
            t.observe(&corpse(9, 0.0, true, OTHER, 5), later),
            Some(OTHER)
        );
    }

    #[test]
    fn another_players_interaction_keeps_a_container_busy_for_the_window() {
        let window = Duration::from_secs(10);
        let mut t = Tracker::default();
        let now = Instant::now();
        let opened = corpse(9, 0.0, false, OTHER, 77);
        t.observe(&opened, now);
        assert!(t.busy(&opened, PLAYER, now + Duration::from_secs(9), window));
        assert!(!t.busy(&opened, PLAYER, now + window, window));
        // The same interaction seen again keeps its first time.
        t.observe(&opened, now + Duration::from_secs(5));
        assert!(!t.busy(&opened, PLAYER, now + window, window));
        // Never busy for the player's own interactions or nobody's.
        assert!(!t.busy(&opened, OTHER, now, window));
        assert!(!t.busy(&corpse(9, 0.0, false, Entity::NONE, 0), PLAYER, now, window));
        // An interaction the mod has not seen counts as just now.
        let reopened = corpse(9, 0.0, false, OTHER, 99);
        assert!(t.busy(&reopened, PLAYER, now + window, window));
    }

    #[test]
    fn around_lists_nearest_first_without_empty_held_or_far_ones() {
        let mut t = Tracker::default();
        let now = Instant::now();
        for (id, x, empty) in [
            (1, 0.0, true),
            (2, 8.0, false),
            (3, 3.0, false),
            (4, 30.0, false),
            (5, 5.0, true),
            (6, 1.0, false),
        ] {
            t.observe(&corpse(id, x, empty, Entity::NONE, 0), now);
        }
        let tall = Container {
            position: [2.0, 9.0, 0.0],
            ..corpse(7, 0.0, false, Entity::NONE, 0)
        };
        t.observe(&tall, now);
        t.hold(Entity(6), now);
        let found = t.around([0.0; 3], 12.0, 4.0, Entity(1), now);
        assert_eq!(found, vec![Entity(3), Entity(2)]);
    }

    #[test]
    fn prune_forgets_unseen_containers_and_ended_holds() {
        let mut t = Tracker::default();
        let now = Instant::now();
        t.observe(&corpse(1, 0.0, false, Entity::NONE, 0), now);
        t.observe(&corpse(2, 0.0, false, Entity::NONE, 0), now + FORGET);
        t.hold(Entity(2), now);
        t.prune(now + FORGET);
        assert_eq!(t.len(), 1);
        assert_eq!(
            t.around([0.0; 3], 1.0, 1.0, Entity::NONE, now + FORGET),
            vec![Entity(2)]
        );
        t.forget(Entity(2));
        t.clear();
        assert_eq!(t.len(), 0);
    }
}
