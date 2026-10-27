use crate::abi::ItemData;
use crate::{Entity, ItemId};

/// A slot: a sub-inventory entity and a slot index in it, as the game packs
/// it, `entity | index << 32`.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::inventory::SlotRef;
///
/// let slot = SlotRef::new(Entity(0x41), 3);
/// assert_eq!(slot.0, 0x3_0000_0041);
/// assert_eq!((slot.inventory(), slot.index()), (Entity(0x41), 3));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, crate::Script)]
pub struct SlotRef(pub u64);

impl SlotRef {
    /// Slot `index` of the sub-inventory `inventory`.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    pub const fn new(inventory: Entity, index: u8) -> Self {
        Self(inventory.0 as u64 | (index as u64) << 32)
    }

    /// The sub-inventory entity.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "the low half")]
    pub const fn inventory(self) -> Entity {
        Entity(self.0 as u32)
    }

    /// The slot index, 0 to 7.
    ///
    /// # Examples
    ///
    /// See [`SlotRef`].
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "a byte")]
    pub const fn index(self) -> u8 {
        (self.0 >> 32) as u8
    }
}

/// What a sub-inventory is for, from the inventory's `InventorySetup`.
///
/// # Examples
///
/// ```
/// use enshwick::inventory::InventoryCategory;
///
/// assert_ne!(InventoryCategory::GENERIC, InventoryCategory::EQUIPMENT);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
pub struct InventoryCategory(pub u8);

impl InventoryCategory {
    /// Not known: a slot read alone.
    pub const INVALID: Self = Self(0);
    /// Looks.
    pub const CUSTOMIZATION: Self = Self(1);
    /// Worn and held items.
    pub const EQUIPMENT: Self = Self(2);
    /// Currency.
    pub const CURRENCY: Self = Self(3);
    /// Ordinary storage: a chest's slots, a player's backpack.
    pub const GENERIC: Self = Self(4);
    /// A link to another inventory, which the reads walk into.
    pub const VIRTUAL: Self = Self(5);
}

/// One slot and what it holds.
///
/// # Examples
///
/// ```
/// use enshwick::Entity;
/// use enshwick::inventory::{InventoryCategory, Slot, SlotRef};
///
/// let slot = Slot { at: SlotRef::new(Entity(9), 0), category: InventoryCategory::GENERIC, item: enshwick::ItemId::new(0), count: 0, pide: None };
/// assert!(slot.is_empty());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct Slot {
    /// Where the slot is.
    pub at: SlotRef,
    /// The sub-inventory's category.
    pub category: InventoryCategory,
    /// The item id, 0 for none.
    pub item: ItemId,
    /// How many.
    pub count: u32,
    /// The item's own entity, for items with durability or a level.
    pub pide: Option<Entity>,
}

impl Slot {
    /// Whether the slot holds nothing.
    ///
    /// # Examples
    ///
    /// See [`Slot`].
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.item.get() == 0 || self.count == 0
    }
}

/// An item's static data.
///
/// # Examples
///
/// ```
/// fn stackable(inventories: &enshwick::inventory::Inventories, item: enshwick::ItemId) -> bool {
///     inventories.item_info(item).is_some_and(|info| info.max_stack_size > 1 && !info.has_pide())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct ItemInfo {
    /// The item id.
    pub id: ItemId,
    /// How many fit one stack.
    pub max_stack_size: u16,
    /// `ItemInfo` flags, see [`ItemData`].
    pub flags: u32,
    /// The game's `ItemCategory`: 1 equipment (armour and rings), 2
    /// weapons, 3 tools, and so on.
    pub category: u8,
}

impl ItemInfo {
    /// Whether each item has an entity of its own (durability, level).
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn has_pide(&self) -> bool {
        self.flags & ItemData::HAS_PIDE != 0
    }

    /// Whether the item is a container.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn is_container(&self) -> bool {
        self.flags & ItemData::IS_CONTAINER != 0
    }

    /// Whether the item is a blueprint.
    ///
    /// # Examples
    ///
    /// See [`ItemInfo`].
    #[must_use]
    pub const fn is_blueprint(&self) -> bool {
        self.flags & ItemData::IS_BLUEPRINT != 0
    }
}

/// A value with its maximum on an item's own entity, as the item's slot
/// shows it: its durability, or its meter (see
/// [`FuelSetup`](crate::content::items::FuelSetup)).
///
/// The name is enshwick's: the game's durability or fuel value with its
/// maximum.
///
/// # Examples
///
/// ```
/// let worn = enshwick::inventory::Bar { current: 40, max: 160 };
/// assert!(worn.current < worn.max);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bar {
    /// What is left.
    pub current: u16,
    /// When full.
    pub max: u16,
}

/// How full a new item's meter starts, for [`Op::Give`](super::Op::Give): a fraction from 0
/// to 1 in steps of 1/65535. The game makes the item's count `max` times
/// the fill, rounded to the nearest whole count.
///
/// # Examples
///
/// ```
/// use enshwick::inventory::Fill;
///
/// let three_of_five = Fill::of(3, 5);
/// assert!((three_of_five.fraction() - 0.6).abs() < 1e-4);
/// assert_eq!(Fill::of(7, 5), Fill::FULL);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Fill(pub u16);

impl Fill {
    /// Empty.
    pub const EMPTY: Self = Self(0);
    /// Full.
    pub const FULL: Self = Self(u16::MAX);

    /// The fill that makes the game give `count` of `max`; full when `count`
    /// is `max` or more, empty when `max` is 0.
    ///
    /// # Examples
    ///
    /// See [`Fill`].
    #[must_use]
    pub fn of(count: u16, max: u16) -> Self {
        if max == 0 {
            return Self::EMPTY;
        }
        let parts = u32::from(count.min(max)) * u32::from(u16::MAX);
        let max = u32::from(max);
        Self(u16::try_from((parts + max / 2) / max).unwrap_or(u16::MAX))
    }

    /// The fill as a fraction from 0 to 1.
    ///
    /// # Examples
    ///
    /// See [`Fill`].
    #[must_use]
    pub fn fraction(self) -> f32 {
        f32::from(self.0) / f32::from(u16::MAX)
    }
}
