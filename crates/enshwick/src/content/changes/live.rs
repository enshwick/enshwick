//! Changes a mod declares at init and sets any time after, such as a stack
//! size from a setting that changes per world.

use crate::abi::{ChangeId, ChangeOp, DataChange, DataSelect, Header, Status, Str};
use crate::content::Guid;
use crate::{Host, Load, Off};

use super::{Field, Items, Value, ValueError};

/// What a change a mod sets later reaches.
///
/// # Examples
///
/// See [`ChangeHandle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Target {
    /// One vanilla entry, such as an item's GUID.
    Entry(Guid),
    /// Every vanilla item a selection picks.
    Items(Items),
}

impl Target {
    /// The ABI record that declares a change of `field` here, from
    /// `source`.
    fn to_abi(self, field: Field, source: &str) -> DataChange {
        let (entry, select, categories) = match self {
            Self::Entry(entry) => (entry.0, DataSelect::ENTRY, 0),
            Self::Items(items) => {
                let (select, categories) = items.to_abi();
                ([0; 16], select, categories)
            }
        };
        DataChange {
            header: Header::of::<DataChange>(),
            entry,
            field: field.to_abi(),
            op: ChangeOp::TIMES,
            value: 1.0,
            source: Str::new(source),
            item: [0; 16],
            select,
            categories,
        }
    }

    /// Whether a change of `field` may reach this target.
    fn check(self, field: Field) -> Result<(), ValueError> {
        match self {
            Self::Items(_) if !field.selectable() => Err(ValueError::NotSelectable { field }),
            _ if field == Field::Ingredient => Err(ValueError::NotLive { field }),
            _ => Ok(()),
        }
    }
}

/// Why [`ChangeHandle::set`] did not take a value.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, SetChangeError, Value};
///
/// let why = Field::MaxStackSize.check(Value::Set(0.0)).unwrap_err();
/// assert_eq!(SetChangeError::Value(why).to_string(), "max_stack 0 is not from 1 to 65535");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SetChangeError {
    /// The field does not take the value.
    #[error(transparent)]
    Value(#[from] ValueError),
    /// The loader refused it: the mod is off.
    #[error("the loader refused the value")]
    Refused,
    /// The value takes a restart: what it reaches does not build again in
    /// place. The value is dropped and the old one stays in force.
    #[error("the value takes a restart")]
    Restart,
}

/// A change of a vanilla value whose value the mod sets after init, any
/// number of times; from [`Load::declare_change`].
///
/// Until [`set`](Self::set) the mod changes nothing. Each set merges again
/// with every mod's and pack's changes by the same rule as
/// [`Load::change`], and the loader builds what the change reaches anew in
/// place, so the game reads the new value from then on.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{ChangeHandle, Field, Items, Target, Value};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<ChangeHandle, enshwick::Off> {
///     let stacks = load.declare_change(Target::Items(Items::Stackable), Field::MaxStackSize, "stacks")?;
///     stacks.set(Value::Times(4.0)).map_err(|why| enshwick::Off::new(why.to_string()))?;
///     Ok(stacks)
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ChangeHandle {
    host: Host,
    id: ChangeId,
    field: Field,
}

impl ChangeHandle {
    /// The field this handle changes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{ChangeHandle, Field};
    ///
    /// fn stacks(handle: &ChangeHandle) -> bool {
    ///     handle.field() == Field::MaxStackSize
    /// }
    /// ```
    #[must_use]
    pub const fn field(&self) -> Field {
        self.field
    }

    /// Puts `value` in force as this mod's value. Any thread; it builds
    /// resources again, so it suits a settings change, not a frame.
    ///
    /// # Errors
    ///
    /// [`SetChangeError::Value`] for a value the field does not take,
    /// [`SetChangeError::Restart`] when it takes a restart,
    /// [`SetChangeError::Refused`] when the loader refuses it.
    ///
    /// # Examples
    ///
    /// See [`ChangeHandle`].
    pub fn set(&self, value: Value) -> Result<(), SetChangeError> {
        self.field.check(value)?;
        let (op, raw) = match value {
            Value::Set(v) => (ChangeOp::SET, v),
            Value::Times(v) => (ChangeOp::TIMES, v),
        };
        self.send(op, raw)
    }

    /// Takes this mod's value out, as if it never set one.
    ///
    /// # Errors
    ///
    /// [`SetChangeError::Restart`] when it takes a restart,
    /// [`SetChangeError::Refused`] when the loader refuses it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{ChangeHandle, SetChangeError};
    ///
    /// fn vanilla(handle: &ChangeHandle) -> Result<(), SetChangeError> {
    ///     handle.reset()
    /// }
    /// ```
    pub fn reset(&self) -> Result<(), SetChangeError> {
        self.send(ChangeOp::RESET, 0.0)
    }

    fn send(&self, op: ChangeOp, value: f64) -> Result<(), SetChangeError> {
        match (self.host.api().set_change)(self.host.handle(), self.id, op, value) {
            Status::OK => Ok(()),
            Status::TOO_LATE => Err(SetChangeError::Restart),
            _ => Err(SetChangeError::Refused),
        }
    }
}

impl Load<'_> {
    /// Declares during `init` a change of `field` of `target`, from
    /// `source`, whose value the mod sets later through the handle. The
    /// loader builds what it reaches from the first read on, so a set after
    /// init needs no restart. A field players' games read needs the same
    /// value on every side: only a mod with `join: true` declares one, and
    /// it sets the value from a synced setting.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field the target does not take (an ingredient, whose
    /// change alters a recipe's length, or a field a selection does not
    /// reach), for a field clients read in a mod without `join: true`, after
    /// init, and when the running build does not reflect the field.
    ///
    /// # Examples
    ///
    /// See [`ChangeHandle`].
    pub fn declare_change(
        &mut self,
        target: Target,
        field: Field,
        source: &str,
    ) -> Result<ChangeHandle, Off> {
        let what = || format!("the change of {field} of {target:?}");
        target
            .check(field)
            .map_err(|why| Off::new(format!("{}: {why}", what())))?;
        if field.seen_by_clients() && !self.joins() {
            return Err(Off::new(format!(
                "{}: players' games read it, so only a mod with `join: true` declares it",
                what()
            )));
        }
        let raw = target.to_abi(field, source);
        let host = self.host();
        let mut id = ChangeId(0);
        let status = (host.api().declare_change)(host.handle(), &raw const raw, &raw mut id);
        crate::load::ok(status, ChangeHandle { host, id, field }, what)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ingredient_is_no_change_a_mod_sets_later() {
        let target = Target::Entry(Guid([1; 16]));
        assert_eq!(
            target.check(Field::Ingredient),
            Err(ValueError::NotLive {
                field: Field::Ingredient
            })
        );
        assert!(target.check(Field::Output).is_ok());
        assert!(
            Target::Items(Items::Stackable)
                .check(Field::Output)
                .is_err()
        );
    }

    #[test]
    fn a_target_names_its_entry_or_its_selection_in_the_abi() {
        let one = Target::Entry(Guid([3; 16])).to_abi(Field::MaxStackSize, "x");
        assert_eq!((one.entry, one.select), ([3; 16], DataSelect::ENTRY));
        let many = Target::Items(Items::Containers).to_abi(Field::Slots, "x");
        assert_eq!((many.entry, many.select), ([0; 16], DataSelect::CONTAINERS));
    }
}
