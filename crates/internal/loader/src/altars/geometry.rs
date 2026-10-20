//! How the engine builds an altar's zone.
//!
//! The engine builds an altar's zone from its trigger shape: the centre is the
//! altar's position plus a quarter metre along its local X and Z, and the box is
//! the axis-aligned bound of the shape's half extents (`range`) turned by the
//! altar's orientation. Altars only turn about the vertical, so for a turn with
//! cosine `c` and sine `s` the bound's half extents are `|c| rx + |s| rz` along
//! X, `|s| rx + |c| rz` along Z and `ry` up.
//!
//! The geometry works in whole 0.5 m cells. Growing the bound by `d` on X and Z
//! alike takes `d / (|c| + |s|)` more on both `rx` and `rz`, which is how a
//! mod's growth turns back into a range the engine can use.

use enshwick_loader_api::abi::Bounds;

const CELLS_PER_METRE: f64 = 2.0;
/// How far an orientation may stray from a pure turn about the vertical.
const TILT_TOLERANCE: f32 = 1e-3;

/// An altar as the engine holds it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placed {
    /// 32.32 fixed point, metres.
    pub(crate) position: [i64; 3],
    /// Quaternion x, y, z, w.
    pub(crate) orientation: [f32; 4],
}

/// The turn about the vertical, as `|cos|` and `|sin|`; `None` when the altar
/// is tilted.
#[must_use]
pub(crate) fn turn(orientation: [f32; 4]) -> Option<(f64, f64)> {
    let [x, y, z, w] = orientation;
    if x.abs() > TILT_TOLERANCE || z.abs() > TILT_TOLERANCE {
        return None;
    }
    let (y, w) = (f64::from(y), f64::from(w));
    let norm = y * y + w * w;
    if norm < 0.5 {
        return None;
    }
    let cos = (w * w - y * y) / norm;
    let sin = 2.0 * y * w / norm;
    Some((cos.abs(), sin.abs()))
}

/// The zone centre in metres.
#[must_use]
#[expect(
    clippy::many_single_char_names,
    reason = "quaternion components and the rotation keep their usual names"
)]
pub(crate) fn centre(placed: &Placed) -> Option<[f64; 3]> {
    let [x, y, z, w] = placed.orientation.map(f64::from);
    turn(placed.orientation)?;
    // The quarter-metre offset (0.25, 0, 0.25) turned by the orientation.
    let rotate = |v: [f64; 3]| -> [f64; 3] {
        let t = [
            2.0 * (y * v[2] - z * v[1]),
            2.0 * (z * v[0] - x * v[2]),
            2.0 * (x * v[1] - y * v[0]),
        ];
        [
            v[0] + w * t[0] + (y * t[2] - z * t[1]),
            v[1] + w * t[1] + (z * t[0] - x * t[2]),
            v[2] + w * t[2] + (x * t[1] - y * t[0]),
        ]
    };
    let shift = rotate([0.25, 0.0, 0.25]);
    let p = enshwick_loader_api::ecs::metres(placed.position);
    Some([p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]])
}

/// The bound's half extents for half extents `range`, in metres.
#[must_use]
pub(crate) fn bound(range: [f32; 3], (cos, sin): (f64, f64)) -> [f64; 3] {
    let [rx, ry, rz] = range.map(f64::from);
    [cos * rx + sin * rz, ry, sin * rx + cos * rz]
}

/// The zone for half extents `range`, in whole cells, rounded outwards.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "world coordinates fit in i64 cells"
)]
pub(crate) fn cells(centre: [f64; 3], range: [f32; 3], turn: (f64, f64)) -> Bounds {
    let half = bound(range, turn);
    let edge = |axis: usize, sign: f64| (centre[axis] + sign * half[axis]) * CELLS_PER_METRE;
    // A hair of slack keeps exact grid values from rounding outwards.
    let min = |axis| (edge(axis, -1.0) + 1e-6).floor() as i64;
    let max = |axis| (edge(axis, 1.0) - 1e-6).ceil() as i64;
    Bounds {
        min: [min(0), min(1), min(2)],
        max: [max(0), max(1), max(2)],
    }
}

/// The range that grows the bound of `range` by `growth` cells on X and Z and
/// makes it `height` cells tall.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "ranges are a few hundred metres, well inside f32"
)]
pub(crate) fn grown_range(
    range: [f32; 3],
    (cos, sin): (f64, f64),
    growth: i64,
    height: i64,
) -> [f32; 3] {
    let extra = growth as f64 / CELLS_PER_METRE / (cos + sin);
    [
        (f64::from(range[0]) + extra) as f32,
        (height as f64 / CELLS_PER_METRE / 2.0) as f32,
        (f64::from(range[2]) + extra) as f32,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_loader_api::ecs::FIXED_PER_METRE;

    const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    /// Half a turn about the vertical, as one live altar has it.
    const HALF_TURN: [f32; 4] = [0.0, 1.0, 0.0, 0.0];

    #[expect(clippy::cast_possible_truncation, reason = "test values")]
    fn fixed(metres: f64) -> i64 {
        (metres * FIXED_PER_METRE) as i64
    }

    #[test]
    fn turns_about_the_vertical() {
        assert_eq!(turn(UPRIGHT), Some((1.0, 0.0)));
        assert_eq!(turn(HALF_TURN), Some((1.0, 0.0)));
        let quarter = std::f32::consts::FRAC_1_SQRT_2;
        let (c, s) = turn([0.0, quarter, 0.0, quarter]).unwrap();
        assert!(c < 1e-6 && (s - 1.0).abs() < 1e-6);
        assert_eq!(turn([0.3, 0.0, 0.0, 0.95]), None);
    }

    #[test]
    fn the_centre_is_a_quarter_metre_along_the_altar() {
        let placed = Placed {
            position: [fixed(3636.75), fixed(860.5), fixed(1339.75)],
            orientation: UPRIGHT,
        };
        assert_eq!(centre(&placed), Some([3637.0, 860.5, 1340.0]));
        let placed = Placed {
            position: [fixed(3788.75), fixed(842.5), fixed(1436.25)],
            orientation: HALF_TURN,
        };
        let c = centre(&placed).unwrap();
        assert!((c[0] - 3788.5).abs() < 1e-9 && (c[2] - 1436.0).abs() < 1e-9);
    }

    #[test]
    fn a_level_1_zone_is_80_cells_wide() {
        let zone = cells([3637.0, 860.5, 1340.0], [20.0, 20.0, 20.0], (1.0, 0.0));
        assert_eq!(
            zone,
            Bounds {
                min: [7234, 1681, 2640],
                max: [7314, 1761, 2720]
            }
        );
    }

    #[test]
    fn a_quarter_turn_swaps_the_axes() {
        let zone = cells([0.0, 0.0, 0.0], [10.0, 5.0, 20.0], (0.0, 1.0));
        assert_eq!(
            zone,
            Bounds {
                min: [-40, -10, -20],
                max: [40, 10, 20]
            }
        );
    }

    #[test]
    fn grown_range_grows_the_bound_by_the_growth() {
        let diagonal = (
            std::f64::consts::FRAC_1_SQRT_2,
            std::f64::consts::FRAC_1_SQRT_2,
        );
        for turn in [(1.0, 0.0), (0.0, 1.0), diagonal] {
            let range = [20.0, 20.0, 20.0];
            let grown = grown_range(range, turn, 6, 100);
            let before = bound(range, turn);
            let after = bound(grown, turn);
            assert!((after[0] - before[0] - 3.0).abs() < 1e-4, "{turn:?}");
            assert!((after[2] - before[2] - 3.0).abs() < 1e-4, "{turn:?}");
            assert!((grown[1] - 25.0).abs() < f32::EPSILON);
        }
    }
}
