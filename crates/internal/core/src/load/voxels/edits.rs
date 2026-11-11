//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off, abi};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(voxel_edits)]
impl Load<'_> {
    /// Calls `handler` for every write of the server's world voxels, on the
    /// game thread after the step: a player's digging, building and
    /// shaping, the game's own changes, and the terrain edits and
    /// transactions of mods, this one's included. An undo or a transaction
    /// writes brick by brick of 8 voxels, one edit each. On a client it
    /// runs while it hosts a game. A panic in `handler` turns the mod off.
    ///
    /// To keep edits, store the changes and replay them when the world
    /// loads: read a box over them with
    /// [`Voxels::read`](crate::voxels::Voxels::read), which loads it, until it
    /// answers, then write it back with a transaction's `write_voxels`, and
    /// skip the edits whose source is [`EditSource::ThisMod`]. A world's
    /// save keeps edits only inside a base, a Flame Altar's building zone.
    ///
    /// On a game build without the editor's write the handler never runs,
    /// and the log and the Mods screen say why. A write the loader cannot
    /// read, or one past 262,144 changed voxels waiting in a step, is not
    /// raised; the log counts it.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::{EditSource, VoxelMaterial};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_voxels_changed(|edit| {
    ///         if edit.source == EditSource::ThisMod {
    ///             return;
    ///         }
    ///         let dug = edit
    ///             .changes
    ///             .iter()
    ///             .filter(|c| matches!(c.after.material(), VoxelMaterial::Air))
    ///             .count();
    ///         enshwick::debug!("{dug} voxels dug out");
    ///     })
    /// }
    /// ```
    #[script]
    pub fn on_voxels_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&VoxelEdit) + Send + Sync + 'static;
}
