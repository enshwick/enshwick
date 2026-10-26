//! Terrain in the server's world: the terrain materials, and adding,
//! digging and flattening ground as the game's terraforming does.
//!
//! The ground is made of voxels half a metre a side, each of a material:
//! terrain materials have ids 1 to 127 ([`Terrain::materials`] lists them
//! with the item digging each yields). An edit changes the ground at once:
//! clients see it, and the save keeps it. Built pieces are never touched.
//! A player's shovel and pickaxe also check where the player may build,
//! take the cost and give the loot and experience; an edit here does none
//! of that, so a mod decides what it needs.
//!
//! Every call belongs on the server's game thread, from a game event, a
//! [`Scheduler`](crate::schedule::Scheduler) task or a command; on a client,
//! while it hosts a game.
//!
//! # Examples
//!
//! ```
//! use enshwick::terrain::Shape;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let terrain = load.terrain();
//!     let world = load.world()?;
//!     load.add_command("crater", "digs a crater where you stand", move |ctx, _args| {
//!         let Some(me) = ctx.character() else {
//!             return "the console has no character".into();
//!         };
//!         let Some(here) = world.position(me) else {
//!             return "nowhere".into();
//!         };
//!         match terrain.dig(here, 4.0, Shape::Round, &[]) {
//!             Ok(volume) => format!("dug {volume:.1} m³").into(),
//!             Err(why) => format!("{why}").into(),
//!         }
//!     })
//! }
//! ```

use crate::abi::{Header, Status, TerrainEdit, TerrainMaterialInfo};
use crate::world::WorldError;
use crate::{Host, Load};

/// The largest radius and height of an edit, in metres.
pub const MAX_RADIUS: f32 = 16.0;

/// A terrain material of the running game, from [`Terrain::materials`].
///
/// # Examples
///
/// ```
/// fn dirt(terrain: &enshwick::terrain::Terrain, dirt_item: u32) -> Option<u8> {
///     terrain.materials().iter().find(|m| m.item == dirt_item).map(|m| m.id)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Material {
    /// Its voxel id, 1 to 127.
    pub id: u8,
    /// The item digging it yields, as an item id.
    pub item: u32,
    /// How hard it is to dig: the game's hardness category from 0.
    pub hardness: u8,
    /// The health a voxel of it has against tools.
    pub health: i32,
    /// Whether a player's tool can flatten it.
    pub flattens: bool,
}

/// The shape of an edit.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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

/// The terrain of the server's world; get it from [`Load::terrain`].
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug)]
pub struct Terrain {
    host: Host,
    dry: bool,
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "world positions in metres fit an f32 as the game's edits take them"
)]
fn metres(position: [f64; 3]) -> [f32; 3] {
    position.map(|axis| axis as f32)
}

impl Terrain {
    /// The terrain materials of the running game; empty off the game thread
    /// or before the game has loaded them.
    ///
    /// # Examples
    ///
    /// See [`Material`].
    #[must_use]
    pub fn materials(&self) -> Vec<Material> {
        let blank = TerrainMaterialInfo {
            header: Header::of::<TerrainMaterialInfo>(),
            id: 0,
            flags: 0,
            hardness: 0,
            reserved: 0,
            item: 0,
            health: 0,
        };
        let mut infos = vec![blank; 128];
        loop {
            let mut count = 0u32;
            let capacity = u32::try_from(infos.len()).unwrap_or(u32::MAX);
            let status = (self.host.api().terrain_materials)(
                self.host.handle(),
                infos.as_mut_ptr(),
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
            .map(|info| Material {
                id: info.id,
                item: info.item,
                hardness: info.hardness,
                health: info.health,
                flattens: info.flags & TerrainMaterialInfo::FLATTENS != 0,
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
    pub fn add(
        &self,
        position: [f64; 3],
        radius: f32,
        height: f32,
        material: u8,
        shape: Shape,
    ) -> Result<f32, WorldError> {
        self.edit(TerrainEdit {
            kind: TerrainEdit::ADD,
            shape: shape.raw(),
            material: u32::from(material),
            height,
            ..self.request(position, radius)
        })
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
    /// position below 0 on any axis, [`WorldError::WrongThread`] off the
    /// game thread and [`WorldError::Missing`] while no world runs.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn dig(
        &self,
        position: [f64; 3],
        radius: f32,
        shape: Shape,
        protect: &[u8],
    ) -> Result<f32, WorldError> {
        let mut bits = [0u8; 16];
        for &id in protect {
            if let Some(byte) = bits.get_mut(usize::from(id / 8)) {
                *byte |= 1 << (id % 8);
            }
        }
        self.edit(TerrainEdit {
            kind: TerrainEdit::DIG,
            shape: shape.raw(),
            protect: bits,
            ..self.request(position, radius)
        })
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
    pub fn flatten(
        &self,
        position: [f64; 3],
        radius: f32,
        material: u8,
        shape: Shape,
    ) -> Result<bool, WorldError> {
        let changed = self.edit(TerrainEdit {
            kind: TerrainEdit::FLATTEN,
            shape: shape.raw(),
            material: u32::from(material),
            ..self.request(position, radius)
        })?;
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
    pub const fn dry(self) -> Self {
        Self {
            host: self.host,
            dry: true,
        }
    }

    fn request(&self, position: [f64; 3], radius: f32) -> TerrainEdit {
        TerrainEdit {
            header: Header::of::<TerrainEdit>(),
            kind: 0,
            shape: 0,
            flags: if self.dry { TerrainEdit::DRY_RUN } else { 0 },
            material: 0,
            position: metres(position),
            radius,
            height: 0.0,
            protect: [0; 16],
        }
    }

    fn edit(&self, edit: TerrainEdit) -> Result<f32, WorldError> {
        let mut amount = 0.0f32;
        let status =
            (self.host.api().terrain_edit)(self.host.handle(), &raw const edit, &raw mut amount);
        WorldError::from_status(status).map(|()| amount)
    }
}

impl Load<'_> {
    /// The [`Terrain`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn terrain(&mut self) -> Terrain {
        Terrain {
            host: self.host(),
            dry: false,
        }
    }
}
