//! The fake loader's transactions: commits, undos and statuses.

use crate::abi::{
    BlockPlace, Header, ModHandle, PropPlace, PropRemove, Status, TerrainEdit, TransactionInfo,
    TransactionOp, Voxel, VoxelRegion, VoxelWrite,
};
use crate::test::table::{put, with};
use crate::test::{FakeChange, FakeTransaction};

/// The most changes the fake takes in one transaction, as the loader.
const OPS_MAX: u32 = 1024;
/// The most bricks of undo data the fake keeps, as the loader.
const BRICKS_MAX: u64 = 65_536;

pub(crate) extern "system" fn transaction_commit(
    me: ModHandle,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    commit(me, crate::Entity(0), ops, count, id)
}

pub(crate) extern "system" fn transaction_commit_for(
    me: ModHandle,
    actor: crate::Entity,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    if actor.is_none() {
        return Status::BAD_ARGS;
    }
    commit(me, actor, ops, count, id)
}

fn commit(
    me: ModHandle,
    actor: crate::Entity,
    ops: *const TransactionOp,
    count: u32,
    id: *mut u64,
) -> Status {
    if ops.is_null() || id.is_null() || count == 0 || count > OPS_MAX {
        return Status::BAD_ARGS;
    }
    let mut changes = Vec::new();
    for i in 0..count as usize {
        // SAFETY: `ops` is non-null, checked above, and `i` is below
        // `count`; the safe layer passes `count` ops at `ops`, so the offset
        // stays inside that array.
        let slot = unsafe { ops.add(i) };
        // SAFETY: `slot` is inside the `count` ops the safe layer passes,
        // borrowed for the call; an unaligned read needs no alignment.
        // The fake host reads the mod's ops array, not game memory.
        // ast-grep-ignore: game-memory-through-raw
        let op = unsafe { slot.read_unaligned() };
        if op.reserved != 0 {
            return Status::BAD_ARGS;
        }
        // SAFETY: the safe layer passes each op's `data` pointing at the
        // record its kind names, with its header, borrowed for the call.
        let Some(change) = (unsafe { change(&op) }) else {
            return Status::BAD_ARGS;
        };
        changes.push(change);
    }
    with(Status::MISSING, |s| {
        if s.terrain.is_none() {
            return Status::MISSING;
        }
        let host = &s.host;
        for change in &changes {
            if let FakeChange::PlaceProp(prop) = change {
                if host.no_prop_items.contains(&prop.item.get()) {
                    return Status::NOT_A_PROP;
                }
                let places = host
                    .items
                    .get(&prop.item.get())
                    .is_some_and(|item| item.placed_entity != [0; 16]);
                if !places {
                    return Status::BAD_ARGS;
                }
            }
            if let FakeChange::Block(block) = change {
                let known = host.blueprints.contains_key(&block.shape.get())
                    && host
                        .building_materials
                        .get(&block.material.get())
                        .is_some_and(|&id| id >= 0x80);
                if !known {
                    return Status::BAD_ARGS;
                }
            }
        }
        let outside = changes.iter().any(|change| match change {
            FakeChange::Terrain(edit) => crate::test::world::edit_outside(host.extent, edit),
            FakeChange::Voxels { min, size, .. } | FakeChange::Erase { min, size } => {
                crate::test::world::outside(host.extent, *min, *size)
            }
            FakeChange::PlaceProp(prop) => host.extent.is_some_and(|end| {
                (0..3).any(|axis| prop.position[axis] * 2.0 >= f64::from(end[axis]))
            }),
            FakeChange::RemoveProp(_) => false,
            FakeChange::Block(block) => host.extent.is_some_and(|end| {
                (0..3).any(|axis| block.position[axis] * 2.0 >= f64::from(end[axis]))
            }),
        });
        if outside {
            return Status::OUTSIDE_WORLD;
        }
        let bound: u64 = changes.iter().map(|change| change.bricks(host)).sum();
        if bound > BRICKS_MAX {
            return Status::BAD_ARGS;
        }
        if claimed(&s.edits.transactions, host) + bound > BRICKS_MAX {
            return Status::REFUSED;
        }
        let next = s.edits.transactions.last().map_or(1, |last| last.id + 1);
        let ops = u32::try_from(changes.len()).unwrap_or(u32::MAX);
        s.edits.transactions.push(FakeTransaction {
            id: next,
            owner: me,
            actor: actor.0,
            changes,
            created: Vec::new(),
            removed: Vec::new(),
            info: TransactionInfo {
                header: Header::of::<TransactionInfo>(),
                state: TransactionInfo::PENDING,
                reason: Status::OK,
                ops,
                applied: 0,
                bricks: 0,
                refusal: 0,
                reserved: 0,
                holds: 0,
                created: 0,
            },
        });
        put(id, next)
    })
}

/// The change `op` hands over, when the loader would take it: a known kind
/// and a valid record.
///
/// # Safety
///
/// - `op.data` is null or points at the record `op.kind` names, readable for
///   the size its header states; a [`VoxelWrite`]'s `voxels` points at
///   `count` voxels; all borrowed for the call.
unsafe fn change(op: &TransactionOp) -> Option<FakeChange> {
    match op.kind {
        TransactionOp::TERRAIN => {
            // SAFETY: the caller guarantees `op.data` for a terrain op.
            let edit = unsafe {
                crate::abi::read_headed(op.data.cast::<TerrainEdit>(), size_of::<TerrainEdit>())
            }?;
            let valid = edit.flags & TerrainEdit::DRY_RUN == 0
                && crate::test::table::world::terrain_edit_valid(&edit);
            valid.then_some(FakeChange::Terrain(edit))
        }
        TransactionOp::VOXELS => {
            // SAFETY: the caller guarantees `op.data` for a voxels op.
            let write = unsafe {
                crate::abi::read_headed(op.data.cast::<VoxelWrite>(), size_of::<VoxelWrite>())
            }?;
            let count =
                Voxel::read_count(write.min, [1; 3]).and(box_count(write.min, write.size))?;
            if write.voxels.is_null() || count != write.count {
                return None;
            }
            let len = usize::try_from(count).ok()?;
            // SAFETY: `write.voxels` is non-null, checked above, and the
            // caller guarantees it points at `count` voxels for the call.
            let voxels = unsafe { core::slice::from_raw_parts(write.voxels, len) }.to_vec();
            voxels
                .iter()
                .all(|voxel| voxel_encodes(*voxel))
                .then_some(FakeChange::Voxels {
                    min: write.min,
                    size: write.size,
                    voxels,
                })
        }
        TransactionOp::ERASE => {
            // SAFETY: the caller guarantees `op.data` for an erase op.
            let region = unsafe {
                crate::abi::read_headed(op.data.cast::<VoxelRegion>(), size_of::<VoxelRegion>())
            }?;
            box_count(region.min, region.size)?;
            Some(FakeChange::Erase {
                min: region.min,
                size: region.size,
            })
        }
        TransactionOp::PLACE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a prop op.
            let prop = unsafe {
                crate::abi::read_headed(op.data.cast::<PropPlace>(), size_of::<PropPlace>())
            }?;
            let finite = prop.position.iter().all(|v| v.is_finite() && *v >= 0.0)
                && prop.orientation.iter().all(|v| v.is_finite())
                && prop.reserved == 0;
            finite.then_some(FakeChange::PlaceProp(prop))
        }
        TransactionOp::REMOVE_PROP => {
            // SAFETY: the caller guarantees `op.data` for a removal op.
            let remove = unsafe {
                crate::abi::read_headed(op.data.cast::<PropRemove>(), size_of::<PropRemove>())
            }?;
            (!remove.entity.is_none() && remove.flags & !PropRemove::DISCARD_STATE == 0)
                .then_some(FakeChange::RemoveProp(remove))
        }
        TransactionOp::BLOCK => {
            // SAFETY: the caller guarantees `op.data` for a block op.
            let block = unsafe {
                crate::abi::read_headed(op.data.cast::<BlockPlace>(), size_of::<BlockPlace>())
            }?;
            let finite = block.position.iter().all(|v| v.is_finite() && *v >= 0.0)
                && block.rotation.iter().all(|v| v.is_finite())
                && block.rotation.iter().any(|v| *v != 0.0);
            finite.then_some(FakeChange::Block(block))
        }
        _ => None,
    }
}

/// Whether the loader encodes `voxel`: its kind is the one its material
/// names (air for 0, terrain for 1 to 0x7f, building from 0x80) and its
/// reserved byte is 0.
fn voxel_encodes(voxel: Voxel) -> bool {
    let kind = match voxel.material {
        0 => Voxel::AIR,
        1..0x80 => Voxel::TERRAIN,
        _ => Voxel::BUILDING,
    };
    voxel.kind == kind && voxel.reserved == 0
}

/// The voxels of the box of `size` from `min`: not empty and within
/// [`Voxel::END`].
fn box_count(min: [u32; 3], size: [u32; 3]) -> Option<u64> {
    let mut count = 1u64;
    for axis in 0..3 {
        let end = min[axis].checked_add(size[axis])?;
        if size[axis] == 0 || end > Voxel::END {
            return None;
        }
        count = count.checked_mul(u64::from(size[axis]))?;
    }
    Some(count)
}

/// The bricks of undo data the transactions in work may come to hold, as
/// the loader counts them.
fn claimed(transactions: &[FakeTransaction], host: &crate::test::FakeHost) -> u64 {
    transactions
        .iter()
        .map(|t| match t.info.state {
            TransactionInfo::PENDING => t.bricks(host),
            TransactionInfo::UNDOING => t.info.bricks,
            _ => 0,
        })
        .sum()
}

pub(crate) extern "system" fn transaction_undo(me: ModHandle, id: u64) -> Status {
    with(Status::REFUSED, |s| {
        let claimed = claimed(&s.edits.transactions, &s.host);
        let Some(transaction) = s
            .edits
            .transactions
            .iter_mut()
            .find(|t| t.id == id && t.owner == me)
        else {
            return Status::REFUSED;
        };
        let props = !transaction.created.is_empty() || !transaction.removed.is_empty();
        let info = &mut transaction.info;
        match info.state {
            TransactionInfo::PENDING if info.applied == 0 => info.state = TransactionInfo::UNDONE,
            TransactionInfo::UNDOING => {}
            TransactionInfo::PENDING => info.state = TransactionInfo::UNDOING,
            TransactionInfo::APPLIED | TransactionInfo::UNDO_REFUSED | TransactionInfo::FAILED
                if info.bricks > 0 || props =>
            {
                if claimed + info.bricks > BRICKS_MAX {
                    return Status::REFUSED;
                }
                info.state = TransactionInfo::UNDOING;
            }
            _ => return Status::REFUSED,
        }
        info.reason = Status::OK;
        info.holds = 0;
        Status::OK
    })
}

pub(crate) extern "system" fn transaction_entities(
    me: ModHandle,
    id: u64,
    out: *mut crate::Entity,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let found = with(None, |s| {
        s.edits
            .transactions
            .iter()
            .find(|t| t.id == id && t.owner == me)
            .map(|t| t.created.clone())
    });
    let Some(created) = found else {
        return Status::MISSING;
    };
    crate::test::table::put_list(&created, out, capacity, count)
}

pub(crate) extern "system" fn transaction_status(
    me: ModHandle,
    id: u64,
    out: *mut TransactionInfo,
) -> Status {
    let found = with(None, |s| {
        s.edits
            .transactions
            .iter()
            .find(|t| t.id == id && t.owner == me)
            .map(|t| t.info)
    });
    let Some(info) = found else {
        return Status::MISSING;
    };
    // SAFETY: the safe layer passes `out` null or pointing at a writable
    // `TransactionInfo` whose header names its size.
    if unsafe { crate::abi::write_headed(out, info) } {
        Status::OK
    } else {
        Status::BAD_ARGS
    }
}
