//! The bases altar zones form: zones that touch, directly or through other
//! zones, share one base, as the game groups them.

use arrayvec::ArrayVec;

use super::limit::Aabb;

/// The most zones the game lists in a world.
/// Found on the 1.0 build 1269579, server and client.
pub(crate) const MAX_LISTED: usize = 0xff;

/// A set of altar zones, by their place in the game's zone list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ZoneSet([u64; 4]);

impl ZoneSet {
    /// The set the game's 32-bit member mask names.
    #[must_use]
    pub(crate) fn from_mask(mask: u32) -> Self {
        Self([u64::from(mask), 0, 0, 0])
    }

    /// The set of `words`, zone `z` at bit `z % 64` of word `z / 64`.
    #[must_use]
    pub(crate) const fn from_words(words: [u64; 4]) -> Self {
        Self(words)
    }

    /// The words of the set, as [`ZoneSet::from_words`] takes them.
    #[must_use]
    pub(crate) const fn words(self) -> [u64; 4] {
        self.0
    }

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
    pub(crate) fn zones(self) -> impl Iterator<Item = usize> {
        self.0.into_iter().enumerate().flat_map(|(at, word)| {
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

/// The most bases a grouping holds: the game's array and the window.
pub(crate) const MAX_GROUPED: usize = super::window::MOST;

/// Bases as a grouping leaves them, on the stack.
pub(crate) type Grouped = ArrayVec<Base, MAX_GROUPED>;

/// Groups `boxes`, the game's zone list, into at most `max_bases` bases in
/// the game's order: by their first zone. The zones of further groups stay
/// apart, in no base; zones past [`MAX_LISTED`] are ignored. Allocates
/// nothing.
#[must_use]
pub(crate) fn group(boxes: &[Aabb], max_bases: usize) -> Grouped {
    let boxes = &boxes[..boxes.len().min(MAX_LISTED)];
    let mut parent = [0u8; MAX_LISTED];
    for (i, slot) in parent.iter_mut().enumerate().take(boxes.len()) {
        *slot = u8::try_from(i).unwrap_or(u8::MAX);
    }
    for i in 0..boxes.len() {
        for j in 0..i {
            if boxes[i].touches(&boxes[j]) {
                let (a, b) = (root(&mut parent, i), root(&mut parent, j));
                parent[a.max(b)] = u8::try_from(a.min(b)).unwrap_or(u8::MAX);
            }
        }
    }
    let max_bases = max_bases.min(MAX_GROUPED);
    let mut bases = Grouped::new();
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
fn root(parent: &mut [u8; MAX_LISTED], mut i: usize) -> usize {
    while usize::from(parent[i]) != i {
        parent[i] = parent[usize::from(parent[i])];
        i = usize::from(parent[i]);
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
        assert_eq!(bases[1].zones.zones().next(), Some(64));
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
    fn a_grouping_holds_up_to_the_window_s_most_bases() {
        let boxes = row(200, |_| false);
        assert_eq!(group(&boxes, 32).len(), 32);
        let bases = group(&boxes, usize::MAX);
        assert_eq!(bases.len(), MAX_GROUPED);
        assert_eq!(bases.last().map(|b| b.first), Some(MAX_GROUPED - 1));
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
            let bases = group(&boxes, MAX_GROUPED);
            let mut seen = ZoneSet::default();
            for (at, base) in bases.iter().enumerate() {
                prop_assert_eq!(base.zones.zones().next(), Some(base.first));
                if at > 0 {
                    prop_assert!(bases[at - 1].first < base.first);
                }
                for i in base.zones.zones() {
                    prop_assert!(!seen.contains(i));
                    seen.insert(i);
                    prop_assert!(base.bounds.touches(&boxes[i]));
                    for other in &bases[at + 1..] {
                        for j in other.zones.zones() {
                            prop_assert!(!boxes[i].touches(&boxes[j]));
                        }
                    }
                }
            }
            if bases.len() < MAX_GROUPED {
                prop_assert_eq!(seen.len(), boxes.len());
            }
            for apart in (0..boxes.len()).filter(|&i| !seen.contains(i)) {
                prop_assert!(seen.zones().all(|i| !boxes[i].touches(&boxes[apart])));
            }
        }

        #[test]
        fn up_to_16_random_zones_group_as_the_game_does(boxes in boxes().prop_map(|b| b.into_iter().take(16).collect::<Vec<_>>())) {
            prop_assert_eq!(masks(&group(&boxes, 16)), vanilla(&boxes));
        }
    }
}
