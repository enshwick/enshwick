//! The bases altar zones form: zones that touch, directly or through other
//! zones, share one base, as the game groups them.

use super::limit::Aabb;

/// The most zones the game lists in a world.
/// Found on the 1.0 build 1269579, server and client.
pub(crate) const MAX_LISTED: usize = 0xff;

/// A set of altar zones, by their place in the game's zone list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ZoneSet([u64; 4]);

impl ZoneSet {
    /// Adds zone `zone`; a zone past the list's capacity is ignored.
    pub(crate) fn insert(&mut self, zone: usize) {
        if let Some(word) = self.0.get_mut(zone / 64) {
            *word |= 1 << (zone % 64);
        }
    }

    /// Whether the set holds zone `zone`.
    #[must_use]
    pub(crate) fn contains(&self, zone: usize) -> bool {
        self.0
            .get(zone / 64)
            .is_some_and(|word| word >> (zone % 64) & 1 == 1)
    }

    /// The zones, in list order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(at, &word)| {
            let mut rest = word;
            core::iter::from_fn(move || {
                (rest != 0).then(|| {
                    let bit = rest.trailing_zeros() as usize;
                    rest &= rest - 1;
                    at * 64 + bit
                })
            })
        })
    }

    /// How many zones the set holds.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }

    /// The first 32 zones as the game's own 32-bit member mask.
    #[must_use]
    pub(crate) fn low_mask(&self) -> u32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the game's mask is the low 32 bits"
        )]
        let low = self.0[0] as u32;
        low
    }
}

/// One base: its zones and the box around them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Base {
    /// The zones the base holds.
    pub(crate) zones: ZoneSet,
    /// The smallest box holding every zone of the base.
    pub(crate) bounds: Aabb,
    /// The base's first zone, which starts it.
    pub(crate) first: usize,
}

/// Groups `boxes`, the game's zone list, into at most `max_bases` bases in
/// the game's order: by their first zone. The zones of further groups stay
/// apart, in no base; zones past [`MAX_LISTED`] are ignored.
#[must_use]
pub(crate) fn group(boxes: &[Aabb], max_bases: usize) -> Vec<Base> {
    let boxes = &boxes[..boxes.len().min(MAX_LISTED)];
    let mut parent: Vec<usize> = (0..boxes.len()).collect();
    for i in 0..boxes.len() {
        for j in 0..i {
            if boxes[i].touches(&boxes[j]) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut bases: Vec<Base> = Vec::new();
    for (i, zone) in boxes.iter().enumerate() {
        let first = root(&mut parent, i);
        if let Some(base) = bases.iter_mut().find(|base| base.first == first) {
            base.zones.insert(i);
            base.bounds = base.bounds.union(zone);
        } else if bases.len() < max_bases {
            let mut zones = ZoneSet::default();
            zones.insert(i);
            bases.push(Base {
                zones,
                bounds: *zone,
                first,
            });
        }
    }
    bases
}

/// The lowest zone of `i`'s group, which every merge keeps as the root.
fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::base_groups::limit::MAX_BASES;

    fn cube(x: f32, z: f32, half: f32) -> Aabb {
        Aabb {
            min: [x - half, -half, z - half],
            max: [x + half, half, z + half],
        }
    }

    /// The members mask the game starts its grouping with for `n` zones,
    /// `shl edi, cl; dec edi; and edi, -2`, whose count wraps at 32.
    fn start_mask(n: u32) -> u32 {
        (1u32 << (n % 32)).wrapping_sub(1) & !1
    }

    /// The game's own grouping as the code runs it, unpatched: only the
    /// first 16 zones are join candidates and members, and a new base is
    /// never refused. Returns each base's member mask.
    fn vanilla(boxes: &[Aabb]) -> Vec<u32> {
        const SCAN: usize = 16;
        let n = u32::try_from(boxes.len()).unwrap();
        if n == 0 {
            return Vec::new();
        }
        let mut bases: Vec<(Aabb, u32)> = vec![(boxes[0], 1)];
        let mut remaining = start_mask(n);
        while remaining != 0 {
            let mut joined = false;
            for base in &mut bases {
                let mut i = 0;
                while i < SCAN.min(boxes.len()) {
                    let member = |j: usize| base.1 >> j & 1 == 1 && boxes[j].touches(&boxes[i]);
                    if remaining >> i & 1 == 1
                        && base.0.touches(&boxes[i])
                        && (base.1.is_power_of_two() || (0..SCAN).any(member))
                    {
                        base.0 = base.0.union(&boxes[i]);
                        base.1 |= 1 << i;
                        remaining &= !(1 << i);
                        joined = true;
                        i = 1;
                        continue;
                    }
                    i += 1;
                }
            }
            if !joined {
                let seed = remaining.trailing_zeros() as usize;
                bases.push((boxes[seed], 1 << seed));
                remaining &= !(1 << seed);
            }
        }
        bases.into_iter().map(|(_, mask)| mask).collect()
    }

    fn masks(bases: &[Base]) -> Vec<u32> {
        bases.iter().map(|base| base.zones.low_mask()).collect()
    }

    /// A row of `n` zones in which zone `i` touches the one before when
    /// `touching(i)`.
    fn row(n: usize, touching: impl Fn(usize) -> bool) -> Vec<Aabb> {
        let mut x = 0.0;
        (0..n)
            .map(|i| {
                x += if i > 0 && touching(i) { 40.0 } else { 100.0 };
                cube(x, 0.0, 20.0)
            })
            .collect()
    }

    #[test]
    fn up_to_16_zones_the_grouping_is_the_game_s() {
        for n in 0..=16 {
            for pattern in 0..8usize {
                let boxes = row(n, |i| (i + pattern) % 3 != 0);
                assert_eq!(
                    masks(&group(&boxes, MAX_BASES as usize)),
                    vanilla(&boxes),
                    "{n} zones, pattern {pattern}"
                );
            }
        }
    }

    #[test]
    fn zones_that_touch_only_through_a_later_zone_form_one_base() {
        let boxes = [
            cube(0.0, 0.0, 30.0),
            cube(100.0, 0.0, 30.0),
            cube(50.0, 0.0, 30.0),
        ];
        let bases = group(&boxes, 16);
        assert_eq!(masks(&bases), [0b111]);
        assert_eq!(bases[0].bounds.min[0], -30.0);
        assert_eq!(bases[0].bounds.max[0], 130.0);
    }

    #[test]
    fn every_listed_zone_joins_its_base_past_32() {
        let boxes = row(MAX_LISTED, |i| i % 64 != 0);
        let bases = group(&boxes, 16);
        assert_eq!(bases.len(), 4);
        assert_eq!(
            bases.iter().map(|b| b.zones.len()).sum::<usize>(),
            MAX_LISTED
        );
        assert!(bases[3].zones.contains(254));
        assert_eq!(bases[1].first, 64);
        assert_eq!(bases[1].zones.iter().next(), Some(64));
        assert_eq!(bases[0].zones.low_mask(), u32::MAX);
        assert_eq!(bases[1].zones.low_mask(), 0);
    }

    #[test]
    fn groups_past_the_most_bases_stay_apart() {
        let boxes = row(20, |_| false);
        let bases = group(&boxes, 16);
        assert_eq!(bases.len(), 16);
        assert_eq!(masks(&bases).iter().fold(0, |all, mask| all | mask), 0xffff);
    }

    #[test]
    fn zones_past_the_list_s_capacity_are_ignored() {
        let boxes = row(MAX_LISTED + 3, |_| true);
        let bases = group(&boxes, 16);
        assert_eq!(bases[0].zones.len(), MAX_LISTED);
        let mut set = ZoneSet::default();
        set.insert(300);
        assert_eq!(set, ZoneSet::default());
    }

    fn boxes() -> impl Strategy<Value = Vec<Aabb>> {
        prop::collection::vec((0u8..40, 0u8..40, 1u8..6), 0..=MAX_LISTED).prop_map(|cells| {
            cells
                .into_iter()
                .map(|(x, z, half)| cube(f32::from(x) * 4.0, f32::from(z) * 4.0, f32::from(half)))
                .collect()
        })
    }

    proptest! {
        #[test]
        fn every_base_is_one_touching_group_and_apart_from_the_others(boxes in boxes()) {
            let bases = group(&boxes, 255);
            let mut seen = ZoneSet::default();
            for (at, base) in bases.iter().enumerate() {
                prop_assert_eq!(base.zones.iter().next(), Some(base.first));
                if at > 0 {
                    prop_assert!(bases[at - 1].first < base.first);
                }
                for i in base.zones.iter() {
                    prop_assert!(!seen.contains(i));
                    seen.insert(i);
                    prop_assert!(base.bounds.touches(&boxes[i]));
                    for other in &bases[at + 1..] {
                        for j in other.zones.iter() {
                            prop_assert!(!boxes[i].touches(&boxes[j]));
                        }
                    }
                }
            }
            prop_assert_eq!(seen.len(), boxes.len());
        }

        #[test]
        fn up_to_16_random_zones_group_as_the_game_does(boxes in boxes().prop_map(|b| b.into_iter().take(16).collect::<Vec<_>>())) {
            prop_assert_eq!(masks(&group(&boxes, 16)), vanilla(&boxes));
        }
    }
}
