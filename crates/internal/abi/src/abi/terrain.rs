//! Terrain and voxels: the terrain materials, terrain edits, and the voxels the server reads and the edits it reports.

use crate::abi::{Header, ItemId, ModHandle};

/// A terrain material of the running game, written by
/// [`HostApi::terrain_materials`](super::HostApi::terrain_materials).
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Header, TerrainMaterialInfo};
///
/// let dirt = TerrainMaterialInfo {
///     header: Header::of::<TerrainMaterialInfo>(),
///     id: 1,
///     flags: TerrainMaterialInfo::FLATTENS,
///     hardness: 0,
///     reserved: 0,
///     item: enshwick::ItemId::new(0x0d9b_7a1c),
///     health: 100,
/// };
/// assert_eq!(dirt.flags & TerrainMaterialInfo::FLATTENS, TerrainMaterialInfo::FLATTENS);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, bytemuck::Zeroable)]
pub struct TerrainMaterialInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// The material's voxel id, 1 to 127.
    pub id: u8,
    /// [`TerrainMaterialInfo::FLATTENS`] or none.
    pub flags: u8,
    /// How hard it is to dig, the game's hardness category from 0.
    pub hardness: u8,
    /// Zero.
    pub reserved: u8,
    /// The item digging it yields.
    pub item: ItemId,
    /// The health a voxel of it has against tools.
    pub health: i32,
}

impl_headed!(TerrainMaterialInfo);

impl TerrainMaterialInfo {
    /// A player's tool can flatten it.
    pub const FLATTENS: u8 = 1;
}

/// A terrain edit, for [`HostApi::terrain_edit`](super::HostApi::terrain_edit).
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Header, TerrainEdit};
///
/// let hole = TerrainEdit {
///     header: Header::of::<TerrainEdit>(),
///     kind: TerrainEdit::DIG,
///     shape: TerrainEdit::ROUND,
///     flags: 0,
///     material: 0,
///     position: [100.0, 40.0, 100.0],
///     radius: 3.0,
///     height: 0.0,
///     protect: [0; 16],
/// };
/// assert_eq!(hole.kind, TerrainEdit::DIG);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Zeroable)]
pub struct TerrainEdit {
    /// Size and version of this struct.
    pub header: Header,
    /// [`TerrainEdit::ADD`], [`TerrainEdit::DIG`] or
    /// [`TerrainEdit::FLATTEN`].
    pub kind: u32,
    /// [`TerrainEdit::ROUND`] or [`TerrainEdit::SQUARE`].
    pub shape: u32,
    /// [`TerrainEdit::DRY_RUN`] or none.
    pub flags: u32,
    /// The terrain material added or filled with, 1 to 127; unused by a
    /// dig.
    pub material: u32,
    /// Where, in metres; every axis 0 or more.
    pub position: [f32; 3],
    /// How far it reaches, in metres, above 0 and up to 16: a mound's or a
    /// flattened disc's radius, a dug sphere's.
    pub radius: f32,
    /// A mound's height at its middle, in metres, above 0 and up to 16;
    /// unused otherwise.
    pub height: f32,
    /// Terrain materials a dig leaves, a bit for each id (bit `id % 8` of
    /// byte `id / 8`): the dig stops at the first shell that meets one.
    pub protect: [u8; 16],
}

impl_headed!(TerrainEdit);

impl TerrainEdit {
    /// Adds a mound of a material, as a player's shovel does.
    pub const ADD: u32 = 0;
    /// Digs out terrain, as a pickaxe does, without its cost.
    pub const DIG: u32 = 1;
    /// Flattens the ground to the level of the position, filling with a
    /// material.
    pub const FLATTEN: u32 = 2;
    /// A disc or a sphere.
    pub const ROUND: u32 = 0;
    /// A square or a cube.
    pub const SQUARE: u32 = 1;
    /// Counts what the edit would change and changes nothing.
    pub const DRY_RUN: u32 = 1;
}

/// A voxel of the server's world, written by
/// [`HostApi::voxel_read`](super::HostApi::voxel_read). The loader decodes
/// the game's voxel into it, so a mod never reads the game's own encoding.
///
/// Its layout is frozen: [`HostApi::voxel_read`](super::HostApi::voxel_read)
/// writes an array of it with no entry size.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::Voxel;
///
/// let rock = Voxel { kind: Voxel::TERRAIN, material: 9, fill: 0xff, reserved: 0 };
/// assert_ne!(rock.kind, Voxel::AIR);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct Voxel {
    /// [`Voxel::AIR`], [`Voxel::TERRAIN`] or [`Voxel::BUILDING`]; a reader
    /// treats a kind it does not know as neither.
    pub kind: u8,
    /// The game's material id: 0 for air, a terrain material's id as
    /// [`TerrainMaterialInfo::id`] has it, or a building material's id.
    pub material: u8,
    /// How full the voxel is, 0 to 255: enshwick's name for the high byte
    /// of the game's voxel, which the game does not name.
    pub fill: u8,
    /// Zero.
    pub reserved: u8,
}

impl Voxel {
    /// Empty: no ground and no building.
    pub const AIR: u8 = 0;
    /// Ground of a terrain material.
    pub const TERRAIN: u8 = 1;
    /// A building block's material.
    pub const BUILDING: u8 = 2;
    /// The most voxels one [`HostApi::voxel_read`](super::HostApi::voxel_read)
    /// reads: a box of 64 voxels, 32 metres, a side.
    pub const READ_MAX: u64 = 64 * 64 * 64;
    /// The end of the coordinates a box covers on any axis, in voxels:
    /// every voxel of a box lies below it. 2³⁰, far past any world's edge.
    pub const END: u32 = 1 << 30;

    /// The voxels in the box of `size` from `min` when one
    /// [`HostApi::voxel_read`](super::HostApi::voxel_read) takes it: `None`
    /// for an empty box, one past [`Voxel::END`] or more than
    /// [`Voxel::READ_MAX`] voxels.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_abi as enshwick;
    /// use enshwick::abi::Voxel;
    ///
    /// assert_eq!(Voxel::read_count([0; 3], [2, 3, 4]), Some(24));
    /// assert_eq!(Voxel::read_count([0; 3], [0, 3, 4]), None);
    /// assert_eq!(Voxel::read_count([Voxel::END, 0, 0], [1; 3]), None);
    /// ```
    #[must_use]
    pub const fn read_count(min: [u32; 3], size: [u32; 3]) -> Option<u64> {
        let mut count = 1u64;
        let mut axis = 0;
        while axis < 3 {
            let within = match min[axis].checked_add(size[axis]) {
                Some(end) => end <= Self::END,
                None => false,
            };
            if size[axis] == 0 || !within {
                return None;
            }
            count = match count.checked_mul(size[axis] as u64) {
                Some(product) if product <= Self::READ_MAX => product,
                _ => return None,
            };
            axis += 1;
        }
        Some(count)
    }
}

/// One voxel a committed edit changed, as [`VoxelEditData`] lists it: where
/// it is and what it was and is.
///
/// Its layout is frozen: [`VoxelEditData`] points at an array of it with no
/// entry size.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Voxel, VoxelChange};
///
/// let dug = VoxelChange {
///     at: [200, 80, 200],
///     before: Voxel { kind: Voxel::TERRAIN, material: 9, fill: 0xff, reserved: 0 },
///     after: Voxel::default(),
/// };
/// assert_eq!(dug.after.kind, Voxel::AIR);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, bytemuck::Zeroable)]
pub struct VoxelChange {
    /// The voxel's place, in voxels from the world's corner.
    pub at: [u32; 3],
    /// The voxel before the edit.
    pub before: Voxel,
    /// The voxel after it.
    pub after: Voxel,
}

/// The data of [`Event::VOXELS_CHANGED`](super::Event::VOXELS_CHANGED): one
/// write of the server's voxel editor and the voxels it changed, borrowed
/// for the call that hands it over.
///
/// The game writes a whole box, aligned to its bricks of 8 voxels and
/// larger than the edit, so `changes` lists only the voxels that differ
/// from the box as it was. Layout grows only by appending; read a later
/// field after [`Header::covers`].
///
/// # Examples
///
/// ```
/// # extern crate enshwick_abi as enshwick;
/// use enshwick::abi::{Header, ModHandle, VoxelChange, VoxelEditData};
///
/// let changes = [VoxelChange::default()];
/// let edit = VoxelEditData {
///     header: Header::of::<VoxelEditData>(),
///     by: ModHandle::NONE,
///     min: [192, 72, 192],
///     size: [16, 16, 16],
///     changes: changes.as_ptr(),
///     count: 1,
/// };
/// assert_eq!(edit.by, ModHandle::NONE, "a player or the game made it");
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct VoxelEditData {
    /// Size and version of this struct.
    pub header: Header,
    /// The mod whose call through the API made the edit, such as a
    /// transaction's write or a terrain edit; [`ModHandle::NONE`] for the
    /// game's own edits, a player's tools and buildings among them, and
    /// for the data packs the loader runs.
    pub by: ModHandle,
    /// The lowest corner of the box the game wrote, in voxels.
    pub min: [u32; 3],
    /// The box's voxels along each axis.
    pub size: [u32; 3],
    /// The voxels that changed, `count` of them, in the box's order: x
    /// fastest, then y, then z. Valid for the call only.
    pub changes: *const VoxelChange,
    /// How many voxels changed, at least one.
    pub count: u64,
}

impl_headed!(VoxelEditData);
