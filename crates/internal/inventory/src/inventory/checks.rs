//! What the loader and the fake host refuse in a mod's inventory request,
//! once for both.

use super::MAX_OPS;
use crate::Entity;
use crate::abi::InventoryOp;

/// Whether the loader refuses `ops` as one request: none, more than
/// [`MAX_OPS`], or one that does not name what its kind needs.
#[must_use]
pub fn refused(ops: &[InventoryOp]) -> bool {
    ops.is_empty() || ops.len() > MAX_OPS || !ops.iter().all(valid)
}

/// Whether an operation names what its kind needs.
#[must_use]
pub fn valid(op: &InventoryOp) -> bool {
    let some = |e: Entity| e != Entity::NONE;
    let slot = some(op.target) && u8::try_from(op.slot).is_ok();
    let count = op.count != 0 && i32::try_from(op.count).is_ok();
    match op.kind {
        InventoryOp::GIVE | InventoryOp::TAKE => {
            some(op.target)
                && op.item != crate::ItemId::new(0)
                && count
                && (op.kind == InventoryOp::TAKE || fill(op).is_some())
        }
        InventoryOp::DESTROY_SLOT => slot,
        InventoryOp::RENEW => {
            slot && some(op.other)
                && op.item != crate::ItemId::new(0)
                && count
                && fill(&renew_give(op)).is_some()
        }
        InventoryOp::MOVE_SLOT => slot && some(op.other) && u16::try_from(op.count).is_ok(),
        InventoryOp::MOVE_ALL => some(op.target) && some(op.other) && op.target != op.other,
        _ => false,
    }
}

/// The slot a slot operation names, as the game packs it:
/// `sub-inventory | index << 32`.
#[must_use]
pub fn slot_ref(op: &InventoryOp) -> u64 {
    u64::from(op.target.get()) | u64::from(op.slot) << 32
}

/// The give half of a renew: the new item into the slot's inventory.
#[must_use]
pub fn renew_give(op: &InventoryOp) -> InventoryOp {
    InventoryOp {
        kind: InventoryOp::GIVE,
        flags: InventoryOp::FILLED,
        target: op.other,
        fill: op.fill,
        item: op.item,
        count: op.count,
        ..InventoryOp::default()
    }
}

/// The item option bytes a give writes: its fill and a set flag when the op
/// is filled, zero otherwise; `None` for a fill outside 0 to 1.
#[must_use]
pub fn fill(op: &InventoryOp) -> Option<[u8; 16]> {
    let mut option = [0u8; 16];
    if op.flags & InventoryOp::FILLED == 0 {
        return Some(option);
    }
    if !(0.0..=1.0).contains(&op.fill) {
        return None;
    }
    option[..4].copy_from_slice(&op.fill.to_le_bytes());
    option[4] = 1;
    Some(option)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn give(count: u32) -> InventoryOp {
        InventoryOp {
            kind: InventoryOp::GIVE,
            target: Entity::from_engine(7),
            item: crate::ItemId::new(3),
            count,
            ..InventoryOp::default()
        }
    }

    #[test]
    fn a_request_without_ops_or_past_the_limit_is_refused() {
        assert!(refused(&[]));
        assert!(refused(&vec![give(1); MAX_OPS + 1]));
        assert!(!refused(&vec![give(1); MAX_OPS]));
    }

    #[test]
    fn a_request_with_one_invalid_op_is_refused() {
        assert!(refused(&[give(1), give(0)]));
    }

    #[test]
    fn a_fill_outside_zero_to_one_is_no_fill() {
        let mut filled = give(1);
        filled.flags = InventoryOp::FILLED;
        filled.fill = 2.0;
        assert_eq!(fill(&filled), None);
        filled.fill = 0.5;
        assert_eq!(fill(&filled).map(|o| o[4]), Some(1));
    }

    #[test]
    fn a_slot_past_a_byte_or_without_its_inventory_is_refused() {
        let destroy = |target, slot| InventoryOp {
            kind: InventoryOp::DESTROY_SLOT,
            target,
            slot,
            ..InventoryOp::default()
        };
        assert!(valid(&destroy(Entity::from_engine(0x41), 3)));
        assert_eq!(
            slot_ref(&destroy(Entity::from_engine(0x41), 3)),
            0x3_0000_0041
        );
        assert!(!valid(&destroy(Entity::from_engine(0x41), 256)));
        assert!(!valid(&destroy(Entity::NONE, 3)));
    }
}
