//! What each field's values must be, apart from the field itself, so a raw
//! change of a field a build without `unstable` cannot name is checked by
//! the same rules as the loader's.

use crate::abi::DataField;
#[cfg(any(feature = "fake", all(test, feature = "unstable")))]
use crate::abi::DataSelect;
#[cfg(feature = "fake")]
use crate::abi::{ChangeOp, DataChange};
#[cfg(feature = "fake")]
use crate::content::Guid;

use super::{Field, MAX_FACTOR, SetFieldError, Value};

/// What a field's values must be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rules {
    /// The smallest value it holds.
    pub(crate) low: f64,
    /// The largest value it holds.
    pub(crate) high: f64,
    /// The step its values come in.
    pub(crate) step: f64,
    /// Whether it holds whole numbers only.
    pub(crate) whole: bool,
    /// Whether it takes sets, factors or both.
    takes: Takes,
    /// Whether a factor below 1 is refused.
    grows_only: bool,
}

/// The kinds of [`Value`] a field takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Takes {
    Both,
    SetsOnly,
    FactorsOnly,
}

/// An item's stack size.
const MAX_STACK: Rules = Rules::whole(1.0, 65535.0);

/// A container's or a backpack's slots: pages of 8 that only grow, since a
/// chest a player upgrades moves every item it holds.
const SLOTS: Rules = Rules {
    step: 8.0,
    grows_only: true,
    ..Rules::whole(8.0, 128.0)
};

impl Rules {
    /// A field of whole numbers from `low` to `high` that takes sets and
    /// factors.
    pub(crate) const fn whole(low: f64, high: f64) -> Self {
        Self {
            low,
            high,
            step: 1.0,
            whole: true,
            takes: Takes::Both,
            grows_only: false,
        }
    }

    /// A field of any number from `low` to `high` that takes sets and
    /// factors.
    pub(crate) const fn fraction(low: f64, high: f64) -> Self {
        Self {
            whole: false,
            ..Self::whole(low, high)
        }
    }

    /// The same, taking only factors.
    pub(crate) const fn factor_only(self) -> Self {
        Self {
            takes: Takes::FactorsOnly,
            ..self
        }
    }

    /// The same, taking only sets.
    pub(crate) const fn set_only(self) -> Self {
        Self {
            takes: Takes::SetsOnly,
            ..self
        }
    }

    /// The rules of the field the ABI numbers `field`, in every build; `None`
    /// for a number no field has. [`Field::rules`] reads them, and the fake
    /// host checks a raw change of a field a mod built without `unstable`
    /// cannot name by them, as the loader, built with it, does.
    pub(crate) const fn of(field: DataField) -> Option<Self> {
        Some(match field {
            DataField::MAX_STACK => MAX_STACK,
            DataField::CONTAINER_SLOTS => SLOTS,
            DataField::RECIPE_OUTPUT | DataField::RECIPE_COST | DataField::LOOT_STACK_SIZE => {
                Self::whole(1.0, 65535.0)
            }
            DataField::RECIPE_INGREDIENT | DataField::LOOT_COUNT => Self::whole(0.0, 65535.0),
            DataField::RECIPE_TIME | DataField::BURN_TIME => Self::fraction(0.0, 86400.0),
            DataField::DAMAGE_MOD => Self::fraction(0.0, 100.0),
            DataField::SPEED_MOD => Self::fraction(0.0, 10.0),
            DataField::ARMOR => Self::fraction(0.0, 1_000_000.0).factor_only(),
            DataField::ENERGY_OUTPUT => Self::whole(0.0, 1_000_000.0),
            DataField::LOOT_WEIGHT => Self::fraction(0.0, 1_000_000.0),
            DataField::LOOT_SPAWN_PROBABILITY => Self::fraction(0.0, 1.0),
            DataField::COMFORT_AMOUNT => Self::whole(0.0, 1000.0),
            DataField::RECIPE_STATION => Self::whole(0.0, 4_294_967_295.0).set_only(),
            _ => return None,
        })
    }

    /// Whether a raw change of a field with these rules reaches a target
    /// the loader takes: an item entry without a contained item, or one of
    /// the item selections.
    #[cfg(feature = "fake")]
    pub(crate) fn takes(self, raw: &DataChange) -> bool {
        let target = match raw.select {
            DataSelect::ENTRY => raw.item == Guid::NIL,
            select => select.selects_items(),
        };
        let value = match raw.op {
            ChangeOp::SET => Value::Set(raw.value),
            ChangeOp::TIMES => Value::Times(raw.value),
            _ => return false,
        };
        target && self.check(value).is_ok()
    }

    /// Whether the field takes `value`.
    pub(crate) fn check(self, value: Value) -> Result<(), Refusal> {
        match value {
            Value::Set(v) if !v.is_finite() || v < self.low || v > self.high => {
                Err(Refusal::OutOfRange(v))
            }
            Value::Set(_) if self.takes == Takes::FactorsOnly => Err(Refusal::FactorOnly),
            Value::Set(v) if self.step > 1.0 && (v / self.step).fract() != 0.0 => {
                Err(Refusal::NotInSteps(v))
            }
            Value::Set(v) if self.whole && v.fract() != 0.0 => Err(Refusal::NotWhole(v)),
            Value::Times(_) if self.takes == Takes::SetsOnly => Err(Refusal::SetOnly),
            Value::Times(f) if self.grows_only && f < 1.0 => Err(Refusal::Shrinks),
            Value::Times(f) if !f.is_finite() || !(0.0..=MAX_FACTOR).contains(&f) => {
                Err(Refusal::BadFactor(f))
            }
            _ => Ok(()),
        }
    }
}

/// Why [`Rules::check`] refuses a value, before it names the field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Refusal {
    OutOfRange(f64),
    FactorOnly,
    NotInSteps(f64),
    NotWhole(f64),
    SetOnly,
    Shrinks,
    BadFactor(f64),
}

impl Refusal {
    /// The error of `field`, whose rules refused the value.
    pub(crate) const fn of(self, field: Field) -> SetFieldError {
        let rules = field.rules();
        match self {
            Self::OutOfRange(value) => SetFieldError::OutOfRange {
                field,
                value,
                low: rules.low,
                high: rules.high,
            },
            Self::FactorOnly => SetFieldError::FactorOnly { field },
            Self::NotInSteps(value) => SetFieldError::NotInSteps {
                field,
                value,
                step: rules.step,
            },
            Self::NotWhole(value) => SetFieldError::NotWhole { field, value },
            Self::SetOnly => SetFieldError::SetOnly { field },
            Self::Shrinks => SetFieldError::Shrinks { field },
            Self::BadFactor(factor) => SetFieldError::BadFactor { factor },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(feature = "unstable", feature = "fake"))]
    fn slots_come_in_pages_of_eight_and_only_grow() {
        assert_eq!(SLOTS.check(Value::Set(48.0)), Ok(()));
        assert_eq!(
            SLOTS.check(Value::Set(50.0)),
            Err(Refusal::NotInSteps(50.0))
        );
        assert_eq!(SLOTS.check(Value::Times(0.5)), Err(Refusal::Shrinks));
        assert_eq!(
            SLOTS.check(Value::Set(136.0)),
            Err(Refusal::OutOfRange(136.0))
        );
    }

    #[test]
    fn every_field_has_rules_by_its_abi_id_and_no_other_id_does() {
        for field in Field::ALL {
            assert!(Rules::of(field.to_abi()).is_some(), "{field:?}");
        }
        assert_eq!(Rules::of(DataField::MAX_STACK), Some(MAX_STACK));
        assert_eq!(Rules::of(DataField::CONTAINER_SLOTS), Some(SLOTS));
        assert_eq!(Rules::of(DataField(0)), None);
    }

    #[test]
    #[cfg(feature = "fake")]
    fn a_raw_change_is_taken_for_an_item_entry_or_a_selection_with_a_value_in_range() {
        let raw = |select, item, op, value| {
            enshwick_abi::headed!(DataChange {
                entry: Guid::NIL,
                field: DataField::CONTAINER_SLOTS,
                op,
                value,
                source: crate::abi::Str::EMPTY,
                item,
                select,
                categories: 0,
            })
        };
        let set = ChangeOp::SET;
        assert!(SLOTS.takes(&raw(DataSelect::CONTAINERS, Guid::NIL, set, 48.0)));
        assert!(SLOTS.takes(&raw(DataSelect::ENTRY, Guid::NIL, set, 48.0)));
        assert!(!SLOTS.takes(&raw(DataSelect::ENTRY, Guid([1; 16]), set, 48.0)));
        assert!(!SLOTS.takes(&raw(DataSelect::RECIPE_KIND, Guid::NIL, set, 48.0)));
        assert!(!SLOTS.takes(&raw(DataSelect::CONTAINERS, Guid::NIL, set, 50.0)));
        assert!(!SLOTS.takes(&raw(
            DataSelect::CONTAINERS,
            Guid::NIL,
            ChangeOp::RESET,
            1.0
        )));
    }
}
