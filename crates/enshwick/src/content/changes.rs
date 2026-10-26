//! Changes of vanilla values, such as how many of an item one slot holds,
//! which the loader merges across every mod and data pack.
//!
//! A mod declares a [`Change`] at init with [`Load::change`]. The loader
//! takes every mod's sets in load order, the last one winning and each
//! overridden one reported, then multiplies every factor, and builds the
//! resource the first time the game reads it.
//!
//! ```
//! use enshwick::content::Guid;
//! use enshwick::content::changes::{Change, Field, Value};
//!
//! const WOOD: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.change(&Change::new(WOOD, Field::MaxStack, Value::Set(999.0)), "my-mod")
//! }
//! ```

use crate::abi::{ChangeOp, DataChange, DataField, Header, Str};
use crate::content::Guid;
use crate::{Load, Off};

/// The largest factor a [`Value::Times`] takes.
pub const MAX_FACTOR: f64 = 100.0;

/// A vanilla value a [`Change`] changes; each says what its entry is.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Value};
///
/// assert!(Field::MaxStack.check(Value::Set(999.0)).is_ok());
/// assert!(Field::MaxStack.check(Value::Set(0.0)).is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Field {
    /// How many of an item one inventory slot holds, 1 to 65535; the entry
    /// is an item. The game cuts a saved stack above it, so a world saved
    /// while it was raised depends on the change.
    MaxStack,
}

impl Field {
    /// Every field, for tools that list them.
    pub const ALL: &[Self] = &[Self::MaxStack];

    /// The name a data pack writes, such as `max_stack`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::Field;
    ///
    /// assert_eq!(Field::MaxStack.name(), "max_stack");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MaxStack => "max_stack",
        }
    }

    /// The smallest and the largest value the field holds.
    const fn range(self) -> (f64, f64) {
        match self {
            Self::MaxStack => (1.0, 65535.0),
        }
    }

    /// Whether the field holds whole numbers only.
    const fn whole(self) -> bool {
        match self {
            Self::MaxStack => true,
        }
    }

    /// Whether `value` is one the field takes: a set inside its range, whole
    /// where the field is, or a finite factor from 0 to [`MAX_FACTOR`].
    ///
    /// # Errors
    ///
    /// [`ValueError`] saying why not.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    pub fn check(self, value: Value) -> Result<(), ValueError> {
        let (low, high) = self.range();
        match value {
            Value::Set(v) if !v.is_finite() || v < low || v > high => Err(ValueError::OutOfRange {
                field: self,
                value: v,
                low,
                high,
            }),
            Value::Set(v) if self.whole() && v.fract() != 0.0 => Err(ValueError::NotWhole {
                field: self,
                value: v,
            }),
            Value::Times(f) if !f.is_finite() || !(0.0..=MAX_FACTOR).contains(&f) => {
                Err(ValueError::BadFactor { factor: f })
            }
            _ => Ok(()),
        }
    }

    /// The value in force: the last set of `values`, or `base` without one,
    /// times every factor; whole fields round half away from zero; the result
    /// stays in the field's range, and a factor never takes it below its
    /// smallest value.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Field, Value};
    ///
    /// let values = [Value::Times(2.0), Value::Set(100.0), Value::Times(1.5)];
    /// assert_eq!(Field::MaxStack.resolve(20.0, values), 300.0);
    /// assert_eq!(Field::MaxStack.resolve(20.0, [Value::Times(0.0)]), 1.0);
    /// ```
    #[must_use]
    pub fn resolve(self, base: f64, values: impl IntoIterator<Item = Value>) -> f64 {
        let (mut set, mut factor) = (None, 1.0_f64);
        for value in values {
            match value {
                Value::Set(v) => set = Some(v),
                Value::Times(f) => factor *= f,
            }
        }
        let value = set.unwrap_or(base) * factor;
        let value = if self.whole() { value.round() } else { value };
        let (low, high) = self.range();
        value.clamp(low, high)
    }

    /// The field's number in the ABI.
    #[must_use]
    pub const fn to_abi(self) -> DataField {
        match self {
            Self::MaxStack => DataField::MAX_STACK,
        }
    }

    /// The field an ABI number names; `None` for one this crate does not
    /// know.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::DataField;
    /// use enshwick::content::changes::Field;
    ///
    /// assert_eq!(Field::from_abi(DataField::MAX_STACK), Some(Field::MaxStack));
    /// assert_eq!(Field::from_abi(DataField(0)), None);
    /// ```
    #[must_use]
    pub fn from_abi(field: DataField) -> Option<Self> {
        Self::ALL.iter().copied().find(|f| f.to_abi() == field)
    }
}

impl core::fmt::Display for Field {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// How a [`Change`] changes its value.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::Value;
///
/// assert_eq!(Value::Times(2.0).to_string(), "times 2");
/// assert_eq!(Value::Set(999.0).to_string(), "999");
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// The value becomes this.
    Set(f64),
    /// The value is multiplied by this.
    Times(f64),
}

impl core::fmt::Display for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Set(v) => write!(f, "{v}"),
            Self::Times(v) => write!(f, "times {v}"),
        }
    }
}

/// Why a [`Field`] does not take a [`Value`].
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Value};
///
/// let error = Field::MaxStack.check(Value::Set(70000.0)).unwrap_err();
/// assert_eq!(error.to_string(), "max_stack 70000 is not from 1 to 65535");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ValueError {
    /// A set outside the field's range.
    #[error("{field} {value} is not from {low} to {high}")]
    OutOfRange {
        /// The field.
        field: Field,
        /// The value asked for.
        value: f64,
        /// The smallest value it holds.
        low: f64,
        /// The largest value it holds.
        high: f64,
    },
    /// A fraction for a field of whole numbers.
    #[error("{field} {value} is no whole number")]
    NotWhole {
        /// The field.
        field: Field,
        /// The value asked for.
        value: f64,
    },
    /// A factor that is not finite or outside 0 to [`MAX_FACTOR`].
    #[error("factor {factor} is not from 0 to 100")]
    BadFactor {
        /// The factor asked for.
        factor: f64,
    },
}

/// One change of a vanilla value: the entry, the field and the value.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Change {
    /// The vanilla entry, such as an item's GUID.
    pub entry: Guid,
    /// What changes.
    pub field: Field,
    /// How.
    pub value: Value,
}

impl Change {
    /// The change of `field` of `entry` by `value`.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(entry: Guid, field: Field, value: Value) -> Self {
        Self {
            entry,
            field,
            value,
        }
    }

    /// The change an ABI record describes, checked as [`Field::check`]
    /// checks it; `None` for a field or operation this crate does not know.
    ///
    /// # Errors
    ///
    /// [`ValueError`] for a value the field does not take.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ChangeOp, DataChange, DataField, Header, Str};
    /// use enshwick::content::changes::{Change, Field, Value};
    ///
    /// let raw = DataChange {
    ///     header: Header::of::<DataChange>(),
    ///     entry: [1; 16],
    ///     field: DataField::MAX_STACK,
    ///     op: ChangeOp::TIMES,
    ///     value: 4.0,
    ///     source: Str::EMPTY,
    /// };
    /// let change = Change::from_abi(&raw)?.ok_or("unknown")?;
    /// assert_eq!(change.value, Value::Times(4.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_abi(raw: &DataChange) -> Result<Option<Self>, ValueError> {
        let value = match raw.op {
            ChangeOp::SET => Value::Set(raw.value),
            ChangeOp::TIMES => Value::Times(raw.value),
            _ => return Ok(None),
        };
        let Some(field) = Field::from_abi(raw.field) else {
            return Ok(None);
        };
        field.check(value)?;
        Ok(Some(Self::new(Guid(raw.entry), field, value)))
    }

    /// The ABI record of the change, with `source` borrowed into it.
    #[must_use]
    fn to_abi(self, source: &str) -> DataChange {
        let (op, value) = match self.value {
            Value::Set(v) => (ChangeOp::SET, v),
            Value::Times(v) => (ChangeOp::TIMES, v),
        };
        DataChange {
            header: Header::of::<DataChange>(),
            entry: self.entry.0,
            field: self.field.to_abi(),
            op,
            value,
            source: Str::new(source),
        }
    }
}

impl Load<'_> {
    /// Declares `change` during `init`, from `source`, such as
    /// `items.toml:12`, which the conflict reports name. The loader merges it
    /// with every mod's changes as the [module](self) says, and applies it on
    /// this side; a value players' games read needs the same change on
    /// every side, so such a mod is a join mod.
    ///
    /// # Errors
    ///
    /// [`Off`] for a value the field does not take, after init, and when the
    /// running build does not reflect the field.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn change(&mut self, change: &Change, source: &str) -> Result<(), Off> {
        change
            .field
            .check(change.value)
            .map_err(|why| Off::new(format!("change of {}: {why}", change.entry)))?;
        let raw = change.to_abi(source);
        let api = self.host().api();
        let status = (api.change_data)(self.host().handle(), &raw const raw);
        crate::load::ok(status, (), || {
            format!("change {} of {}", change.field, change.entry)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_set_wins_and_factors_multiply_whatever_their_order() {
        let a = [
            Value::Times(2.0),
            Value::Set(10.0),
            Value::Set(30.0),
            Value::Times(3.0),
        ];
        let b = [
            Value::Set(10.0),
            Value::Times(3.0),
            Value::Set(30.0),
            Value::Times(2.0),
        ];
        assert_eq!(Field::MaxStack.resolve(5.0, a), 180.0);
        assert_eq!(Field::MaxStack.resolve(5.0, b), 180.0);
    }

    #[test]
    fn a_whole_field_rounds_half_away_from_zero_and_stays_in_range() {
        assert_eq!(Field::MaxStack.resolve(5.0, [Value::Times(0.5)]), 3.0);
        assert_eq!(
            Field::MaxStack.resolve(60000.0, [Value::Times(2.0)]),
            65535.0
        );
        assert_eq!(Field::MaxStack.resolve(1.0, [Value::Times(0.1)]), 1.0);
    }

    #[test]
    fn no_value_keeps_the_base() {
        assert_eq!(Field::MaxStack.resolve(20.0, []), 20.0);
    }

    #[test]
    fn fractions_and_bad_factors_are_refused() {
        assert!(Field::MaxStack.check(Value::Set(2.5)).is_err());
        assert!(Field::MaxStack.check(Value::Times(f64::NAN)).is_err());
        assert!(Field::MaxStack.check(Value::Times(101.0)).is_err());
        assert!(Field::MaxStack.check(Value::Times(0.0)).is_ok());
    }

    #[test]
    fn every_field_goes_through_the_abi_and_back() {
        for &field in Field::ALL {
            assert_eq!(Field::from_abi(field.to_abi()), Some(field));
        }
    }

    #[test]
    fn a_change_goes_through_the_abi_and_back() {
        let change = Change::new(Guid([7; 16]), Field::MaxStack, Value::Set(50.0));
        let raw = change.to_abi("items.toml:3");
        assert_eq!(Change::from_abi(&raw), Ok(Some(change)));
    }
}
