//! What the base limit decides, apart from the game.
//!
//! The game groups altar zones into bases: zones that touch, directly or
//! through other zones, form one base. Its own grouping takes 16 zones, and
//! its array holds 16 bases. Raised, every listed zone joins its base and a
//! base past the limit is never started, which leaves that zone in no base;
//! the cursor refuses an altar that would start one before it gets there.

/// The altar zones the game's own grouping puts into bases.
pub(crate) const VANILLA_ZONES: u32 = 16;
/// The most bases the game holds.
pub(crate) const MAX_BASES: u32 = 16;
/// Altar zones and bases: what the game groups, or what a mod asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    /// Altar zones the game puts into bases.
    pub(crate) zones: u32,
    /// Bases the game holds.
    pub(crate) bases: u32,
}

impl Limits {
    /// The game's own.
    pub(crate) const VANILLA: Self = Self {
        zones: VANILLA_ZONES,
        bases: MAX_BASES,
    };
    /// What raising the limit asks for.
    pub(crate) const RAISED: Self = Self {
        zones: 32,
        bases: MAX_BASES,
    };
    /// The most a mod may ask for: every zone the game lists, in the bases
    /// its array holds.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the game lists at most 255 zones"
    )]
    pub(crate) const MOST: Self = Self {
        zones: super::grouping::MAX_LISTED as u32,
        bases: MAX_BASES,
    };

    /// Whether neither value passes `most`'s.
    #[must_use]
    pub(crate) const fn within(self, most: Self) -> bool {
        self.zones <= most.zones && self.bases <= most.bases
    }
}

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

    /// The smallest box holding both.
    #[must_use]
    pub(crate) fn union(&self, other: &Self) -> Self {
        Self {
            min: [0, 1, 2].map(|axis| self.min[axis].min(other.min[axis])),
            max: [0, 1, 2].map(|axis| self.max[axis].max(other.max[axis])),
        }
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

    /// Whether an altar whose zone is `candidate` would start a base past
    /// `most`: the bases are full and the zone touches none of their zones.
    #[must_use]
    pub(crate) fn refuses(&self, candidate: &Aabb, most: u32) -> bool {
        self.bases >= most
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
        assert!(!zones.refuses(&near, 16));
        assert!(zones.refuses(&far, 16));
        zones.bases = 15;
        assert!(!zones.refuses(&far, 16));
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
        assert!(zones.refuses(&candidate([0.0, 0.0, 30.0], [40.0, 40.0, 40.0]), 16));
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
