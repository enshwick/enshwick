//! Changes of one field of every vanilla item a selection picks, which the
//! loader turns into one change per item from the game's data.

use crate::abi::{ChangeOp, DataChange, DataSelect, Header, ItemData, Str};
use crate::content::Guid;
use crate::content::categories::ItemCategoryMask;
use crate::{Load, Off};

use super::{Change, Field, Value, ValueError, field_and_value};

/// The vanilla items an [`ItemsChange`] reaches at once.
///
/// A stack size reaches only items one slot already holds more than one of
/// and that keep no state of their own, such as durability; container slots
/// reach only items that place a container or are backpacks, and
/// [`Items::Containers`] and [`Items::Backpacks`] pick one kind alone.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, ItemTraits, Items};
///
/// let wood = ItemTraits { max_stack: 20, category: 9, flags: 0, slots: None, backpack_slots: None };
/// assert!(Items::Stackable.picks(Field::MaxStackSize, &wood));
/// assert!(!Items::Containers.picks(Field::Slots, &wood));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Items {
    /// Every item of these categories.
    Category(ItemCategoryMask),
    /// Every item one slot holds more than one of.
    Stackable,
    /// Every item that places a container.
    Containers,
    /// Every backpack: an item worn in the game's `Backpack` equipment slot,
    /// which carries an inventory of its own.
    Backpacks,
}

/// What a selection reads of one vanilla item, from the game's data.
///
/// # Examples
///
/// See [`Items`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemTraits {
    /// How many of it one slot holds.
    pub max_stack: u16,
    /// The number of its `ItemCategory`.
    pub category: u8,
    /// Its flags, such as [`ItemData::HAS_PIDE`].
    pub flags: u32,
    /// How many slots the container it places holds; `None` when it places
    /// none.
    pub slots: Option<u8>,
    /// How many slots it holds as a backpack; `None` for an item that is no
    /// backpack.
    pub backpack_slots: Option<u8>,
}

impl Items {
    /// Whether a change of `field` through this selection reaches `item`.
    ///
    /// # Examples
    ///
    /// See [`Items`].
    #[must_use]
    pub const fn picks(self, field: Field, item: &ItemTraits) -> bool {
        let selected = match self {
            Self::Category(mask) => mask.holds(item.category),
            Self::Stackable => item.max_stack > 1,
            Self::Containers => item.slots.is_some(),
            Self::Backpacks => item.backpack_slots.is_some(),
        };
        selected
            && match field {
                Field::MaxStackSize => item.max_stack > 1 && item.flags & ItemData::HAS_PIDE == 0,
                Field::Slots => match self {
                    Self::Containers => item.slots.is_some(),
                    Self::Backpacks => item.backpack_slots.is_some(),
                    _ => item.slots.is_some() || item.backpack_slots.is_some(),
                },
                _ => false,
            }
    }

    /// The selection's number in the ABI and its categories.
    pub(super) const fn to_abi(self) -> (DataSelect, u32) {
        match self {
            Self::Category(mask) => (DataSelect::CATEGORY, mask.0),
            Self::Stackable => (DataSelect::STACKABLE, 0),
            Self::Containers => (DataSelect::CONTAINERS, 0),
            Self::Backpacks => (DataSelect::BACKPACKS, 0),
        }
    }

    /// The selection an ABI record names; `None` for [`DataSelect::ENTRY`]
    /// and a selection this crate does not know.
    const fn from_abi(select: DataSelect, categories: u32) -> Option<Self> {
        match select {
            DataSelect::CATEGORY => Some(Self::Category(ItemCategoryMask(categories))),
            DataSelect::STACKABLE => Some(Self::Stackable),
            DataSelect::CONTAINERS => Some(Self::Containers),
            DataSelect::BACKPACKS => Some(Self::Backpacks),
            _ => None,
        }
    }
}

impl Field {
    /// Whether a selection of items changes this field: a stack size or
    /// container slots.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::Field;
    ///
    /// assert!(Field::MaxStackSize.selectable());
    /// assert!(!Field::Output.selectable());
    /// ```
    #[must_use]
    pub const fn selectable(self) -> bool {
        matches!(self, Self::MaxStackSize | Self::Slots)
    }
}

/// One change of a field of every vanilla item a selection picks.
///
/// # Examples
///
/// ```
/// use enshwick::content::changes::{Field, Items, ItemsChange, Value};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let stacks = ItemsChange::new(Items::Stackable, Field::MaxStackSize, Value::Times(4.0));
///     load.change_items(&stacks, "my-mod")
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemsChange {
    /// The items.
    pub items: Items,
    /// What changes.
    pub field: Field,
    /// How.
    pub value: Value,
}

impl ItemsChange {
    /// The change of `field` of every item `items` picks by `value`.
    ///
    /// # Examples
    ///
    /// See [`ItemsChange`].
    #[must_use]
    pub const fn new(items: Items, field: Field, value: Value) -> Self {
        Self {
            items,
            field,
            value,
        }
    }

    /// Whether the loader takes the change: a field a selection changes,
    /// and a value [`Field::check`] takes.
    ///
    /// # Errors
    ///
    /// [`ValueError`] saying why not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Field, Items, ItemsChange, Value};
    ///
    /// let output = ItemsChange::new(Items::Stackable, Field::Output, Value::Set(2.0));
    /// assert_eq!(
    ///     output.check().unwrap_err().to_string(),
    ///     "output_count is no field a selection of items changes"
    /// );
    /// ```
    pub fn check(&self) -> Result<(), ValueError> {
        if !self.field.selectable() {
            return Err(ValueError::NotSelectable { field: self.field });
        }
        self.field.check(self.value)
    }

    /// The same change of the one item `item`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::content::changes::{Field, Items, ItemsChange, Value};
    ///
    /// let stacks = ItemsChange::new(Items::Stackable, Field::MaxStackSize, Value::Times(2.0));
    /// assert_eq!(stacks.of(Guid([7; 16])).entry, Guid([7; 16]));
    /// ```
    #[must_use]
    pub const fn of(&self, item: Guid) -> Change {
        Change::new(item, self.field, self.value)
    }

    /// The change an ABI record describes, checked as
    /// [`ItemsChange::check`] checks it; `None` for a record that names one
    /// entry, or a selection, field or operation this crate does not know.
    ///
    /// # Errors
    ///
    /// [`ValueError`] for a field or value the selection does not take.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{ChangeOp, DataChange, DataField, DataSelect, Header, Str};
    /// use enshwick::content::changes::{Items, ItemsChange, Value};
    ///
    /// let raw = DataChange {
    ///     header: Header::of::<DataChange>(),
    ///     entry: [0; 16],
    ///     field: DataField::MAX_STACK,
    ///     op: ChangeOp::TIMES,
    ///     value: 4.0,
    ///     source: Str::EMPTY,
    ///     item: [0; 16],
    ///     select: DataSelect::STACKABLE,
    ///     categories: 0,
    /// };
    /// let change = ItemsChange::from_abi(&raw)?.ok_or("unknown")?;
    /// assert_eq!((change.items, change.value), (Items::Stackable, Value::Times(4.0)));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_abi(raw: &DataChange) -> Result<Option<Self>, ValueError> {
        let Some(items) = Items::from_abi(raw.select, raw.categories) else {
            return Ok(None);
        };
        let Some((field, value)) = field_and_value(raw) else {
            return Ok(None);
        };
        let change = Self::new(items, field, value);
        change.check()?;
        Ok(Some(change))
    }

    /// The ABI record of the change, with `source` borrowed into it.
    fn to_abi(self, source: &str) -> DataChange {
        let (op, value) = match self.value {
            Value::Set(v) => (ChangeOp::SET, v),
            Value::Times(v) => (ChangeOp::TIMES, v),
        };
        let (select, categories) = self.items.to_abi();
        DataChange {
            header: Header::of::<DataChange>(),
            entry: [0; 16],
            field: self.field.to_abi(),
            op,
            value,
            source: Str::new(source),
            item: [0; 16],
            select,
            categories,
        }
    }
}

impl Load<'_> {
    /// Declares `change` during `init`, from `source`, as
    /// [`Load::change`] declares a change of one entry; the loader turns it
    /// into one change of each vanilla item the selection picks, read from
    /// the game's data, in this mod's place in load order. Items of mods
    /// are not picked.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field a selection does not change, a value the field
    /// does not take, after init, and when the running build does not
    /// reflect the field.
    ///
    /// # Examples
    ///
    /// See [`ItemsChange`].
    pub fn change_items(&mut self, change: &ItemsChange, source: &str) -> Result<(), Off> {
        change
            .check()
            .map_err(|why| Off::new(format!("change of {:?}: {why}", change.items)))?;
        let raw = change.to_abi(source);
        let api = self.host().api();
        let status = (api.change_data)(self.host().handle(), &raw const raw);
        crate::load::ok(status, (), || {
            format!("change {} of {:?}", change.field, change.items)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WOOD: ItemTraits = ItemTraits {
        max_stack: 20,
        category: 9,
        flags: 0,
        slots: None,
        backpack_slots: None,
    };
    const SWORD: ItemTraits = ItemTraits {
        max_stack: 1,
        category: 2,
        flags: ItemData::HAS_PIDE,
        slots: None,
        backpack_slots: None,
    };
    const CHEST: ItemTraits = ItemTraits {
        max_stack: 1,
        category: 6,
        flags: 0,
        slots: Some(24),
        backpack_slots: None,
    };
    const BACKPACK: ItemTraits = ItemTraits {
        max_stack: 1,
        category: 1,
        flags: ItemData::HAS_PIDE,
        slots: None,
        backpack_slots: Some(16),
    };

    #[test]
    fn backpack_slots_reach_backpacks_alone() {
        assert!(Items::Backpacks.picks(Field::Slots, &BACKPACK));
        assert!(!Items::Backpacks.picks(Field::Slots, &CHEST));
        assert!(!Items::Containers.picks(Field::Slots, &BACKPACK));
        assert!(!Items::Backpacks.picks(Field::MaxStackSize, &BACKPACK));
        assert!(Items::Category(ItemCategoryMask::EQUIPMENT).picks(Field::Slots, &BACKPACK));
    }

    #[test]
    fn a_stack_size_reaches_only_items_that_stack() {
        let every = Items::Category(ItemCategoryMask(u32::MAX));
        assert!(every.picks(Field::MaxStackSize, &WOOD));
        assert!(!every.picks(Field::MaxStackSize, &SWORD));
        assert!(!every.picks(Field::MaxStackSize, &CHEST));
        let held = ItemTraits {
            max_stack: 5,
            flags: ItemData::HAS_PIDE,
            ..WOOD
        };
        assert!(!Items::Stackable.picks(Field::MaxStackSize, &held));
    }

    #[test]
    fn container_slots_reach_only_items_that_place_one() {
        assert!(Items::Containers.picks(Field::Slots, &CHEST));
        assert!(!Items::Containers.picks(Field::Slots, &WOOD));
        assert!(Items::Category(ItemCategoryMask::BUILD_TOOLS).picks(Field::Slots, &CHEST));
        assert!(!Items::Containers.picks(Field::MaxStackSize, &CHEST));
    }

    #[test]
    fn a_category_picks_its_items_alone() {
        let materials = Items::Category(ItemCategoryMask::MATERIALS);
        assert!(materials.picks(Field::MaxStackSize, &WOOD));
        let ammo = ItemTraits {
            category: 8,
            ..WOOD
        };
        assert!(!materials.picks(Field::MaxStackSize, &ammo));
    }

    #[test]
    fn a_selection_goes_through_the_abi_and_back() {
        for items in [
            Items::Category(ItemCategoryMask::MATERIALS | ItemCategoryMask::AMMUNITION),
            Items::Stackable,
            Items::Containers,
            Items::Backpacks,
        ] {
            let field = if matches!(items, Items::Containers | Items::Backpacks) {
                Field::Slots
            } else {
                Field::MaxStackSize
            };
            let change = ItemsChange::new(items, field, Value::Times(2.0));
            let raw = change.to_abi("my-mod");
            assert_eq!(ItemsChange::from_abi(&raw), Ok(Some(change)));
            assert_eq!(Change::from_abi(&raw), Ok(None));
        }
    }

    #[test]
    fn a_record_of_one_entry_is_no_selection() {
        let one = Change::new(Guid([7; 16]), Field::MaxStackSize, Value::Set(50.0));
        assert_eq!(ItemsChange::from_abi(&one.to_abi("x")), Ok(None));
    }

    #[test]
    fn a_selection_refuses_fields_it_does_not_reach() {
        let damage = ItemsChange::new(Items::Stackable, Field::DamageMod, Value::Times(2.0));
        assert!(damage.check().is_err());
        let shrink = ItemsChange::new(Items::Containers, Field::Slots, Value::Times(0.5));
        assert!(shrink.check().is_err());
    }
}
