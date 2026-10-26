//! Captures: a region's voxels and props as plain data, to turn, move and
//! keep.

use crate::abi;
use crate::voxels::{READ_MAX, Voxel, VoxelBox, VoxelPos, Voxels};

use super::{Prop, World, WorldError};

/// The most voxels one [`World::capture`] takes: 256 reads of the largest
/// box, a region of 128 metres a side and 64 high.
pub const CAPTURE_MAX: u64 = 256 * READ_MAX;

/// Voxels along each axis of one read of a capture: the largest box a read
/// takes is a cube of this side.
const TILE: u32 = 64;

const _: () = assert!(TILE as u64 * TILE as u64 * TILE as u64 == READ_MAX);

/// Metres a voxel spans along each axis.
const VOXEL_METRES: f64 = 0.5;

/// A region of the world as plain data, from [`World::capture`], to turn,
/// move and keep.
///
/// Its voxels and its props are both counted from the region's lowest
/// corner. A capture is enshwick's own; it implements serde's traits, so a
/// mod can keep it in its own format, and turns and moves without touching
/// the world.
///
/// # Examples
///
/// ```
/// use enshwick::voxels::VoxelPos;
/// use enshwick::world::{Capture, World, WorldError};
///
/// fn shed(world: World, corner: VoxelPos) -> Result<Capture, WorldError> {
///     let [x, y, z] = corner.to_array();
///     let capture = world.capture(corner, VoxelPos::new(x + 15, y + 9, z + 11))?;
///     enshwick::info!("{} props, {:?} voxels", capture.props().len(), capture.size());
///     Ok(capture.rotated(1))
/// }
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Parts")]
pub struct Capture {
    origin: VoxelPos,
    voxels: VoxelBox,
    props: Vec<Prop>,
}

/// A [`Capture`] as serde reads it, before its voxels are checked.
#[derive(serde::Deserialize)]
struct Parts {
    origin: VoxelPos,
    voxels: VoxelBox,
    props: Vec<Prop>,
}

impl TryFrom<Parts> for Capture {
    type Error = CaptureCornerError;

    fn try_from(parts: Parts) -> Result<Self, CaptureCornerError> {
        if parts.voxels.min() != VoxelPos::default() {
            return Err(CaptureCornerError(parts.voxels.min()));
        }
        Ok(Self {
            origin: parts.origin,
            voxels: parts.voxels,
            props: parts.props,
        })
    }
}

/// Why a [`Capture`] read with serde was refused: its voxels do not start
/// at the capture's corner.
#[derive(Debug, thiserror::Error)]
#[error("a capture's voxels start at {0:?}, not at its corner")]
struct CaptureCornerError(VoxelPos);

impl Capture {
    /// Where the region's lowest corner lay in the world, or lies after
    /// [`Capture::translated`].
    ///
    /// # Examples
    ///
    /// See [`Capture::translated`].
    #[must_use]
    pub const fn origin(&self) -> VoxelPos {
        self.origin
    }

    /// The region's voxels along each axis.
    ///
    /// # Examples
    ///
    /// See [`Capture`].
    #[must_use]
    pub const fn size(&self) -> [u32; 3] {
        self.voxels.size()
    }

    /// The region's voxels, with places counted from its corner: the
    /// corner voxel is at `(0, 0, 0)`.
    ///
    /// # Examples
    ///
    /// ```
    /// fn solid(capture: &enshwick::world::Capture) -> usize {
    ///     capture.voxels().iter().filter(|(_, voxel)| !voxel.is_air()).count()
    /// }
    /// ```
    #[must_use]
    pub const fn voxels(&self) -> &VoxelBox {
        &self.voxels
    }

    /// The region's props, with positions in metres from its corner.
    ///
    /// # Examples
    ///
    /// See [`Capture`].
    #[must_use]
    pub fn props(&self) -> &[Prop] {
        &self.props
    }

    /// The capture turned by `quarter_turns` quarter turns about the
    /// vertical, each taking the +x side to +z, as seen from above. The
    /// region keeps its corner: the turned region's lowest corner is still
    /// [`Capture::origin`]. Voxels move to their turned places, and props
    /// turn their positions and orientations with them.
    ///
    /// # Examples
    ///
    /// ```
    /// fn turned(capture: &enshwick::world::Capture) {
    ///     let [x, y, z] = capture.size();
    ///     assert_eq!(capture.rotated(1).size(), [z, y, x]);
    ///     assert_eq!(capture.rotated(-2), capture.rotated(2));
    ///     assert_eq!(capture.rotated(4), *capture);
    /// }
    /// ```
    #[must_use]
    pub fn rotated(&self, quarter_turns: i32) -> Self {
        let mut turned = self.clone();
        for _ in 0..quarter_turns.rem_euclid(4) {
            turned = turned.turned_once();
        }
        turned
    }

    /// The capture moved by `by` voxels; `None` when its corner would
    /// leave the coordinates of the world.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::VoxelPos;
    ///
    /// fn moved(capture: &enshwick::world::Capture) -> Option<VoxelPos> {
    ///     let up = capture.translated([0, 8, 0])?;
    ///     assert_eq!(up.voxels(), capture.voxels());
    ///     Some(up.origin())
    /// }
    /// ```
    #[must_use]
    pub fn translated(&self, by: [i64; 3]) -> Option<Self> {
        let [x, y, z] = [0, 1, 2].map(|axis| {
            let moved = i64::from(self.origin.to_array()[axis]).checked_add(by[axis])?;
            u32::try_from(moved)
                .ok()
                .filter(|&moved| moved < abi::Voxel::END)
        });
        Some(Self {
            origin: VoxelPos::new(x?, y?, z?),
            ..self.clone()
        })
    }

    /// The capture turned by one quarter turn: (x, z) to (depth − z, x).
    fn turned_once(&self) -> Self {
        let [width, height, depth] = self.voxels.size();
        let size = [depth, height, width];
        let mut voxels = Vec::with_capacity(self.voxels.as_slice().len());
        for z in 0..width {
            for y in 0..height {
                for x in 0..depth {
                    let from = VoxelPos::new(z, y, depth - 1 - x);
                    voxels.push(self.voxels.get(from).unwrap_or_default());
                }
            }
        }
        let depth_metres = f64::from(depth) * VOXEL_METRES;
        let props = self
            .props
            .iter()
            .map(|prop| {
                let mut prop = *prop;
                let [x, y, z] = prop.transform.position;
                prop.transform.position = [depth_metres - z, y, x];
                prop.transform.orientation = turn(prop.transform.orientation);
                prop
            })
            .collect();
        Self {
            origin: self.origin,
            voxels: VoxelBox::from_parts(VoxelPos::default(), size, voxels)
                .unwrap_or_else(|| self.voxels.clone()),
            props,
        }
    }
}

/// `orientation` after a quarter turn about +y that takes +x to +z: the
/// Hamilton product of the turn (0, −√½, 0, √½) and `orientation`, the
/// world's turn applied after the prop's own.
fn turn(orientation: [f32; 4]) -> [f32; 4] {
    let half = core::f32::consts::FRAC_1_SQRT_2;
    let [x, y, z, w] = orientation;
    [
        half * (x - z),
        half * (y - w),
        half * (z + x),
        half * (w + y),
    ]
}

impl World {
    /// The voxels and props of the region between voxels `a` and `b`, both
    /// included, as plain data counted from the region's lowest corner.
    /// It reads the voxels in boxes of up to [`READ_MAX`], and takes a prop
    /// whose position lies in the region.
    ///
    /// Every voxel read can pull a part of the world into memory: a region
    /// not loaded yet fails with [`WorldError::NotLoaded`] after every part
    /// has started loading, so the next try a second or so later finds
    /// more of it. Each read of the largest box costs about half a
    /// millisecond on the game thread.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for more than [`CAPTURE_MAX`] voxels;
    /// [`WorldError::NotLoaded`] while part of the region is not loaded,
    /// [`WorldError::OutsideWorld`] for one past the world's edge, and the
    /// other errors of [`Voxels::read`] and [`World::props_in`].
    ///
    /// # Examples
    ///
    /// See [`Capture`].
    pub fn capture(self, a: VoxelPos, b: VoxelPos) -> Result<Capture, WorldError> {
        let (a, b) = (a.to_array(), b.to_array());
        let low = [0, 1, 2].map(|axis| a[axis].min(b[axis]));
        let high = [0, 1, 2].map(|axis| a[axis].max(b[axis]));
        let size = [0, 1, 2].map(|axis| (high[axis] - low[axis]).saturating_add(1));
        let count = abi::Voxel::read_count(low, [1; 3])
            .and(
                size.iter()
                    .try_fold(1u64, |count, &axis| count.checked_mul(u64::from(axis))),
            )
            .filter(|&count| count <= CAPTURE_MAX)
            .ok_or(WorldError::Refused)?;
        let origin = VoxelPos::from(low);
        let voxels = self.capture_voxels(origin, size, count)?;
        let start = origin.to_metres();
        let end = VoxelPos::from(high)
            .to_metres()
            .map(|axis| axis + VOXEL_METRES);
        let props = self
            .props_in(start, end)?
            .into_iter()
            .filter(|prop| (0..3).all(|axis| prop.transform.position[axis] < end[axis]))
            .map(|mut prop| {
                for (axis, from) in prop.transform.position.iter_mut().zip(start) {
                    *axis -= from;
                }
                prop
            })
            .collect();
        Ok(Capture {
            origin,
            voxels,
            props,
        })
    }

    /// The voxels of the region of `size` from `origin`, `count` of them,
    /// read a tile at a time.
    fn capture_voxels(
        self,
        origin: VoxelPos,
        size: [u32; 3],
        count: u64,
    ) -> Result<VoxelBox, WorldError> {
        let reader = Voxels::new(self.0.host);
        let mut voxels =
            vec![Voxel::default(); usize::try_from(count).map_err(|_| WorldError::Refused)?];
        let mut not_loaded = false;
        let [width, height, _] = size.map(|axis| axis as usize);
        for tile in tiles(size) {
            let corner = [0, 1, 2].map(|axis| origin.to_array()[axis] + tile[axis]);
            let extent = [0, 1, 2].map(|axis| (size[axis] - tile[axis]).min(TILE));
            let block = match reader.read(VoxelPos::from(corner), extent) {
                Ok(block) => block,
                Err(WorldError::NotLoaded) => {
                    not_loaded = true;
                    continue;
                }
                Err(why) => return Err(why),
            };
            for (at, voxel) in &block {
                let [x, y, z] =
                    [0, 1, 2].map(|axis| (at.to_array()[axis] - origin.to_array()[axis]) as usize);
                if let Some(slot) = voxels.get_mut(x + width * (y + height * z)) {
                    *slot = voxel;
                }
            }
        }
        if not_loaded {
            return Err(WorldError::NotLoaded);
        }
        VoxelBox::from_parts(VoxelPos::default(), size, voxels).ok_or(WorldError::Refused)
    }
}

/// The corners of the tiles that cover a region of `size`, from its
/// corner, x fastest.
fn tiles(size: [u32; 3]) -> impl Iterator<Item = [u32; 3]> {
    let steps = |axis: u32| (0..axis).step_by(TILE as usize);
    steps(size[2])
        .flat_map(move |z| steps(size[1]).flat_map(move |y| steps(size[0]).map(move |x| [x, y, z])))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ItemId;
    use crate::ecs::Entity;
    use crate::voxels::VoxelMaterial;
    use crate::world::WorldTransform;

    /// A capture of `size` whose voxels each name their own place.
    fn numbered(size: [u32; 3], props: Vec<Prop>) -> Capture {
        let [width, height, depth] = size;
        let voxels = (0..depth)
            .flat_map(|z| (0..height).flat_map(move |y| (0..width).map(move |x| (x, y, z))))
            .map(|(x, y, z)| {
                let id = u8::try_from((x * 7 + y * 3 + z * 5) % 251).unwrap_or(0);
                Voxel::new(
                    VoxelMaterial::Building(id),
                    u8::try_from(x % 256).unwrap_or(0),
                )
            })
            .collect();
        Capture {
            origin: VoxelPos::new(1000, 200, 3000),
            voxels: VoxelBox::from_parts(VoxelPos::default(), size, voxels).unwrap(),
            props,
        }
    }

    /// Where `orientation` takes `direction`: q v q⁻¹ for a unit q.
    fn apply(orientation: [f32; 4], direction: [f64; 3]) -> [f64; 3] {
        let [qx, qy, qz, qw] = orientation.map(f64::from);
        let [dx, dy, dz] = direction;
        let cross = [
            2.0 * (qy * dz - qz * dy),
            2.0 * (qz * dx - qx * dz),
            2.0 * (qx * dy - qy * dx),
        ];
        [
            dx + qw * cross[0] + (qy * cross[2] - qz * cross[1]),
            dy + qw * cross[1] + (qz * cross[0] - qx * cross[2]),
            dz + qw * cross[2] + (qx * cross[1] - qy * cross[0]),
        ]
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        (0..3).all(|axis| (a[axis] - b[axis]).abs() < 1e-5)
    }

    fn unit_quaternion() -> impl Strategy<Value = [f32; 4]> {
        prop::array::uniform4(-1.0f32..1.0)
            .prop_filter("not near zero", |q| {
                q.iter().map(|p| p * p).sum::<f32>() > 0.01
            })
            .prop_map(|q| {
                let norm = q.iter().map(|p| p * p).sum::<f32>().sqrt();
                q.map(|p| p / norm)
            })
    }

    #[test]
    fn a_quarter_turn_takes_plus_x_to_plus_z() {
        let turned = turn(WorldTransform::UPRIGHT);
        assert!(close(apply(turned, [1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
        assert!(close(apply(turned, [0.0, 0.0, 1.0]), [-1.0, 0.0, 0.0]));
        assert!(close(apply(turned, [0.0, 1.0, 0.0]), [0.0, 1.0, 0.0]));
    }

    #[test]
    fn the_tiles_cover_a_region_once() {
        let size = [130, 3, 64];
        let mut covered = 0u64;
        for tile in tiles(size) {
            let extent = [0, 1, 2].map(|axis| u64::from((size[axis] - tile[axis]).min(TILE)));
            covered += extent.iter().product::<u64>();
        }
        assert_eq!(covered, 130 * 3 * 64);
        assert_eq!(tiles(size).count(), 3);
    }

    proptest! {
        #[test]
        fn a_quarter_turn_maps_the_box_exactly(
            width in 1u32..9, height in 1u32..5, depth in 1u32..9, turns in -8i32..8,
        ) {
            let capture = numbered([width, height, depth], Vec::new());
            let turned = capture.rotated(turns);
            let mut seen: Vec<Voxel> = turned.voxels().as_slice().to_vec();
            let mut all: Vec<Voxel> = capture.voxels().as_slice().to_vec();
            seen.sort_unstable_by_key(|v| format!("{v:?}"));
            all.sort_unstable_by_key(|v| format!("{v:?}"));
            prop_assert_eq!(seen, all);
            let expected = if turns.rem_euclid(2) == 1 { [depth, height, width] } else { [width, height, depth] };
            prop_assert_eq!(turned.size(), expected);
            prop_assert_eq!(turned.origin(), capture.origin());
            prop_assert_eq!(capture.rotated(turns).rotated(4 - turns.rem_euclid(4)), capture.clone());
        }

        #[test]
        fn a_prop_turns_with_the_voxel_it_stands_in(
            width in 1u32..9, height in 1u32..5, depth in 1u32..9,
            x in 0u32..8, y in 0u32..4, z in 0u32..8, turns in 0i32..4,
            orientation in unit_quaternion(),
        ) {
            let (x, y, z) = (x % width, y % height, z % depth);
            let centre = [x, y, z].map(|axis| (f64::from(axis) + 0.5) * VOXEL_METRES);
            let prop = Prop {
                entity: Entity(9),
                item: ItemId(4),
                transform: WorldTransform { position: centre, orientation },
            };
            let capture = numbered([width, height, depth], vec![prop]);
            let marked = capture.voxels().get(VoxelPos::new(x, y, z)).unwrap();
            let turned = capture.rotated(turns);
            let moved = turned.props()[0].transform;
            let at = VoxelPos::from_metres(moved.position).unwrap();
            prop_assert_eq!(turned.voxels().get(at), Some(marked));

            let mut facing = apply(orientation, [1.0, 0.0, 0.0]);
            for _ in 0..turns {
                facing = [-facing[2], facing[1], facing[0]];
            }
            prop_assert!(close(apply(moved.orientation, [1.0, 0.0, 0.0]), facing));
            let norm: f32 = moved.orientation.iter().map(|p| p * p).sum();
            prop_assert!((norm - 1.0).abs() < 1e-4);
        }

        #[test]
        fn a_move_keeps_the_content_and_shifts_the_corner(
            dx in -2000i64..2000, dy in -300i64..300, dz in -4000i64..4000,
        ) {
            let capture = numbered([2, 2, 2], Vec::new());
            let moved = capture.translated([dx, dy, dz]);
            let expected = [1000 + dx, 200 + dy, 3000 + dz];
            if expected.iter().all(|&axis| axis >= 0) {
                let moved = moved.unwrap();
                prop_assert_eq!(moved.voxels(), capture.voxels());
                let origin = moved.origin().to_array().map(i64::from);
                prop_assert_eq!(origin, expected);
                prop_assert_eq!(moved.translated([-dx, -dy, -dz]), Some(capture));
            } else {
                prop_assert_eq!(moved, None);
            }
        }
    }

    #[test]
    fn a_capture_whose_voxels_leave_its_corner_is_refused_by_serde() {
        let mut capture = numbered([1, 1, 1], Vec::new());
        capture.voxels =
            VoxelBox::from_parts(VoxelPos::new(1, 0, 0), [1, 1, 1], vec![Voxel::default()])
                .unwrap();
        let bytes = postcard::to_allocvec(&capture).unwrap();
        assert!(postcard::from_bytes::<Capture>(&bytes).is_err());
        let ok = numbered([2, 1, 1], Vec::new());
        let bytes = postcard::to_allocvec(&ok).unwrap();
        assert_eq!(postcard::from_bytes::<Capture>(&bytes).unwrap(), ok);
    }
}
