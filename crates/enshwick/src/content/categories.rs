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

    /// Every category by the game's own name for it, the reflected
    /// `keen::ItemCategory` constant, in the order of their numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::categories::ItemCategoryMask;
    ///
    /// assert!(ItemCategoryMask::NAMES.contains(&("Materials", ItemCategoryMask::MATERIALS)));
    /// ```
    pub const NAMES: &[(&str, Self)] = &[
        ("Customization", Self::CUSTOMIZATION),
        ("Equipment", Self::EQUIPMENT),
        ("Weapons", Self::WEAPONS),
        ("Tools", Self::TOOLS),
        ("Instrument", Self::INSTRUMENT),
        ("FishingRod", Self::FISHING_ROD),
        ("BuildTools", Self::BUILD_TOOLS),
        ("Consumables", Self::CONSUMABLES),
        ("Ammunition", Self::AMMUNITION),
        ("Materials", Self::MATERIALS),
        ("Blueprints", Self::BLUEPRINTS),
        ("Currency", Self::CURRENCY),
        ("Collectibles", Self::COLLECTIBLES),
        ("AnimalFood", Self::ANIMAL_FOOD),
        ("PetFood", Self::PET_FOOD),
        ("WeaponGem", Self::WEAPON_GEM),
        ("ColorPalette", Self::COLOR_PALETTE),
    ];

    /// The category the game names `name`, as [`ItemCategoryMask::NAMES`]
    /// lists them; `None` for a name the game does not use.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::categories::ItemCategoryMask;
    ///
    /// assert_eq!(ItemCategoryMask::named("FishingRod"), Some(ItemCategoryMask::FISHING_ROD));
    /// assert_eq!(ItemCategoryMask::named("fishing rod"), None);
    /// ```
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(game, _)| *game == name)
            .map(|&(_, mask)| mask)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_is_found_by_the_game_s_name_and_one_bit() {
        for &(name, mask) in ItemCategoryMask::NAMES {
            assert_eq!(ItemCategoryMask::named(name), Some(mask));
            assert_eq!(mask.0.count_ones(), 1, "{name}");
        }
        assert_eq!(ItemCategoryMask::named("Material"), None);
    }
}
