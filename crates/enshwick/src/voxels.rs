//! Voxels of the server's world: what the ground and the building blocks
//! are made of, voxel by voxel.
//!
//! The world is a grid of voxels half a metre a side, counted from the
//! world's corner ([`VoxelPos`]). Each voxel is air, ground of a terrain
//! material ([`Terrain::materials`](crate::terrain::Terrain::materials)
//! lists them) or a building block's material, and is more or less full.
//! The server holds only some parts of the world in memory: a read
//! elsewhere fails with [`WorldError::NotLoaded`] and starts loading that
//! part, which is there to read about a second later. Every read can pull
//! a part of the world into memory, so a mod reads where it needs to, never
//! the whole world.
//! A read past the world's edge, or above the 1,024 metres the loader
//! takes, fails with [`WorldError::OutsideWorld`].
//!
//! Every read belongs on the server's game thread at a quiet moment, as the
//! world's writes do: from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action, never from
//! a filter or a hook inside a step; on a client, while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Outcome};
//! use enshwick::voxels::{VoxelMaterial, VoxelPos};
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let voxels = load.voxels();
//!     let world = load.world()?;
//!     let actions = load.actions(enshwick::i18n::Texts::default())?;
//!     actions.add(Action::new("ground", "What am I standing on?"), move |request| {
//!         let Some([x, y, z]) = world.position(request.player.entity) else {
//!             return Outcome::refused("You are nowhere");
//!         };
//!         let Some(feet) = VoxelPos::from_metres([x, y - 0.5, z]) else {
//!             return Outcome::refused("Outside the world");
//!         };
//!         match voxels.sample(feet).map(|voxel| voxel.material()) {
//!             Ok(VoxelMaterial::Terrain(id)) => Outcome::done(format!("Ground of material {id}")),
//!             Ok(VoxelMaterial::Building(id)) => Outcome::done(format!("A block of material {id}")),
//!             Ok(_) => Outcome::done("Nothing"),
//!             Err(why) => Outcome::refused(format!("{why}")),
//!         }
//!     })
//! }
//! ```

use crate::abi;
use crate::world::WorldError;
use crate::{Host, Load};

/// The most voxels one [`Voxels::read`] takes: a box of 64 voxels, 32
/// metres, a side.
pub const READ_MAX: u64 = abi::Voxel::READ_MAX;

/// Voxels per metre on every axis.
const PER_METRE: f64 = 2.0;

/// A voxel's place in the world: its index on each axis from the world's
/// corner, half a metre a step, with y up.
///
/// # Examples
///
/// ```
/// use enshwick::voxels::VoxelPos;
///
/// let at = VoxelPos::from_metres([10.2, 4.0, 7.9]).ok_or("outside the world")?;
/// assert_eq!(at, VoxelPos::new(20, 8, 15));
/// assert_eq!(at.to_metres(), [10.0, 4.0, 7.5]);
/// # Ok::<(), &str>(())
/// ```
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct VoxelPos {
    x: u32,
    y: u32,
    z: u32,
}

impl VoxelPos {
    /// The voxel at these indices.
    ///
    /// # Examples
    ///
    /// See [`VoxelPos`].
    #[must_use]
    pub const fn new(x: u32, y: u32, z: u32) -> Self {
        Self { x, y, z }
    }

    /// The voxel holding the point `metres`, as world positions give it;
    /// `None` for a point below the world's corner, too far out or not a
    /// number.
    ///
    /// # Examples
    ///
    /// See [`VoxelPos`].
    #[must_use]
    pub fn from_metres(metres: [f64; 3]) -> Option<Self> {
        let [x, y, z] = metres.map(index);
        Some(Self::new(x?, y?, z?))
    }

    /// The lowest corner of the voxel, in metres.
    ///
    /// # Examples
    ///
    /// See [`VoxelPos`].
    #[must_use]
    pub fn to_metres(self) -> [f64; 3] {
        self.to_array().map(|axis| f64::from(axis) / PER_METRE)
    }

    /// The index along x.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::voxels::VoxelPos::new(1, 2, 3).x(), 1);
    /// ```
    #[must_use]
    pub const fn x(self) -> u32 {
        self.x
    }

    /// The index along y, upwards.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::voxels::VoxelPos::new(1, 2, 3).y(), 2);
    /// ```
    #[must_use]
    pub const fn y(self) -> u32 {
        self.y
    }

    /// The index along z.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::voxels::VoxelPos::new(1, 2, 3).z(), 3);
    /// ```
    #[must_use]
    pub const fn z(self) -> u32 {
        self.z
    }

    /// The indices as `[x, y, z]`.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::voxels::VoxelPos::new(1, 2, 3).to_array(), [1, 2, 3]);
    /// ```
    #[must_use]
    pub const fn to_array(self) -> [u32; 3] {
        [self.x, self.y, self.z]
    }
}

impl From<[u32; 3]> for VoxelPos {
    fn from([x, y, z]: [u32; 3]) -> Self {
        Self::new(x, y, z)
    }
}

impl From<VoxelPos> for [u32; 3] {
    fn from(at: VoxelPos) -> Self {
        at.to_array()
    }
}

/// The voxel index holding `metres` on one axis, below [`abi::Voxel::END`].
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is checked to lie in 0 to `Voxel::END` first"
)]
fn index(metres: f64) -> Option<u32> {
    let voxels = (metres * PER_METRE).floor();
    (voxels >= 0.0 && voxels < f64::from(abi::Voxel::END)).then_some(voxels as u32)
}

/// What a voxel is made of. The kinds are enshwick's names for the ranges
/// of the game's material ids.
///
/// # Examples
///
/// ```
/// use enshwick::voxels::VoxelMaterial;
///
/// fn is_ground(material: VoxelMaterial) -> bool {
///     matches!(material, VoxelMaterial::Terrain(_))
/// }
/// assert!(!is_ground(VoxelMaterial::Air));
/// ```
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum VoxelMaterial {
    /// Empty: no ground and no building.
    #[default]
    Air,
    /// Ground of the terrain material with this id, as
    /// [`TerrainTerraformingConfig::terrain_material`](crate::terrain::TerrainTerraformingConfig::terrain_material)
    /// has it.
    Terrain(u8),
    /// A building block of the building material with this id.
    Building(u8),
    /// A kind of voxel this version of the API does not know, with the
    /// game's material id.
    Unknown(u8),
}

/// One voxel of the world: its material and how full it is.
///
/// # Examples
///
/// ```
/// use enshwick::voxels::{Voxel, VoxelMaterial};
///
/// let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
/// assert!(!rock.is_air());
/// assert_eq!(rock.fill(), 255);
/// ```
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct Voxel {
    material: VoxelMaterial,
    fill: u8,
}

impl Voxel {
    /// A voxel of `material`, `fill` full out of 255.
    ///
    /// # Examples
    ///
    /// See [`Voxel`].
    #[must_use]
    pub const fn new(material: VoxelMaterial, fill: u8) -> Self {
        Self { material, fill }
    }

    /// What it is made of.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn material(self) -> VoxelMaterial {
        self.material
    }

    /// How full it is, 0 to 255, which shapes the ground's surface between
    /// voxels. The name is enshwick's: the game does not name it.
    ///
    /// # Examples
    ///
    /// See [`Voxel`].
    #[must_use]
    pub const fn fill(self) -> u8 {
        self.fill
    }

    /// Whether it holds no ground and no building.
    ///
    /// # Examples
    ///
    /// See [`Voxel`].
    #[must_use]
    pub const fn is_air(self) -> bool {
        matches!(self.material, VoxelMaterial::Air)
    }

    /// The voxel the loader wrote.
    pub(crate) const fn from_abi(raw: abi::Voxel) -> Self {
        let material = match raw.kind {
            abi::Voxel::AIR => VoxelMaterial::Air,
            abi::Voxel::TERRAIN => VoxelMaterial::Terrain(raw.material),
            abi::Voxel::BUILDING => VoxelMaterial::Building(raw.material),
            _ => VoxelMaterial::Unknown(raw.material),
        };
        Self::new(material, raw.fill)
    }

    /// The voxel as the loader writes it.
    #[cfg(any(test, feature = "test"))]
    pub(crate) const fn to_abi(self) -> abi::Voxel {
        let (kind, material) = match self.material {
            VoxelMaterial::Air => (abi::Voxel::AIR, 0),
            VoxelMaterial::Terrain(id) => (abi::Voxel::TERRAIN, id),
            VoxelMaterial::Building(id) => (abi::Voxel::BUILDING, id),
            VoxelMaterial::Unknown(id) => (u8::MAX, id),
        };
        abi::Voxel {
            kind,
            material,
            fill: self.fill,
            reserved: 0,
        }
    }
}

/// A box of voxels read from the world, from [`Voxels::read`].
///
/// # Examples
///
/// ```
/// use enshwick::voxels::{VoxelPos, Voxels};
///
/// fn solid(voxels: &Voxels, corner: VoxelPos) -> Result<usize, enshwick::world::WorldError> {
///     let block = voxels.read(corner, [4, 4, 4])?;
///     Ok(block.iter().filter(|(_, voxel)| !voxel.is_air()).count())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Parts")]
pub struct VoxelBox {
    min: VoxelPos,
    size: [u32; 3],
    voxels: Vec<Voxel>,
}

/// A [`VoxelBox`] as serde reads it, before its voxels are counted.
#[derive(serde::Deserialize)]
struct Parts {
    min: VoxelPos,
    size: [u32; 3],
    voxels: Vec<Voxel>,
}

/// Why a [`VoxelBox`] read with serde was refused.
#[derive(Debug, thiserror::Error)]
#[error("a voxel box of {size:?} from {min:?} holding {held} voxels")]
struct VoxelBoxError {
    min: VoxelPos,
    size: [u32; 3],
    held: usize,
}

impl TryFrom<Parts> for VoxelBox {
    type Error = VoxelBoxError;

    fn try_from(parts: Parts) -> Result<Self, VoxelBoxError> {
        let (min, size, held) = (parts.min, parts.size, parts.voxels.len());
        Self::from_parts(min, size, parts.voxels).ok_or(VoxelBoxError { min, size, held })
    }
}

impl VoxelBox {
    /// The box of `size` from `min` holding `voxels`, x fastest, then y,
    /// then z; `None` unless there is one voxel for each place and the box
    /// ends below [`abi::Voxel::END`] on every axis.
    pub(crate) fn from_parts(min: VoxelPos, size: [u32; 3], voxels: Vec<Voxel>) -> Option<Self> {
        let within = (0..3).all(|axis| {
            min.to_array()[axis]
                .checked_add(size[axis])
                .is_some_and(|end| end <= abi::Voxel::END)
        });
        let count = size
            .iter()
            .try_fold(1usize, |count, &axis| count.checked_mul(axis as usize))?;
        (within && count == voxels.len()).then_some(Self { min, size, voxels })
    }

    /// The box turned a quarter turn about the vertical, keeping its
    /// corner: the voxel at (x, y, z) from the corner moves to
    /// (depth − 1 − z, y, x).
    pub(crate) fn turned(&self) -> Self {
        let [width, height, depth] = self.size.map(|axis| axis as usize);
        let mut voxels = Vec::with_capacity(self.voxels.len());
        for new_z in 0..width {
            for y in 0..height {
                for new_x in 0..depth {
                    let (x, z) = (new_z, depth - 1 - new_x);
                    voxels.push(self.voxels[x + width * (y + height * z)]);
                }
            }
        }
        let [width, height, depth] = self.size;
        Self {
            min: self.min,
            size: [depth, height, width],
            voxels,
        }
    }

    /// The same voxels with the corner at `(0, 0, 0)`.
    pub(crate) fn at_corner(self) -> Self {
        Self {
            min: VoxelPos::default(),
            ..self
        }
    }

    /// Copies `block`, which lies inside this box, into its place.
    pub(crate) fn fill_from(&mut self, block: &Self) {
        let [width, height, _] = self.size.map(|axis| axis as usize);
        let [row, rows, _] = block.size.map(|axis| axis as usize);
        let [x, y, z] =
            [0, 1, 2].map(|axis| (block.min.to_array()[axis] - self.min.to_array()[axis]) as usize);
        for (index, source) in block.voxels.chunks_exact(row).enumerate() {
            let start = x + width * (y + index % rows + height * (z + index / rows));
            self.voxels[start..start + row].copy_from_slice(source);
        }
    }

    /// Its lowest corner.
    ///
    /// # Examples
    ///
    /// See [`VoxelBox::get`].
    #[must_use]
    pub const fn min(&self) -> VoxelPos {
        self.min
    }

    /// Its voxels along each axis.
    ///
    /// # Examples
    ///
    /// See [`VoxelBox::get`].
    #[must_use]
    pub const fn size(&self) -> [u32; 3] {
        self.size
    }

    /// The voxel at `at`, a place in the world; `None` outside the box.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::{Voxel, VoxelBox, VoxelPos};
    ///
    /// fn top(block: &VoxelBox) -> Option<Voxel> {
    ///     let [x, y, z] = block.min().to_array();
    ///     let [_, height, _] = block.size();
    ///     block.get(VoxelPos::new(x, y + height - 1, z))
    /// }
    /// ```
    #[must_use]
    pub fn get(&self, at: VoxelPos) -> Option<Voxel> {
        let mut index = 0usize;
        for axis in (0..3).rev() {
            let offset = at.to_array()[axis].checked_sub(self.min.to_array()[axis])?;
            if offset >= self.size[axis] {
                return None;
            }
            index = index * self.size[axis] as usize + offset as usize;
        }
        self.voxels.get(index).copied()
    }

    /// Every voxel, x fastest, then y, then z.
    ///
    /// # Examples
    ///
    /// ```
    /// fn all_air(block: &enshwick::voxels::VoxelBox) -> bool {
    ///     block.as_slice().iter().all(|voxel| voxel.is_air())
    /// }
    /// ```
    #[must_use]
    pub fn as_slice(&self) -> &[Voxel] {
        &self.voxels
    }

    /// Every voxel with its place in the world, x fastest, then y, then z.
    ///
    /// # Examples
    ///
    /// See [`VoxelBox`].
    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            block: self,
            next: 0,
        }
    }
}

impl<'a> IntoIterator for &'a VoxelBox {
    type Item = (VoxelPos, Voxel);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

/// The voxels of a [`VoxelBox`] with their places, from [`VoxelBox::iter`].
///
/// # Examples
///
/// See [`VoxelBox`].
#[derive(Clone, Debug)]
pub struct Iter<'a> {
    block: &'a VoxelBox,
    next: usize,
}

impl Iterator for Iter<'_> {
    type Item = (VoxelPos, Voxel);

    fn next(&mut self) -> Option<Self::Item> {
        let voxel = *self.block.voxels.get(self.next)?;
        let [width, height, _] = self.block.size.map(|axis| axis as usize);
        let [x, y, z] = [
            self.next % width,
            self.next / width % height,
            self.next / (width * height),
        ]
        .map(|offset| u32::try_from(offset).unwrap_or(u32::MAX));
        let [min_x, min_y, min_z] = self.block.min.to_array();
        self.next += 1;
        Some((VoxelPos::new(min_x + x, min_y + y, min_z + z), voxel))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.block.voxels.len() - self.next;
        (left, Some(left))
    }
}

impl ExactSizeIterator for Iter<'_> {}

/// Voxel reads of the server's world; get it from [`Load::voxels`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Voxels {
    host: Host,
}

impl Voxels {
    /// The reads through `host`.
    pub(crate) const fn new(host: Host) -> Self {
        Self { host }
    }

    /// The voxels of the box of `size` voxels from `min`, its lowest corner.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for an empty box, more than [`READ_MAX`]
    /// voxels or one past 2³⁰ voxels on an axis;
    /// [`WorldError::NotLoaded`] while part of the box is not loaded yet,
    /// which this read starts loading: try again in a second or so;
    /// [`WorldError::Missing`] before a world runs or on
    /// a game build without the read; [`WorldError::WrongThread`] off the
    /// game thread or inside a step; [`WorldError::OutsideWorld`] for a box
    /// past the world's edge or above the height the loader takes, 1,024
    /// metres.
    ///
    /// # Examples
    ///
    /// See [`VoxelBox`].
    pub fn read(&self, min: VoxelPos, size: [u32; 3]) -> Result<VoxelBox, WorldError> {
        let count = abi::Voxel::read_count(min.to_array(), size).ok_or(WorldError::Refused)?;
        let mut raw =
            vec![abi::Voxel::default(); usize::try_from(count).map_err(|_| WorldError::Refused)?];
        let corner = min.to_array();
        let status = (self.host.api().voxel_read)(
            self.host.handle(),
            &raw const corner,
            &raw const size,
            raw.as_mut_ptr(),
            count,
        );
        WorldError::from_status(status)?;
        Ok(VoxelBox {
            min,
            size,
            voxels: raw.into_iter().map(Voxel::from_abi).collect(),
        })
    }

    /// The voxel at `at`.
    ///
    /// # Errors
    ///
    /// As for [`Voxels::read`].
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn sample(&self, at: VoxelPos) -> Result<Voxel, WorldError> {
        let mut raw = abi::Voxel::default();
        let corner = at.to_array();
        let size = [1u32; 3];
        let status = (self.host.api().voxel_read)(
            self.host.handle(),
            &raw const corner,
            &raw const size,
            &raw mut raw,
            1,
        );
        WorldError::from_status(status).map(|()| Voxel::from_abi(raw))
    }
}

impl Load<'_> {
    /// The [`Voxels`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn voxels(&mut self) -> Voxels {
        Voxels { host: self.host() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_lies_in_the_voxel_below_it_on_every_axis() {
        assert_eq!(
            VoxelPos::from_metres([0.0, 0.49, 0.5]),
            Some(VoxelPos::new(0, 0, 1))
        );
        assert_eq!(VoxelPos::from_metres([-0.1, 0.0, 0.0]), None);
        assert_eq!(VoxelPos::from_metres([f64::NAN, 0.0, 0.0]), None);
        assert_eq!(VoxelPos::from_metres([0.0, 1e12, 0.0]), None);
    }

    #[test]
    fn every_material_survives_the_abi() {
        for material in [
            VoxelMaterial::Air,
            VoxelMaterial::Terrain(9),
            VoxelMaterial::Building(0x83),
            VoxelMaterial::Unknown(4),
        ] {
            let voxel = Voxel::new(material, 0x40);
            assert_eq!(Voxel::from_abi(voxel.to_abi()), voxel);
        }
    }

    #[test]
    fn serde_refuses_a_box_whose_count_or_end_is_wrong() {
        let short = VoxelBox {
            min: VoxelPos::new(0, 0, 0),
            size: [2, 1, 1],
            voxels: vec![Voxel::default()],
        };
        let bytes = postcard::to_allocvec(&short).unwrap();
        assert!(postcard::from_bytes::<VoxelBox>(&bytes).is_err());

        let past_the_end = VoxelBox {
            min: VoxelPos::new(abi::Voxel::END - 1, 0, 0),
            size: [2, 1, 1],
            voxels: vec![Voxel::default(); 2],
        };
        let bytes = postcard::to_allocvec(&past_the_end).unwrap();
        assert!(postcard::from_bytes::<VoxelBox>(&bytes).is_err());

        let wrapping = VoxelBox {
            min: VoxelPos::new(u32::MAX, 0, 0),
            size: [2, 1, 1],
            voxels: vec![Voxel::default(); 2],
        };
        let bytes = postcard::to_allocvec(&wrapping).unwrap();
        assert!(postcard::from_bytes::<VoxelBox>(&bytes).is_err());

        let fine = VoxelBox {
            min: VoxelPos::new(abi::Voxel::END - 2, 0, 0),
            size: [2, 1, 1],
            voxels: vec![Voxel::default(); 2],
        };
        let bytes = postcard::to_allocvec(&fine).unwrap();
        assert_eq!(postcard::from_bytes::<VoxelBox>(&bytes).unwrap(), fine);
    }

    #[test]
    fn a_block_fills_its_place_row_by_row() {
        let mut whole = VoxelBox {
            min: VoxelPos::new(10, 10, 10),
            size: [4, 3, 2],
            voxels: vec![Voxel::default(); 24],
        };
        let rock = |i: u8| Voxel::new(VoxelMaterial::Terrain(i + 1), 255);
        let block = VoxelBox {
            min: VoxelPos::new(11, 11, 10),
            size: [2, 2, 2],
            voxels: (0..8).map(rock).collect(),
        };
        whole.fill_from(&block);
        for (at, voxel) in &block {
            assert_eq!(whole.get(at), Some(voxel), "{at:?}");
        }
        assert_eq!(whole.iter().filter(|(_, v)| !v.is_air()).count(), 8);
    }

    #[test]
    fn a_box_finds_each_voxel_where_iter_puts_it() {
        let size = [3, 2, 4];
        let voxels = (0..24)
            .map(|i| Voxel::new(VoxelMaterial::Terrain(i), i))
            .collect();
        let block = VoxelBox {
            min: VoxelPos::new(10, 20, 30),
            size,
            voxels,
        };
        assert_eq!(block.iter().len(), 24);
        for (at, voxel) in &block {
            assert_eq!(block.get(at), Some(voxel), "{at:?}");
        }
        assert_eq!(
            block.get(VoxelPos::new(11, 21, 32)).map(Voxel::fill),
            Some(1 + 3 + 2 * 6)
        );
        assert_eq!(block.get(VoxelPos::new(13, 20, 30)), None);
        assert_eq!(block.get(VoxelPos::new(9, 20, 30)), None);
    }
}
