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
//! const WOOD: Guid = Guid::parse("3f4093ae-75ed-4e5d-bd0b-784ba83b88f5");
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.change(&Change::new(WOOD, Field::MaxStackSize, Value::Set(999.0)), "my-mod")
//! }
//! ```

use crate::abi::{ChangeOp, DataChange, DataField, DataSelect, Header, Str};
use crate::content::Guid;
use crate::{Load, Off};

mod items;

pub use items::{ItemTraits, Items, ItemsChange};

/// The largest factor a [`Value::Times`] takes.
pub const MAX_FACTOR: f64 = 100.0;

/// A vanilla value a [`Change`] changes; each says what its entry is.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Value};
///
/// assert!(Field::MaxStackSize.check(Value::Set(999.0)).is_ok());
/// assert!(Field::MaxStackSize.check(Value::Set(0.0)).is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Field {
    /// How many of an item one inventory slot holds, 1 to 65535; the entry
    /// is an item. The game cuts a saved stack above it, so a world saved
    /// while it was raised depends on the change.
    MaxStackSize,
    /// How many of each item a recipe makes, 1 to 65535; the entry is a
    /// recipe.
    Output,
    /// How many of each ingredient a recipe takes, 1 to 65535; the entry is
    /// a recipe.
    Cost,
    /// How many of one ingredient a recipe takes, 0 to 65535: 0 takes the
    /// ingredient out, and an item the recipe lacks is added. The entry is a
    /// recipe and [`Change::item`] the ingredient.
    Ingredient,
    /// How long a recipe takes, in seconds from 0 to 86400; the entry is a
    /// recipe.
    CraftingDuration,
    /// How many slots a container holds, 8 to 128 in steps of 8 and never
    /// fewer than it has; the entry is the item that places it, and the
    /// change reaches every item that places the same container. Containers
    /// placed before keep their size, which the save holds.
    Slots,
    /// A weapon's damage factor, `dmgMod`, from 0 to 100; the entry is an
    /// item that deals damage. Weapons the game sets up after the change
    /// deal it; the number a tooltip shows stays until the item is made.
    DamageMod,
    /// A weapon's attack speed factor, `speedMod`, from 0 to 10; the entry
    /// is an item that deals damage.
    SpeedMod,
    /// Every armour value of an item, scaled only, by a factor; the entry is
    /// an item that gives armour.
    Armor,
    /// How long an item burns as fuel, in seconds from 0 to 86400; the entry
    /// is a fuel item.
    BurnTime,
    /// The energy an item gives as fuel, 0 to 1000000; the entry is a fuel
    /// item.
    EnergyOutput,
    /// The comfort a placed item gives, 0 to 1000; the entry is an item
    /// that gives comfort.
    ComfortAmount,
    /// The stack size random loot rolls for an item, 1 to 65535: its least
    /// and its largest, with the ceiling the game scales a roll to kept in
    /// proportion; the entry is the item. Only the server rolls.
    StackSize,
    /// The count a container's fill table gives of an item, at least and at
    /// most alike, 0 to 65535; the entry is the item, in every table that
    /// lists it.
    Count,
    /// The chance a container's fill table gives an item, from 0 to 1; the
    /// entry is the item.
    SpawnProbability,
    /// The weight of an item among the ones a fill table picks from, 0 to
    /// 1000000; the entry is the item.
    Weight,
}

impl Field {
    /// Every field, for tools that list them.
    pub const ALL: &[Self] = &[
        Self::MaxStackSize,
        Self::Output,
        Self::Cost,
        Self::Ingredient,
        Self::CraftingDuration,
        Self::Slots,
        Self::DamageMod,
        Self::SpeedMod,
        Self::Armor,
        Self::BurnTime,
        Self::EnergyOutput,
        Self::ComfortAmount,
        Self::StackSize,
        Self::Count,
        Self::SpawnProbability,
        Self::Weight,
    ];

    /// The name a data pack writes, such as `max_stack`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::Field;
    ///
    /// assert_eq!(Field::MaxStackSize.name(), "max_stack");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::MaxStackSize => "max_stack",
            Self::Output => "output_count",
            Self::Cost => "cost",
            Self::Ingredient => "ingredient",
            Self::CraftingDuration => "time",
            Self::Slots => "slots",
            Self::DamageMod => "damage_mod",
            Self::SpeedMod => "speed_mod",
            Self::Armor => "armor",
            Self::BurnTime => "burn_time",
            Self::EnergyOutput => "energy_output",
            Self::ComfortAmount => "comfort_amount",
            Self::StackSize => "stack_size",
            Self::Count => "count",
            Self::SpawnProbability => "spawn_probability",
            Self::Weight => "weight",
        }
    }

    /// What kind of entry the field belongs to.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Entry, Field};
    ///
    /// assert_eq!(Field::MaxStackSize.entry(), Entry::Item);
    /// assert_eq!(Field::CraftingDuration.entry(), Entry::Recipe);
    /// ```
    #[must_use]
    pub const fn entry(self) -> Entry {
        match self {
            Self::MaxStackSize
            | Self::Slots
            | Self::DamageMod
            | Self::SpeedMod
            | Self::Armor
            | Self::BurnTime
            | Self::EnergyOutput
            | Self::ComfortAmount => Entry::Item,
            Self::StackSize | Self::Count | Self::SpawnProbability | Self::Weight => Entry::Loot,
            Self::Output | Self::Cost | Self::Ingredient | Self::CraftingDuration => Entry::Recipe,
        }
    }

    /// The smallest and the largest value the field holds.
    const fn range(self) -> (f64, f64) {
        match self {
            Self::MaxStackSize | Self::Output | Self::Cost | Self::StackSize => (1.0, 65535.0),
            Self::Ingredient | Self::Count => (0.0, 65535.0),
            Self::CraftingDuration | Self::BurnTime => (0.0, 86400.0),
            Self::Slots => (8.0, 128.0),
            Self::DamageMod => (0.0, 100.0),
            Self::SpeedMod => (0.0, 10.0),
            Self::Armor | Self::EnergyOutput | Self::Weight => (0.0, 1_000_000.0),
            Self::SpawnProbability => (0.0, 1.0),
            Self::ComfortAmount => (0.0, 1000.0),
        }
    }

    /// The step the field's values come in: 8 for slots, which come in
    /// pages of 8, else 1.
    const fn step(self) -> f64 {
        match self {
            Self::Slots => 8.0,
            _ => 1.0,
        }
    }

    /// Whether the field holds whole numbers only.
    const fn whole(self) -> bool {
        match self {
            Self::MaxStackSize
            | Self::Output
            | Self::Cost
            | Self::Ingredient
            | Self::Slots
            | Self::EnergyOutput
            | Self::ComfortAmount
            | Self::StackSize
            | Self::Count => true,
            Self::CraftingDuration
            | Self::DamageMod
            | Self::SpeedMod
            | Self::Armor
            | Self::BurnTime
            | Self::SpawnProbability
            | Self::Weight => false,
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
            Value::Set(_) if matches!(self, Self::Armor) => {
                Err(ValueError::FactorOnly { field: self })
            }
            Value::Set(v) if self.step() > 1.0 && (v / self.step()).fract() != 0.0 => {
                Err(ValueError::NotInSteps {
                    field: self,
                    value: v,
                    step: self.step(),
                })
            }
            Value::Set(v) if self.whole() && v.fract() != 0.0 => Err(ValueError::NotWhole {
                field: self,
                value: v,
            }),
            Value::Times(f) if matches!(self, Self::Slots) && f < 1.0 => {
                Err(ValueError::Shrinks { field: self })
            }
            Value::Times(f) if !f.is_finite() || !(0.0..=MAX_FACTOR).contains(&f) => {
                Err(ValueError::BadFactor { factor: f })
            }
            _ => Ok(()),
        }
    }

    /// The value in force: the last set of `values`, or `base` without one,
    /// times every factor; whole fields round half away from zero; the result
    /// stays in the field's range, and a factor other than 0 never takes a
    /// whole count of 1 or more below 1.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Field, Value};
    ///
    /// let values = [Value::Times(2.0), Value::Set(100.0), Value::Times(1.5)];
    /// assert_eq!(Field::MaxStackSize.resolve(20.0, values), 300.0);
    /// assert_eq!(Field::MaxStackSize.resolve(20.0, [Value::Times(0.0)]), 1.0);
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
        let start = set.unwrap_or(base);
        let value = start * factor;
        let value = if self.whole() {
            (value / self.step()).round() * self.step()
        } else {
            value
        };
        let (low, high) = self.range();
        let low = if self.whole() && start >= 1.0 && factor > 0.0 {
            low.max(1.0)
        } else {
            low
        };
        value.clamp(low, high)
    }

    /// The field's number in the ABI.
    #[must_use]
    pub const fn to_abi(self) -> DataField {
        match self {
            Self::MaxStackSize => DataField::MAX_STACK,
            Self::Output => DataField::RECIPE_OUTPUT,
            Self::Cost => DataField::RECIPE_COST,
            Self::Ingredient => DataField::RECIPE_INGREDIENT,
            Self::CraftingDuration => DataField::RECIPE_TIME,
            Self::Slots => DataField::CONTAINER_SLOTS,
            Self::DamageMod => DataField::DAMAGE_MOD,
            Self::SpeedMod => DataField::SPEED_MOD,
            Self::Armor => DataField::ARMOR,
            Self::BurnTime => DataField::BURN_TIME,
            Self::EnergyOutput => DataField::ENERGY_OUTPUT,
            Self::ComfortAmount => DataField::COMFORT_AMOUNT,
            Self::StackSize => DataField::LOOT_STACK_SIZE,
            Self::Count => DataField::LOOT_COUNT,
            Self::SpawnProbability => DataField::LOOT_SPAWN_PROBABILITY,
            Self::Weight => DataField::LOOT_WEIGHT,
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
    /// assert_eq!(Field::from_abi(DataField::MAX_STACK), Some(Field::MaxStackSize));
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

/// The kind of entry a [`Field`] belongs to.
///
/// # Examples
///
/// See [`Field::entry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Entry {
    /// An item.
    Item,
    /// A recipe.
    Recipe,
    /// An item in the game's loot, which only the server rolls.
    Loot,
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
/// let error = Field::MaxStackSize.check(Value::Set(70000.0)).unwrap_err();
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
    /// A factor below 1 for a field that only grows.
    #[error("{field} only grows: a chest a player upgrades moves every item it holds")]
    Shrinks {
        /// The field.
        field: Field,
    },
    /// A set for a field that takes only a factor.
    #[error("{field} takes only a factor: {{ times = x }}")]
    FactorOnly {
        /// The field.
        field: Field,
    },
    /// A value between the field's steps.
    #[error("{field} {value} is not a multiple of {step}")]
    NotInSteps {
        /// The field.
        field: Field,
        /// The value asked for.
        value: f64,
        /// The step.
        step: f64,
    },
    /// A selection of items for a field it does not reach.
    #[error("{field} is no field a selection of items changes")]
    NotSelectable {
        /// The field.
        field: Field,
    },
    /// An item for a field that names none, or none for one that does.
    #[error("{field} {}", if *named { "names no item" } else { "needs an item" })]
    Item {
        /// The field.
        field: Field,
        /// Whether the change named an item.
        named: bool,
    },
}

/// One change of a vanilla value: the entry, the field, the item inside the
/// entry where the field names one, and the value.
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
    /// The item inside the entry, for [`Field::Ingredient`].
    pub item: Option<Guid>,
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
            item: None,
            value,
        }
    }

    /// The change of how many of `item` the recipe `recipe` takes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::changes::{Change, Field, Value};
    ///
    /// let change = Change::ingredient(Guid([1; 16]), Guid([2; 16]), Value::Set(0.0));
    /// assert_eq!((change.field, change.item), (Field::Ingredient, Some(Guid([2; 16]))));
    /// assert!(change.check().is_ok());
    /// ```
    #[must_use]
    pub const fn ingredient(recipe: Guid, item: Guid, value: Value) -> Self {
        Self {
            entry: recipe,
            field: Field::Ingredient,
            item: Some(item),
            value,
        }
    }

    /// Whether the change is one the loader takes: its value as
    /// [`Field::check`] checks it, and an item exactly where the field names
    /// one.
    ///
    /// # Errors
    ///
    /// [`ValueError`] saying why not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::changes::{Change, Field, Value};
    ///
    /// let change = Change::new(Guid([1; 16]), Field::Ingredient, Value::Set(2.0));
    /// assert_eq!(change.check().unwrap_err().to_string(), "ingredient needs an item");
    /// ```
    pub fn check(&self) -> Result<(), ValueError> {
        let wants_item = self.field == Field::Ingredient;
        if wants_item != self.item.is_some() {
            return Err(ValueError::Item {
                field: self.field,
                named: self.item.is_some(),
            });
        }
        self.field.check(self.value)
    }

    /// The change an ABI record describes, checked as [`Change::check`]
    /// checks it; `None` for a field or operation this crate does not know,
    /// or a record that selects items ([`ItemsChange::from_abi`]).
    ///
    /// # Errors
    ///
    /// [`ValueError`] for a value the field does not take.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ChangeOp, DataChange, DataField, DataSelect, Header, Str};
    /// use enshwick::content::changes::{Change, Field, Value};
    ///
    /// let raw = DataChange {
    ///     header: Header::of::<DataChange>(),
    ///     entry: [1; 16],
    ///     field: DataField::MAX_STACK,
    ///     op: ChangeOp::TIMES,
    ///     value: 4.0,
    ///     source: Str::EMPTY,
    ///     item: [0; 16],
    ///     select: DataSelect::ENTRY,
    ///     categories: 0,
    /// };
    /// let change = Change::from_abi(&raw)?.ok_or("unknown")?;
    /// assert_eq!(change.value, Value::Times(4.0));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_abi(raw: &DataChange) -> Result<Option<Self>, ValueError> {
        if raw.select != DataSelect::ENTRY {
            return Ok(None);
        }
        let Some((field, value)) = field_and_value(raw) else {
            return Ok(None);
        };
        let change = Self {
            entry: Guid(raw.entry),
            field,
            item: (raw.item != [0; 16]).then_some(Guid(raw.item)),
            value,
        };
        change.check()?;
        Ok(Some(change))
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
            item: self.item.map_or([0; 16], |item| item.0),
            select: DataSelect::ENTRY,
            categories: 0,
        }
    }
}

/// The field and the value of an ABI record; `None` for a field or an
/// operation this crate does not know.
fn field_and_value(raw: &DataChange) -> Option<(Field, Value)> {
    let value = match raw.op {
        ChangeOp::SET => Value::Set(raw.value),
        ChangeOp::TIMES => Value::Times(raw.value),
        _ => return None,
    };
    Some((Field::from_abi(raw.field)?, value))
}

impl Load<'_> {
    /// Declares `change` during `init`, from `source`, such as
    /// `items.toml:12`, which the conflict reports name. The loader merges it
    /// with every mod's changes as the [module](self) says, and applies it on
    /// this side; a value players' games read needs the same change on
    /// every side, so such a mod is a join mod. A mod that raises a stack
    /// size keeps a [`Hold`](crate::state::Hold) of its own for the worlds
    /// it saves; the loader holds the game for data packs only.
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
            .check()
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
        assert_eq!(Field::MaxStackSize.resolve(5.0, a), 180.0);
        assert_eq!(Field::MaxStackSize.resolve(5.0, b), 180.0);
    }

    #[test]
    fn a_whole_field_rounds_half_away_from_zero_and_stays_in_range() {
        assert_eq!(Field::MaxStackSize.resolve(5.0, [Value::Times(0.5)]), 3.0);
        assert_eq!(
            Field::MaxStackSize.resolve(60000.0, [Value::Times(2.0)]),
            65535.0
        );
        assert_eq!(Field::MaxStackSize.resolve(1.0, [Value::Times(0.1)]), 1.0);
    }

    #[test]
    fn no_value_keeps_the_base() {
        assert_eq!(Field::MaxStackSize.resolve(20.0, []), 20.0);
    }

    #[test]
    fn fractions_and_bad_factors_are_refused() {
        assert!(Field::MaxStackSize.check(Value::Set(2.5)).is_err());
        assert!(Field::MaxStackSize.check(Value::Times(f64::NAN)).is_err());
        assert!(Field::MaxStackSize.check(Value::Times(101.0)).is_err());
        assert!(Field::MaxStackSize.check(Value::Times(0.0)).is_ok());
    }

    #[test]
    fn every_field_goes_through_the_abi_and_back() {
        for &field in Field::ALL {
            assert_eq!(Field::from_abi(field.to_abi()), Some(field));
        }
    }

    #[test]
    fn an_ingredient_goes_through_the_abi_and_back() {
        let change = Change::ingredient(Guid([7; 16]), Guid([8; 16]), Value::Times(2.0));
        let raw = change.to_abi("recipes.toml:3");
        assert_eq!(Change::from_abi(&raw), Ok(Some(change)));
    }

    #[test]
    fn a_field_without_an_item_refuses_one() {
        let mut change = Change::new(Guid([7; 16]), Field::Output, Value::Set(2.0));
        change.item = Some(Guid([8; 16]));
        assert_eq!(
            change.check().unwrap_err().to_string(),
            "output_count names no item"
        );
    }

    #[test]
    fn a_count_of_one_or_more_stays_one_or_more_when_scaled_down() {
        assert_eq!(Field::Ingredient.resolve(1.0, [Value::Times(0.1)]), 1.0);
        assert_eq!(Field::Ingredient.resolve(3.0, [Value::Set(0.0)]), 0.0);
        assert_eq!(Field::Ingredient.resolve(3.0, [Value::Times(0.0)]), 0.0);
        assert_eq!(
            Field::CraftingDuration.resolve(60.0, [Value::Times(0.25)]),
            15.0
        );
    }

    #[test]
    fn slots_come_in_pages_of_eight() {
        assert!(Field::Slots.check(Value::Set(48.0)).is_ok());
        assert_eq!(
            Field::Slots
                .check(Value::Set(50.0))
                .unwrap_err()
                .to_string(),
            "slots 50 is not a multiple of 8"
        );
        assert!(Field::Slots.check(Value::Set(136.0)).is_err());
        assert_eq!(Field::Slots.resolve(24.0, [Value::Times(1.5)]), 40.0);
        assert_eq!(Field::Slots.resolve(72.0, [Value::Times(2.0)]), 128.0);
        assert!(Field::Slots.check(Value::Times(0.5)).is_err());
    }

    #[test]
    fn armor_takes_only_a_factor() {
        assert_eq!(
            Field::Armor
                .check(Value::Set(10.0))
                .unwrap_err()
                .to_string(),
            "armor takes only a factor: { times = x }"
        );
        assert!(Field::Armor.check(Value::Times(1.5)).is_ok());
        assert_eq!(Field::DamageMod.resolve(1.25, [Value::Times(2.0)]), 2.5);
    }

    #[test]
    fn a_change_goes_through_the_abi_and_back() {
        let change = Change::new(Guid([7; 16]), Field::MaxStackSize, Value::Set(50.0));
        let raw = change.to_abi("items.toml:3");
        assert_eq!(Change::from_abi(&raw), Ok(Some(change)));
    }
}
