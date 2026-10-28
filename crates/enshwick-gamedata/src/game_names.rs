//! The API's names for things the game names itself, each beside the
//! reflected type, field or enum constant it stands for, so a test holds
//! them to the game's names; each row compiles only while its API item
//! exists.

/// One API name and the game's item it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GameName {
    /// The API type.
    pub api_type: &'static str,
    /// Its field, variant or constant; empty for the type itself.
    pub api_member: &'static str,
    /// The reflected type.
    pub game_type: &'static str,
    /// The reflected field or enum constant; empty for the type itself.
    pub game_member: &'static str,
}

/// The API type `$ty` is the reflected type `$game`.
macro_rules! ty {
    ($ty:ty => $game:literal) => {{
        let _: Option<$ty> = None;
        GameName {
            api_type: stringify!($ty),
            api_member: "",
            game_type: $game,
            game_member: "",
        }
    }};
}

/// The field `$field` of `$ty` is the reflected field `$member` of `$game`.
macro_rules! field {
    ($ty:ty, $field:tt => $game:literal . $member:literal) => {{
        let _ = core::mem::offset_of!($ty, $field);
        GameName {
            api_type: stringify!($ty),
            api_member: stringify!($field),
            game_type: $game,
            game_member: $member,
        }
    }};
}

/// The variant or constant `$item` of `$ty` is the enum constant `$member`
/// of `$game`.
macro_rules! constant {
    ($ty:ident :: $item:ident => $game:literal . $member:literal) => {{
        let _ = $ty::$item;
        GameName {
            api_type: stringify!($ty),
            api_member: stringify!($item),
            game_type: $game,
            game_member: $member,
        }
    }};
}

use enshwick::abi::PlayerDeathData;
use enshwick::affixes::ItemRarity;

use enshwick::buffs::{Buff, BuffSlotType, BuffType, BuffTypeId};
use enshwick::content::categories::ItemCategoryMask;
use enshwick::content::changes::Field;
use enshwick::content::items::{FuelSetup, ImpactUiValue};
use enshwick::damage::DamageTypeNew;
use enshwick::daytime::TimeOfDay;
use enshwick::effects::NpcCommandType;
use enshwick::inventory::{InventoryCategory, ItemInfo};
use enshwick::knowledge::{KnowledgeType, Known};
use enshwick::loot::{Container, InteractionOfferRuntime};
use enshwick::survivors::BedCheck;
use enshwick::terrain::TerrainTerraformingConfig;
use enshwick::weather::WeatherPhenomenonEffect;
use enshwick::world::{BuildRefusal, PlayerPermissions};

/// Every API name the game names itself outside the catalogue's ids, which
/// hold the typed events, their values and the game settings.
pub(crate) const GAME_NAMES: &[GameName] = &[
    ty!(DamageTypeNew => "keen::impact::DamageTypeNew"),
    constant!(DamageTypeNew::PhysicalCut => "keen::impact::DamageTypeNew"."Physical_Cut"),
    constant!(DamageTypeNew::PhysicalPierce => "keen::impact::DamageTypeNew"."Physical_Pierce"),
    constant!(DamageTypeNew::PhysicalBlunt => "keen::impact::DamageTypeNew"."Physical_Blunt"),
    constant!(DamageTypeNew::MagicalFire => "keen::impact::DamageTypeNew"."Magical_Fire"),
    constant!(DamageTypeNew::MagicalIce => "keen::impact::DamageTypeNew"."Magical_Ice"),
    constant!(DamageTypeNew::MagicalFog => "keen::impact::DamageTypeNew"."Magical_Fog"),
    constant!(DamageTypeNew::MagicalLightning => "keen::impact::DamageTypeNew"."Magical_Lightning"),
    constant!(DamageTypeNew::Healing => "keen::impact::DamageTypeNew"."Healing"),
    constant!(DamageTypeNew::Poison => "keen::impact::DamageTypeNew"."Poison"),
    constant!(DamageTypeNew::Wood => "keen::impact::DamageTypeNew"."Wood"),
    constant!(DamageTypeNew::Stone => "keen::impact::DamageTypeNew"."Stone"),
    constant!(DamageTypeNew::Metal => "keen::impact::DamageTypeNew"."Metal"),
    constant!(DamageTypeNew::Explosion => "keen::impact::DamageTypeNew"."Explosion"),
    ty!(BuffSlotType => "keen::BuffSlotType"),
    constant!(BuffSlotType::Food => "keen::BuffSlotType"."Food"),
    constant!(BuffSlotType::Buff => "keen::BuffSlotType"."Buff"),
    constant!(BuffSlotType::Debuff => "keen::BuffSlotType"."Debuff"),
    constant!(BuffSlotType::Rested => "keen::BuffSlotType"."Rested"),
    constant!(BuffSlotType::TerrainDebuff => "keen::BuffSlotType"."TerrainDebuff"),
    ty!(BuffTypeId => "keen::BuffTypeId"),
    ty!(BuffType => "keen::BuffType"),
    field!(BuffType, buff_type_id => "keen::BuffType"."buffTypeId"),
    field!(BuffType, slot => "keen::BuffType"."slot"),
    field!(BuffType, default_life_time => "keen::BuffType"."defaultLifeTime"),
    field!(BuffType, is_saved => "keen::BuffType"."isSaved"),
    field!(BuffType, hide_in_ui => "keen::BuffType"."hideInUi"),
    field!(BuffType, name => "keen::BuffType"."name"),
    field!(Buff, type_id => "keen::ecs::BuffInstance"."typeId"),
    field!(Buff, buff_entity_id => "keen::ecs::BuffInstance"."buffEntityId"),
    field!(Buff, life_time => "keen::ecs::BuffNetworkData"."lifeTime"),
    field!(Buff, level => "keen::ecs::BuffNetworkData"."level"),
    ty!(KnowledgeType => "keen::KnowledgeType"),
    constant!(KnowledgeType::Bool => "keen::KnowledgeType"."Bool"),
    constant!(KnowledgeType::Counter => "keen::KnowledgeType"."Counter"),
    constant!(KnowledgeType::Flags => "keen::KnowledgeType"."Flags"),
    field!(Known, id => "keen::KnowledgeObjectInfo"."id"),
    field!(Known, r#type => "keen::KnowledgeObjectInfo"."type"),
    field!(Known, expected_flags => "keen::KnowledgeObjectInfo"."expectedFlags"),
    ty!(WeatherPhenomenonEffect => "keen::WeatherPhenomenonEffect"),
    constant!(WeatherPhenomenonEffect::None => "keen::WeatherPhenomenonEffect"."None"),
    constant!(WeatherPhenomenonEffect::Rain => "keen::WeatherPhenomenonEffect"."Rain"),
    constant!(WeatherPhenomenonEffect::Blizzard => "keen::WeatherPhenomenonEffect"."Blizzard"),
    ty!(ItemRarity => "keen::ItemRarity"),
    constant!(ItemRarity::Common => "keen::ItemRarity"."Common"),
    constant!(ItemRarity::Uncommon => "keen::ItemRarity"."Uncommon"),
    constant!(ItemRarity::Rare => "keen::ItemRarity"."Rare"),
    constant!(ItemRarity::Epic => "keen::ItemRarity"."Epic"),
    constant!(ItemRarity::Legendary => "keen::ItemRarity"."Legendary"),
    ty!(ItemCategoryMask => "keen::ItemCategoryMask"),
    constant!(ItemCategoryMask::EQUIPMENT => "keen::ItemCategory"."Equipment"),
    constant!(ItemCategoryMask::WEAPONS => "keen::ItemCategory"."Weapons"),
    constant!(ItemCategoryMask::TOOLS => "keen::ItemCategory"."Tools"),
    constant!(ItemCategoryMask::INSTRUMENT => "keen::ItemCategory"."Instrument"),
    constant!(ItemCategoryMask::FISHING_ROD => "keen::ItemCategory"."FishingRod"),
    constant!(ItemCategoryMask::CUSTOMIZATION => "keen::ItemCategory"."Customization"),
    constant!(ItemCategoryMask::BUILD_TOOLS => "keen::ItemCategory"."BuildTools"),
    constant!(ItemCategoryMask::CONSUMABLES => "keen::ItemCategory"."Consumables"),
    constant!(ItemCategoryMask::AMMUNITION => "keen::ItemCategory"."Ammunition"),
    constant!(ItemCategoryMask::MATERIALS => "keen::ItemCategory"."Materials"),
    constant!(ItemCategoryMask::BLUEPRINTS => "keen::ItemCategory"."Blueprints"),
    constant!(ItemCategoryMask::CURRENCY => "keen::ItemCategory"."Currency"),
    constant!(ItemCategoryMask::COLLECTIBLES => "keen::ItemCategory"."Collectibles"),
    constant!(ItemCategoryMask::ANIMAL_FOOD => "keen::ItemCategory"."AnimalFood"),
    constant!(ItemCategoryMask::PET_FOOD => "keen::ItemCategory"."PetFood"),
    constant!(ItemCategoryMask::WEAPON_GEM => "keen::ItemCategory"."WeaponGem"),
    constant!(ItemCategoryMask::COLOR_PALETTE => "keen::ItemCategory"."ColorPalette"),
    ty!(InventoryCategory => "keen::InventoryCategory"),
    constant!(InventoryCategory::INVALID => "keen::InventoryCategory"."Invalid"),
    constant!(InventoryCategory::CUSTOMIZATION => "keen::InventoryCategory"."Customization"),
    constant!(InventoryCategory::EQUIPMENT => "keen::InventoryCategory"."Equipment"),
    constant!(InventoryCategory::CURRENCY => "keen::InventoryCategory"."Currency"),
    constant!(InventoryCategory::GENERIC => "keen::InventoryCategory"."Generic"),
    constant!(InventoryCategory::VIRTUAL => "keen::InventoryCategory"."Virtual"),
    ty!(ItemInfo => "keen::ItemInfo"),
    field!(ItemInfo, max_stack_size => "keen::ItemInfo"."maxStackSize"),
    ty!(ImpactUiValue => "keen::ImpactUiValue"),
    field!(ImpactUiValue, loca_id => "keen::ImpactUiValue"."locaId"),
    field!(ImpactUiValue, value_format => "keen::ImpactUiValue"."valueFormat"),
    field!(ImpactUiValue, value => "keen::ImpactUiValue"."value"),
    ty!(FuelSetup => "keen::ecs::FuelSetup"),
    field!(FuelSetup, max_fuel_charges => "keen::ecs::FuelSetup"."maxFuelCharges"),
    constant!(Field::MaxStackSize => "keen::ItemInfo"."maxStackSize"),
    constant!(Field::CraftingDuration => "keen::RecipeInfo"."craftingDuration"),
    ty!(TerrainTerraformingConfig => "keen::TerrainTerraformingConfig"),
    field!(TerrainTerraformingConfig, terrain_material => "keen::TerrainTerraformingConfig"."terrainMaterial"),
    field!(TerrainTerraformingConfig, terrain_item => "keen::TerrainTerraformingConfig"."terrainItem"),
    field!(TerrainTerraformingConfig, hardness => "keen::TerrainTerraformingConfig"."hardness"),
    field!(TerrainTerraformingConfig, health_points => "keen::TerrainTerraformingConfig"."healthPoints"),
    field!(TerrainTerraformingConfig, can_be_flattened => "keen::TerrainTerraformingConfig"."canBeFlattened"),
    ty!(InteractionOfferRuntime => "keen::ecs::InteractionOfferRuntime"),
    field!(InteractionOfferRuntime, guest_id => "keen::ecs::InteractionAcceptData"."guestId"),
    field!(InteractionOfferRuntime, last_accept_time => "keen::ecs::InteractionOfferRuntime"."lastAcceptTime"),
    field!(InteractionOfferRuntime, is_disabled => "keen::ecs::InteractionOfferRuntime"."isDisabled"),
    field!(Container, is_empty => "keen::ecs::InventoryEmptyState"."isEmpty"),
    ty!(NpcCommandType => "keen::ecs::NpcCommandType"),
    constant!(NpcCommandType::Reset => "keen::ecs::NpcCommandType"."Reset"),
    constant!(NpcCommandType::GoToPosition => "keen::ecs::NpcCommandType"."GoToPosition"),
    field!(TimeOfDay, day_time_duration => "keen::ecs::GameSettings"."dayTimeDuration"),
    field!(TimeOfDay, night_time_duration => "keen::ecs::GameSettings"."nightTimeDuration"),
    ty!(PlayerPermissions => "keen::PlayerPermissions"),
    constant!(PlayerPermissions::CanEditBase => "keen::PlayerPermissions"."CanEditBase"),
    constant!(PlayerPermissions::CanEditWorld => "keen::PlayerPermissions"."CanEditWorld"),
    constant!(BuildRefusal::NoPermission => "keen::ecs::CursorFlag"."NoPermission"),
    constant!(BuildRefusal::CursorOutsideBuildingZone => "keen::ecs::CursorFlag"."CursorOutsideBuildingZone"),
    constant!(BuildRefusal::CursorInsideStrictBuildInhibitorZone => "keen::ecs::CursorFlag"."CursorInsideStrictBuildInhibitorZone"),
    field!(PlayerDeathData, cause => "keen::PlayerDeath"."cause"),
    field!(PlayerDeathData, position => "keen::PlayerDeath"."position"),
    field!(PlayerDeathData, cause_guid => "keen::PlayerDeath"."causeGuid"),
    field!(PlayerDeathData, was_deadly_fog => "keen::PlayerDeath"."wasDeadlyFog"),
    field!(PlayerDeathData, drowned => "keen::PlayerDeath"."drowned"),
    constant!(PlayerDeathData::IMPACT => "keen::DeathCause"."Impact"),
    constant!(PlayerDeathData::FALL_DAMAGE => "keen::DeathCause"."FallDamage"),
    constant!(PlayerDeathData::FOG => "keen::DeathCause"."Fog"),
    constant!(PlayerDeathData::SUFFOCATED => "keen::DeathCause"."Suffocated"),
    field!(BedCheck, npc_id => "keen::ecs::ActiveNpcState"."npcId"),
    field!(BedCheck, bed_id => "keen::ecs::NpcCommon"."bedId"),
];

#[cfg(test)]
mod tests {
    use super::GAME_NAMES;

    fn plain(name: &str) -> String {
        name.trim_start_matches("r#")
            .chars()
            .filter(|&c| c != '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn the_category_names_packs_read_are_the_game_s_names() {
        let mut game: Vec<&str> = GAME_NAMES
            .iter()
            .filter(|row| row.api_type == "ItemCategoryMask" && !row.game_member.is_empty())
            .map(|row| row.game_member)
            .collect();
        let mut ours: Vec<&str> = super::ItemCategoryMask::NAMES
            .iter()
            .map(|(name, _)| *name)
            .collect();
        game.sort_unstable();
        ours.sort_unstable();
        assert_eq!(ours, game);
    }

    #[test]
    fn every_listed_api_name_is_the_game_s_name() {
        for row in GAME_NAMES {
            if row.api_member.is_empty() {
                let (_, game) = row
                    .game_type
                    .rsplit_once("::")
                    .unwrap_or(("", row.game_type));
                assert_eq!(plain(row.api_type), plain(game), "{row:?}");
            } else {
                assert_eq!(plain(row.api_member), plain(row.game_member), "{row:?}");
            }
        }
    }
}
