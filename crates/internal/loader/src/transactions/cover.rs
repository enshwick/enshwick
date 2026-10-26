//! Which 8³-voxel bricks a terrain edit can write, and which edits a transaction takes.

use enshwick::abi::{Status, TerrainEdit, Voxel};

use crate::voxels::WorldExtent;

/// Voxels along a brick's side.
pub(crate) const BRICK: u32 = 8;
/// Voxels in a brick.
pub(crate) const BRICK_VOXELS: usize = (BRICK * BRICK * BRICK) as usize;

/// The largest radius and height an edit takes, in metres, as
/// `crate::terrain` checks them.
const MAX_REACH: f32 = 16.0;
/// The last terrain material; above it, building materials.
const TERRAIN_LAST: u32 = 0x7f;
/// Voxels a metre.
const PER_METRE: f64 = 2.0;
/// Voxels added on every side of the box the game computes, so a rounding
/// apart between its `f32` bounds and these `f64` ones never leaves a
/// written voxel out.
const MARGIN: u64 = 1;

/// A brick by its index along each axis: voxel `8 * index` and the seven
/// after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct Brick(pub(crate) [u32; 3]);

impl Brick {
    /// Its first voxel along each axis.
    pub(crate) const fn min(self) -> [u32; 3] {
        let [x, y, z] = self.0;
        [x * BRICK, y * BRICK, z * BRICK]
    }
}

/// The bricks from `first` to `last` on every axis, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Bricks {
    first: [u32; 3],
    last: [u32; 3],
}

impl Bricks {
    /// How many bricks.
    pub(crate) fn count(self) -> usize {
        (0..3)
            .map(|axis| (self.last[axis] - self.first[axis] + 1) as usize)
            .product()
    }

    /// Whether every brick lies inside the world.
    pub(crate) fn inside(self, extent: WorldExtent) -> bool {
        let min = self.first.map(|index| index * BRICK);
        let size = [0, 1, 2].map(|axis| (self.last[axis] - self.first[axis] + 1) * BRICK);
        extent.holds(min, size)
    }

    /// Every brick, x fastest, then y, then z.
    pub(crate) fn iter(self) -> impl Iterator<Item = Brick> {
        let [x0, y0, z0] = self.first;
        let [x1, y1, z1] = self.last;
        (z0..=z1).flat_map(move |z| {
            (y0..=y1).flat_map(move |y| (x0..=x1).map(move |x| Brick([x, y, z])))
        })
    }
}

/// Whether a transaction takes `edit`: what `crate::terrain` writes, a
/// known kind and shape, a radius above 0 and up to 16 metres, a position
/// at 0 or more inside the coordinates a box covers, a terrain material for
/// an add or a flatten and a height for an add; no dry run, which writes
/// nothing to undo.
pub(crate) fn check(edit: &TerrainEdit) -> Result<Bricks, Status> {
    let reach = |metres: f32| metres > 0.0 && metres <= MAX_REACH;
    let material = (1..=TERRAIN_LAST).contains(&edit.material);
    let valid = edit.flags & TerrainEdit::DRY_RUN == 0
        && reach(edit.radius)
        && edit
            .position
            .iter()
            .all(|axis| axis.is_finite() && *axis >= 0.0)
        && edit.shape <= TerrainEdit::SQUARE
        && match edit.kind {
            TerrainEdit::ADD => material && reach(edit.height),
            TerrainEdit::DIG => true,
            TerrainEdit::FLATTEN => material,
            _ => false,
        };
    if !valid {
        return Err(Status::BAD_ARGS);
    }
    cover(edit).ok_or(Status::BAD_ARGS)
}

/// The bricks `edit` can write: the box the game's edit reads and writes
/// back, widened to whole bricks as its editor does (server 399, client
/// 426). Every edit clamps each bound at 0 and takes voxels
/// `floor(2 * low)` to `ceil(2 * high)`; around the position `p` and radius
/// `r` the box is
///
/// - add: `p ± r`, but from `p.y - (r - 0.5)` up; the height stays inside;
/// - flatten: `p ± r` across, `p.y ± max(r / 2, 1.5)` up and down;
/// - dig, round: `p ± r`; square: `p ± r√3`, the reach of its turned cube.
///
/// `None` for a box past [`Voxel::END`].
pub(crate) fn cover(edit: &TerrainEdit) -> Option<Bricks> {
    let r = f64::from(edit.radius);
    let (below, above, across) = match edit.kind {
        TerrainEdit::ADD => (r - 0.5, r, r),
        TerrainEdit::FLATTEN => {
            let h = (r * 0.5).max(1.5);
            (h, h, r)
        }
        _ if edit.shape == TerrainEdit::SQUARE => {
            let reach = r * 3f64.sqrt();
            (reach, reach, reach)
        }
        _ => (r, r, r),
    };
    let [x, y, z] = edit.position.map(f64::from);
    let low = [x - across, y - below, z - across];
    let high = [x + across, y + above, z + across];
    let mut first = [0; 3];
    let mut last = [0; 3];
    for axis in 0..3 {
        let min = voxel((low[axis].max(0.0) * PER_METRE).floor()).saturating_sub(MARGIN);
        let max = voxel((high[axis].max(0.0) * PER_METRE).ceil()).checked_add(MARGIN)?;
        if max >= u64::from(Voxel::END) {
            return None;
        }
        first[axis] = u32::try_from(min / u64::from(BRICK)).ok()?;
        last[axis] = u32::try_from(max / u64::from(BRICK)).ok()?;
    }
    Some(Bricks { first, last })
}

/// A whole, non-negative number of voxels as an index, saturating.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the caller passes a whole, non-negative number; `as` saturates past u64"
)]
fn voxel(voxels: f64) -> u64 {
    voxels as u64
}

#[cfg(test)]
mod tests {
    use enshwick::abi::Header;

    use super::*;

    fn edit(kind: u32, shape: u32, position: [f32; 3], radius: f32) -> TerrainEdit {
        TerrainEdit {
            header: Header::of::<TerrainEdit>(),
            kind,
            shape,
            flags: 0,
            material: 9,
            position,
            radius,
            height: 1.0,
            protect: [0; 16],
        }
    }

    fn span(bricks: Bricks) -> [[u32; 2]; 3] {
        [0, 1, 2].map(|axis| [bricks.first[axis], bricks.last[axis]])
    }

    #[test]
    fn a_round_dig_covers_its_sphere_and_a_voxel_more() {
        let bricks = check(&edit(TerrainEdit::DIG, TerrainEdit::ROUND, [20.0; 3], 4.0)).unwrap();
        // Voxels 32 - 1 to 48 + 1 on every axis.
        assert_eq!(span(bricks), [[3, 6]; 3]);
        assert_eq!(bricks.count(), 64);
    }

    #[test]
    fn a_square_dig_reaches_the_corners_of_its_turned_cube() {
        let bricks = check(&edit(TerrainEdit::DIG, TerrainEdit::SQUARE, [20.0; 3], 4.0)).unwrap();
        // 4√3 = 6.93 metres: voxels 26 - 1 to 54 + 1.
        assert_eq!(span(bricks), [[3, 6]; 3]);
        let wide = check(&edit(
            TerrainEdit::DIG,
            TerrainEdit::SQUARE,
            [40.0; 3],
            16.0,
        ))
        .unwrap();
        // 16√3 = 27.7 metres: voxels 24 - 1 to 136 + 1.
        assert_eq!(span(wide), [[2, 17]; 3]);
    }

    #[test]
    fn an_add_starts_half_a_metre_up_and_a_flatten_reaches_its_band() {
        let add = check(&edit(TerrainEdit::ADD, TerrainEdit::ROUND, [20.0; 3], 4.0)).unwrap();
        // y from 16.5 metres: voxel 33 - 1, still brick 4.
        assert_eq!(span(add), [[3, 6], [4, 6], [3, 6]]);
        let flat = check(&edit(
            TerrainEdit::FLATTEN,
            TerrainEdit::ROUND,
            [20.0; 3],
            2.0,
        ))
        .unwrap();
        // Across 18 to 22 metres; up and down 1.5: voxels 37 - 1 to 43 + 1.
        assert_eq!(span(flat), [[4, 5], [4, 5], [4, 5]]);
    }

    #[test]
    fn a_box_at_the_corner_of_the_world_stops_at_zero() {
        let bricks = check(&edit(TerrainEdit::DIG, TerrainEdit::ROUND, [0.0; 3], 3.0)).unwrap();
        assert_eq!(span(bricks), [[0, 0]; 3]);
        assert_eq!(bricks.iter().collect::<Vec<_>>(), [Brick([0; 3])]);
    }

    #[test]
    fn bricks_run_x_fastest_then_y_then_z() {
        let bricks = Bricks {
            first: [1, 2, 3],
            last: [2, 3, 3],
        };
        let order: Vec<[u32; 3]> = bricks.iter().map(|brick| brick.0).collect();
        assert_eq!(order, [[1, 2, 3], [2, 2, 3], [1, 3, 3], [2, 3, 3]]);
        assert_eq!(bricks.count(), order.len());
        assert_eq!(Brick([1, 2, 3]).min(), [8, 16, 24]);
    }

    #[test]
    fn edits_the_terrain_refuses_are_refused_here_too() {
        let fine = edit(TerrainEdit::DIG, TerrainEdit::ROUND, [20.0; 3], 4.0);
        let refused = [
            TerrainEdit {
                radius: 0.0,
                ..fine
            },
            TerrainEdit {
                radius: 16.5,
                ..fine
            },
            TerrainEdit {
                radius: f32::NAN,
                ..fine
            },
            TerrainEdit { shape: 2, ..fine },
            TerrainEdit { kind: 3, ..fine },
            TerrainEdit {
                flags: TerrainEdit::DRY_RUN,
                ..fine
            },
            TerrainEdit {
                position: [-0.5, 0.0, 0.0],
                ..fine
            },
            TerrainEdit {
                position: [f32::INFINITY, 0.0, 0.0],
                ..fine
            },
            TerrainEdit {
                position: [1e30, 0.0, 0.0],
                ..fine
            },
            TerrainEdit {
                kind: TerrainEdit::ADD,
                material: 0,
                ..fine
            },
            TerrainEdit {
                kind: TerrainEdit::ADD,
                height: 0.0,
                ..fine
            },
            TerrainEdit {
                kind: TerrainEdit::FLATTEN,
                material: 0x80,
                ..fine
            },
        ];
        assert!(check(&fine).is_ok());
        for edit in refused {
            assert_eq!(check(&edit), Err(Status::BAD_ARGS), "{edit:?}");
        }
    }
}
