//! Writes of the world's voxels, as the loader raises them after a step.

use crate::abi::{self, Event, Header, ModHandle};
use crate::voxels::{EditSource, VoxelChange};

use super::Installed;
use super::world::VoxelGrid;

impl Installed {
    /// Raises one write of the world's voxels as the loader does after the
    /// step: [`Event::VOXELS_CHANGED`] with `changes`, made by `source`,
    /// [`EditSource::Game`] for a player's tool or building. A write that
    /// changes nothing raises none, nor do changes whose voxel stays the
    /// same. The edit's box is the one of the game's bricks of 8 voxels
    /// around `changes`. Returns how many handlers ran. The fake also
    /// raises the voxels each transaction writes, and its undo puts back,
    /// as its mod's edits brick by brick when
    /// [`Installed::run_transactions`] runs it; its terrain edits raise
    /// none, so a test raises those here.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::voxels::EditSource;
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.voxels_changed(EditSource::Game, &[]), 0);
    /// ```
    #[must_use = "the count says whether the mod handles the event"]
    pub fn voxels_changed(&self, source: EditSource, changes: &[VoxelChange]) -> usize {
        let changes: Vec<abi::VoxelChange> = changes
            .iter()
            .filter(|change| change.before != change.after)
            .map(|change| abi::VoxelChange {
                at: change.at.to_array(),
                before: change.before.to_abi(),
                after: change.after.to_abi(),
            })
            .collect();
        let me = self.handle();
        let by = match source {
            EditSource::Game => ModHandle::NONE,
            EditSource::ThisMod => me,
            EditSource::OtherMod => ModHandle(me.0.wrapping_add(1).max(1)),
        };
        self.raise_edit(by, &changes)
    }

    /// Raises `changes`, one write by the mod of `by` over the bricks
    /// around them; none when empty.
    pub(crate) fn raise_edit(&self, by: ModHandle, changes: &[abi::VoxelChange]) -> usize {
        if changes.is_empty() {
            return 0;
        }
        let min = [0, 1, 2]
            .map(|axis| changes.iter().map(|c| c.at[axis]).min().unwrap_or(0) / BRICK * BRICK);
        let end = [0, 1, 2].map(|axis| {
            let last = changes.iter().map(|c| c.at[axis]).max().unwrap_or(0);
            (last / BRICK).saturating_add(1).saturating_mul(BRICK)
        });
        let edit = abi::VoxelEditData {
            header: Header::of::<abi::VoxelEditData>(),
            by,
            min,
            size: [0, 1, 2].map(|axis| end[axis] - min[axis]),
            changes: changes.as_ptr(),
            count: changes.len() as u64,
        };
        // SAFETY: `edit` is the data of `VOXELS_CHANGED`, its `changes`
        // pointing at `count` changes, all living for the call.
        unsafe { self.raise_raw(Event::VOXELS_CHANGED, (&raw const edit).cast()) }
    }

    /// Raises `changes` brick by brick, one write each, as the loader
    /// raises a transaction's write and its undo.
    pub(crate) fn raise_bricks(&self, by: ModHandle, changes: &[abi::VoxelChange]) -> usize {
        let mut bricks: std::collections::BTreeMap<[u32; 3], Vec<abi::VoxelChange>> =
            std::collections::BTreeMap::new();
        for change in changes {
            let [x, y, z] = change.at.map(|axis| axis / BRICK);
            bricks.entry([z, y, x]).or_default().push(*change);
        }
        bricks
            .values()
            .map(|brick| self.raise_edit(by, brick))
            .sum()
    }
}

/// The side of the game's voxel bricks, which its writes are aligned to.
const BRICK: u32 = 8;

/// The voxels a write of `voxels` over the box of `size` from `min`
/// changes against the fake's loaded `grids`, where a voxel no grid holds
/// was air.
pub(crate) fn written(
    grids: &[VoxelGrid],
    min: [u32; 3],
    size: [u32; 3],
    voxels: &[abi::Voxel],
) -> Vec<abi::VoxelChange> {
    let [width, height, _] = size.map(|axis| axis as usize);
    voxels
        .iter()
        .enumerate()
        .filter_map(|(index, &after)| {
            let offset = [
                index % width,
                index / width % height,
                index / (width * height),
            ];
            let at = [0, 1, 2].map(|axis| min[axis] + u32::try_from(offset[axis]).unwrap_or(0));
            let before = grids
                .iter()
                .rev()
                .find_map(|grid| grid.get(at))
                .unwrap_or_default();
            (before != after).then_some(abi::VoxelChange { at, before, after })
        })
        .collect()
}
