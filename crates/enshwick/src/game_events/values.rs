//! The game's enums and masks that events carry, each value read by the
//! name of the engine's constant.

use crate::engine_ids as ids;

/// Declares a mask type over the bits of an engine mask, each bit a
/// constant named after the engine's constant for it.
macro_rules! mask {
    (
        $(#[$doc:meta])*
        $name:ident($bits:ty) {
            $($(#[$cdoc:meta])* $constant:ident = $bit:literal, $id:path;)*
        }
    ) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name($bits);

        impl $name {
            $($(#[$cdoc])* pub const $constant: Self = Self(1 << $bit);)*

            /// Whether every bit of `other` is set.
            ///
            /// # Examples
            ///
            #[doc = concat!("See [`", stringify!($name), "`].")]
            #[must_use]
            pub const fn contains(self, other: Self) -> bool {
                self.0 & other.0 == other.0
            }

            /// Each bit with the id of its engine constant.
            pub(super) const IDS: [(Self, &'static str); [$(stringify!($constant)),*].len()] =
                [$((Self::$constant, $id)),*];

            /// Each constant's name with the id of its engine constant.
            #[cfg(test)]
            const NAMES: [(&'static str, &'static str); [$(stringify!($constant)),*].len()] =
                [$((stringify!($constant), $id)),*];

            /// The set of the API's bits `bits`.
            pub(super) const fn from_bits(bits: $bits) -> Self {
                Self(bits)
            }

            /// The API's bits of the set.
            pub(super) const fn bits(self) -> $bits {
                self.0
            }
        }

        impl core::ops::BitOr for $name {
            type Output = Self;

            fn bitor(self, other: Self) -> Self {
                Self(self.0 | other.0)
            }
        }
    };
}

mask! {
    /// The damage types of a hit, a set the loader fills from the engine's
    /// damage type mask by the name of each bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::DamageTypeMask;
    ///
    /// let types = DamageTypeMask::MAGICAL_FIRE | DamageTypeMask::PHYSICAL_CUT;
    /// assert!(types.contains(DamageTypeMask::MAGICAL_FIRE));
    /// assert!(!types.contains(DamageTypeMask::MAGICAL_ICE));
    /// ```
    DamageTypeMask(u16) {
        /// Physical cut.
        PHYSICAL_CUT = 0, ids::DAMAGE_TYPE_PHYSICAL_CUT;
        /// Physical pierce.
        PHYSICAL_PIERCE = 1, ids::DAMAGE_TYPE_PHYSICAL_PIERCE;
        /// Physical blunt.
        PHYSICAL_BLUNT = 2, ids::DAMAGE_TYPE_PHYSICAL_BLUNT;
        /// Fire.
        MAGICAL_FIRE = 3, ids::DAMAGE_TYPE_MAGICAL_FIRE;
        /// Ice.
        MAGICAL_ICE = 4, ids::DAMAGE_TYPE_MAGICAL_ICE;
        /// The shroud's damage.
        MAGICAL_FOG = 5, ids::DAMAGE_TYPE_MAGICAL_FOG;
        /// Lightning.
        MAGICAL_LIGHTNING = 6, ids::DAMAGE_TYPE_MAGICAL_LIGHTNING;
        /// Healing.
        HEALING = 7, ids::DAMAGE_TYPE_HEALING;
        /// Poison.
        POISON = 8, ids::DAMAGE_TYPE_POISON;
        /// Wood, against trees and wooden things.
        WOOD = 9, ids::DAMAGE_TYPE_WOOD;
        /// Stone, against rock.
        STONE = 10, ids::DAMAGE_TYPE_STONE;
        /// Metal, against ore.
        METAL = 11, ids::DAMAGE_TYPE_METAL;
        /// Explosion.
        EXPLOSION = 12, ids::DAMAGE_TYPE_EXPLOSION;
    }
}

mask! {
    /// The attack categories of a hit, a set the loader fills from the
    /// engine's attack category mask by the name of each bit.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::game_events::AttackCategoryMask;
    ///
    /// let categories = AttackCategoryMask::MELEE | AttackCategoryMask::AOE;
    /// assert!(categories.contains(AttackCategoryMask::MELEE));
    /// assert!(!categories.contains(AttackCategoryMask::RANGED));
    /// ```
    AttackCategoryMask(u8) {
        /// Melee.
        MELEE = 0, ids::ATTACK_CATEGORY_MELEE;
        /// Ranged.
        RANGED = 1, ids::ATTACK_CATEGORY_RANGED;
        /// Magic.
        MAGIC = 2, ids::ATTACK_CATEGORY_MAGIC;
        /// A tool, such as a pickaxe.
        TOOL = 3, ids::ATTACK_CATEGORY_TOOL;
        /// Anything else.
        OTHER = 4, ids::ATTACK_CATEGORY_OTHER;
        /// Area of effect.
        AOE = 5, ids::ATTACK_CATEGORY_AO_E;
        /// A skill shot.
        SKILL_SHOT = 6, ids::ATTACK_CATEGORY_SKILL_SHOT;
    }
}

/// Where experience came from; the loader reads the engine's value of each
/// variant by its constant's name.
///
/// # Examples
///
/// See [`XpGainedEvent`](crate::game_events::XpGainedEvent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExperienceSourceType {
    /// No source given.
    None,
    /// Exploring.
    Exploration,
    /// Reading lore.
    Lore,
    /// A quest.
    Quest,
    /// A kill.
    Combat,
    /// Mining.
    Mining,
    /// A completed item set.
    ItemSetCompleted,
    /// A value this crate does not name, as the game gave it.
    Other(u8),
}

impl ExperienceSourceType {
    /// Each source with the id of its engine constant.
    pub(crate) const IDS: [(Self, &'static str); 7] = [
        (Self::None, ids::EXPERIENCE_SOURCE_NONE),
        (Self::Exploration, ids::EXPERIENCE_SOURCE_EXPLORATION),
        (Self::Lore, ids::EXPERIENCE_SOURCE_LORE),
        (Self::Quest, ids::EXPERIENCE_SOURCE_QUEST),
        (Self::Combat, ids::EXPERIENCE_SOURCE_COMBAT),
        (Self::Mining, ids::EXPERIENCE_SOURCE_MINING),
        (
            Self::ItemSetCompleted,
            ids::EXPERIENCE_SOURCE_ITEM_SET_COMPLETED,
        ),
    ];
}

/// What a player did at a Flame Altar; the loader reads the engine's value
/// of each variant by its constant's name.
///
/// # Examples
///
/// See [`FlameAltarEvent`](crate::game_events::FlameAltarEvent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FlameAltarActionType {
    /// Abandoned the base.
    Abandon,
    /// Reclaimed an abandoned base.
    Reclaim,
    /// Removed the water.
    RemoveWater,
    /// Made it the preferred respawn altar.
    SetAsPreferredSpawnAltar,
    /// A value this crate does not name, as the game gave it.
    Other(u8),
}

impl FlameAltarActionType {
    /// Each action with the id of its engine constant.
    pub(super) const IDS: [(Self, &'static str); 4] = [
        (Self::Abandon, ids::ALTAR_ACTION_ABANDON),
        (Self::Reclaim, ids::ALTAR_ACTION_RECLAIM),
        (Self::RemoveWater, ids::ALTAR_ACTION_REMOVE_WATER),
        (
            Self::SetAsPreferredSpawnAltar,
            ids::ALTAR_ACTION_SET_AS_PREFERRED_SPAWN_ALTAR,
        ),
    ];
}

/// A character's equipment slot; the loader reads the engine's value of
/// each variant by its constant's name.
///
/// # Examples
///
/// See [`HandleEquipmentSlotChangedEvent`](crate::game_events::HandleEquipmentSlotChangedEvent).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EquipmentSlot {
    /// The vanity head slot.
    VanityHead,
    /// The vanity upper body slot.
    VanityUpperBody,
    /// The vanity arms slot.
    VanityArms,
    /// The vanity lower body slot.
    VanityLowerBody,
    /// The vanity feet slot.
    VanityFeet,
    /// The head armour.
    ArmourHead,
    /// The upper body armour.
    ArmourUpperBody,
    /// The arms armour.
    ArmourArms,
    /// The lower body armour.
    ArmourLowerBody,
    /// The feet armour.
    ArmourFeet,
    /// The first ring.
    Ring0,
    /// The second ring.
    Ring1,
    /// The ranged weapon.
    RangedWeapon,
    /// The shield.
    Shield,
    /// The arrows.
    Arrow,
    /// The spell.
    Spell,
    /// The fishing bait.
    FishingBait,
    /// The terrain material for building.
    TerrainMaterial,
    /// The building block material.
    BlueprintMaterialDefault,
    /// The roof material.
    BlueprintMaterialRoof,
    /// The overgrowth material.
    OvergrowthMaterial,
    /// The grappling hook.
    HookShot,
    /// The glider.
    Glider,
    /// The backpack.
    Backpack,
    /// The main hand.
    MainHand,
    /// A slot this crate does not name, as the game gave it.
    Other(u8),
}

impl EquipmentSlot {
    /// Each slot with the id of its engine constant.
    pub(super) const IDS: [(Self, &'static str); 25] = [
        (Self::VanityHead, ids::EQUIPMENT_SLOT_VANITY_HEAD),
        (Self::VanityUpperBody, ids::EQUIPMENT_SLOT_VANITY_UPPER_BODY),
        (Self::VanityArms, ids::EQUIPMENT_SLOT_VANITY_ARMS),
        (Self::VanityLowerBody, ids::EQUIPMENT_SLOT_VANITY_LOWER_BODY),
        (Self::VanityFeet, ids::EQUIPMENT_SLOT_VANITY_FEET),
        (Self::ArmourHead, ids::EQUIPMENT_SLOT_ARMOUR_HEAD),
        (Self::ArmourUpperBody, ids::EQUIPMENT_SLOT_ARMOUR_UPPER_BODY),
        (Self::ArmourArms, ids::EQUIPMENT_SLOT_ARMOUR_ARMS),
        (Self::ArmourLowerBody, ids::EQUIPMENT_SLOT_ARMOUR_LOWER_BODY),
        (Self::ArmourFeet, ids::EQUIPMENT_SLOT_ARMOUR_FEET),
        (Self::Ring0, ids::EQUIPMENT_SLOT_RING0),
        (Self::Ring1, ids::EQUIPMENT_SLOT_RING1),
        (Self::RangedWeapon, ids::EQUIPMENT_SLOT_RANGED_WEAPON),
        (Self::Shield, ids::EQUIPMENT_SLOT_SHIELD),
        (Self::Arrow, ids::EQUIPMENT_SLOT_ARROW),
        (Self::Spell, ids::EQUIPMENT_SLOT_SPELL),
        (Self::FishingBait, ids::EQUIPMENT_SLOT_FISHING_BAIT),
        (Self::TerrainMaterial, ids::EQUIPMENT_SLOT_TERRAIN_MATERIAL),
        (
            Self::BlueprintMaterialDefault,
            ids::EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_DEFAULT,
        ),
        (
            Self::BlueprintMaterialRoof,
            ids::EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_ROOF,
        ),
        (
            Self::OvergrowthMaterial,
            ids::EQUIPMENT_SLOT_OVERGROWTH_MATERIAL,
        ),
        (Self::HookShot, ids::EQUIPMENT_SLOT_HOOK_SHOT),
        (Self::Glider, ids::EQUIPMENT_SLOT_GLIDER),
        (Self::Backpack, ids::EQUIPMENT_SLOT_BACKPACK),
        (Self::MainHand, ids::EQUIPMENT_SLOT_MAIN_HAND),
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_enum::assert_named_after_engine;

    #[test]
    fn every_value_and_bit_takes_the_engine_s_name() {
        assert_named_after_engine(&EquipmentSlot::IDS);
        assert_named_after_engine(&ExperienceSourceType::IDS);
        assert_named_after_engine(&FlameAltarActionType::IDS);
        for names in [
            DamageTypeMask::NAMES.as_slice(),
            AttackCategoryMask::NAMES.as_slice(),
        ] {
            let named: Vec<_> = names
                .iter()
                .map(|&(name, id)| (Constant(name), id))
                .collect();
            assert_named_after_engine(&named);
        }
    }

    /// A constant's name, shown as it is.
    struct Constant(&'static str);

    impl core::fmt::Debug for Constant {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.0)
        }
    }
}
