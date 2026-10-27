//! The game's side of transactions: brick reads, terrain edits, blocks, rights and undo writes.

mod blocks;
mod props;

use enshwick::abi::{BuildRights, Header, Status, TerrainEdit};

use super::block::Block;
use super::cover::{BRICK, BRICK_VOXELS, Brick};
use super::ledger::{Image, Voxels};
use super::paste::Erase;
use super::props::Prop;
use super::record::UndoRecord;
use crate::host::Loader;
use crate::pinned;
use crate::voxels::ReadBox;
pub(crate) use blocks::resolve;
pub(crate) use props::resolve as resolve_prop;

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
    write_block: Option<blocks::WriteBlockFn>,
    events: blocks::Events,
    /// The materials a block may write over, read at the step's first block.
    replace: Option<[u8; super::block::MATERIALS]>,
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
            write_block: loader
                .address(pinned::voxel::WRITE_BLOCK)
                .and_then(|address| usize::try_from(address).ok())
                // SAFETY: the table's `voxel.write_block` is the address of
                // the game function with the `WriteBlockFn` signature on this
                // build, in the image, which stays mapped.
                .map(|address| unsafe {
                    core::mem::transmute::<usize, blocks::WriteBlockFn>(address)
                }),
            events: blocks::Events::new(loader),
            replace: None,
        })
    }
}

impl Voxels for Game<'_> {
    fn rights(&mut self, actor: u32, min: [f64; 3], max: [f64; 3]) -> Result<u32, Status> {
        let check = BuildRights {
            header: Header::of::<BuildRights>(),
            player: enshwick::Entity(actor),
            refusal: BuildRights::ALLOWED,
            min,
            max,
        };
        crate::rights::check(self.loader, &check)
    }

    fn place(&mut self, block: &Block, actor: u32) -> Result<(), Status> {
        let write = self.write_block.ok_or(Status::MISSING)?;
        let replace = match self.replace {
            Some(replace) => replace,
            None => *self.replace.insert(blocks::replaceable(self.loader)?),
        };
        let game = blocks::rebuild(self.loader, block)?;
        // SAFETY: `write` is the table's `voxel.write_block`; `editor` and
        // `alloc` are the server's live voxel editor and allocator, checked
        // by `crate::terrain::editor_and_allocator` when this step began on
        // the game thread at a quiet moment; `game` comes from `rebuild` in
        // this step.
        unsafe { blocks::write(write, self.editor, self.alloc, &replace, block, &game) };
        self.events.placed(self.loader, block, actor);
        Ok(())
    }

    fn spawn_prop(&mut self, prop: &Prop, actor: u32) -> Result<u32, Status> {
        let entity = props::spawn(self.loader, prop, actor)?;
        self.events.prop_placed(self.loader, prop, entity, actor);
        Ok(entity)
    }

    fn read_prop(&mut self, entity: u32) -> Result<(Prop, u32), Status> {
        props::read(self.loader, entity)
    }

    fn start_state(&mut self, entity: u32) -> Result<Option<Vec<u8>>, Status> {
        props::start_state(self.loader, entity)
    }

    fn hold_prop(&mut self, entity: u32) -> Result<(), Status> {
        props::hold(self.loader, entity)
    }

    fn still_held(&mut self, entity: u32) -> Result<(), Status> {
        if super::holds().still_held(entity, crate::interactions::filter_off()) {
            Ok(())
        } else {
            Err(Status::MISSING)
        }
    }

    fn release_prop(&mut self, entity: u32) {
        props::release(self.loader, entity);
    }

    fn prop_at(&mut self, prop: &Prop) -> Result<bool, Status> {
        props::stands(self.loader, prop)
    }

    fn remove_prop(&mut self, entity: u32, actor: u32) -> Result<(), Status> {
        let (prop, _) = props::read(self.loader, entity)?;
        props::remove(self.loader, entity)?;
        self.events.prop_removed(self.loader, &prop, actor);
        Ok(())
    }

    fn erased(&mut self, erase: &Erase, actor: u32) {
        self.events.torn_down(self.loader, erase.region(), actor);
    }

    fn extent(&mut self) -> Result<crate::voxels::WorldExtent, Status> {
        crate::voxels::extent(self.loader)
    }

    fn loaded(&mut self, brick: Brick) -> Result<(), Status> {
        crate::voxels::column_loaded(self.loader, brick.min())
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

    fn write(&mut self, brick: Brick, image: &[u16; BRICK_VOXELS]) -> Result<(), Status> {
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

/// Puts back the Quick Stash `deposits` of props no longer held,
/// by entity. On the game thread.
pub(crate) fn restore_deposits(deposits: &[(u32, crate::prop_holds::Stash)]) {
    let Some(loader) = crate::host::loader() else {
        return;
    };
    for &(entity, before) in deposits {
        props::set_deposit(loader, entity, before);
    }
}

/// Around one call of the game's save: with `saving`, puts back the Quick
/// Stash of the magic chests held, so a save keeps it as players left it;
/// after it, turns it off again. Plain boxes take no
/// Quick Stash and are never written. The game's `server.save_step`
/// runs on the game thread after the step's systems and before the
/// post-step, with none running (server 399, client 426), a quiet moment
/// `props::set_deposit` checks before every write.
pub(crate) fn saving(saving: bool) {
    let Some(loader) = crate::host::loader() else {
        return;
    };
    for (entity, before) in super::holds().deposits() {
        let value = if saving { before } else { props::STASH_OFF };
        props::set_deposit(loader, entity, value);
    }
}
