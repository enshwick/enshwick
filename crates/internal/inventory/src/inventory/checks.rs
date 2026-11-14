//! What the loader and the fake host refuse in a mod's inventory request,
//! once for both.

use super::MAX_OPS;
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
    let entity = |v: u64| u32::try_from(v).is_ok_and(|v| v != 0);
    match op.kind {
        InventoryOp::GIVE | InventoryOp::TAKE => {
            entity(op.target)
                && op.item != crate::ItemId::new(0)
                && op.count != 0
                && i32::try_from(op.count).is_ok()
                && (op.kind == InventoryOp::TAKE || fill(op).is_some())
        }
        InventoryOp::DESTROY_SLOT => op.target & 0xffff_ffff != 0,
        InventoryOp::RENEW => {
            op.target & 0xffff_ffff != 0
                && op.other & 0xffff_ffff != 0
                && op.item != crate::ItemId::new(0)
                && op.count != 0
                && i32::try_from(op.count).is_ok()
                && fill(&renew_give(op)).is_some()
        }
        InventoryOp::MOVE_SLOT => {
            op.target & 0xffff_ffff != 0 && entity(op.other) && u16::try_from(op.count).is_ok()
        }
        InventoryOp::MOVE_ALL => entity(op.target) && entity(op.other) && op.target != op.other,
        _ => false,
    }
}

/// The give half of a renew: the new item into the slot's inventory.
#[must_use]
pub fn renew_give(op: &InventoryOp) -> InventoryOp {
    InventoryOp {
        kind: InventoryOp::GIVE,
        flags: InventoryOp::FILLED,
        target: op.other & 0xffff_ffff,
        other: op.other >> 32,
        item: op.item,
        count: op.count,
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
    let fill = f32::from_bits(u32::try_from(op.other).ok()?);
    if !(0.0..=1.0).contains(&fill) {
        return None;
    }
    option[..4].copy_from_slice(&fill.to_le_bytes());
    option[4] = 1;
    Some(option)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn give(count: u32) -> InventoryOp {
        InventoryOp {
            kind: InventoryOp::GIVE,
            flags: 0,
            target: 7,
            other: 0,
            item: crate::ItemId::new(3),
            count,
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
        filled.other = u64::from(2.0f32.to_bits());
        assert_eq!(fill(&filled), None);
        filled.other = u64::from(0.5f32.to_bits());
        assert_eq!(fill(&filled).map(|o| o[4]), Some(1));
    }
}
