//! Host functions for transactions of the world: commits, statuses and undos.

use enshwick::abi::{
    BlockPlace, ModHandle, PropPlace, PropRemove, Status, TerrainEdit, TransactionInfo,
    TransactionOp, VoxelRegion, VoxelWrite,
};

use crate::host::memory::{put, put_headed, put_list};
use crate::host::{caller_index, entry};
use crate::transactions::{Change, Erase, Paste};

pub(crate) extern "system" fn transaction_commit(
    me: ModHandle,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    entry("transaction_commit", || commit(me, None, ops, count, id))
}

pub(crate) extern "system" fn transaction_commit_for(
    me: ModHandle,
    actor: u32,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    entry("transaction_commit_for", || {
        if actor == 0 {
            return Status::BAD_ARGS;
        }
        commit(me, Some(actor), ops, count, id)
    })
}

fn commit(
    me: ModHandle,
    actor: Option<u32>,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    let count = count as usize;
    if ops.is_null() || id.is_null() || count == 0 || count > crate::transactions::LIMITS.ops {
        return Status::BAD_ARGS;
    }
    let (loader, owner) = match caller_index(me) {
        Ok(caller) => caller,
        Err(status) => return status,
    };
    let mut changes = Vec::with_capacity(count);
    for i in 0..count {
        // SAFETY: `ops` is non-null, checked above, and `i` is below
        // `count`; `HostApi::transaction_commit` has the mod pass `count`
        // ops at `ops`, so the offset stays inside that array.
        let slot = unsafe { ops.add(i) };
        // SAFETY: `slot` is inside the `count` ops the mod passes, borrowed
        // for the call; an unaligned read needs no alignment.
        let op = unsafe { slot.read_unaligned() };
        if op.reserved != 0 {
            return Status::BAD_ARGS;
        }
        // SAFETY: `HostApi::transaction_commit` has the mod pass each op's
        // `data` null or pointing at the record its kind names, with its
        // header, and a `VoxelWrite`'s voxels at `voxels`, all borrowed for
        // the call.
        match unsafe { change(loader, &op) } {
            Ok(change) => changes.push(change),
            Err(status) => return status,
        }
    }
    match crate::transactions::commit(loader, owner, actor, changes) {
        // SAFETY: `id` is non-null, checked above; `HostApi::transaction_commit`
        // has the mod pass it pointing at a writable, aligned `u64` that no
        // other thread touches during the call.
        Ok(next) => unsafe { put(id, next) },
        Err(status) => status,
    }
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
unsafe fn change(loader: &crate::host::Loader, op: &TransactionOp) -> Result<Change, Status> {
    match op.kind {
        TransactionOp::TERRAIN => {
            // SAFETY: the caller guarantees `op.data` for a terrain op.
            let edit = unsafe {
                enshwick::abi::read_headed(op.data.cast::<TerrainEdit>(), size_of::<TerrainEdit>())
            };
            edit.map(Change::Terrain).ok_or(Status::BAD_ARGS)
        }
        TransactionOp::VOXELS => {
            // SAFETY: the caller guarantees `op.data` for a voxels op.
            let write = unsafe {
                enshwick::abi::read_headed(op.data.cast::<VoxelWrite>(), size_of::<VoxelWrite>())
            }
            .ok_or(Status::BAD_ARGS)?;
            let most = crate::transactions::LIMITS.bricks as u64 * 512;
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
                .map(|voxel| crate::voxels::encode(*voxel))
                .collect::<Result<Vec<_>, _>>()?;
            Paste::new(write.min, write.size, words).map(Change::Voxels)
        }
        TransactionOp::ERASE => {
            // SAFETY: the caller guarantees `op.data` for an erase op.
            let region = unsafe {
                enshwick::abi::read_headed(op.data.cast::<VoxelRegion>(), size_of::<VoxelRegion>())
            }
            .ok_or(Status::BAD_ARGS)?;
            Erase::new(region.min, region.size).map(Change::Erase)
        }
        #[cfg(windows)]
        TransactionOp::BLOCK => {
            // SAFETY: the caller guarantees `op.data` for a block op.
            let block = unsafe {
                enshwick::abi::read_headed(op.data.cast::<BlockPlace>(), size_of::<BlockPlace>())
            }
            .ok_or(Status::BAD_ARGS)?;
            crate::transactions::resolve(loader, &block).map(|block| Change::Block(Box::new(block)))
        }
        #[cfg(windows)]
        TransactionOp::PLACE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a prop op.
            let prop = unsafe {
                enshwick::abi::read_headed(op.data.cast::<PropPlace>(), size_of::<PropPlace>())
            }
            .ok_or(Status::BAD_ARGS)?;
            crate::transactions::resolve_prop(loader, &prop).map(Change::PlaceProp)
        }
        TransactionOp::REMOVE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a removal op.
            let remove = unsafe {
                enshwick::abi::read_headed(op.data.cast::<PropRemove>(), size_of::<PropRemove>())
            }
            .ok_or(Status::BAD_ARGS)?;
            if remove.entity == 0 || remove.flags & !PropRemove::DISCARD_STATE != 0 {
                return Err(Status::BAD_ARGS);
            }
            Ok(Change::RemoveProp {
                entity: remove.entity,
                discard_state: remove.flags & PropRemove::DISCARD_STATE != 0,
            })
        }
        _ => Err(Status::BAD_ARGS),
    }
}

pub(crate) extern "system" fn transaction_entities(
    me: ModHandle,
    id: u64,
    out: *mut u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    entry("transaction_entities", || {
        let owner = match caller_index(me) {
            Ok((_, owner)) => owner,
            Err(status) => return status,
        };
        let Some(created) = crate::transactions::entities(owner, id) else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::transaction_entities` has the mod pass `count`
        // null or a writable `u32`, and `out` null or `capacity` writable
        // `u32`s, untouched by other threads during the call.
        unsafe { put_list(&created, out, capacity, count) }
    })
}

pub(crate) extern "system" fn transaction_status(
    me: ModHandle,
    id: u64,
    out: *mut TransactionInfo,
) -> Status {
    entry("transaction_status", || {
        let owner = match caller_index(me) {
            Ok((_, owner)) => owner,
            Err(status) => return status,
        };
        let Some(info) = crate::transactions::info(owner, id) else {
            return Status::MISSING;
        };
        // SAFETY: `HostApi::transaction_status` has the mod pass `out` null
        // or pointing at a writable `TransactionInfo` whose header names its
        // size, untouched by other threads during the call.
        unsafe { put_headed(out, info) }
    })
}

pub(crate) extern "system" fn transaction_undo(me: ModHandle, id: u64) -> Status {
    entry("transaction_undo", || {
        match caller_index(me).and_then(|(_, owner)| crate::transactions::undo(owner, id)) {
            Ok(()) => Status::OK,
            Err(status) => status,
        }
    })
}
