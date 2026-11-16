//! Host functions for transactions of the world: commits, statuses and undos.

use enshwick_core::abi::{
    BlockPlace, PropPlace, PropRemove, Status, TerrainEdit, TransactionInfo, TransactionOp,
    TransactionSettledFn, VoxelRegion, VoxelWrite,
};

use crate::host::layer::{
    Callback, HeadedOut, ListOut, Mod, Out, Refusal, Refused, UnalignedList, Values, entries,
};
use crate::world::transactions::{Change, Erase, Paste, Standing};

entries! {
    fn transaction_commit(
        me as Mod,
        actor: enshwick_core::Entity,
        [ops, count] as UnalignedList<TransactionOp>,
        id as Out<u64>,
    ) -> Status {
        let actor = (actor != enshwick_core::Entity(0)).then_some(actor.0);
        id.put(commit(me, actor, ops)?)
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
        let standing = crate::world::transactions::info(me.index, id).ok_or(Status::MISSING)?;
        out.put(info(&standing))
    }

    fn transaction_undo(me as Mod, id: u64) -> Status {
        crate::world::transactions::undo(me.index, id)?;
        Ok(())
    }

    fn transaction_settled(
        me as Mod,
        id: u64,
        within_ns: u64,
        callback as Callback<TransactionSettledFn>,
        user: *mut core::ffi::c_void,
    ) -> Status {
        crate::world::transactions::info(me.index, id).ok_or(Status::MISSING)?;
        let within = core::time::Duration::from_nanos(within_ns);
        let user = user.expose_provenance();
        let end = Box::new(move |standing: Option<&Standing>| {
            let info = standing.map(info);
            let info = info.as_ref().map_or(core::ptr::null(), core::ptr::from_ref);
            // SAFETY: the callback lives in the mod's DLL, which stays
            // loaded, with the pointer the mod asked with; `info` is null or
            // lives for the call; the waits run each end once, on the
            // server's game thread.
            unsafe { callback(id, info, core::ptr::with_exposed_provenance_mut(user)) };
        });
        crate::world::transactions::when_settled(me.index, id, within, end)?;
        Ok(())
    }
}

/// Where a transaction stands as its mod reads it, counts saturating.
fn info(standing: &Standing) -> TransactionInfo {
    TransactionInfo {
        header: enshwick_core::abi::Header::of::<TransactionInfo>(),
        state: standing.state,
        reason: standing.reason.map_or(Status::OK, |reason| reason.status()),
        ops: u32::try_from(standing.ops).unwrap_or(u32::MAX),
        applied: u32::try_from(standing.applied).unwrap_or(u32::MAX),
        bricks: standing.bricks as u64,
        refusal: standing.refusal,
        reserved: 0,
        holds: standing.holds,
        created: u32::try_from(standing.created).unwrap_or(u32::MAX),
    }
}

/// Commits `ops` as the calling mod, for `actor` when it names one, and
/// gives the transaction's id.
fn commit(me: Mod, actor: Option<u32>, ops: Values<'_, TransactionOp>) -> Result<u64, Refused> {
    if ops.len() == 0 || ops.len() > crate::world::transactions::LIMITS.ops {
        return Err(Status::BAD_ARGS.into());
    }
    let changes = ops
        .iter()
        .map(|op| {
            if op.reserved != 0 {
                return Err(Status::BAD_ARGS.into());
            }
            // SAFETY: `HostApi::transaction_commit` has the mod pass each
            // op's `data` null or pointing at the record its kind names, with
            // its header, and a `VoxelWrite`'s voxels at `voxels`, all
            // borrowed for the call.
            unsafe { change(me.loader, &op) }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crate::world::transactions::commit(
        me.loader, me.index, actor, changes,
    )?)
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
) -> Result<Change, Refused> {
    match op.kind {
        TransactionOp::TERRAIN => {
            // SAFETY: the caller guarantees `op.data` for a terrain op.
            let edit = unsafe {
                enshwick_core::abi::read_headed(
                    op.data.cast::<TerrainEdit>(),
                    <TerrainEdit as enshwick_core::abi::Headed>::BORN,
                )
            };
            Ok(edit.map(Change::Terrain).ok_or(Status::BAD_ARGS)?)
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
            if write.voxels.is_null() || u64::from(write.count) > most {
                return Err(Status::BAD_ARGS.into());
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
            Ok(Paste::new(write.min, write.size, words).map(Change::Voxels)?)
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
            Ok(Erase::new(region.min, region.size).map(Change::Erase)?)
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
            Ok(Change::Block(Box::new(
                crate::world::transactions::resolve(loader, &block)?,
            )))
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
            Ok(crate::world::transactions::resolve_prop(loader, &prop).map(Change::PlaceProp)?)
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
                return Err(Status::BAD_ARGS.into());
            }
            Ok(Change::RemoveProp {
                entity: remove.entity.0,
                discard_state: remove.flags & PropRemove::DISCARD_STATE != 0,
            })
        }
        _ => Err(Status::BAD_ARGS.into()),
    }
}
