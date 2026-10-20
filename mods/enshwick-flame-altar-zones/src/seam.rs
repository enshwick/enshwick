//! Building across the seams between altars.
//!
//! Vanilla accepts an object only when both corners of its box lie in one zone.
//! Inside a cluster that is too strict: an object may sit across two members'
//! zones. The hook accepts a box that the union of the zones covers whole.
//! Zones that together cover a connected box overlap or touch, and the engine
//! groups touching zones into one base, so a covered box always lies in a
//! single base.
//!
//! Coordinates are the engine's 32.32 fixed point. A zone is half-open,
//! `min - margin <= p < max + margin`, like the engine's point test; the
//! object's box is closed, its corners included, like vanilla's two-corner
//! test. The check runs while a player aims a piece, so it never allocates.

use std::sync::OnceLock;

use arrayvec::ArrayVec;
use enshwick_loader_api::raw::read_at;
use enshwick_loader_api::{Load, Next, Off};

/// The engine keeps at most 255 zones.
const MAX_ZONES: usize = 256;
const FIXED_PER_METRE: f32 = 4_294_967_296.0;

type Zone = ([i64; 3], [i64; 3]);

/// Where the engine keeps the zone list and an object's corners.
#[derive(Debug)]
struct Layout {
    entries: usize,
    count: usize,
    entry_size: usize,
    entry_shape: usize,
    entry_ignored: usize,
    corner_size: usize,
}

static LAYOUT: OnceLock<Layout> = OnceLock::new();

enshwick_loader_api::detour! {
    static SEAM: unsafe extern "system" fn(zones: *const u8, corners: *const i64, margin: f32) -> u8 = seam;
}

/// Links the seam check.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let size = |name| {
        load.constant(name)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    };
    let _first = LAYOUT.set(Layout {
        entries: size("building.zone_list_entries")?,
        count: size("building.zone_list_count")?,
        entry_size: size("building.zone_entry_size")?,
        entry_shape: size("building.zone_entry_shape")?,
        entry_ignored: size("building.zone_entry_ignored")?,
        corner_size: size("building.corner_size")?,
    });
    load.hook(&SEAM, load.symbol("building.seam_check")?)
}

fn seam(
    next: Next<unsafe extern "system" fn(*const u8, *const i64, f32) -> u8>,
    zones: *const u8,
    corners: *const i64,
    margin: f32,
) -> u8 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let vanilla = unsafe { next.get()(zones, corners, margin) };
    if vanilla != 0 {
        return vanilla;
    }
    let Some(layout) = LAYOUT.get() else {
        return vanilla;
    };
    // SAFETY: the game passes its zone list and an object's two corners.
    u8::from(unsafe { across(layout, zones, corners, margin) })
}

/// # Safety
///
/// `list` is the engine's zone list and `corners` two corner points.
#[expect(clippy::cast_possible_truncation, reason = "margins are a few metres")]
unsafe fn across(layout: &Layout, list: *const u8, corners: *const i64, margin: f32) -> bool {
    // SAFETY: the list holds its entries pointer and count at these offsets.
    let entries: *const u8 = unsafe { read_at(list, layout.entries) };
    // SAFETY: as above.
    let count: usize = unsafe { read_at(list, layout.count) };
    if entries.is_null() || count > MAX_ZONES {
        return false;
    }
    let mut zones: ArrayVec<Zone, MAX_ZONES> = ArrayVec::new();
    for i in 0..count {
        let entry = entries.wrapping_add(i * layout.entry_size);
        // SAFETY: `i < count` entries of `entry_size` bytes.
        let ignored: u8 = unsafe { read_at(entry, layout.entry_ignored) };
        // SAFETY: as above.
        let shape: u8 = unsafe { read_at(entry, layout.entry_shape) };
        if ignored != 0 || shape != 0 {
            continue;
        }
        // SAFETY: an entry starts with min and max as six i64.
        let bounds: [i64; 6] = unsafe { read_at(entry, 0) };
        zones.push((
            [bounds[0], bounds[1], bounds[2]],
            [bounds[3], bounds[4], bounds[5]],
        ));
    }
    // SAFETY: the game passes two corners of three i64 each.
    let low: [i64; 3] = unsafe { read_at(corners.cast(), 0) };
    // SAFETY: as above.
    let high: [i64; 3] = unsafe { read_at(corners.cast(), layout.corner_size) };
    let low_corner = [0, 1, 2].map(|a| low[a].min(high[a]));
    let high_corner = [0, 1, 2].map(|a| low[a].max(high[a]));
    covered(
        low_corner,
        high_corner,
        &zones,
        (margin * FIXED_PER_METRE) as i64,
    )
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
