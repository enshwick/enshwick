//! Building across the seams between altars.
//!
//! Vanilla accepts an object only when both corners of its box lie in one zone.
//! Inside a cluster that is too strict: an object may sit across two members'
//! zones. The check accepts a box that the union of the zones covers whole.
//! Zones that together cover a connected box overlap or touch, and the engine
//! groups touching zones into one base, so a covered box always lies in a
//! single base.
//!
//! A zone is half-open, `min - margin <= p < max + margin`, like the engine's
//! point test; the object's box is closed, its corners included, like
//! vanilla's two-corner test. The check runs while a player aims a piece, so
//! it never allocates.

use arrayvec::ArrayVec;
use enshwick_loader_api::altars::ZoneCheck;

/// The engine keeps at most 255 zones.
const MAX_ZONES: usize = 256;

type Zone = ([i64; 3], [i64; 3]);

/// Whether the union of the check's zones covers the object.
#[must_use]
pub(crate) fn accepts(check: &ZoneCheck<'_>) -> bool {
    let mut zones: ArrayVec<Zone, MAX_ZONES> = ArrayVec::new();
    for zone in check.zones.iter().take(MAX_ZONES) {
        zones.push((zone.min, zone.max));
    }
    covered(check.object.min, check.object.max, &zones, check.margin)
}

/// Whether the closed box `[low, high]` lies within the union of the half-open
/// `zones`, each widened by `margin` on every side.
#[must_use]
pub(crate) fn covered(low: [i64; 3], high: [i64; 3], zones: &[Zone], margin: i64) -> bool {
    if (0..3).any(|a| low[a] > high[a]) {
        return false;
    }
    let mut near: ArrayVec<Zone, MAX_ZONES> = ArrayVec::new();
    for (min, max) in zones.iter().take(MAX_ZONES) {
        let zone = (
            min.map(|v| v.saturating_sub(margin)),
            max.map(|v| v.saturating_add(margin)),
        );
        if (0..3).all(|a| zone.0[a] <= high[a] && low[a] < zone.1[a]) {
            near.push(zone);
        }
    }
    if near.is_empty() {
        return false;
    }

    // Split the box at every zone face inside it; each piece is either inside a
    // zone or not. A piece is tested at its lowest point, and the box's own high
    // faces close the last piece on each axis.
    let cuts = |axis: usize| {
        let mut cuts: ArrayVec<i64, { 2 * MAX_ZONES + 1 }> = ArrayVec::new();
        cuts.push(low[axis]);
        for (min, max) in &near {
            for v in [min[axis], max[axis]] {
                if low[axis] < v && v <= high[axis] {
                    cuts.push(v);
                }
            }
        }
        cuts.sort_unstable();
        let mut unique: ArrayVec<i64, { 2 * MAX_ZONES + 1 }> = ArrayVec::new();
        for v in cuts {
            if unique.last() != Some(&v) {
                unique.push(v);
            }
        }
        unique
    };
    let (xs, ys, zs) = (cuts(0), cuts(1), cuts(2));
    xs.iter().all(|&x| {
        ys.iter().all(|&y| {
            zs.iter().all(|&z| {
                let p = [x, y, z];
                near.iter()
                    .any(|(min, max)| (0..3).all(|a| min[a] <= p[a] && p[a] < max[a]))
            })
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_inside_one_zone_is_covered() {
        assert!(covered(
            [1, 1, 1],
            [9, 9, 9],
            &[([0, 0, 0], [10, 10, 10])],
            0
        ));
    }

    #[test]
    fn a_box_across_two_touching_zones_is_covered() {
        let zones = [([0, 0, 0], [10, 10, 10]), ([10, 0, 0], [20, 10, 10])];
        assert!(covered([5, 1, 1], [15, 9, 9], &zones, 0));
    }

    #[test]
    fn the_far_face_is_outside_a_half_open_zone() {
        let zones = [([0, 0, 0], [10, 10, 10])];
        assert!(!covered([1, 1, 1], [10, 9, 9], &zones, 0));
        assert!(covered([1, 1, 1], [10, 9, 9], &zones, 1));
    }

    #[test]
    fn corners_in_two_zones_do_not_cover_the_gap_of_an_l() {
        let zones = [([0, 0, 0], [20, 10, 10]), ([0, 0, 0], [10, 20, 10])];
        assert!(covered([1, 1, 1], [9, 19, 9], &zones, 0));
        assert!(!covered([1, 1, 1], [15, 15, 9], &zones, 0));
    }

    #[test]
    fn a_gap_between_zones_is_not_covered() {
        let zones = [([0, 0, 0], [10, 10, 10]), ([11, 0, 0], [20, 10, 10])];
        assert!(!covered([5, 1, 1], [15, 9, 9], &zones, 0));
    }

    #[test]
    fn no_zone_covers_nothing() {
        assert!(!covered([0, 0, 0], [1, 1, 1], &[], 0));
    }
}
