//! Every write of the world's voxels, as [`Load::on_voxels_changed`] hears
//! it.

use super::{Voxel, VoxelPos};
use crate::{Load, Off, abi};

/// Who made a [`VoxelEdit`].
///
/// # Examples
///
/// See [`Load::on_voxels_changed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum EditSource {
    /// The game: a player's tools and buildings, or the game's own work;
    /// also the data packs the loader runs, which are no mod of their own.
    Game,
    /// This mod, through the API: its terrain edits, its transactions and
    /// their undos, such as a replay of edits it stored.
    ThisMod,
    /// Another mod, through the API.
    OtherMod,
}

/// One voxel a [`VoxelEdit`] changed: where, and what it was and is.
///
/// # Examples
///
/// See [`Load::on_voxels_changed`].
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, crate::Script,
)]
#[non_exhaustive]
pub struct VoxelChange {
    /// Its place.
    pub at: VoxelPos,
    /// The voxel before the edit.
    pub before: Voxel,
    /// The voxel after it.
    pub after: Voxel,
}

impl VoxelChange {
    /// The change of the voxel at `at` from `before` to `after`, such as
    /// one a test hands the fake host.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::{Voxel, VoxelChange, VoxelMaterial, VoxelPos};
    ///
    /// let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
    /// let dug = VoxelChange::new(VoxelPos::new(10, 20, 30), rock, Voxel::default());
    /// assert!(dug.after.is_air());
    /// ```
    #[must_use]
    pub const fn new(at: VoxelPos, before: Voxel, after: Voxel) -> Self {
        Self { at, before, after }
    }
}

/// One write of the world's voxels and the voxels it changed, from
/// [`Load::on_voxels_changed`]. The name is enshwick's: the game names no
/// such write.
///
/// # Examples
///
/// See [`Load::on_voxels_changed`].
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct VoxelEdit {
    /// Who made it.
    pub source: EditSource,
    /// The lowest corner of the box the game wrote, aligned to its bricks
    /// of 8 voxels: a [`Voxels::read`](super::Voxels::read) of `min` and
    /// `size` reads the whole box again.
    pub min: VoxelPos,
    /// The box's voxels along each axis.
    pub size: [u32; 3],
    /// The voxels that changed, at least one, x fastest, then y, then z.
    pub changes: Vec<VoxelChange>,
}

impl VoxelEdit {
    /// The edit the loader hands over to the mod of handle `me`.
    ///
    /// # Safety
    ///
    /// `data.changes` points at `data.count` changes readable for the call,
    /// or `data.count` is 0.
    unsafe fn of(data: &abi::VoxelEditData, me: abi::ModHandle) -> Self {
        let changes = match usize::try_from(data.count) {
            Ok(count) if count > 0 && !data.changes.is_null() => {
                // SAFETY: the caller guarantees `count` changes readable at
                // the non-null `changes` for the call.
                unsafe { core::slice::from_raw_parts(data.changes, count) }
            }
            _ => &[],
        };
        Self {
            source: match data.by {
                abi::ModHandle::NONE => EditSource::Game,
                by if by == me => EditSource::ThisMod,
                _ => EditSource::OtherMod,
            },
            min: VoxelPos::from(data.min),
            size: data.size,
            changes: changes
                .iter()
                .map(|change| VoxelChange {
                    at: VoxelPos::from(change.at),
                    before: Voxel::from_abi(change.before),
                    after: Voxel::from_abi(change.after),
                })
                .collect(),
        }
    }
}

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
    /// [`Voxels::read`](super::Voxels::read), which loads it, until it
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
        F: Fn(&VoxelEdit) + Send + Sync + 'static,
    {
        let me = self.host().handle();
        self.on_raw_event(abi::Event::VOXELS_CHANGED, move |raw| {
            // SAFETY: subscribed under `Event::VOXELS_CHANGED`, whose data
            // the loader passes as a `VoxelEditData`, null or valid for the
            // call.
            let data = unsafe {
                abi::read_headed(
                    raw.cast::<abi::VoxelEditData>(),
                    size_of::<abi::VoxelEditData>(),
                )
            };
            if let Some(data) = data {
                // SAFETY: the loader's `changes` point at `count` changes
                // readable for this call, by `VoxelEditData`'s contract.
                handler(&unsafe { VoxelEdit::of(&data, me) });
            }
        })
    }
}
