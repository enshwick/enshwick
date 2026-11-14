//! Host functions for transactions of the world: commits, statuses and undos.

use enshwick_core::abi::{
    BlockPlace, PropPlace, PropRemove, Status, TerrainEdit, TransactionInfo, TransactionOp,
    VoxelRegion, VoxelWrite,
};

use crate::host::layer::{HeadedOut, ListOut, Mod, Out, UnalignedList, Values, entries};
use crate::world::transactions::{Change, Erase, Paste};

entries! {
    fn transaction_commit(me as Mod, [ops, count] as UnalignedList<TransactionOp>, id as Out<u64>) -> Status {
        id.put(commit(me, None, ops)?)
    }

    fn transaction_commit_for(
        me as Mod,
        actor: enshwick_core::Entity,
        [ops, count] as UnalignedList<TransactionOp>,
        id as Out<u64>,
    ) -> Status {
        if actor == enshwick_core::Entity(0) {
            return Err(Status::BAD_ARGS.into());
        }
        id.put(commit(me, Some(actor.0), ops)?)
    }

    fn transaction_entities(
        me as Mod,
        id: u64,
        [out, capacity, count] as ListOut<enshwick_core::Entity>,
    ) -> Status {
        let created = crate::world::transactions::entities(me.index, id).ok_or(Status::MISSING)?;
        let created: Vec<enshwick_core::Entity> =
            created.into_iter().map(enshwick_core::Entity).collect();
        out.put(&created)
    }

    fn transaction_status(me as Mod, id: u64, out as HeadedOut<TransactionInfo>) -> Status {
        out.put(crate::world::transactions::info(me.index, id).ok_or(Status::MISSING)?)
    }

    fn transaction_undo(me as Mod, id: u64) -> Status {
        crate::world::transactions::undo(me.index, id)?;
        Ok(())
    }
}

/// Commits `ops` as the calling mod, for `actor` when it names one, and
/// gives the transaction's id.
fn commit(me: Mod, actor: Option<u32>, ops: Values<'_, TransactionOp>) -> Result<u64, Status> {
    if ops.len() == 0 || ops.len() > crate::world::transactions::LIMITS.ops {
        return Err(Status::BAD_ARGS);
    }
    let changes = ops
        .iter()
        .map(|op| {
            if op.reserved != 0 {
                return Err(Status::BAD_ARGS);
            }
            // SAFETY: `HostApi::transaction_commit` has the mod pass each
            // op's `data` null or pointing at the record its kind names, with
            // its header, and a `VoxelWrite`'s voxels at `voxels`, all
            // borrowed for the call.
            unsafe { change(me.loader, &op) }
        })
        .collect::<Result<Vec<_>, _>>()?;
    crate::world::transactions::commit(me.loader, me.index, actor, changes)
}

/// The change `op` hands over; [`Status::BAD_ARGS`] for a kind or a
/// record the loader refuses.
///
/// # Safety
///
/// - `op.data` is null or points at the record `op.kind` names, readable for
///   the size its header states, borrowed for the call.
/// - A [`VoxelWrite`]'s `voxels` is null or points at `count` voxels,
///   borrowed for the call.
unsafe fn change(
    loader: &crate::engine::loader::Loader,
    op: &TransactionOp,
) -> Result<Change, Status> {
    match op.kind {
        TransactionOp::TERRAIN => {
            // SAFETY: the caller guarantees `op.data` for a terrain op.
            let edit = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<TerrainEdit>(),
                    <TerrainEdit as enshwick_core::abi::Headed>::BORN,
                )
            };
            edit.map(Change::Terrain).ok_or(Status::BAD_ARGS)
        }
        TransactionOp::VOXELS => {
            // SAFETY: the caller guarantees `op.data` for a voxels op.
            let write = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<VoxelWrite>(),
                    <VoxelWrite as enshwick_core::abi::Headed>::BORN,
                )
            }
            .ok_or(Status::BAD_ARGS)?;
            let most = crate::world::transactions::LIMITS.bricks as u64 * 512;
            if write.voxels.is_null() || write.count > most {
                return Err(Status::BAD_ARGS);
            }
            let count = usize::try_from(write.count).map_err(|_| Status::BAD_ARGS)?;
            // SAFETY: `write.voxels` is non-null, checked above, and the
            // caller guarantees `count` voxels at it for the call; `Voxel`
            // has alignment 1.
            let voxels = unsafe { core::slice::from_raw_parts(write.voxels, count) };
            let words = voxels
                .iter()
                .map(|voxel| crate::world::voxels::encode(*voxel))
                .collect::<Result<Vec<_>, _>>()?;
            Paste::new(write.min, write.size, words).map(Change::Voxels)
        }
        TransactionOp::ERASE => {
            // SAFETY: the caller guarantees `op.data` for an erase op.
            let region = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<VoxelRegion>(),
                    <VoxelRegion as enshwick_core::abi::Headed>::BORN,
                )
            }
            .ok_or(Status::BAD_ARGS)?;
            Erase::new(region.min, region.size).map(Change::Erase)
        }
        #[cfg(windows)]
        TransactionOp::BLOCK => {
            // SAFETY: the caller guarantees `op.data` for a block op.
            let block = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<BlockPlace>(),
                    <BlockPlace as enshwick_core::abi::Headed>::BORN,
                )
            }
            .ok_or(Status::BAD_ARGS)?;
            crate::world::transactions::resolve(loader, &block)
                .map(|block| Change::Block(Box::new(block)))
        }
        #[cfg(windows)]
        TransactionOp::PLACE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a prop op.
            let prop = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<PropPlace>(),
                    <PropPlace as enshwick_core::abi::Headed>::BORN,
                )
            }
            .ok_or(Status::BAD_ARGS)?;
            crate::world::transactions::resolve_prop(loader, &prop).map(Change::PlaceProp)
        }
        TransactionOp::REMOVE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a removal op.
            let remove = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<PropRemove>(),
                    <PropRemove as enshwick_core::abi::Headed>::BORN,
                )
            }
            .ok_or(Status::BAD_ARGS)?;
            if remove.entity == enshwick_core::Entity(0)
                || remove.flags & !PropRemove::DISCARD_STATE != 0
            {
                return Err(Status::BAD_ARGS);
            }
            Ok(Change::RemoveProp {
                entity: remove.entity.0,
                discard_state: remove.flags & PropRemove::DISCARD_STATE != 0,
            })
        }
        _ => Err(Status::BAD_ARGS),
    }
}
