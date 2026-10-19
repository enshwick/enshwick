//! Properties of the clusters: budgets kept, growth never shrinking, order
//! independence, checked on random layouts and by brute force on a lattice.

#[cfg(test)]
mod tests {
    use enshwick_geometry::{Altar, Box3, ClusterError, Placement, clusters, union_footprint_area};
    use proptest::prelude::*;

    /// A square altar zone of `edge` cells with its corner at (x, z), height 40.
    fn square(key: u64, x: i64, z: i64, edge: i64) -> Altar {
        Altar::new(key, Box3::new([x, 0, z], [x + edge, 40, z + edge]))
    }

    /// An altar centred at (x, 0, z) whose level `l` zone is a cube of `10 l`
    /// cells, at every level from 1 to `level`, like the engine's zones, which
    /// grow around the altar. Level 0 is an empty zone.
    fn upgraded(key: u64, x: i64, z: i64, level: i64) -> Altar {
        let zone = |l: i64| {
            Box3::new(
                [x - 5 * l, -5 * l, z - 5 * l],
                [x + 5 * l, 5 * l, z + 5 * l],
            )
        };
        let levels = if level == 0 {
            vec![zone(0)]
        } else {
            (1..=level).map(zone).collect()
        };
        Altar { key, levels }
    }

    fn by_key(placements: &[Placement], key: u64) -> Placement {
        *placements.iter().find(|p| p.key == key).unwrap()
    }

    #[test]
    fn a_lone_altar_keeps_its_zone() {
        let altar = square(7, 10, 10, 40);
        let result = clusters(std::slice::from_ref(&altar)).unwrap();
        assert_eq!(
            result,
            vec![Placement {
                key: 7,
                cluster: 7,
                growth: 0,
                zone: altar.zone()
            }]
        );
    }

    #[test]
    fn no_altars_no_zones() {
        assert_eq!(clusters(&[]).unwrap(), vec![]);
    }

    #[test]
    fn edge_to_edge_altars_form_one_cluster_without_growth() {
        let result = clusters(&[square(1, 0, 0, 40), square(2, 40, 0, 40)]).unwrap();
        assert_eq!(by_key(&result, 1).cluster, 1);
        assert_eq!(by_key(&result, 2).cluster, 1);
        assert_eq!(by_key(&result, 1).growth, 0);
        assert_eq!(by_key(&result, 2).zone, square(2, 40, 0, 40).zone());
    }

    #[test]
    fn overlap_is_given_back_by_growth_rounded_to_the_nearest_step() {
        // Union 60 x 40 = 2400, budget 3200. Areas by k: k=3 gives 66 x 46 = 3036,
        // k=4 gives 68 x 48 = 3264. The budget is 164 into a step of 228, which is
        // more than half, so k rounds up to 4.
        let result = clusters(&[square(1, 0, 0, 40), square(2, 20, 0, 40)]).unwrap();
        let a = by_key(&result, 1);
        let b = by_key(&result, 2);
        assert_eq!((a.cluster, b.cluster), (1, 1));
        assert_eq!((a.growth, b.growth), (4, 4));
        assert_eq!(a.zone, Box3::new([-4, 0, -4], [44, 40, 44]));
        assert_eq!(b.zone, Box3::new([16, 0, -4], [64, 40, 44]));
    }

    #[test]
    fn less_than_half_a_step_rounds_down() {
        // Overlap of 2 cells: union 78 x 40 = 3120, budget 3200. k=0 gives 3120,
        // k=1 gives 80 x 42 = 3360. 80 into a step of 240 is less than half.
        let result = clusters(&[square(1, 0, 0, 40), square(2, 38, 0, 40)]).unwrap();
        assert_eq!(by_key(&result, 1).growth, 0);
    }

    #[test]
    fn exactly_half_a_step_rounds_up() {
        // Five 1 x 1 altars on the same cell: union 1, budget 5. k=0 gives 1,
        // k=1 gives 3 x 3 = 9. The budget is 4 into a step of 8, exactly half.
        let altars: Vec<Altar> = (1..=5)
            .map(|key| Altar::new(key, Box3::new([0, 0, 0], [1, 1, 1])))
            .collect();
        let result = clusters(&altars).unwrap();
        assert!(result.iter().all(|p| p.growth == 1 && p.cluster == 1));
    }

    #[test]
    fn far_apart_altars_stay_separate() {
        let result = clusters(&[square(1, 0, 0, 40), square(2, 100, 0, 40)]).unwrap();
        assert_eq!(by_key(&result, 1).cluster, 1);
        assert_eq!(by_key(&result, 2).cluster, 2);
    }

    #[test]
    fn clusters_whose_grown_shapes_touch_merge_without_shrinking() {
        // Altars 1 and 2 overlap and grow by 4 to x 64. Altar 3 starts at x 64:
        // its vanilla zone does not touch theirs (60 < 64), the grown one does.
        // On its own budget the merged cluster would round to k=2: union 4000,
        // budget 4800, k=2 gives 108 x 44 = 4752. The group {1, 2} still needs 4,
        // so the merged cluster keeps 4 and nobody's zone shrinks.
        let result = clusters(&[
            square(1, 0, 0, 40),
            square(2, 20, 0, 40),
            square(3, 64, 0, 40),
        ])
        .unwrap();
        assert!(result.iter().all(|p| p.cluster == 1 && p.growth == 4));
    }

    #[test]
    fn an_upgrade_never_shrinks_a_neighbour() {
        // Two level 1 altars (10 x 10) on one spot: union 100, budget 200,
        // A(1) + A(2) = 144 + 196 = 340 < 400, A(2) + A(3) = 196 + 256 > 400: k=2.
        // After altar 2's upgrade to 20 x 20 the pair on its own would round to
        // k=1 (union 400, budget 500, A(1) + A(2) = 484 + 576 > 1000), shrinking
        // altar 1. The earlier pair is still a group of the cluster, so k stays 2.
        let before = clusters(&[upgraded(1, 0, 0, 1), upgraded(2, 0, 0, 1)]).unwrap();
        let after = clusters(&[upgraded(1, 0, 0, 1), upgraded(2, 0, 0, 2)]).unwrap();
        assert_eq!(by_key(&before, 1).growth, 2);
        assert_eq!(by_key(&after, 1).growth, 2);
        assert_eq!(by_key(&after, 1).zone.min[0], -7);
    }

    #[test]
    fn every_member_gets_the_tallest_height_around_its_own_centre() {
        let low = Altar::new(1, Box3::new([0, 10, 0], [40, 50, 40]));
        let tall = Altar::new(2, Box3::new([40, 0, 0], [80, 80, 40]));
        let result = clusters(&[low, tall.clone()]).unwrap();
        assert_eq!(
            by_key(&result, 1).zone,
            Box3::new([0, -10, 0], [40, 70, 40])
        );
        assert_eq!(by_key(&result, 2).zone, tall.zone());
    }

    #[test]
    fn an_altar_inside_a_zone_adds_its_budget() {
        let result = clusters(&[square(1, 0, 0, 40), square(2, 10, 10, 20)]).unwrap();
        // Union 1600, budget 2000. A(1) = 42^2 = 1764, A(2) = 44^2 = 1936,
        // A(3) = 46^2 = 2116: 64 of 180, down to 2.
        assert!(result.iter().all(|p| p.cluster == 1 && p.growth == 2));
    }

    #[test]
    fn empty_zones_take_part_in_nothing() {
        // A level 0 altar has a zone of size 0.
        let empty = Altar::new(2, Box3::new([40, 0, 0], [40, 0, 0]));
        let result = clusters(&[square(1, 0, 0, 40), empty.clone()]).unwrap();
        assert_eq!(
            by_key(&result, 2),
            Placement {
                key: 2,
                cluster: 2,
                growth: 0,
                zone: empty.zone()
            }
        );
        assert_eq!(by_key(&result, 1).zone, square(1, 0, 0, 40).zone());
    }

    #[test]
    fn bad_input_is_an_error_not_a_panic() {
        assert_eq!(
            clusters(&[square(1, 0, 0, 40), square(1, 100, 0, 40)]),
            Err(ClusterError::DuplicateKey(1))
        );
        let inverted = Altar::new(3, Box3::new([10, 0, 0], [0, 40, 40]));
        assert_eq!(clusters(&[inverted]), Err(ClusterError::InvertedBox(3)));
        let none = Altar {
            key: 4,
            levels: vec![],
        };
        assert_eq!(clusters(&[none]), Err(ClusterError::NoLevels(4)));
        let shifted = Altar {
            key: 5,
            levels: vec![
                Box3::new([0, 0, 0], [10, 10, 10]),
                Box3::new([5, 0, 0], [25, 20, 20]),
            ],
        };
        assert_eq!(clusters(&[shifted]), Err(ClusterError::NotNested(5)));
    }

    #[test]
    fn union_area_counts_overlap_once() {
        let a = Box3::new([0, 0, 0], [40, 1, 40]);
        let b = Box3::new([20, 0, 20], [60, 1, 60]);
        assert_eq!(union_footprint_area(&[a, b]), 1600 + 1600 - 400);
        assert_eq!(union_footprint_area(&[]), 0);
    }

    /// Up to `count` altars of levels 0 to 3 near each other.
    fn altar_set(count: usize) -> impl Strategy<Value = Vec<Altar>> {
        prop::collection::vec((-30i64..30, -30i64..30, 0i64..4), 0..count).prop_map(|raw| {
            raw.into_iter()
                .enumerate()
                .map(|(i, (x, z, level))| upgraded(i as u64 * 3 + 1, x, z, level))
                .collect()
        })
    }

    fn members(result: &[Placement], cluster: u64) -> Vec<Placement> {
        result
            .iter()
            .copied()
            .filter(|p| p.cluster == cluster)
            .collect()
    }

    /// The growth a group of zones would get on its own: the least k with
    /// `2 B < A(k) + A(k + 1)`.
    fn nearest_growth(zones: &[Box3]) -> i64 {
        let budget: i128 = zones.iter().map(Box3::footprint_area).sum();
        let area =
            |k: i64| union_footprint_area(&zones.iter().map(|z| z.grown(k)).collect::<Vec<_>>());
        (0..i64::MAX)
            .find(|&k| 2 * budget < area(k) + area(k + 1))
            .unwrap()
    }

    /// The definition, by brute force: the most any group of the cluster, each
    /// member at any level up to its current one or absent, would grow.
    fn lattice_growth(chains: &[Vec<Box3>]) -> i64 {
        let combos: usize = chains.iter().map(|c| c.len() + 1).product();
        let mut best = 0;
        for mut combo in 0..combos {
            let mut zones = Vec::new();
            for chain in chains {
                let level = combo % (chain.len() + 1);
                combo /= chain.len() + 1;
                if level > 0 && chain[level - 1].footprint_area() > 0 {
                    zones.push(chain[level - 1]);
                }
            }
            if !zones.is_empty() {
                best = best.max(nearest_growth(&zones));
            }
        }
        best
    }

    fn shuffled(altars: &[Altar], seed: u64) -> Vec<Altar> {
        let mut out = altars.to_vec();
        let mut state = seed | 1;
        for i in (1..out.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let bound = u64::try_from(i).unwrap() + 1;
            out.swap(i, usize::try_from(state % bound).unwrap());
        }
        out
    }

    fn contains(outer: &Box3, inner: &Box3) -> bool {
        (0..3).all(|axis| outer.min[axis] <= inner.min[axis] && inner.max[axis] <= outer.max[axis])
    }

    proptest! {
        #[test]
        fn order_never_matters(altars in altar_set(7), seed in any::<u64>()) {
            prop_assert_eq!(clusters(&altars).unwrap(), clusters(&shuffled(&altars, seed)).unwrap());
        }

        #[test]
        fn zones_only_grow_and_members_grow_alike(altars in altar_set(7)) {
            let result = clusters(&altars).unwrap();
            prop_assert_eq!(result.len(), altars.len());
            for p in &result {
                let vanilla = altars.iter().find(|a| a.key == p.key).unwrap().zone();
                prop_assert!(p.growth >= 0);
                prop_assert!(contains(&p.zone, &vanilla));
                if vanilla.footprint_area() > 0 {
                    prop_assert_eq!(p.zone.min[0], vanilla.min[0] - p.growth);
                    prop_assert_eq!(p.zone.max[2], vanilla.max[2] + p.growth);
                }
                for q in members(&result, p.cluster) {
                    prop_assert_eq!(q.growth, p.growth);
                    prop_assert_eq!(q.zone.max[1] - q.zone.min[1], p.zone.max[1] - p.zone.min[1]);
                }
            }
        }

        #[test]
        fn growth_is_the_lattice_maximum(altars in altar_set(5)) {
            let result = clusters(&altars).unwrap();
            for p in result.iter().filter(|p| p.zone.footprint_area() > 0 && p.key == p.cluster) {
                let chains: Vec<Vec<Box3>> = members(&result, p.cluster)
                    .iter()
                    .map(|q| altars.iter().find(|a| a.key == q.key).unwrap().levels.clone())
                    .collect();
                prop_assert_eq!(p.growth, lattice_growth(&chains));
            }
        }

        #[test]
        fn no_cluster_gets_less_than_its_budget(altars in altar_set(7)) {
            let result = clusters(&altars).unwrap();
            for p in result.iter().filter(|p| p.zone.footprint_area() > 0) {
                let current: Vec<Box3> = members(&result, p.cluster)
                    .iter()
                    .map(|q| altars.iter().find(|a| a.key == q.key).unwrap().zone())
                    .collect();
                prop_assert!(p.growth >= nearest_growth(&current));
            }
        }

        #[test]
        fn placing_or_upgrading_an_altar_never_shrinks_a_zone(
            altars in altar_set(6),
            x in -30i64..30,
            z in -30i64..30,
            pick in any::<prop::sample::Index>(),
            place in any::<bool>(),
        ) {
            let mut next = altars.clone();
            if place || altars.is_empty() {
                next.push(upgraded(1000, x, z, 1));
            } else {
                let i = pick.index(altars.len());
                let level = i64::try_from(altars[i].levels.len()).unwrap();
                let has_zone = altars[i].zone().footprint_area() > 0;
                let (cx, cz) = (i64::midpoint(altars[i].zone().min[0], altars[i].zone().max[0]), i64::midpoint(altars[i].zone().min[2], altars[i].zone().max[2]));
                next[i] = upgraded(altars[i].key, cx, cz, if has_zone { level + 1 } else { 1 });
            }
            let before = clusters(&altars).unwrap();
            let after = clusters(&next).unwrap();
            for b in &before {
                let a = after.iter().find(|a| a.key == b.key).unwrap();
                prop_assert!(contains(&a.zone, &b.zone), "{:?} shrank from {:?} to {:?}", b.key, b.zone, a.zone);
            }
        }

        #[test]
        fn separate_clusters_never_touch(altars in altar_set(7)) {
            let result = clusters(&altars).unwrap();
            for p in &result {
                for q in &result {
                    if p.cluster != q.cluster && p.zone.footprint_area() > 0 && q.zone.footprint_area() > 0 {
                        prop_assert!(!p.zone.footprint_touches(&q.zone));
                    }
                }
            }
        }
    }
}
