//! Changes a mod declares at init and sets later: each stands at a factor of
//! 1 until set, so the loader patches what it reaches from the first read
//! and a later set rebuilds in place.

use enshwick::abi::ChangeId;
use enshwick::content::changes::Value;

use super::{Declared, Pending};
use crate::mod_index::ModIndex;

/// The value a change declared for later holds until its mod sets one, or
/// after a reset: it leaves the value as the other changes make it.
pub const UNSET: Value = Value::Times(1.0);

/// `declared` with every change that came from mod `owner`'s change `id`
/// at `value`, or at [`UNSET`] for `None`; every other change and the load
/// order stay.
#[must_use]
pub fn set_live(
    declared: &[Declared],
    owner: ModIndex,
    id: ChangeId,
    value: Option<Value>,
) -> Vec<Declared> {
    declared
        .iter()
        .map(|d| {
            let mut d = d.clone();
            if d.owner == owner && d.live == Some(id) {
                d.change.value = value.unwrap_or(UNSET);
            }
            d
        })
        .collect()
}

/// Puts `value`, or [`UNSET`] for `None`, on mod `owner`'s change `id`
/// among the changes declared before the seal.
pub fn set_pending(pending: &mut [Pending], owner: ModIndex, id: ChangeId, value: Option<Value>) {
    let value = value.unwrap_or(UNSET);
    for p in pending {
        match p {
            Pending::One(d) if d.owner == owner && d.live == Some(id) => d.change.value = value,
            Pending::Each {
                owner: o,
                change,
                live,
                ..
            } if *o == owner && *live == Some(id) => change.value = value,
            _ => {}
        }
    }
}

/// Whether the resources the changes patch keep their base, which a
/// rebuild in place starts from: under a development reload, or once a mod
/// declared a change it sets later.
#[must_use]
pub const fn keeps_bases(dev_reload: bool, live: bool) -> bool {
    dev_reload || live
}

#[cfg(test)]
mod tests {
    use enshwick::content::Guid;
    use enshwick::content::changes::{Change, Field};

    use super::*;
    use crate::data_changes::{Key, merge};

    fn change(owner: usize, entry: u8, value: Value, live: Option<u32>) -> Declared {
        Declared {
            owner: ModIndex::new(owner),
            change: Change::new(Guid([entry; 16]), Field::MaxStackSize, value),
            source: String::new(),
            live: live.map(ChangeId),
        }
    }

    fn value(declared: &[Declared], entry: u8) -> Option<f64> {
        let (merged, _) = merge(declared, |_, _| false);
        merged.value(
            Key::new(Guid([entry; 16]), Field::MaxStackSize),
            20.0,
            |_| true,
        )
    }

    #[test]
    fn an_unset_change_leaves_the_value_as_the_others_make_it() {
        let declared = vec![
            change(0, 1, Value::Set(50.0), None),
            change(1, 1, UNSET, Some(1)),
            change(1, 2, UNSET, Some(1)),
        ];
        assert_eq!(value(&declared, 1), Some(50.0));
        assert_eq!(value(&declared, 2), Some(20.0));
    }

    #[test]
    fn a_set_reaches_every_change_of_its_id_in_place() {
        let declared = vec![
            change(1, 1, UNSET, Some(1)),
            change(0, 1, Value::Set(50.0), None),
            change(1, 2, UNSET, Some(1)),
            change(1, 2, UNSET, Some(2)),
        ];
        let set = set_live(
            &declared,
            ModIndex::new(1),
            ChangeId(1),
            Some(Value::Times(4.0)),
        );
        assert_eq!(value(&set, 1), Some(200.0));
        assert_eq!(value(&set, 2), Some(80.0));
        assert_eq!(set[3].change.value, UNSET);
        let owners: Vec<usize> = set.iter().map(|d| d.owner.get()).collect();
        assert_eq!(owners, vec![1, 0, 1, 1]);
    }

    #[test]
    fn a_set_before_the_seal_reaches_its_change_alone() {
        use enshwick::content::changes::{Items, ItemsChange};
        let mut pending = vec![
            Pending::One(change(1, 1, UNSET, Some(1))),
            Pending::Each {
                owner: ModIndex::new(1),
                change: ItemsChange::new(Items::Stackable, Field::MaxStackSize, UNSET),
                source: String::new(),
                live: Some(ChangeId(2)),
            },
            Pending::One(change(2, 1, UNSET, Some(1))),
        ];
        set_pending(
            &mut pending,
            ModIndex::new(1),
            ChangeId(2),
            Some(Value::Times(3.0)),
        );
        set_pending(
            &mut pending,
            ModIndex::new(1),
            ChangeId(1),
            Some(Value::Set(99.0)),
        );
        let values: Vec<Value> = pending
            .iter()
            .map(|p| match p {
                Pending::One(d) => d.change.value,
                Pending::Each { change, .. } => change.value,
            })
            .collect();
        assert_eq!(values, vec![Value::Set(99.0), Value::Times(3.0), UNSET]);
        set_pending(&mut pending, ModIndex::new(1), ChangeId(1), None);
        assert!(matches!(&pending[0], Pending::One(d) if d.change.value == UNSET));
    }

    #[test]
    fn a_live_change_keeps_the_bases_a_later_set_rebuilds_from() {
        assert!(keeps_bases(false, true));
        assert!(keeps_bases(true, false));
        assert!(!keeps_bases(false, false));
    }

    #[test]
    fn a_reset_takes_the_value_out_and_another_mod_s_id_stays() {
        let declared = vec![
            change(1, 1, Value::Set(999.0), Some(1)),
            change(2, 1, Value::Times(2.0), Some(1)),
        ];
        let reset = set_live(&declared, ModIndex::new(1), ChangeId(1), None);
        assert_eq!(value(&reset, 1), Some(40.0));
    }
}
