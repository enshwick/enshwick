//! Which base and which zone hold a point, as the game answers it, over any
//! number of listed zones.
//!
//! A point lies in a box when `min <= p < max` on every axis. A base with one
//! zone is tested against its box alone; a base of more zones against each
//! of its zones, after its box when the game tests that. The first base in
//! index order wins.

use super::grouping::ZoneSet;
use super::limit::Aabb;

/// The bases of a world as the lookups read them.
pub(crate) trait Bases {
    /// How many bases there are.
    fn count(&self) -> usize;
    /// The box around base `i`'s zones.
    fn bounds(&self, i: usize) -> Aabb;
    /// The zones of base `i`.
    fn members(&self, i: usize) -> ZoneSet;
    /// How many zones the list holds.
    fn listed(&self) -> usize;
    /// Zone `z`'s box; `z` is below [`Bases::listed`].
    fn zone(&self, z: usize) -> Aabb;
}

/// What holds a point: a base with one zone, by its box, or a zone of a
/// base with more.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    /// Base `i`, whose box is its only zone.
    Base(usize),
    /// Zone `z` of the list.
    Zone(usize),
}

impl Aabb {
    /// Whether `p` lies in the box, the low faces in and the high faces out.
    #[must_use]
    pub(crate) fn holds(&self, p: [f32; 3]) -> bool {
        (0..3).all(|axis| self.min[axis] <= p[axis] && p[axis] < self.max[axis])
    }
}

/// The listed zones of `members`.
fn listed(bases: &impl Bases, members: ZoneSet) -> impl Iterator<Item = usize> {
    let listed = bases.listed();
    members.zones().take_while(move |&z| z < listed)
}

/// Whether base `i` holds `p`.
#[must_use]
pub(crate) fn in_base(bases: &impl Bases, i: usize, p: [f32; 3]) -> bool {
    if !bases.bounds(i).holds(p) {
        return false;
    }
    let members = bases.members(i);
    members.len() == 1 || listed(bases, members).any(|z| bases.zone(z).holds(p))
}

/// The first base that holds `p`.
#[must_use]
pub(crate) fn base_at(bases: &impl Bases, p: [f32; 3]) -> Option<usize> {
    (0..bases.count()).find(|&i| in_base(bases, i, p))
}

/// The first box of a base that holds `p`: a base's own with one zone, a
/// zone of a base with more, with no test of that base's box first.
#[must_use]
pub(crate) fn holder_at(bases: &impl Bases, p: [f32; 3]) -> Option<Holder> {
    (0..bases.count()).find_map(|i| {
        let members = bases.members(i);
        if members.len() == 1 {
            bases.bounds(i).holds(p).then_some(Holder::Base(i))
        } else {
            listed(bases, members)
                .find(|&z| bases.zone(z).holds(p))
                .map(Holder::Zone)
        }
    })
}

/// The listed zones of the base that holds `p`, in list order.
#[must_use]
pub(crate) fn zones_at(bases: &impl Bases, p: [f32; 3]) -> Option<impl Iterator<Item = usize>> {
    base_at(bases, p).map(|i| listed(bases, bases.members(i)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base_groups::grouping::{Base, group};

    /// A world: its zone list and the bases the grouping made of it.
    #[derive(Debug)]
    struct World {
        zones: Vec<Aabb>,
        bases: Vec<Base>,
    }

    impl World {
        fn new(zones: Vec<Aabb>) -> Self {
            let bases = group(&zones, 16).to_vec();
            Self { zones, bases }
        }
    }

    impl Bases for World {
        fn count(&self) -> usize {
            self.bases.len()
        }
        fn bounds(&self, i: usize) -> Aabb {
            self.bases[i].bounds
        }
        fn members(&self, i: usize) -> ZoneSet {
            self.bases[i].zones
        }
        fn listed(&self) -> usize {
            self.zones.len()
        }
        fn zone(&self, z: usize) -> Aabb {
            self.zones[z]
        }
    }

    fn square(x: f32, z: f32, side: f32) -> Aabb {
        Aabb {
            min: [x, 0.0, z],
            max: [x + side, 10.0, z + side],
        }
    }

    /// 40 zones in a row, each touching the next, and one apart: an L of
    /// two arms, so the base's box covers ground no zone does.
    fn world() -> World {
        let mut zones: Vec<Aabb> = (0u8..20)
            .map(|i| square(f32::from(i) * 10.0, 0.0, 10.0))
            .collect();
        zones.extend((1u8..=20).map(|i| square(0.0, f32::from(i) * 10.0, 10.0)));
        zones.push(square(1000.0, 1000.0, 10.0));
        World::new(zones)
    }

    #[test]
    fn a_point_in_a_zone_past_32_is_in_its_base() {
        let world = world();
        assert_eq!(world.bases.len(), 2);
        let far_arm = [5.0, 5.0, 195.0];
        assert_eq!(base_at(&world, far_arm), Some(0));
        assert!(in_base(&world, 0, far_arm));
        assert_eq!(holder_at(&world, far_arm), Some(Holder::Zone(38)));
        assert_eq!(base_at(&world, [1005.0, 5.0, 1005.0]), Some(1));
        assert_eq!(
            holder_at(&world, [1005.0, 5.0, 1005.0]),
            Some(Holder::Base(1))
        );
    }

    #[test]
    fn a_point_in_the_box_but_in_no_zone_is_in_no_base() {
        let world = world();
        let inner_corner = [150.0, 5.0, 150.0];
        assert!(world.bases[0].bounds.holds(inner_corner));
        assert_eq!(base_at(&world, inner_corner), None);
        assert_eq!(holder_at(&world, inner_corner), None);
        assert!(zones_at(&world, inner_corner).is_none());
    }

    #[test]
    fn the_high_faces_are_out() {
        let world = World::new(vec![square(0.0, 0.0, 10.0)]);
        assert_eq!(base_at(&world, [0.0, 0.0, 0.0]), Some(0));
        assert_eq!(base_at(&world, [10.0, 5.0, 5.0]), None);
        assert_eq!(base_at(&world, [5.0, f32::NAN, 5.0]), None);
    }

    #[test]
    fn the_zones_of_a_base_are_every_listed_zone_it_holds() {
        let world = world();
        let zones: Vec<usize> = zones_at(&world, [5.0, 5.0, 5.0]).unwrap().collect();
        assert_eq!(zones, (0..40).collect::<Vec<_>>());
    }

    /// The game's lookups as they read a base's 32-bit mask.
    mod by_mask {
        use super::*;

        fn mask(world: &World, i: usize) -> u32 {
            world.bases[i].zones.low_mask()
        }

        fn members(world: &World, i: usize) -> impl Iterator<Item = usize> + '_ {
            (0..32).filter(move |&m| mask(world, i) >> m & 1 == 1)
        }

        pub(super) fn in_base(world: &World, i: usize, p: [f32; 3]) -> bool {
            world.bases[i].bounds.holds(p)
                && (mask(world, i).is_power_of_two()
                    || members(world, i).any(|m| world.zones[m].holds(p)))
        }

        pub(super) fn base_at(world: &World, p: [f32; 3]) -> Option<usize> {
            (0..world.bases.len()).find(|&i| in_base(world, i, p))
        }

        pub(super) fn holder_at(world: &World, p: [f32; 3]) -> Option<Holder> {
            (0..world.bases.len()).find_map(|i| {
                if mask(world, i).is_power_of_two() {
                    world.bases[i].bounds.holds(p).then_some(Holder::Base(i))
                } else {
                    members(world, i)
                        .find(|&m| world.zones[m].holds(p))
                        .map(Holder::Zone)
                }
            })
        }
    }

    fn worlds() -> impl proptest::strategy::Strategy<Value = (World, Vec<[f32; 3]>)> {
        use proptest::prelude::*;
        let zones = prop::collection::vec((0u8..30, 0u8..30, 1u8..5), 0..=32).prop_map(|cells| {
            cells
                .into_iter()
                .map(|(x, z, half)| {
                    let (x, z, half) = (f32::from(x) * 2.0, f32::from(z) * 2.0, f32::from(half));
                    Aabb {
                        min: [x - half, -half, z - half],
                        max: [x + half, half, z + half],
                    }
                })
                .collect::<Vec<_>>()
        });
        let points = prop::collection::vec((-6i8..66, -6i8..6, -6i8..66), 1..40).prop_map(|ps| {
            ps.into_iter()
                .map(|(x, y, z)| [x, y, z].map(f32::from))
                .collect()
        });
        (zones.prop_map(World::new), points)
    }

    proptest::proptest! {
        #[test]
        fn up_to_32_zones_the_lookups_answer_as_the_game_s_masks((world, points) in worlds()) {
            for p in points {
                proptest::prop_assert_eq!(base_at(&world, p), by_mask::base_at(&world, p));
                proptest::prop_assert_eq!(holder_at(&world, p), by_mask::holder_at(&world, p));
                for i in 0..world.bases.len() {
                    proptest::prop_assert_eq!(in_base(&world, i, p), by_mask::in_base(&world, i, p));
                }
            }
        }
    }

    #[test]
    fn members_past_the_list_are_skipped() {
        let mut world = world();
        world.zones.truncate(30);
        let zones: Vec<usize> = zones_at(&world, [5.0, 5.0, 5.0]).unwrap().collect();
        assert_eq!(zones, (0..30).collect::<Vec<_>>());
        assert_eq!(base_at(&world, [5.0, 5.0, 195.0]), None);
    }
}
