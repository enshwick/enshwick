//! The game's enums and masks that events carry, each value read by the
//! name of the engine's constant.

use crate::engine_ids as ids;

/// The damage types of a hit, a set the loader fills from the engine's
/// damage type mask by the name of each bit.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::DamageTypes;
///
/// let types = DamageTypes::FIRE | DamageTypes::CUT;
/// assert!(types.contains(DamageTypes::FIRE));
/// assert!(!types.contains(DamageTypes::ICE));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DamageTypes(u16);

impl DamageTypes {
    /// Physical cut.
    pub const CUT: Self = Self(1);
    /// Physical pierce.
    pub const PIERCE: Self = Self(1 << 1);
    /// Physical blunt.
    pub const BLUNT: Self = Self(1 << 2);
    /// Fire.
    pub const FIRE: Self = Self(1 << 3);
    /// Ice.
    pub const ICE: Self = Self(1 << 4);
    /// Shroud.
    pub const SHROUD: Self = Self(1 << 5);
    /// Lightning.
    pub const LIGHTNING: Self = Self(1 << 6);
    /// Healing.
    pub const HEALING: Self = Self(1 << 7);
    /// Poison.
    pub const POISON: Self = Self(1 << 8);
    /// Wood, against trees and wooden things.
    pub const WOOD: Self = Self(1 << 9);
    /// Stone, against rock.
    pub const STONE: Self = Self(1 << 10);
    /// Metal, against ore.
    pub const METAL: Self = Self(1 << 11);
    /// Explosion.
    pub const EXPLOSION: Self = Self(1 << 12);

    /// Whether every bit of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`DamageTypes`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Each type with the id of its engine bit.
    pub(super) const IDS: [(Self, &'static str); 13] = [
        (Self::CUT, ids::DAMAGE_TYPE_PHYSICAL_CUT),
        (Self::PIERCE, ids::DAMAGE_TYPE_PHYSICAL_PIERCE),
        (Self::BLUNT, ids::DAMAGE_TYPE_PHYSICAL_BLUNT),
        (Self::FIRE, ids::DAMAGE_TYPE_MAGICAL_FIRE),
        (Self::ICE, ids::DAMAGE_TYPE_MAGICAL_ICE),
        (Self::SHROUD, ids::DAMAGE_TYPE_MAGICAL_FOG),
        (Self::LIGHTNING, ids::DAMAGE_TYPE_MAGICAL_LIGHTNING),
        (Self::HEALING, ids::DAMAGE_TYPE_HEALING),
        (Self::POISON, ids::DAMAGE_TYPE_POISON),
        (Self::WOOD, ids::DAMAGE_TYPE_WOOD),
        (Self::STONE, ids::DAMAGE_TYPE_STONE),
        (Self::METAL, ids::DAMAGE_TYPE_METAL),
        (Self::EXPLOSION, ids::DAMAGE_TYPE_EXPLOSION),
    ];
}

impl DamageTypes {
    /// The set of the API's bits `bits`.
    pub(super) const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    /// The API's bits of the set.
    pub(super) const fn bits(self) -> u16 {
        self.0
    }
}

impl core::ops::BitOr for DamageTypes {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// The attack categories of a hit, a set the loader fills from the
/// engine's attack category mask by the name of each bit.
///
/// # Examples
///
/// ```
/// use enshwick::game_events::AttackCategories;
///
/// let categories = AttackCategories::MELEE | AttackCategories::AREA;
/// assert!(categories.contains(AttackCategories::MELEE));
/// assert!(!categories.contains(AttackCategories::RANGED));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct AttackCategories(u8);

impl AttackCategories {
    /// Melee.
    pub const MELEE: Self = Self(1);
    /// Ranged.
    pub const RANGED: Self = Self(1 << 1);
    /// Magic.
    pub const MAGIC: Self = Self(1 << 2);
    /// A tool, such as a pickaxe.
    pub const TOOL: Self = Self(1 << 3);
    /// Anything else.
    pub const OTHER: Self = Self(1 << 4);
    /// Area of effect.
    pub const AREA: Self = Self(1 << 5);
    /// A skill shot.
    pub const SKILL_SHOT: Self = Self(1 << 6);

    /// Whether every bit of `other` is set.
    ///
    /// # Examples
    ///
    /// See [`AttackCategories`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Each category with the id of its engine bit.
    pub(super) const IDS: [(Self, &'static str); 7] = [
        (Self::MELEE, ids::ATTACK_CATEGORY_MELEE),
        (Self::RANGED, ids::ATTACK_CATEGORY_RANGED),
        (Self::MAGIC, ids::ATTACK_CATEGORY_MAGIC),
        (Self::TOOL, ids::ATTACK_CATEGORY_TOOL),
        (Self::OTHER, ids::ATTACK_CATEGORY_OTHER),
        (Self::AREA, ids::ATTACK_CATEGORY_AO_E),
        (Self::SKILL_SHOT, ids::ATTACK_CATEGORY_SKILL_SHOT),
    ];
}

impl AttackCategories {
    /// The set of the API's bits `bits`.
    pub(super) const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// The API's bits of the set.
    pub(super) const fn bits(self) -> u8 {
        self.0
    }
}

impl core::ops::BitOr for AttackCategories {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Where experience came from; the loader reads the engine's value of each
/// variant by its constant's name.
///
/// # Examples
///
/// See [`ExperienceGained`](crate::game_events::ExperienceGained).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExperienceSource {
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

impl ExperienceSource {
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
/// See [`AltarUsed`](crate::game_events::AltarUsed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AltarAction {
    /// Abandoned the base.
    Abandon,
    /// Reclaimed an abandoned base.
    Reclaim,
    /// Removed the water.
    RemoveWater,
    /// Made it the preferred respawn altar.
    SetSpawn,
    /// A value this crate does not name, as the game gave it.
    Other(u8),
}

impl AltarAction {
    /// Each action with the id of its engine constant.
    pub(super) const IDS: [(Self, &'static str); 4] = [
        (Self::Abandon, ids::ALTAR_ACTION_ABANDON),
        (Self::Reclaim, ids::ALTAR_ACTION_RECLAIM),
        (Self::RemoveWater, ids::ALTAR_ACTION_REMOVE_WATER),
        (
            Self::SetSpawn,
            ids::ALTAR_ACTION_SET_AS_PREFERRED_SPAWN_ALTAR,
        ),
    ];
}

/// A character's equipment slot; the loader reads the engine's value of
/// each variant by its constant's name.
///
/// # Examples
///
/// See [`EquipmentChanged`](crate::game_events::EquipmentChanged).
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
    fn slots_and_experience_sources_take_the_engine_s_names() {
        assert_named_after_engine(&EquipmentSlot::IDS);
        assert_named_after_engine(&ExperienceSource::IDS);
    }
}
