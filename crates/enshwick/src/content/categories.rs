//! The game's item categories, as a mask that selects items by category.

/// Item categories, a bit per member of the game's `ItemCategory`, whose
/// number is the bit's index.
///
/// # Examples
///
/// ```
/// use enshwick::content::categories::ItemCategoryMask;
///
/// let gear = ItemCategoryMask::EQUIPMENT | ItemCategoryMask::WEAPONS;
/// assert!(gear.contains(ItemCategoryMask::WEAPONS));
/// assert!(gear.holds(2));
/// assert!(!gear.holds(3));
///
/// const HELD: ItemCategoryMask = ItemCategoryMask::TOOLS.union(ItemCategoryMask::WEAPONS);
/// assert!(HELD.contains(ItemCategoryMask::TOOLS | ItemCategoryMask::WEAPONS));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ItemCategoryMask(pub u32);

impl ItemCategoryMask {
    /// Character looks, the game's `Customization`.
    pub const CUSTOMIZATION: Self = Self(1 << 0);
    /// Armour and rings, the game's `Equipment`.
    pub const EQUIPMENT: Self = Self(1 << 1);
    /// Weapons.
    pub const WEAPONS: Self = Self(1 << 2);
    /// Tools.
    pub const TOOLS: Self = Self(1 << 3);
    /// Instruments.
    pub const INSTRUMENT: Self = Self(1 << 4);
    /// Fishing rods.
    pub const FISHING_ROD: Self = Self(1 << 5);
    /// Building tools.
    pub const BUILD_TOOLS: Self = Self(1 << 6);
    /// Food, potions and other items a player uses up.
    pub const CONSUMABLES: Self = Self(1 << 7);
    /// Arrows and other ammunition.
    pub const AMMUNITION: Self = Self(1 << 8);
    /// Materials.
    pub const MATERIALS: Self = Self(1 << 9);
    /// Building blueprints.
    pub const BLUEPRINTS: Self = Self(1 << 10);
    /// Currency.
    pub const CURRENCY: Self = Self(1 << 11);
    /// Collectibles.
    pub const COLLECTIBLES: Self = Self(1 << 12);
    /// Animal food.
    pub const ANIMAL_FOOD: Self = Self(1 << 13);
    /// Pet food.
    pub const PET_FOOD: Self = Self(1 << 14);
    /// Weapon gems.
    pub const WEAPON_GEM: Self = Self(1 << 15);
    /// Colour palettes.
    pub const COLOR_PALETTE: Self = Self(1 << 16);

    /// Whether every category of `other` is here.
    ///
    /// # Examples
    ///
    /// See [`ItemCategoryMask`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether the game's `ItemCategory` numbered `category` is here.
    ///
    /// # Examples
    ///
    /// See [`ItemCategoryMask`].
    #[must_use]
    pub const fn holds(self, category: u8) -> bool {
        category < 32 && self.0 & (1 << category) != 0
    }

    /// Every category of `self` and of `other`, as `|` gives, in a `const`
    /// too.
    ///
    /// # Examples
    ///
    /// See [`ItemCategoryMask`].
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl core::ops::BitOr for ItemCategoryMask {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}
