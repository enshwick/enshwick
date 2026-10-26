use crate::abi::{Header, InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult};
use crate::engine_enum::Values;

use super::*;

fn all(from: u32, to: u32) -> Op {
    Op::MoveAll {
        from: Entity(from),
        to: Entity(to),
        include_special: false,
    }
}

fn give(to: u32) -> Op {
    Op::Give {
        to: Entity(to),
        item: 7,
        count: 1,
        fill: None,
    }
}

#[test]
fn slot_refs_pack_the_sub_inventory_low_and_the_index_high() {
    let slot = SlotRef::new(Entity(u32::MAX), 7);
    assert_eq!(slot.0, 0x7_ffff_ffff);
    assert_eq!(slot.inventory(), Entity(u32::MAX));
    assert_eq!(slot.index(), 7);
    assert_eq!(SlotRef(0x41).index(), 0);
}

#[test]
fn batches_are_checked_against_the_game_limits() {
    assert_eq!(Batch::new().check(), Err(BatchError::Empty));
    assert_eq!(
        Batch::new().with(all(5, 5)).check(),
        Err(BatchError::Malformed)
    );
    let renew = Op::Renew {
        slot: SlotRef::new(Entity(41), 2),
        root: Entity(40),
        item: 7,
        count: 1,
        fill: Fill::FULL,
    };
    assert_eq!(Batch::new().with(renew).check(), Ok(()));
    let raw = renew.raw();
    assert_eq!(
        (raw.kind, raw.target, raw.other & 0xffff_ffff),
        (InventoryOp::RENEW, SlotRef::new(Entity(41), 2).0, 40)
    );
    assert_eq!(raw.other >> 32, u64::from(1.0f32.to_bits()));
    assert_eq!(
        Batch::new()
            .with(Op::Renew {
                slot: SlotRef::new(Entity(41), 2),
                root: Entity::NONE,
                item: 7,
                count: 1,
                fill: Fill::FULL,
            })
            .check(),
        Err(BatchError::Malformed)
    );
    let zero = Op::Give {
        to: Entity(2),
        item: 7,
        count: 0,
        fill: None,
    };
    assert_eq!(Batch::new().with(zero).check(), Err(BatchError::Malformed));
    let mut many = Batch::new();
    for _ in 0..=MAX_OPS {
        many.push(Op::DestroySlot(SlotRef(0x41)));
    }
    assert_eq!(many.check(), Err(BatchError::TooManyOps));
    let mut wide = Batch::new();
    for to in 1..=17 {
        wide.push(give(to));
    }
    assert_eq!(wide.check(), Err(BatchError::TooManyInventories));
    let heavy = Batch::new().with(all(1, 2)).with(all(3, 2)).with(give(2));
    assert_eq!(heavy.check(), Err(BatchError::TooLarge));
    assert_eq!(Batch::new().with(all(1, 2)).with(all(3, 2)).check(), Ok(()));
}

#[test]
fn splitting_keeps_order_and_every_part_fits() {
    let mut big = Batch::new().allow_partial().actor(Entity(2));
    for from in 100..105 {
        big.push(all(from, 2));
    }
    for to in 200..230 {
        big.push(give(to));
    }
    let ops = big.ops().to_vec();
    let parts = big.split();
    assert!(parts.len() > 3);
    assert!(parts.iter().all(|p| p.check().is_ok()));
    assert!(
        parts
            .iter()
            .all(|p| p.allow_partial && p.actor == Entity(2))
    );
    let joined: Vec<Op> = parts.iter().flat_map(|p| p.ops().to_vec()).collect();
    assert_eq!(joined, ops);
    assert_eq!(Batch::new().with(give(2)).split().len(), 1);
    assert!(Batch::new().split().is_empty());
}

#[test]
fn requests_notify_for_gives_and_credit_the_first_destination() {
    let loot = Batch::new().with(all(40, 2));
    assert_eq!(loot.request_flags(), 0);
    assert_eq!(
        loot.clone().notify().request_flags(),
        InventoryRequest::NOTIFY
    );
    assert_eq!(loot.actor_entity(), Entity(2));
    let paid = Batch::new()
        .with(Op::Take {
            from: Entity(3),
            item: 1,
            count: 1,
        })
        .with(give(3))
        .allow_partial();
    assert_eq!(
        paid.request_flags(),
        InventoryRequest::ALLOW_PARTIAL | InventoryRequest::NOTIFY
    );
    assert_eq!(paid.actor_entity(), Entity(3));
    assert_eq!(loot.actor(Entity(9)).actor_entity(), Entity(9));
}

#[test]
fn operations_become_the_loaders_records() {
    let slot = SlotRef::new(Entity(0x41), 2);
    let raw = Op::MoveSlot {
        slot,
        to: Entity(2),
        amount: Some(3),
    }
    .raw();
    assert_eq!(
        (raw.kind, raw.target, raw.other, raw.count),
        (InventoryOp::MOVE_SLOT, slot.0, 2, 3)
    );
    let special = Op::MoveAll {
        from: Entity(40),
        to: Entity(2),
        include_special: true,
    }
    .raw();
    assert_eq!(special.flags, InventoryOp::INCLUDE_SPECIAL);
}

#[test]
fn outcomes_read_the_loaders_result() {
    let ops = [
        InventoryOpResult {
            error: 0,
            remaining: 0,
        },
        InventoryOpResult {
            error: 0x15,
            remaining: 4,
        },
    ];
    let raw = InventoryResult {
        header: Header::of::<InventoryResult>(),
        outcome: InventoryResult::ROLLED_BACK,
        op_count: 2,
        ops: ops.as_ptr(),
    };
    let errors = Values::new([
        (OpError::InvalidState, Some(0x15)),
        (OpError::OutOfMemory, None),
    ]);
    // SAFETY: `raw.ops` points at the two results of `ops`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(outcome.done, Done::RolledBack);
    assert!(!outcome.is_committed());
    assert_eq!(
        outcome.ops[1],
        OpResult {
            error: Some(OpError::InvalidState),
            remaining: 4
        }
    );
    assert!(outcome.ops[0].ok());
    let other = [InventoryOpResult {
        error: 0x27,
        remaining: 0,
    }];
    let raw = InventoryResult {
        op_count: 1,
        ops: other.as_ptr(),
        ..raw
    };
    // SAFETY: `raw.ops` points at the one result of `other`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(
        outcome.ops[0].error,
        Some(OpError::Other(0x27)),
        "a code whose constant the build lacks stays as the game gave it"
    );
    let skipped = [InventoryOpResult {
        error: InventoryOpResult::NOT_RUN,
        remaining: 0,
    }];
    let raw = InventoryResult {
        ops: skipped.as_ptr(),
        ..raw
    };
    // SAFETY: `raw.ops` points at the one result of `skipped`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(outcome.ops[0].error, Some(OpError::NotRun));
    let empty = InventoryResult {
        outcome: InventoryResult::TIMED_OUT,
        op_count: 0,
        ops: core::ptr::null(),
        ..raw
    };
    // SAFETY: `empty.ops` is null.
    let outcome = unsafe { Outcome::from_raw(&empty, &errors) };
    assert_eq!(outcome, Outcome::not_run(Done::TimedOut));
}

#[test]
fn op_errors_take_the_engine_s_names() {
    crate::engine_enum::assert_named_after_engine(&OpError::IDS);
}
