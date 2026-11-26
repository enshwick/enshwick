//! Terrain in the server's world: the terrain materials, and adding,
//! digging and flattening ground as the game's terraforming does.
//!
//! The ground is made of voxels half a metre a side, each of a material:
//! terrain materials have ids 1 to 127 ([`Terrain::materials`] lists them
//! with the item digging each yields). An edit changes the ground at once
//! and clients see it; the save keeps it only inside a base, a Flame
//! Altar's building zone (server 399). Built pieces are never touched.
//! A player's shovel and pickaxe also check where the player may build,
//! take the cost and give the loot and experience; an edit here does none
//! of that, so a mod decides what it needs. An edit here keeps no undo; a
//! `Transaction` of the same edits does. An
//! edit where the world is not loaded yet writes nothing and fails with
//! [`WorldError::NotLoaded`]: the game would compute it against air (an
//! add counting its whole mound, a dig removing nothing) and merge that
//! onto the ground once it loads. The check starts loading the area, so
//! the same edit works again a second or so later.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or an action; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::terrain::{Shape, Terrain};
//!
//! fn crater(terrain: &Terrain, at: [f64; 3]) -> Result<f32, enshwick::WorldError> {
//!     terrain.dig(at, 4.0, Shape::Round, &[])
//! }
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<Terrain, enshwick::Off> {
//!     Ok(load.terrain())
//! }
//! ```

use crate::WorldError;
use crate::abi::{Status, TerrainEdit, TerrainMaterialInfo};
use crate::{Host, ItemId, Off};
#[cfg(feature = "unstable")]
use crate::{
    abi::TerrainMaterialAdd,
    content::{Guid, Id},
};

/// The largest radius and height of an edit, in metres.
pub const MAX_RADIUS: f32 = 16.0;

/// A terrain material of the running game, from [`Terrain::materials`].
///
/// # Examples
///
/// ```
/// fn dirt(terrain: &enshwick::terrain::Terrain, dirt_item: enshwick::ItemId) -> Option<u8> {
///     terrain.materials().iter().find(|m| m.terrain_item == dirt_item).map(|m| m.terrain_material)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct TerrainTerraformingConfig {
    /// Its voxel id, 1 to 127.
    pub terrain_material: u8,
    /// The item digging it yields.
    pub terrain_item: ItemId,
    /// How hard it is to dig: the game's hardness category from 0.
    pub hardness: u8,
    /// The health a voxel of it has against tools.
    pub health_points: i32,
    /// Whether a player's tool can flatten it.
    pub can_be_flattened: bool,
}

/// A terrain material of a mod's own: a copy of one of the game's, its look
/// included, with rules changed, for
/// [`Load::add_terrain_material`](../struct.Load.html#method.add_terrain_material).
///
/// The loader gives mods' materials the terrain ids the game leaves free,
/// 117 to 127, in load order, so the twelfth is refused. Its id holds for
/// one run; saves name its ground by its content id, so a world keeps it
/// from run to run. In a world loaded without the mod, its ground loads as
/// `fallback` under Enshwick and as air under a server without it. Every
/// client draws the ground from its own copy of the game's tables, so a mod
/// that adds one runs on clients too.
///
/// Unstable: needs the `unstable` feature and may change in any release.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::terrain::NewTerrainMaterial;
///
/// fn tar_sand(load: &mut enshwick::Load<'_>, sand: u8, tar: Guid) -> Result<u8, enshwick::Off> {
///     let new = NewTerrainMaterial::new(Id::new("you.sands:tar-sand"), sand)
///         .terrain_item(tar)
///         .health_points(7);
///     load.add_terrain_material(new)
/// }
/// ```
#[cfg(feature = "unstable")]
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct NewTerrainMaterial {
    /// Its content id, `mod:name`.
    pub id: Id,
    /// The game's terrain material it copies, by voxel id: its look and
    /// every rule it does not change.
    pub like: u8,
    /// The game's terrain material its ground loads as in a world loaded
    /// without the mod, by voxel id; `None` for `like`.
    #[script(default)]
    pub fallback: Option<u8>,
    /// The item digging it yields, by its GUID.
    #[script(default)]
    pub terrain_item: Option<Guid>,
    /// How hard it is to dig: the game's hardness category, from 0 (`Soft`)
    /// to 5 (`Unbreakable`).
    #[script(default)]
    pub hardness: Option<u8>,
    /// The health a voxel of it has against tools, 1 or more.
    #[script(default)]
    pub health_points: Option<i32>,
    /// Whether a player's tool can flatten it.
    #[script(default)]
    pub can_be_flattened: Option<bool>,
}

#[cfg(feature = "unstable")]
impl NewTerrainMaterial {
    /// A copy of the game's terrain material `like` under the content id
    /// `id`, falling back to `like`.
    ///
    /// # Examples
    ///
    /// See [`NewTerrainMaterial`].
    #[must_use]
    pub const fn new(id: Id, like: u8) -> Self {
        Self {
            id,
            like,
            fallback: None,
            terrain_item: None,
            hardness: None,
            health_points: None,
            can_be_flattened: None,
        }
    }

    /// Makes its ground load as the game's material `fallback` instead in a
    /// world loaded without the mod.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    /// use enshwick::terrain::NewTerrainMaterial;
    ///
    /// let ore = NewTerrainMaterial::new(Id::new("you.ores:glow-rock"), 24).fallback(9);
    /// assert_eq!(ore.fallback, Some(9));
    /// ```
    #[must_use]
    pub const fn fallback(mut self, fallback: u8) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Sets the item digging it yields.
    ///
    /// # Examples
    ///
    /// See [`NewTerrainMaterial`].
    #[must_use]
    pub const fn terrain_item(mut self, item: Guid) -> Self {
        self.terrain_item = Some(item);
        self
    }

    /// Sets how hard it is to dig.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    /// use enshwick::terrain::NewTerrainMaterial;
    ///
    /// let hard = NewTerrainMaterial::new(Id::new("you.ores:hard-sand"), 9).hardness(3);
    /// assert_eq!(hard.hardness, Some(3));
    /// ```
    #[must_use]
    pub const fn hardness(mut self, hardness: u8) -> Self {
        self.hardness = Some(hardness);
        self
    }

    /// Sets the health a voxel of it has against tools.
    ///
    /// # Examples
    ///
    /// See [`NewTerrainMaterial`].
    #[must_use]
    pub const fn health_points(mut self, health: i32) -> Self {
        self.health_points = Some(health);
        self
    }

    /// Sets whether a player's tool can flatten it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    /// use enshwick::terrain::NewTerrainMaterial;
    ///
    /// let rough = NewTerrainMaterial::new(Id::new("you.ores:rough-sand"), 9).can_be_flattened(false);
    /// assert_eq!(rough.can_be_flattened, Some(false));
    /// ```
    #[must_use]
    pub const fn can_be_flattened(mut self, flattens: bool) -> Self {
        self.can_be_flattened = Some(flattens);
        self
    }

    /// The material as the loader takes it, under the GUID `guid`.
    fn request(&self, guid: Guid) -> TerrainMaterialAdd {
        let mut changes = 0;
        let mut set = |value: bool, bit| {
            if value {
                changes |= bit;
            }
        };
        set(self.terrain_item.is_some(), TerrainMaterialAdd::ITEM);
        set(self.hardness.is_some(), TerrainMaterialAdd::HARDNESS);
        set(self.health_points.is_some(), TerrainMaterialAdd::HEALTH);
        set(
            self.can_be_flattened.is_some(),
            TerrainMaterialAdd::FLATTENS,
        );
        enshwick_abi::headed!(TerrainMaterialAdd {
            guid,
            item: self.terrain_item.unwrap_or(Guid::NIL),
            health: self.health_points.unwrap_or(0),
            like: self.like,
            fallback: self.fallback.unwrap_or(self.like),
            changes,
            hardness: self.hardness.unwrap_or(0),
            flags: if self.can_be_flattened == Some(true) {
                TerrainMaterialInfo::FLATTENS
            } else {
                0
            },
            reserved: [0; 3],
        })
    }
}

/// The shape of an edit.
///
/// The name is enshwick's, not the game's.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Shape {
    /// A disc or a sphere.
    #[default]
    Round,
    /// A square or a cube.
    Square,
}

impl Shape {
    const fn raw(self) -> u32 {
        match self {
            Self::Round => TerrainEdit::ROUND,
            Self::Square => TerrainEdit::SQUARE,
        }
    }
}

/// The terrain of the server's world; get it from [`Load::terrain`](../struct.Load.html#method.terrain).
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Terrain {
    host: Host,
    dry: bool,
}

impl crate::script::Object for Terrain {
    const CLASS: &'static crate::script::Class = &crate::script::Class::of::<Self>(
        "Terrain",
        "The world's terrain, or a dry run of its edits that changes nothing.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_EDITS];
}

#[crate::script(edits)]
impl Terrain {
    /// The terrain materials of the running game; empty off the game thread
    /// or before the game has loaded them.
    ///
    /// # Examples
    ///
    /// See [`TerrainTerraformingConfig`].
    #[must_use]
    #[script]
    pub fn materials(&self) -> Vec<TerrainTerraformingConfig> {
        let blank = enshwick_abi::headed!(TerrainMaterialInfo {
            id: 0,
            flags: 0,
            hardness: 0,
            reserved: 0,
            item: ItemId::new(0),
            health: 0,
        });
        let mut infos = vec![blank; 128];
        loop {
            let mut count = 0u32;
            let capacity = u32::try_from(infos.len()).unwrap_or(u32::MAX);
            let status = (self.host.api().terrain_materials)(
                self.host.handle(),
                infos.as_mut_ptr(),
                u32::try_from(size_of::<TerrainMaterialInfo>()).unwrap_or(u32::MAX),
                capacity,
                &raw mut count,
            );
            if status != Status::OK {
                return Vec::new();
            }
            let count = count as usize;
            if count <= infos.len() {
                infos.truncate(count);
                break;
            }
            infos = vec![blank; count];
        }
        infos
            .iter()
            .map(|info| TerrainTerraformingConfig {
                terrain_material: info.id,
                terrain_item: info.item,
                hardness: info.hardness,
                health_points: info.health,
                can_be_flattened: info.flags & TerrainMaterialInfo::FLATTENS != 0,
            })
            .collect()
    }

    /// Adds a mound of `material` at `position`, in metres: `radius` across
    /// and `height` high at its middle, as a player's shovel does. Returns
    /// the volume added, in cubic metres.
    ///
    /// # Errors
    ///
    /// As for [`Terrain::dig`]; also [`WorldError::Refused`] for a material
    /// outside 1 to 127 or a height outside 0 to [`MAX_RADIUS`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::terrain::Shape;
    ///
    /// fn hill(terrain: &enshwick::terrain::Terrain, at: [f64; 3], dirt: u8) {
    ///     let _added = terrain.add(at, 6.0, 3.0, dirt, Shape::Round);
    /// }
    /// ```
    #[script]
    pub fn add(
        &self,
        position: [f64; 3],
        radius: f32,
        height: f32,
        material: u8,
        shape: Shape,
    ) -> Result<f32, WorldError> {
        self.edit(add_edit(position, radius, height, material, shape))
    }

    /// Digs out the terrain within `radius` of `position`, in metres, as a
    /// pickaxe would with no limit. A material in `protect` is left, and the
    /// dig stops at the first shell of half a metre that meets it, as a
    /// pickaxe stops at rock it cannot break. Returns the volume removed, in
    /// cubic metres.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a radius outside 0 to [`MAX_RADIUS`] or a
    /// position below 0 on any axis, [`WorldError::OutsideWorld`] for an
    /// edit reaching past the world's edge or above 1,024 metres,
    /// [`WorldError::NotLoaded`] while part of what it can reach is not
    /// loaded yet (it writes nothing, and starts loading it: try again in a
    /// second or so),
    /// [`WorldError::WrongThread`] off the game thread and
    /// [`WorldError::Unavailable`] while no world runs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn dig(
        &self,
        position: [f64; 3],
        radius: f32,
        shape: Shape,
        protect: &[u8],
    ) -> Result<f32, WorldError> {
        self.edit(dig_edit(position, radius, shape, protect))
    }

    /// Flattens the ground within `radius` of `position`, in metres, to
    /// the level of `position`: ground above goes, and gaps below are filled
    /// with `material`. Returns whether anything changed.
    ///
    /// # Errors
    ///
    /// As for [`Terrain::add`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::terrain::Shape;
    ///
    /// fn pad(terrain: &enshwick::terrain::Terrain, at: [f64; 3], stone: u8) -> bool {
    ///     terrain.flatten(at, 5.0, stone, Shape::Square).unwrap_or(false)
    /// }
    /// ```
    #[script]
    pub fn flatten(
        &self,
        position: [f64; 3],
        radius: f32,
        material: u8,
        shape: Shape,
    ) -> Result<bool, WorldError> {
        let changed = self.edit(flatten_edit(position, radius, material, shape))?;
        Ok(changed > 0.0)
    }

    /// The same terrain for edits that only count: each returns what it
    /// would change and changes nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::terrain::{Shape, Terrain};
    ///
    /// fn solid(terrain: &Terrain, at: [f64; 3]) -> bool {
    ///     terrain.dry().dig(at, 1.0, Shape::Round, &[]).is_ok_and(|volume| volume > 0.0)
    /// }
    /// ```
    #[must_use]
    #[script]
    pub const fn dry(self) -> Self {
        Self {
            host: self.host,
            dry: true,
        }
    }

    fn edit(&self, mut edit: TerrainEdit) -> Result<f32, WorldError> {
        if self.dry {
            edit.flags |= TerrainEdit::DRY_RUN;
        }
        let mut amount = 0.0f32;
        let status =
            (self.host.api().terrain_edit)(self.host.handle(), &raw const edit, &raw mut amount);
        WorldError::from_status(status).map(|()| amount)
    }
}

/// The edit with nothing set but where and how far.
fn request(position: [f64; 3], radius: f32, shape: Shape) -> TerrainEdit {
    enshwick_abi::headed!(TerrainEdit {
        kind: 0,
        shape: shape.raw(),
        flags: 0,
        material: 0,
        position,
        radius,
        height: 0.0,
        protect: [0; 16],
    })
}

/// The edit [`Terrain::add`] asks for.
pub(crate) fn add_edit(
    position: [f64; 3],
    radius: f32,
    height: f32,
    material: u8,
    shape: Shape,
) -> TerrainEdit {
    enshwick_abi::headed!(TerrainEdit {
        kind: TerrainEdit::ADD,
        material: u32::from(material),
        height,
        ..request(position, radius, shape)
    })
}

/// The edit [`Terrain::dig`] asks for.
pub(crate) fn dig_edit(
    position: [f64; 3],
    radius: f32,
    shape: Shape,
    protect: &[u8],
) -> TerrainEdit {
    let mut bits = [0u8; 16];
    for &id in protect {
        if let Some(byte) = bits.get_mut(usize::from(id / 8)) {
            *byte |= 1 << (id % 8);
        }
    }
    enshwick_abi::headed!(TerrainEdit {
        kind: TerrainEdit::DIG,
        protect: bits,
        ..request(position, radius, shape)
    })
}

/// The edit [`Terrain::flatten`] asks for.
pub(crate) fn flatten_edit(
    position: [f64; 3],
    radius: f32,
    material: u8,
    shape: Shape,
) -> TerrainEdit {
    enshwick_abi::headed!(TerrainEdit {
        kind: TerrainEdit::FLATTEN,
        material: u32::from(material),
        ..request(position, radius, shape)
    })
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::terrain`.
    #[doc(hidden)]
    #[must_use]
    fn terrain(&mut self) -> Terrain;

    /// As `Load::add_terrain_material`.
    #[cfg(feature = "unstable")]
    #[doc(hidden)]
    fn add_terrain_material(&mut self, new: NewTerrainMaterial) -> Result<u8, Off>;
}

impl crate::script::Bound for Terrain {
    const NAME: &'static str = "terrain";
    const DOC: &'static str =
        "The world's terrain: add, dig and flatten ground, and its materials.";
    // One list with the object class the type has.
    const PARTS: &'static [&'static [crate::script::Function<Self>]] =
        <Self as crate::script::Object>::PARTS;

    fn resolve(load: &mut impl crate::Loading) -> Result<Self, Off> {
        Ok(load.base_mut().terrain())
    }
}

impl LoadMethods for crate::BaseLoad {
    fn terrain(&mut self) -> Terrain {
        Terrain {
            host: self.host(),
            dry: false,
        }
    }

    #[cfg(feature = "unstable")]
    fn add_terrain_material(&mut self, new: NewTerrainMaterial) -> Result<u8, Off> {
        let host = self.host();
        let guid = crate::resources::declare(host, &new.id.to_string())?;
        let request = new.request(guid);
        let mut id = 0u8;
        let status =
            (host.api().terrain_material_add)(host.handle(), &raw const request, &raw mut id);
        crate::load::ok(status, id, || format!("terrain material {}", new.id))
    }
}
