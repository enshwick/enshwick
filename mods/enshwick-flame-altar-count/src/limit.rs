//! What the base limit decides, apart from the game.
//!
//! The game groups altar zones into bases: zones that touch, directly or
//! through other zones, form one base. A base names its zones in a 32-bit
//! mask, so 32 zones fit; the bases live in an array of 16, so 16 bases fit.
//! Patched, the grouping takes all 32 zones and the new base call refuses a
//! 17th base, which leaves that zone in no base. The cursor refuses an altar
//! that would start a 17th base before it gets there.

/// The most bases the game holds.
pub(crate) const MAX_BASES: u32 = 16;
/// The most zones the vanilla grouping takes.
pub(crate) const VANILLA_ZONES: u32 = 16;
/// The most zones one base mask names.
pub(crate) const MAX_ZONES: usize = 32;
/// Slack around a candidate altar's zone, in metres: the cursor leans
/// towards allowing, since the new base guard catches what it lets through.
const SLACK: f32 = 0.5;

/// An axis-aligned box, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Aabb {
    /// The lowest corner.
    pub(crate) min: [f32; 3],
    /// The highest corner.
    pub(crate) max: [f32; 3],
}

impl Aabb {
    /// Whether the boxes share a point, faces included: the game's test.
    #[must_use]
    pub(crate) fn touches(&self, other: &Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.max[axis] && other.min[axis] <= self.max[axis])
    }
}

/// One altar zone and whether a base holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Zone {
    /// The zone's box.
    pub(crate) bounds: Aabb,
    /// Whether some base names the zone.
    pub(crate) based: bool,
}

/// The zones and bases of a world, as the grouping last left them.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Zones {
    /// Every zone, in the game's order.
    pub(crate) zones: Vec<Zone>,
    /// How many bases there are.
    pub(crate) bases: u32,
}

impl Zones {
    /// How many zones no base holds.
    #[must_use]
    pub(crate) fn apart(&self) -> usize {
        self.zones.iter().filter(|zone| !zone.based).count()
    }

    /// Whether an altar whose zone is `candidate` would start a 17th base:
    /// the bases are full and the zone touches none of their zones.
    #[must_use]
    pub(crate) fn refuses(&self, candidate: &Aabb) -> bool {
        self.bases >= MAX_BASES
            && !self
                .zones
                .iter()
                .any(|zone| zone.based && zone.bounds.touches(candidate))
    }
}

/// The zone of an altar at `position` whose level 1 zone measures `size`,
/// widened to cover any turn about the vertical and the slack.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "world positions in metres fit in f32 as the game keeps them"
)]
pub(crate) fn candidate(position: [f64; 3], size: [f32; 3]) -> Aabb {
    let across = size[0].max(size[2]) / 2.0 + SLACK;
    let half = [across, size[1] / 2.0 + SLACK, across];
    let centre = position.map(|v| v as f32);
    Aabb {
        min: [0, 1, 2].map(|axis| centre[axis] - half[axis]),
        max: [0, 1, 2].map(|axis| centre[axis] + half[axis]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(x: f32, z: f32, half: f32) -> Aabb {
        Aabb {
            min: [x - half, -half, z - half],
            max: [x + half, half, z + half],
        }
    }

    /// The members mask the game starts the grouping with for `n` zones.
    /// `wide` is the patched code: `bts rdi, rcx; dec edi`, where vanilla
    /// runs `shl edi, cl; dec edi`, whose count wraps at 32.
    fn start_mask(n: u32, wide: bool) -> u32 {
        let bit = if wide {
            u32::try_from((1u64 << (n % 64)) & u64::from(u32::MAX)).unwrap()
        } else {
            1u32 << (n % 32)
        };
        bit.wrapping_sub(1) & !1
    }

    /// The game's grouping as the code runs it: `scan` zones are tested as
    /// join candidates and as members; `guard` refuses a base past 16.
    /// Returns each base's member mask.
    fn group(boxes: &[Aabb], scan: usize, wide: bool, guard: bool) -> Vec<u32> {
        let n = u32::try_from(boxes.len()).unwrap();
        if n == 0 {
            return Vec::new();
        }
        let mut bases: Vec<(Aabb, u32)> = vec![(boxes[0], 1)];
        let new_base = |bases: &mut Vec<(Aabb, u32)>, i: usize| {
            if !guard || bases.len() < MAX_BASES as usize {
                bases.push((boxes[i], 1 << i));
            }
        };
        let mut remaining = start_mask(n, wide);
        while remaining != 0 {
            let mut joined = false;
            for base in &mut bases {
                let mut i = 0;
                while i < scan {
                    let member = |j: usize| base.1 >> j & 1 == 1 && boxes[j].touches(&boxes[i]);
                    if remaining >> i & 1 == 1
                        && base.0.touches(&boxes[i])
                        && (base.1.is_power_of_two() || (0..scan).any(member))
                    {
                        for axis in 0..3 {
                            base.0.min[axis] = base.0.min[axis].min(boxes[i].min[axis]);
                            base.0.max[axis] = base.0.max[axis].max(boxes[i].max[axis]);
                        }
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
                new_base(&mut bases, seed);
                remaining &= !(1 << seed);
            }
        }
        bases.into_iter().map(|(_, mask)| mask).collect()
    }

    fn root(parent: &[usize], mut i: usize) -> usize {
        while parent[i] != i {
            i = parent[i];
        }
        i
    }

    /// Zones that touch, directly or through others, as masks ordered by
    /// their lowest zone.
    fn components(boxes: &[Aabb]) -> Vec<u32> {
        let mut parent: Vec<usize> = (0..boxes.len()).collect();
        for i in 0..boxes.len() {
            for j in 0..i {
                if boxes[i].touches(&boxes[j]) {
                    let (a, b) = (root(&parent, i), root(&parent, j));
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
        let mut masks: Vec<(usize, u32)> = Vec::new();
        for i in 0..boxes.len() {
            let r = root(&parent, i);
            match masks.iter_mut().find(|(at, _)| *at == r) {
                Some((_, mask)) => *mask |= 1 << i,
                None => masks.push((r, 1 << i)),
            }
        }
        masks.sort_unstable();
        masks.into_iter().map(|(_, mask)| mask).collect()
    }

    /// A row of `n` zones where every `link`-th pair touches.
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
    fn the_start_mask_holds_32_zones_once_patched() {
        assert_eq!(start_mask(3, false), 0b110);
        assert_eq!(start_mask(16, true), start_mask(16, false));
        assert_eq!(
            start_mask(32, false),
            0,
            "vanilla loses every zone but the first"
        );
        assert_eq!(start_mask(32, true), !1);
        assert_eq!(start_mask(40, true), !1, "zones past 32 stay apart");
    }

    #[test]
    fn up_to_16_zones_the_patched_grouping_is_vanilla() {
        for n in 0..=16 {
            for pattern in 0..8usize {
                let boxes = row(n, |i| (i + pattern) % 3 != 0);
                assert_eq!(
                    group(&boxes, MAX_ZONES, true, true),
                    group(&boxes, 16, false, false),
                    "{n} zones, pattern {pattern}"
                );
            }
        }
    }

    #[test]
    fn the_patched_grouping_finds_the_touching_groups_of_32_zones() {
        for pattern in 2..6usize {
            let boxes = row(MAX_ZONES, |i| i % pattern != 0);
            let expected = components(&boxes);
            assert!(expected.len() <= MAX_BASES as usize, "pattern {pattern}");
            assert_eq!(group(&boxes, MAX_ZONES, true, true), expected);
        }
        // Zones that touch only through a later zone still form one base.
        let boxes = [
            cube(0.0, 0.0, 30.0),
            cube(100.0, 0.0, 30.0),
            cube(50.0, 0.0, 30.0),
        ];
        assert_eq!(group(&boxes, MAX_ZONES, true, true), vec![0b111]);
    }

    #[test]
    fn vanilla_past_16_zones_writes_more_bases_than_it_holds() {
        let boxes = row(20, |_| false);
        assert_eq!(group(&boxes, 16, false, false).len(), 20);
    }

    #[test]
    fn the_guard_leaves_zones_past_the_16th_base_apart() {
        let boxes = row(20, |_| false);
        let bases = group(&boxes, MAX_ZONES, true, true);
        assert_eq!(bases.len(), MAX_BASES as usize);
        assert_eq!(bases.iter().fold(0, |all, mask| all | mask), 0xffff);
    }

    #[test]
    fn a_17th_base_is_refused_and_a_touching_altar_is_not() {
        let mut zones = Zones {
            zones: (0u8..16)
                .map(|i| Zone {
                    bounds: cube(100.0 * f32::from(i), 0.0, 20.0),
                    based: true,
                })
                .collect(),
            bases: 16,
        };
        let size = [40.0, 40.0, 40.0];
        let near = candidate([0.0, 0.0, 40.0], size);
        let far = candidate([0.0, 0.0, 200.0], size);
        assert!(!zones.refuses(&near));
        assert!(zones.refuses(&far));
        zones.bases = 15;
        assert!(!zones.refuses(&far));
    }

    #[test]
    fn a_zone_apart_does_not_let_an_altar_in() {
        let zones = Zones {
            zones: vec![Zone {
                bounds: cube(0.0, 0.0, 20.0),
                based: false,
            }],
            bases: 16,
        };
        assert_eq!(zones.apart(), 1);
        assert!(zones.refuses(&candidate([0.0, 0.0, 30.0], [40.0, 40.0, 40.0])));
    }

    #[test]
    fn the_candidate_covers_a_turned_zone() {
        let zone = candidate([10.0, 5.0, -10.0], [40.0, 20.0, 30.0]);
        let expected = Aabb {
            min: [-10.5, -5.5, -30.5],
            max: [30.5, 15.5, 10.5],
        };
        assert!(zone.touches(&expected) && expected.touches(&zone));
        assert!(
            (0..3).all(|axis| (zone.min[axis] - expected.min[axis]).abs() < 1e-6
                && (zone.max[axis] - expected.max[axis]).abs() < 1e-6)
        );
    }
}
