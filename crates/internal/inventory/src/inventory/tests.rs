use crate::abi::{InventoryOp, InventoryOpResult, InventoryRequest, InventoryResult};
use crate::engine_enum::Values;

use super::*;

fn all(from: u32, to: u32) -> Op {
    Op::MoveAll {
        from: Entity::from_engine(from),
        to: Entity::from_engine(to),
        include_special: false,
    }
}

fn give(to: u32) -> Op {
    Op::Give {
        to: Entity::from_engine(to),
        item: crate::ItemId::new(7),
        count: 1,
        fill: None,
    }
}

#[test]
fn slot_refs_pack_the_sub_inventory_low_and_the_index_high() {
    let slot = SlotRef::new(Entity::from_engine(u32::MAX), 7);
    assert_eq!(slot.0, 0x7_ffff_ffff);
    assert_eq!(slot.inventory(), Entity::from_engine(u32::MAX));
    assert_eq!(slot.index(), 7);
    assert_eq!(SlotRef(0x41).index(), 0);
}

#[test]
fn batches_are_checked_against_the_game_limits() {
    assert_eq!(Batch::new().check(), Err(RunBatchError::Empty));
    assert_eq!(
        Batch::new().with(all(5, 5)).check(),
        Err(RunBatchError::Malformed)
    );
    let renew = Op::Renew {
        slot: SlotRef::new(Entity::from_engine(41), 2),
        root: Entity::from_engine(40),
        item: crate::ItemId::new(7),
        count: 1,
        fill: Fill::FULL,
    };
    assert_eq!(Batch::new().with(renew).check(), Ok(()));
    let raw = renew.raw();
    assert_eq!(
        (raw.kind, raw.target, raw.slot, raw.other),
        (
            InventoryOp::RENEW,
            Entity::from_engine(41),
            2,
            Entity::from_engine(40)
        )
    );
    assert!((raw.fill - 1.0).abs() < f32::EPSILON);
    assert_eq!(
        Batch::new()
            .with(Op::Renew {
                slot: SlotRef::new(Entity::from_engine(41), 2),
                root: Entity::NONE,
                item: crate::ItemId::new(7),
                count: 1,
                fill: Fill::FULL,
            })
            .check(),
        Err(RunBatchError::Malformed)
    );
    let zero = Op::Give {
        to: Entity::from_engine(2),
        item: crate::ItemId::new(7),
        count: 0,
        fill: None,
    };
    assert_eq!(
        Batch::new().with(zero).check(),
        Err(RunBatchError::Malformed)
    );
    let mut many = Batch::new();
    for _ in 0..=MAX_OPS {
        many.push(Op::DestroySlot(SlotRef(0x41)));
    }
    assert_eq!(many.check(), Err(RunBatchError::TooManyOps));
    let mut wide = Batch::new();
    for to in 1..=17 {
        wide.push(give(to));
    }
    assert_eq!(wide.check(), Err(RunBatchError::TooManyInventories));
    let heavy = Batch::new().with(all(1, 2)).with(all(3, 2)).with(give(2));
    assert_eq!(heavy.check(), Err(RunBatchError::TooLarge));
    assert_eq!(Batch::new().with(all(1, 2)).with(all(3, 2)).check(), Ok(()));
}

#[test]
fn splitting_keeps_order_and_every_part_fits() {
    let mut big = Batch::new().allow_partial().actor(Entity::from_engine(2));
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
            .all(|p| p.allow_partial && p.actor == Some(Entity::from_engine(2)))
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
    assert_eq!(loot.actor_entity(), Entity::from_engine(2));
    let paid = Batch::new()
        .with(Op::Take {
            from: Entity::from_engine(3),
            item: crate::ItemId::new(1),
            count: 1,
        })
        .with(give(3))
        .allow_partial();
    assert_eq!(
        paid.request_flags(),
        InventoryRequest::ALLOW_PARTIAL | InventoryRequest::NOTIFY
    );
    assert_eq!(paid.actor_entity(), Entity::from_engine(3));
    assert_eq!(
        loot.actor(Entity::from_engine(9)).actor_entity(),
        Entity::from_engine(9)
    );
}

#[test]
fn operations_become_the_loaders_records() {
    let slot = SlotRef::new(Entity::from_engine(0x41), 2);
    let raw = Op::MoveSlot {
        slot,
        to: Entity::from_engine(2),
        amount: Some(3),
    }
    .raw();
    assert_eq!(
        (raw.kind, raw.target, raw.slot, raw.other, raw.count),
        (
            InventoryOp::MOVE_SLOT,
            slot.inventory(),
            2,
            Entity::from_engine(2),
            3
        )
    );
    let special = Op::MoveAll {
        from: Entity::from_engine(40),
        to: Entity::from_engine(2),
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
    let raw = enshwick_abi::headed!(InventoryResult {
        outcome: InventoryResult::ROLLED_BACK,
        op_count: 2,
        ops: ops.as_ptr(),
    });
    let errors = Values::new([
        (RunOpError::InvalidState, Some(0x15)),
        (RunOpError::OutOfMemory, None),
    ]);
    // SAFETY: `raw.ops` points at the two results of `ops`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(outcome.done, Done::RolledBack);
    assert!(!outcome.is_committed());
    assert_eq!(
        outcome.ops[1],
        OpResult {
            error: Some(RunOpError::InvalidState),
            remaining: 4
        }
    );
    assert!(outcome.ops[0].ok());
    let other = [InventoryOpResult {
        error: 0x27,
        remaining: 0,
    }];
    let raw = enshwick_abi::headed!(InventoryResult {
        op_count: 1,
        ops: other.as_ptr(),
        ..raw
    });
    // SAFETY: `raw.ops` points at the one result of `other`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(
        outcome.ops[0].error,
        Some(RunOpError::Other(0x27)),
        "a code whose constant the build lacks stays as the game gave it"
    );
    let skipped = [InventoryOpResult {
        error: InventoryOpResult::NOT_RUN,
        remaining: 0,
    }];
    let raw = enshwick_abi::headed!(InventoryResult {
        ops: skipped.as_ptr(),
        ..raw
    });
    // SAFETY: `raw.ops` points at the one result of `skipped`, alive here.
    let outcome = unsafe { Outcome::from_raw(&raw, &errors) };
    assert_eq!(outcome.ops[0].error, Some(RunOpError::NotRun));
    let empty = enshwick_abi::headed!(InventoryResult {
        outcome: InventoryResult::TIMED_OUT,
        op_count: 0,
        ops: core::ptr::null(),
        ..raw
    });
    // SAFETY: `empty.ops` is null.
    let outcome = unsafe { Outcome::from_raw(&empty, &errors) };
    assert_eq!(outcome, Outcome::not_run(Done::TimedOut));
}

#[test]
fn op_errors_take_the_engine_s_names() {
    crate::engine_enum::assert_named_after_engine(&RunOpError::IDS);
}

#[test]
fn only_items_with_an_entity_are_instances() {
    let slot = |item, count, pide| Slot {
        at: SlotRef::new(Entity::from_engine(41), 0),
        category: InventoryCategory::GENERIC,
        item,
        count,
        pide: Entity::from_raw(pide),
    };
    assert!(ItemInstance::of(&slot(crate::ItemId::new(7), 1, 900)).is_some());
    assert!(ItemInstance::of(&slot(crate::ItemId::new(7), 5, 0)).is_none());
    assert!(ItemInstance::of(&slot(crate::ItemId::new(0), 0, 900)).is_none());
}
