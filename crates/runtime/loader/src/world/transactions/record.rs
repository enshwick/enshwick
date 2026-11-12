//! The voxel undo record the game's editor writes back from, owned by the loader.

use super::cover::{BRICK, BRICK_VOXELS, Brick};

/// The head of the game's voxel undo record, as the editor's undo write
/// reads it: the box, then its earlier voxels, x fastest, then y, then z,
/// at full detail. The write reads only these 0x28 bytes, never frees the
/// voxels and writes exactly them (server 399, client 426). The game's own
/// records go on from `+0x28`; the loader keeps them zeroed.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct UndoRecord {
    /// The box's first voxel on each axis.
    min: [u32; 3],
    /// The box's voxels along each axis.
    size: [u32; 3],
    /// The box's earlier voxels.
    voxels: *const u16,
    /// The voxels at `voxels`.
    count: u64,
    /// The rest of the game's record, which the write does not read.
    rest: [u8; REST],
}

/// The bytes of the game's record past its head, which the loader keeps
/// zeroed: a material feedback id, an event box, flags and its type.
const REST: usize = 0x28; // bound, not a game offset: zeroed room past the head the undo write reads

const _: () = {
    assert!(core::mem::offset_of!(UndoRecord, min) == 0);
    assert!(core::mem::offset_of!(UndoRecord, size) == 0xc);
    assert!(core::mem::offset_of!(UndoRecord, voxels) == 0x18);
    assert!(core::mem::offset_of!(UndoRecord, count) == 0x20);
    assert!(size_of::<UndoRecord>() == 0x50);
};

impl UndoRecord {
    /// Whether the undo write reads only the record's head, as the build
    /// table's `voxel.undo_extent` gives how far it reads.
    pub(crate) fn head_holds(extent: Option<usize>) -> bool {
        extent == Some(core::mem::offset_of!(Self, rest))
    }

    /// The record that puts `brick` back to `voxels`, which must outlive
    /// every use of the record.
    pub(crate) fn brick(brick: Brick, voxels: &[u16; BRICK_VOXELS]) -> Self {
        Self {
            min: brick.min(),
            size: [BRICK; 3],
            voxels: voxels.as_ptr(),
            count: BRICK_VOXELS as u64,
            rest: [0; REST],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_undo_write_must_read_exactly_the_head() {
        assert!(UndoRecord::head_holds(Some(0x28)));
        assert!(!UndoRecord::head_holds(Some(0x30)), "reads past the head");
        assert!(
            !UndoRecord::head_holds(Some(0x20)),
            "a head of another shape"
        );
        assert!(!UndoRecord::head_holds(None), "not in the table");
    }

    #[test]
    fn a_brick_record_names_its_box_and_all_its_voxels() {
        let voxels = [7u16; BRICK_VOXELS];
        let record = UndoRecord::brick(Brick([1, 2, 3]), &voxels);
        assert_eq!(record.min, [8, 16, 24]);
        assert_eq!(record.size, [8, 8, 8]);
        assert_eq!(record.voxels, voxels.as_ptr());
        assert_eq!(record.count, 512);
        assert!(record.rest.iter().all(|&byte| byte == 0));
    }
}
