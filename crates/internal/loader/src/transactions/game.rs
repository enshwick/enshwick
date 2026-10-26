//! The game's side of transactions: brick reads, terrain edits and undo writes.

use enshwick::abi::{Status, TerrainEdit};

use super::cover::{BRICK, BRICK_VOXELS, Brick};
use super::ledger::{Image, Voxels};
use super::record::UndoRecord;
use crate::host::Loader;
use crate::pinned;
use crate::voxels::ReadBox;

/// The editor's undo write: puts the voxels of `record`'s box back through
/// the same write as an edit, which marks them for clients and the save;
/// it reads only the record's head, never frees its voxels, and always
/// answers 1 (server 399, client 426).
type UndoApplyFn =
    unsafe extern "system" fn(editor: *mut u8, record: *const UndoRecord, alloc: *mut u8) -> u8;

/// Whether this build has every piece a transaction writes with, and a
/// server runs.
pub(crate) fn ready(loader: &Loader) -> bool {
    crate::world::hosting()
        && [
            pinned::voxel::UNDO_APPLY,
            pinned::voxel::READ_BOX,
            pinned::terrain::ADD,
            pinned::terrain::DIG,
            pinned::terrain::FLATTEN,
        ]
        .into_iter()
        .all(|symbol| loader.address(symbol).is_some())
}

/// The server's voxel editor for one step, on the game thread at a quiet
/// moment.
pub(crate) struct Game<'a> {
    loader: &'a Loader,
    editor: *mut u8,
    alloc: *mut u8,
    undo_apply: UndoApplyFn,
}

impl<'a> Game<'a> {
    /// The editor, when the caller is on the game thread at a quiet moment
    /// and the build has the undo write.
    pub(crate) fn new(loader: &'a Loader) -> Result<Self, Status> {
        let function = loader
            .address(pinned::voxel::UNDO_APPLY)
            .and_then(|address| usize::try_from(address).ok())
            .ok_or(Status::MISSING)?;
        let (editor, alloc) = crate::terrain::editor_and_allocator(loader)?;
        Ok(Self {
            loader,
            editor: core::ptr::with_exposed_provenance_mut(editor),
            alloc: core::ptr::with_exposed_provenance_mut(alloc),
            // SAFETY: the table's `voxel.undo_apply` is the address of the
            // game function with the `UndoApplyFn` signature on this build,
            // in the image, which stays mapped.
            undo_apply: unsafe { core::mem::transmute::<usize, UndoApplyFn>(function) },
        })
    }
}

impl Voxels for Game<'_> {
    fn loaded(&mut self, brick: Brick) -> Result<(), Status> {
        let corner = ReadBox::new(brick.min(), [1; 3], 1)?;
        crate::voxels::read(self.loader, &corner).map(drop)
    }

    fn read(&mut self, brick: Brick) -> Result<Image, Status> {
        let whole = ReadBox::new(brick.min(), [BRICK; 3], BRICK_VOXELS as u64)?;
        let words = crate::voxels::read(self.loader, &whole)?;
        words
            .into_boxed_slice()
            .try_into()
            .map_err(|_| Status::MISSING)
    }

    fn edit(&mut self, edit: &TerrainEdit) -> Result<(), Status> {
        crate::terrain::edit(self.loader, edit).map(drop)
    }

    fn restore(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status> {
        let record = UndoRecord::brick(brick, image);
        // SAFETY: `undo_apply` is the table's `voxel.undo_apply`, called on
        // the game thread at a quiet moment with the server's live, non-zero
        // voxel editor and allocator (both checked by
        // `crate::terrain::editor_and_allocator` when this step began).
        // `record` is a head the write reads whole: an 8-aligned brick box
        // inside the coordinates the read takes, and `image`'s 512 voxels,
        // borrowed past the call and only read.
        unsafe { (self.undo_apply)(self.editor, &raw const record, self.alloc) };
        Ok(())
    }
}
