//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::terrain::Terrain;
#[cfg(feature = "unstable")]
use crate::{Off, terrain::NewTerrainMaterial};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Terrain`] of the server's world; on a client, the built-in
    /// server's while it hosts a game.
    ///
    /// # Examples
    ///
    /// See the [module](crate::terrain).
    #[must_use]
    pub fn terrain(&mut self) -> Terrain;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(terrain_materials)]
impl Load<'_> {
    /// Adds the terrain material `new` to the game and returns its voxel
    /// id for this run, to place with [`Terrain::add`]; the material's
    /// rules, effects and look are served on both sides, and
    /// [`Terrain::materials`] lists it once the world runs.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, on a build whose archive lacks the terrain
    /// palette, for a `like` or `fallback` that is none of the game's terrain
    /// materials, a health below 1, a content id taken, and once mods
    /// loaded before took the game's 11 free terrain ids.
    ///
    /// # Examples
    ///
    /// See [`NewTerrainMaterial`].
    #[script]
    pub fn add_terrain_material(&mut self, new: NewTerrainMaterial) -> Result<u8, Off>;
}
