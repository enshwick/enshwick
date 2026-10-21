//! Which copy of a survivor belongs to which altar.
//!
//! A slot is a survivor and an altar, and holds at most one entity: a
//! cluster of N altars can therefore hold at most N copies of a survivor,
//! and a copy only ever stands inside a zone. The original, the entity the
//! game's own record points at, is never touched: survivors not yet rescued
//! live outside every zone, and vanilla keeps them.
//!
//! Pure logic in whole 0.5 m cells, free of the game, so it can be tested.

use std::collections::BTreeMap;

/// A survivor's id, the hash of its template.
pub(crate) type Survivor = u32;
/// An entity id.
pub(crate) type Entity = u32;
/// An altar's identity: its entity.
pub(crate) type AltarKey = u64;

/// An altar's zone in whole cells, half-open like the engine's point test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Zone {
    /// The inclusive lower corner.
    pub(crate) min: [i64; 3],
    /// The exclusive upper corner.
    pub(crate) max: [i64; 3],
}

impl Zone {
    pub(crate) const fn new(min: [i64; 3], max: [i64; 3]) -> Self {
        Self { min, max }
    }

    /// Whether `point` lies inside: `min <= point < max` on every axis.
    fn contains_point(&self, point: [i64; 3]) -> bool {
        (0..3).all(|axis| self.min[axis] <= point[axis] && point[axis] < self.max[axis])
    }
}

/// What a copy staff's use does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyStaff {
    /// Places a new copy: the altar has none.
    Place,
    /// Moves the altar's copy to the target, free.
    Move(Entity),
    /// Refused, nothing spent.
    Refuse(Refusal),
}

/// Why a copy staff's use is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The target lies outside every altar zone.
    OutsideZones,
    /// The original stands at the target's altar.
    OriginalThere,
    /// The survivor has as many entities as there are altars.
    Full,
}

/// What to do with an entity the uniqueness pass sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Keep,
    /// A second copy in a slot that already has one.
    Remove,
    /// A copy outside every known zone: its altar is gone, or not known yet
    /// while the altars arrive. It stays filed until the caller removes it.
    Outside,
}

#[derive(Debug, Default)]
pub(crate) struct Copies {
    /// The altars' zones, as the altar mod grows them.
    altars: Vec<(AltarKey, Zone)>,
    /// Whether zones were ever known: right after a load none are, which
    /// is not the same as no altar left.
    known: bool,
    slots: BTreeMap<(Survivor, AltarKey), Entity>,
    /// Each kept entity's survivor and home.
    homes: BTreeMap<Entity, (Survivor, [i64; 3])>,
    /// How many kept entities each survivor has, so the lookups that ask on
    /// every frame never walk `homes`.
    counts: BTreeMap<Survivor, usize>,
}

impl Copies {
    /// Takes the altars' zones. No zones before any were known are no news;
    /// after, they mean the last altar is gone, and every copy stands
    /// outside the zones.
    pub(crate) fn set_altars(&mut self, altars: impl IntoIterator<Item = (AltarKey, Zone)>) {
        let altars: Vec<(AltarKey, Zone)> = altars.into_iter().collect();
        if altars.is_empty() && !self.known {
            return;
        }
        self.known = true;
        self.altars = altars;
        self.altars.sort_by_key(|(key, _)| *key);
        let known: Vec<AltarKey> = self.altars.iter().map(|(k, _)| *k).collect();
        self.slots
            .retain(|(_, altar), _| known.binary_search(altar).is_ok());
    }

    /// The altar `point` belongs to: of the zones that hold it, the one
    /// whose altar stands nearest, so each altar keeps its own surroundings
    /// where zones overlap; ties go to the smaller key.
    pub(crate) fn altar_at(&self, point: [i64; 3]) -> Option<AltarKey> {
        self.altars
            .iter()
            .filter(|(_, zone)| zone.contains_point(point))
            .min_by_key(|(key, zone)| {
                let centre = |a: usize| i64::midpoint(zone.min[a], zone.max[a]);
                let d: i128 = [0, 2]
                    .iter()
                    .map(|&a| i128::from(centre(a) - point[a]).pow(2))
                    .sum();
                (d, *key)
            })
            .map(|(key, _)| *key)
    }

    /// The entity a summoning staff aimed at `target` should move, or `None`
    /// to spawn a new copy there. `Some(original)` outside every zone, so the
    /// staff behaves as vanilla there.
    pub(crate) fn for_spawn(
        &self,
        survivor: Survivor,
        target: [i64; 3],
        original: Entity,
    ) -> Option<Entity> {
        match self.altar_at(target) {
            Some(altar) => self.slots.get(&(survivor, altar)).copied(),
            None => (original != 0).then_some(original),
        }
    }

    /// What a copy staff of `survivor` used at `target` does; `original` is
    /// the entity the game's own record names, 0 when unknown. `limit` turns
    /// the number of altars and the survivor's entities into the most
    /// entities it may have, asked only before a new copy.
    pub(crate) fn copy_staff(
        &self,
        survivor: Survivor,
        target: [i64; 3],
        original: Entity,
        limit: impl FnOnce(usize, usize) -> usize,
    ) -> CopyStaff {
        let Some(altar) = self.altar_at(target) else {
            return CopyStaff::Refuse(Refusal::OutsideZones);
        };
        let original_home = self
            .homes
            .get(&original)
            .filter(|_| original != 0)
            .and_then(|(_, home)| self.altar_at(*home));
        if original_home == Some(altar) {
            return CopyStaff::Refuse(Refusal::OriginalThere);
        }
        match self.slots.get(&(survivor, altar)) {
            Some(&e) if e == original => CopyStaff::Refuse(Refusal::OriginalThere),
            Some(&e) => CopyStaff::Move(e),
            None => {
                let count = self.entity_count(survivor, original);
                if count >= limit(self.altars.len(), count) {
                    CopyStaff::Refuse(Refusal::Full)
                } else {
                    CopyStaff::Place
                }
            }
        }
    }

    /// The copy a bed or an order at `point` belongs to: the copy whose home
    /// is nearest, so a bed and its sleeper agree even where zones overlap;
    /// the original outside every zone.
    pub(crate) fn at(&self, survivor: Survivor, point: [i64; 3], original: Entity) -> Entity {
        let Some(altar) = self.altar_at(point) else {
            return original;
        };
        if let Some(&copy) = self.slots.get(&(survivor, altar)) {
            return copy;
        }
        let distance = |home: &[i64; 3]| -> i128 {
            (0..3).map(|a| i128::from(home[a] - point[a]).pow(2)).sum()
        };
        self.homes
            .iter()
            .filter(|(_, (s, _))| *s == survivor)
            .min_by_key(|(e, (_, home))| (distance(home), **e))
            .map_or(original, |(e, _)| *e)
    }

    /// Decides about `entity`, a copy of `survivor` standing at `home`, and
    /// files it in its slot when it may stay. `original` is the entity the
    /// game's own record points at.
    pub(crate) fn observe(
        &mut self,
        survivor: Survivor,
        entity: Entity,
        home: [i64; 3],
        original: Entity,
    ) -> Verdict {
        let altar = self.altar_at(home);
        self.set_home(entity, survivor, home);
        // A copy moved away keeps no slot behind it.
        self.slots
            .retain(|(s, a), e| !(*s == survivor && *e == entity && Some(*a) != altar));
        let Some(altar) = altar else {
            if entity == original || original == 0 {
                return Verdict::Keep;
            }
            return Verdict::Outside;
        };
        match self.slots.get(&(survivor, altar)) {
            None => {
                self.slots.insert((survivor, altar), entity);
                Verdict::Keep
            }
            Some(&e) if e == entity => Verdict::Keep,
            // The original wins its own slot back.
            Some(_) if entity == original => {
                self.slots.insert((survivor, altar), entity);
                Verdict::Keep
            }
            Some(_) => {
                self.drop_home(entity);
                Verdict::Remove
            }
        }
    }

    /// Forgets an entity the game destroyed.
    pub(crate) fn forget(&mut self, entity: Entity) {
        self.slots.retain(|_, e| *e != entity);
        self.drop_home(entity);
    }

    /// The copy in an altar's slot.
    pub(crate) fn slot(&self, survivor: Survivor, altar: AltarKey) -> Option<Entity> {
        self.slots.get(&(survivor, altar)).copied()
    }

    /// Every kept copy of `survivor` with its home.
    pub(crate) fn copies_of(
        &self,
        survivor: Survivor,
    ) -> impl Iterator<Item = (Entity, [i64; 3])> + '_ {
        self.homes
            .iter()
            .filter(move |(_, (s, _))| *s == survivor)
            .map(|(&e, &(_, home))| (e, home))
    }

    /// How many kept copies of `survivor` there are.
    pub(crate) fn copy_count(&self, survivor: Survivor) -> usize {
        self.counts.get(&survivor).copied().unwrap_or(0)
    }

    /// How many entities `survivor` has, its `original` counted wherever it
    /// stands: unseen, such as out in the world where no player is near, it
    /// still exists.
    pub(crate) fn entity_count(&self, survivor: Survivor, original: Entity) -> usize {
        let unseen = original != 0 && !self.homes.contains_key(&original);
        self.copy_count(survivor) + usize::from(unseen)
    }

    fn set_home(&mut self, entity: Entity, survivor: Survivor, home: [i64; 3]) {
        if let Some((old, _)) = self.homes.insert(entity, (survivor, home)) {
            self.uncount(old);
        }
        *self.counts.entry(survivor).or_default() += 1;
    }

    fn drop_home(&mut self, entity: Entity) {
        if let Some((survivor, _)) = self.homes.remove(&entity) {
            self.uncount(survivor);
        }
    }

    fn uncount(&mut self, survivor: Survivor) {
        if let Some(count) = self.counts.get_mut(&survivor) {
            *count -= 1;
            if *count == 0 {
                self.counts.remove(&survivor);
            }
        }
    }

    /// Whether the altar zones are known yet, none at all included.
    pub(crate) fn knows_altars(&self) -> bool {
        self.known
    }

    /// How many copies of `survivor` stand in the zones.
    #[cfg(test)]
    pub(crate) fn count(&self, survivor: Survivor) -> usize {
        self.slots.keys().filter(|(s, _)| *s == survivor).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMITH: Survivor = 0x47a2_094a;

    fn zone(x: i64) -> Zone {
        Zone::new([x, 0, 0], [x + 80, 80, 80])
    }

    fn two_altars() -> Copies {
        let mut copies = Copies::default();
        copies.set_altars([(1, zone(0)), (2, zone(80))]);
        copies
    }

    #[test]
    fn each_altar_gets_one_copy() {
        let mut copies = two_altars();
        assert_eq!(copies.observe(SMITH, 10, [10, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 11, [90, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 12, [95, 10, 10], 10), Verdict::Remove);
        assert_eq!(copies.count(SMITH), 2);
    }

    #[test]
    fn the_staff_spawns_where_no_copy_is_and_moves_where_one_is() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        assert_eq!(copies.for_spawn(SMITH, [50, 10, 10], 10), Some(10));
        assert_eq!(copies.for_spawn(SMITH, [90, 10, 10], 10), None);
        // Outside every zone the staff moves the original, as vanilla does.
        assert_eq!(copies.for_spawn(SMITH, [500, 10, 10], 10), Some(10));
    }

    #[test]
    fn a_copy_staff_places_moves_or_is_refused() {
        let mut copies = Copies::default();
        copies.set_altars([(1, zone(0)), (2, zone(80)), (3, zone(160))]);
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        // An altar without a copy: a new one.
        assert_eq!(
            copies.copy_staff(SMITH, [170, 10, 10], 10, altars),
            CopyStaff::Place
        );
        // An altar with a copy: that copy moves, free.
        assert_eq!(
            copies.copy_staff(SMITH, [95, 10, 10], 10, altars),
            CopyStaff::Move(11)
        );
        // Where the original stands: refused.
        assert_eq!(
            copies.copy_staff(SMITH, [20, 10, 10], 10, altars),
            CopyStaff::Refuse(Refusal::OriginalThere)
        );
        assert_eq!(
            copies.copy_staff(SMITH, [900, 10, 10], 10, altars),
            CopyStaff::Refuse(Refusal::OutsideZones)
        );
        // Every altar taken counts the original: one entity per altar.
        copies.observe(SMITH, 12, [170, 10, 10], 10);
        assert_eq!(copies.copy_count(SMITH), 3);
        assert_eq!(
            copies.copy_staff(SMITH, [175, 10, 10], 10, altars),
            CopyStaff::Move(12)
        );
    }

    /// The limit the mod answers before other mods: one entity per altar.
    fn altars(altars: usize, _copies: usize) -> usize {
        altars
    }

    #[test]
    fn another_mods_limit_refuses_or_allows_a_new_copy() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [900, 10, 10], 10);
        assert_eq!(
            copies.copy_staff(SMITH, [90, 10, 10], 10, |_, _| 1),
            CopyStaff::Refuse(Refusal::Full)
        );
        assert_eq!(
            copies.copy_staff(SMITH, [90, 10, 10], 10, |altars, copies| {
                assert_eq!((altars, copies), (2, 1));
                9
            }),
            CopyStaff::Place
        );
    }

    #[test]
    fn a_copy_staff_is_refused_once_the_survivor_has_one_per_altar() {
        let mut copies = two_altars();
        // The original out in the world, a copy at each altar.
        copies.observe(SMITH, 10, [900, 10, 10], 10);
        copies.observe(SMITH, 11, [10, 10, 10], 10);
        assert_eq!(
            copies.copy_staff(SMITH, [90, 10, 10], 10, altars),
            CopyStaff::Refuse(Refusal::Full)
        );
    }

    #[test]
    fn an_unseen_original_still_counts_toward_the_limit() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [900, 10, 10], 10);
        copies.observe(SMITH, 11, [10, 10, 10], 10);
        // Its chunk unloads: the uniqueness pass no longer sees it.
        copies.forget(10);
        assert_eq!(copies.entity_count(SMITH, 10), 2);
        assert_eq!(
            copies.copy_staff(SMITH, [90, 10, 10], 10, altars),
            CopyStaff::Refuse(Refusal::Full)
        );
        assert_eq!(copies.entity_count(SMITH, 0), 1, "an unknown original");
    }

    #[test]
    fn the_original_is_never_removed() {
        let mut copies = two_altars();
        assert_eq!(copies.observe(SMITH, 11, [10, 10, 10], 10), Verdict::Keep);
        // The original shows up in the same altar: it takes the slot back.
        assert_eq!(copies.observe(SMITH, 10, [20, 10, 10], 10), Verdict::Keep);
        assert_eq!(copies.observe(SMITH, 11, [10, 10, 10], 10), Verdict::Remove);
        // Out in the world, not yet rescued, it stays too.
        assert_eq!(copies.observe(SMITH, 10, [900, 10, 10], 10), Verdict::Keep);
    }

    #[test]
    fn a_copy_outside_every_zone_goes() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        assert_eq!(
            copies.observe(SMITH, 11, [900, 10, 10], 10),
            Verdict::Outside
        );
        assert_eq!(copies.count(SMITH), 0);
        assert_eq!(copies.copy_count(SMITH), 1, "filed until it is removed");
        copies.forget(11);
        assert_eq!(copies.copy_count(SMITH), 0);
    }

    #[test]
    fn overlapping_zones_go_to_the_nearest_altar() {
        let mut copies = Copies::default();
        copies.set_altars([(7, zone(0)), (3, zone(40))]);
        // Zone 7 is centred on x 40, zone 3 on x 80.
        assert_eq!(copies.altar_at([50, 10, 40]), Some(7));
        assert_eq!(copies.altar_at([70, 10, 40]), Some(3));
        assert_eq!(copies.altar_at([60, 10, 40]), Some(3));
        assert_eq!(copies.altar_at([10, 10, 10]), Some(7));
    }

    #[test]
    fn beds_and_orders_find_the_copy_of_their_altar() {
        let mut copies = two_altars();
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        assert_eq!(copies.at(SMITH, [100, 10, 10], 10), 11);
        assert_eq!(copies.at(SMITH, [20, 10, 10], 10), 10);
        assert_eq!(copies.at(SMITH, [900, 10, 10], 10), 10);
    }

    #[test]
    fn where_zones_overlap_a_bed_belongs_to_the_nearest_copy() {
        let mut copies = Copies::default();
        let (a, b) = (
            Zone::new([0, 0, 0], [80, 80, 80]),
            Zone::new([40, 0, 0], [120, 80, 80]),
        );
        copies.set_altars([(1, a), (2, b)]);
        copies.observe(SMITH, 10, [45, 10, 10], 10);
        copies.observe(SMITH, 11, [110, 10, 10], 10);
        // In both zones, nearer the original's home: the original's bed.
        assert_eq!(copies.at(SMITH, [50, 10, 10], 10), 10);
        assert_eq!(copies.at(SMITH, [100, 10, 10], 10), 11);
    }

    #[test]
    fn a_removed_altar_frees_its_slots() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.set_altars([(1, zone(0))]);
        assert_eq!(copies.count(SMITH), 0);
        assert_eq!(
            copies.observe(SMITH, 11, [90, 10, 10], 10),
            Verdict::Outside
        );
    }

    #[test]
    fn a_destroyed_entity_frees_its_slot() {
        let mut copies = two_altars();
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.forget(11);
        assert_eq!(copies.for_spawn(SMITH, [90, 10, 10], 10), None);
    }

    #[test]
    fn the_copy_count_follows_every_change() {
        let mut copies = two_altars();
        let counted = |c: &Copies| (c.copy_count(SMITH), c.copies_of(SMITH).count());
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.observe(SMITH, 11, [91, 10, 10], 10);
        assert_eq!(counted(&copies), (2, 2));
        assert_eq!(copies.observe(SMITH, 12, [95, 10, 10], 10), Verdict::Remove);
        assert_eq!(counted(&copies), (2, 2));
        copies.forget(11);
        copies.forget(11);
        assert_eq!(counted(&copies), (1, 1));
        copies.forget(10);
        assert_eq!(counted(&copies), (0, 0));
    }

    #[test]
    fn no_zones_before_any_are_no_news_but_after_the_last_altar_goes() {
        let mut copies = Copies::default();
        copies.set_altars([]);
        assert!(!copies.knows_altars());
        copies.set_altars([(1, zone(0)), (2, zone(80))]);
        copies.observe(SMITH, 10, [10, 10, 10], 10);
        copies.observe(SMITH, 11, [90, 10, 10], 10);
        copies.set_altars([(1, zone(0))]);
        copies.set_altars([]);
        assert!(copies.knows_altars(), "the last altar went");
        assert_eq!(copies.count(SMITH), 0);
        assert_eq!(
            copies.observe(SMITH, 11, [90, 10, 10], 10),
            Verdict::Outside
        );
        assert_eq!(copies.observe(SMITH, 10, [10, 10, 10], 10), Verdict::Keep);
        assert_eq!(
            copies.copy_staff(SMITH, [10, 10, 10], 10, altars),
            CopyStaff::Refuse(Refusal::OutsideZones)
        );
    }
}
