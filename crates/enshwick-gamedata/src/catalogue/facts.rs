// Long file: one table of every engine fact, kept whole so each id of the
// API finds its one row in one place.

//! The catalogue's table: each id the API asks for, by its constant, with
//! the engine fact that answers it.

use super::{Fact, enum_constant, ids, offset};

/// Every fact the API asks for, one row per id of [`ids`].
pub const FACTS: &[(&str, Fact)] = &[
    (ids::HIT_EVENT, Fact::Type("keen::ecs::HitEvent")),
    (
        ids::HIT_EVENT_ROOT_SOURCE_ID,
        offset(
            "keen::ecs::BaseHitEvent",
            "rootSourceId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::HIT_EVENT_TARGET_ID,
        offset("keen::ecs::BaseHitEvent", "targetId", "keen::ecs::EntityId"),
    ),
    (
        ids::HIT_EVENT_SOURCE_ID,
        offset("keen::ecs::HitEvent", "sourceId", "keen::ecs::EntityId"),
    ),
    (
        ids::HIT_EVENT_ROOT_TARGET_ID,
        offset("keen::ecs::HitEvent", "rootTargetId", "keen::ecs::EntityId"),
    ),
    (
        ids::HIT_EVENT_WEAPON_PIDE_ID,
        offset("keen::ecs::HitEvent", "weaponPideId", "keen::ecs::EntityId"),
    ),
    (
        ids::HIT_HEALTH_CHANGE,
        offset("keen::ecs::HitEvent", "healthChange", "keen::sint32"),
    ),
    (
        ids::HIT_STAMINA_CHANGE,
        offset("keen::ecs::HitEvent", "staminaChange", "keen::sint32"),
    ),
    (
        ids::HIT_DAMAGE_TYPES,
        offset(
            "keen::ecs::HitEvent",
            "damageTypes",
            "keen::impact::DamageTypeMask",
        ),
    ),
    (
        ids::HIT_EVENT_ATTACK_CATEGORY,
        offset(
            "keen::ecs::HitEvent",
            "attackCategory",
            "keen::impact::AttackCategoryMask",
        ),
    ),
    (
        ids::HIT_TARGET_FACTION,
        offset("keen::ecs::HitEvent", "targetFaction", "keen::FactionType"),
    ),
    (
        ids::ENTITY_DIED_EVENT,
        Fact::Type("keen::ecs::EntityDiedEvent"),
    ),
    (
        ids::ENTITY_DIED_EVENT_DEAD_ENTITY_ID,
        offset(
            "keen::ecs::EntityDiedEvent",
            "deadEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (ids::HIT_HISTORY, Fact::Type("keen::ecs::HitHistory")),
    (
        ids::HIT_HISTORY_LAST_INDEX,
        offset("keen::ecs::HitHistory", "lastIndex", "keen::uint32"),
    ),
    (
        ids::HIT_HISTORY_ENTRIES,
        offset(
            "keen::ecs::HitHistory",
            "history",
            "keen::StaticArray<keen::ecs::HitEvent,10>",
        ),
    ),
    (
        ids::HIT_HISTORY_END,
        offset(
            "keen::ecs::HitHistory",
            "targetTransform",
            "keen::StaticArray<keen::WorldTransform,10>",
        ),
    ),
    (
        ids::EXPERIENCE_CREDIT,
        Fact::Type("keen::ecs::ExperienceSource"),
    ),
    (
        ids::EXPERIENCE_CREDIT_LAST_PLAYER,
        offset(
            "keen::ecs::ExperienceSource",
            "lastHitPlayerId",
            "keen::ecs::EntityId",
        ),
    ),
    (ids::LEVEL_UP_EVENT, Fact::Type("keen::ecs::LevelUpEvent")),
    (
        ids::LEVEL_UP_EVENT_PLAYER_ENTITY_ID,
        offset(
            "keen::ecs::LevelUpEvent",
            "playerEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::LEVEL_UP_EVENT_LEVEL,
        offset("keen::ecs::LevelUpEvent", "level", "keen::sint32"),
    ),
    (ids::XP_GAINED_EVENT, Fact::Type("keen::ecs::XPGainedEvent")),
    (
        ids::XP_GAINED_EVENT_PLAYER_ENTITY_ID,
        offset(
            "keen::ecs::XPGainedEvent",
            "playerEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::XP_GAINED_EVENT_AMOUNT,
        offset("keen::ecs::XPGainedEvent", "amount", "keen::uint32"),
    ),
    (
        ids::XP_GAINED_EVENT_SOURCE,
        offset(
            "keen::ecs::XPGainedEvent",
            "source",
            "keen::ExperienceSourceType",
        ),
    ),
    (
        ids::CRAFT_RECIPE_EVENT,
        Fact::Type("keen::ecs::CraftRecipeEvent"),
    ),
    (
        ids::CRAFT_RECIPE_EVENT_CRAFTING_OPERATOR_ID,
        offset(
            "keen::ecs::CraftRecipeEvent",
            "craftingOperatorId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::CRAFT_RECIPE_EVENT_CRAFTING_STATION_ID,
        offset(
            "keen::ecs::CraftRecipeEvent",
            "craftingStationId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::CRAFT_RECIPE_EVENT_RECIPE_ID,
        offset("keen::ecs::CraftRecipeEvent", "recipeId", "keen::RecipeId"),
    ),
    (
        ids::CRAFT_RECIPE_EVENT_RECIPE_AMOUNT,
        offset(
            "keen::ecs::CraftRecipeEvent",
            "recipeAmount",
            "keen::uint16",
        ),
    ),
    (
        ids::CRAFT_RECIPE_SUCCESS_EVENT,
        Fact::Type("keen::ecs::CraftRecipeSuccessEvent"),
    ),
    (
        ids::CRAFT_RECIPE_SUCCESS_EVENT_WORKSHOP_ID,
        offset(
            "keen::ecs::CraftRecipeSuccessEvent",
            "workshopId",
            "keen::WorkshopId",
        ),
    ),
    (
        ids::CRAFT_RECIPE_SUCCESS_EVENT_RECIPE_ID,
        offset(
            "keen::ecs::CraftRecipeSuccessEvent",
            "recipeId",
            "keen::RecipeId",
        ),
    ),
    (
        ids::ACCEPT_OFFERED_INTERACTION_EVENT,
        Fact::Type("keen::ecs::AcceptOfferedInteractionEvent"),
    ),
    (
        ids::ACCEPT_OFFERED_INTERACTION_EVENT_GUEST_ENTITY_ID,
        offset(
            "keen::ecs::AcceptOfferedInteractionEvent",
            "guestEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::ACCEPT_OFFERED_INTERACTION_EVENT_HOST_ENTITY_ID,
        offset(
            "keen::ecs::AcceptOfferedInteractionEvent",
            "hostEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::ACCEPT_OFFERED_INTERACTION_EVENT_OFFER_ID,
        offset(
            "keen::ecs::AcceptOfferedInteractionEvent",
            "offerId",
            "keen::uint32",
        ),
    ),
    (
        ids::BUILDING_PLACE_EVENT,
        Fact::Type("keen::ecs::BuildingPlaceEvent"),
    ),
    (
        ids::BUILDING_PLACE_EVENT_OWNER_ID,
        offset(
            "keen::ecs::BuildingPlaceEvent",
            "ownerId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::BUILDING_PLACE_EVENT_POSITION,
        offset("keen::ecs::BuildingPlaceEvent", "position", "keen::float3"),
    ),
    (
        ids::BUILDING_PLACE_EVENT_ORIENTATION,
        offset(
            "keen::ecs::BuildingPlaceEvent",
            "orientation",
            "keen::float4",
        ),
    ),
    (
        ids::BUILDING_PLACE_EVENT_VOLUME_MIN,
        offset("keen::ecs::BuildingPlaceEvent", "volumeMin", "keen::float3"),
    ),
    (
        ids::BUILDING_PLACE_EVENT_VOLUME_MAX,
        offset("keen::ecs::BuildingPlaceEvent", "volumeMax", "keen::float3"),
    ),
    (
        ids::BUILDING_PLACE_EVENT_MATERIAL,
        offset(
            "keen::ecs::BuildingPlaceEvent",
            "material",
            "keen::MaterialFeedbackId",
        ),
    ),
    (
        ids::BUILDING_PLACE_EVENT_TRACKING_ITEM_ID,
        offset(
            "keen::ecs::BuildingPlaceEvent",
            "trackingItemId",
            "keen::ItemId",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT,
        Fact::Type("keen::ecs::BuildingTearDownEvent"),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_OWNER_ID,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "ownerId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_POSITION,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "position",
            "keen::float3",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_ORIENTATION,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "orientation",
            "keen::float4",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MIN,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "volumeMin",
            "keen::float3",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_VOLUME_MAX,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "volumeMax",
            "keen::float3",
        ),
    ),
    (
        ids::BUILDING_TEAR_DOWN_EVENT_MATERIAL,
        offset(
            "keen::ecs::BuildingTearDownEvent",
            "material",
            "keen::MaterialFeedbackId",
        ),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT,
        Fact::Type("keen::ecs::BuildingPickUpEvent"),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_OWNER_ID,
        offset(
            "keen::ecs::BuildingPickUpEvent",
            "ownerId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_POSITION,
        offset("keen::ecs::BuildingPickUpEvent", "position", "keen::float3"),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_VOLUME_MIN,
        offset(
            "keen::ecs::BuildingPickUpEvent",
            "volumeMin",
            "keen::float3",
        ),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_VOLUME_MAX,
        offset(
            "keen::ecs::BuildingPickUpEvent",
            "volumeMax",
            "keen::float3",
        ),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_MATERIAL,
        offset(
            "keen::ecs::BuildingPickUpEvent",
            "material",
            "keen::MaterialFeedbackId",
        ),
    ),
    (
        ids::BUILDING_PICK_UP_EVENT_PICKED_ENTITY_ID,
        offset(
            "keen::ecs::BuildingPickUpEvent",
            "pickedEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::FLAME_ALTAR_EVENT,
        Fact::Type("keen::ecs::FlameAltarEvent"),
    ),
    (
        ids::FLAME_ALTAR_EVENT_PLAYER_ENTITY_ID,
        offset(
            "keen::ecs::FlameAltarEvent",
            "playerEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::FLAME_ALTAR_EVENT_BASE_ENTITY_ID,
        offset(
            "keen::ecs::FlameAltarEvent",
            "baseEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::FLAME_ALTAR_EVENT_TYPE,
        offset(
            "keen::ecs::FlameAltarEvent",
            "type",
            "keen::ecs::FlameAltarActionType",
        ),
    ),
    (
        ids::LOOT_INTERACTION_EVENT,
        Fact::Type("keen::ecs::LootInteractionEvent"),
    ),
    (
        ids::LOOT_INTERACTION_EVENT_GUEST_ID,
        offset(
            "keen::ecs::LootInteractionEvent",
            "guestId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::LOOT_INTERACTION_EVENT_HOST_ID,
        offset(
            "keen::ecs::LootInteractionEvent",
            "hostId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::LOOT_INTERACTION_EVENT_LOOT_ALL,
        offset("keen::ecs::LootInteractionEvent", "lootAll", "keen::bool"),
    ),
    (
        ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT,
        Fact::Type("keen::ecs::HandleEquipmentSlotChangedEvent"),
    ),
    (
        ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_PLAYER_ENTITY_ID,
        offset(
            "keen::ecs::HandleEquipmentSlotChangedEvent",
            "playerEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT,
        offset(
            "keen::ecs::HandleEquipmentSlotChangedEvent",
            "equipmentSlot",
            "keen::EquipmentSlot",
        ),
    ),
    (
        ids::REVIVED_PLAYER_EVENT,
        Fact::Type("keen::ecs::RevivedPlayerEvent"),
    ),
    (
        ids::REVIVED_PLAYER_EVENT_TARGET_ENTITY_ID,
        offset(
            "keen::ecs::RevivedPlayerEvent",
            "targetEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::REVIVED_PLAYER_EVENT_SOURCE_ENTITY_ID,
        offset(
            "keen::ecs::RevivedPlayerEvent",
            "sourceEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::WORLD_TRANSFORM_POSITION,
        offset("keen::WorldTransform", "position", "keen::WorldPosition"),
    ),
    (
        ids::WORLD_TRANSFORM_ORIENTATION,
        offset("keen::WorldTransform", "orientation", "keen::Quaternion"),
    ),
    (
        ids::WORLD_TRANSFORM_SCALE,
        offset("keen::WorldTransform", "scale", "keen::float3"),
    ),
    (
        ids::NPC_COMMAND_EVENT,
        Fact::Type("keen::ecs::NpcCommandEvent"),
    ),
    (
        ids::NPC_COMMAND_EVENT_TYPE,
        offset(
            "keen::ecs::NpcCommandEvent",
            "type",
            "keen::ecs::NpcCommandType",
        ),
    ),
    (
        ids::NPC_COMMAND_EVENT_NPC_ID,
        offset("keen::ecs::NpcCommandEvent", "npcId", "keen::NpcId"),
    ),
    (
        ids::NPC_COMMAND_EVENT_TARGET_TRANSFORM,
        offset(
            "keen::ecs::NpcCommandEvent",
            "targetTransform",
            "keen::WorldTransform",
        ),
    ),
    (
        ids::SET_TOGGLE_STATE_EVENT,
        Fact::Type("keen::ecs::SetToggleStateEvent"),
    ),
    (
        ids::SET_TOGGLE_STATE_EVENT_SENDER_ID,
        offset(
            "keen::ecs::SetToggleStateEvent",
            "senderId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::SET_TOGGLE_STATE_EVENT_SIGNAL_ID,
        offset(
            "keen::ecs::SetToggleStateEvent",
            "signalId",
            "keen::SignalId",
        ),
    ),
    (
        ids::SET_TOGGLE_STATE_EVENT_NEW_STATE,
        offset("keen::ecs::SetToggleStateEvent", "newState", "keen::bool"),
    ),
    (
        ids::ADD_THREAT_TARGET_EVENT,
        Fact::Type("keen::ecs::AddThreatTargetEvent"),
    ),
    (
        ids::ADD_THREAT_TARGET_EVENT_ATTACKER_ID,
        offset(
            "keen::ecs::AddThreatTargetEvent",
            "attackerId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::ADD_THREAT_TARGET_EVENT_TARGET_ID,
        offset(
            "keen::ecs::AddThreatTargetEvent",
            "targetId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::ADD_THREAT_TARGET_EVENT_THREAT_VALUE,
        offset(
            "keen::ecs::AddThreatTargetEvent",
            "threatValue",
            "keen::float32",
        ),
    ),
    (ids::UI_PING_EVENT, Fact::Type("keen::ecs::UiPingEvent")),
    (
        ids::UI_PING_EVENT_SENDER_ENTITY_ID,
        offset(
            "keen::ecs::UiPingEvent",
            "senderEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::UI_PING_POSITION,
        offset("keen::ecs::UiPingEvent", "position", "keen::WorldPosition"),
    ),
    (ids::ITEM_INFO, Fact::Type("keen::ItemInfo")),
    (
        ids::ITEM_INFO_ID,
        offset("keen::ItemInfo", "itemId", "keen::ItemId"),
    ),
    (
        ids::ITEM_INFO_OBJECT,
        offset("keen::ItemInfo", "objectId", "keen::Guid"),
    ),
    (
        ids::ITEM_INFO_NAME,
        offset("keen::ItemInfo", "name", "keen::NameLocaTag"),
    ),
    (
        ids::ITEM_INFO_DESCRIPTION,
        offset("keen::ItemInfo", "description", "keen::DescriptionLocaTag"),
    ),
    (
        ids::ITEM_INFO_FLAGS,
        offset("keen::ItemInfo", "flags", "keen::ItemFlags"),
    ),
    (
        ids::ITEM_INFO_CATEGORY,
        offset("keen::ItemInfo", "category", "keen::ItemCategory"),
    ),
    (
        ids::ITEM_INFO_MAX_STACK,
        offset("keen::ItemInfo", "maxStackSize", "keen::uint16"),
    ),
    (
        ids::ITEM_INFO_PID_ENTITY,
        offset(
            "keen::ItemInfo",
            "pidEntity",
            "keen::ecs::TemplateReference",
        ),
    ),
    (
        ids::ITEM_INFO_UI_VALUES,
        offset(
            "keen::ItemInfo",
            "uiValues",
            "keen::BlobArray<keen::ImpactUiValue>",
        ),
    ),
    (
        ids::ITEM_INFO_ICON_IMAGE,
        offset("keen::ItemInfo", "iconImage", "keen::UiImageReference"),
    ),
    (
        ids::ITEM_INFO_PLACED_ENTITY,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "placedEntity"),
            ],
            holds: "keen::ecs::TemplateReference",
        },
    ),
    (
        ids::ITEM_INFO_BUILDING_VOXEL,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "voxelData"),
                ("keen::ChangeVoxelData", "isBuildingVoxel"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ITEM_INFO_PLACE_VOXEL_MATERIAL_ID,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "voxelData"),
                ("keen::ChangeVoxelData", "placeVoxelMaterialId"),
            ],
            holds: "keen::uint8",
        },
    ),
    (
        ids::ITEM_INFO_EQUIPMENT_SLOT,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "slot"),
            ],
            holds: "keen::EquipmentSlotType",
        },
    ),
    (
        ids::ITEM_INFO_TAGS,
        offset(
            "keen::ItemInfo",
            "tags",
            "keen::BlobArray<keen::ItemTagWeight>",
        ),
    ),
    (
        ids::ITEM_INFO_DAMAGE_MOD,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "damageSetup"),
                ("keen::ItemDamageSetup", "dmgMod"),
            ],
            holds: "keen::float32",
        },
    ),
    (
        ids::ITEM_INFO_SPEED_MOD,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "damageSetup"),
                ("keen::ItemDamageSetup", "speedMod"),
            ],
            holds: "keen::float32",
        },
    ),
    (
        ids::ITEM_INFO_DAMAGE_SET,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "damageSetup"),
                ("keen::ItemDamageSetup", "isSet"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ITEM_INFO_ARMOR,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "armorSetup"),
                ("keen::ItemArmorSetup", "distribution"),
            ],
            holds: "keen::impact::ArmorDistribution",
        },
    ),
    (
        ids::ITEM_INFO_ARMOR_SET,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "armorSetup"),
                ("keen::ItemArmorSetup", "isSet"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ITEM_INFO_BURN_TIME,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "fuelSetup"),
                ("keen::ItemFuelData", "burnTime"),
            ],
            holds: "keen::Time",
        },
    ),
    (
        ids::ITEM_INFO_ENERGY_OUTPUT,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "fuelSetup"),
                ("keen::ItemFuelData", "energyOutput"),
            ],
            holds: "keen::uint32",
        },
    ),
    (
        ids::ITEM_INFO_FUEL_SET,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "fuelSetup"),
                ("keen::ItemFuelData", "isSet"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ITEM_INFO_COMFORT_AMOUNT,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "comfortSetup"),
                ("keen::ItemComfortSetup", "comfortAmount"),
            ],
            holds: "keen::uint32",
        },
    ),
    (
        ids::ITEM_INFO_COMFORT_SET,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "comfortSetup"),
                ("keen::ItemComfortSetup", "isSet"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ARMOR_DISTRIBUTION,
        Fact::Type("keen::impact::ArmorDistribution"),
    ),
    (ids::ITEM_SLOT_TYPE, Fact::Type("keen::EquipmentSlotType")),
    (ids::ITEM_TAG_WEIGHT, Fact::Type("keen::ItemTagWeight")),
    (
        ids::ITEM_TAG_WEIGHT_TAG,
        offset("keen::ItemTagWeight", "tag", "keen::ItemTagReference"),
    ),
    (ids::ITEM_STAT, Fact::Type("keen::ImpactUiValue")),
    (
        ids::ITEM_STAT_LABEL,
        offset("keen::ImpactUiValue", "locaId", "keen::LocaTagId"),
    ),
    (
        ids::ITEM_STAT_FORMAT,
        offset(
            "keen::ImpactUiValue",
            "valueFormat",
            "keen::impact::ImpcatValueNumberFormat",
        ),
    ),
    (
        ids::ITEM_STAT_VALUE,
        offset("keen::ImpactUiValue", "value", "keen::sint32"),
    ),
    (
        ids::ITEM_STAT_SIGNED,
        offset("keen::ImpactUiValue", "isSigned", "keen::bool"),
    ),
    (ids::ITEM_REGISTRY, Fact::Type("keen::ItemRegistryResource")),
    (
        ids::ITEM_REGISTRY_REFS,
        offset(
            "keen::ItemRegistryResource",
            "itemRefs",
            "keen::BlobArray<keen::ItemReference>",
        ),
    ),
    (
        ids::ITEM_REGISTRY_NAMES,
        offset(
            "keen::ItemRegistryResource",
            "dbgNames",
            "keen::BlobArray<keen::BlobString>",
        ),
    ),
    (ids::TEMPLATE, Fact::Type("keen::ecs::TemplateResource")),
    (
        ids::TEMPLATE_COMPONENTS,
        offset(
            "keen::ecs::TemplateResource",
            "components",
            "keen::BlobArray<keen::BlobVariant<keen::ecs::Component>>",
        ),
    ),
    (
        ids::TEMPLATE_NAME,
        offset("keen::ecs::TemplateResource", "name", "keen::string"),
    ),
    (
        ids::TEMPLATE_COLLECTION,
        Fact::Type("keen::ecs::TemplateCollectionResource"),
    ),
    (
        ids::TEMPLATE_COLLECTION_TEMPLATES,
        offset(
            "keen::ecs::TemplateCollectionResource",
            "templates",
            "keen::BlobArray<keen::ecs::TemplateResourceReference>",
        ),
    ),
    (ids::FUEL_SETUP, Fact::Type("keen::ecs::FuelSetup")),
    (
        ids::FUEL_SETUP_MAX,
        offset("keen::ecs::FuelSetup", "maxFuelCharges", "keen::sint32"),
    ),
    (
        ids::FUEL_SETUP_START,
        offset(
            "keen::ecs::FuelSetup",
            "initialFuelPercentage",
            "keen::float",
        ),
    ),
    (ids::CRAFTING, Fact::Type("keen::ecs::Crafting")),
    (
        ids::CRAFTING_WORKSHOP,
        offset(
            "keen::ecs::Crafting",
            "workshop",
            "keen::ObjectReference<keen::WorkshopDefinition>",
        ),
    ),
    (
        ids::CRAFTING_WORKSHOP_ID,
        offset("keen::ecs::Crafting", "workshopId", "keen::WorkshopId"),
    ),
    (
        ids::CRAFTING_INTERACTION,
        Fact::Type("keen::ecs::CraftingInteraction"),
    ),
    (
        ids::CRAFTING_INTERACTION_WORKSHOP,
        offset(
            "keen::ecs::CraftingInteraction",
            "workshop",
            "keen::ObjectReference<keen::WorkshopDefinition>",
        ),
    ),
    (
        ids::CRAFTING_INTERACTION_WORKSHOP_ID,
        offset(
            "keen::ecs::CraftingInteraction",
            "workshopId",
            "keen::WorkshopId",
        ),
    ),
    (
        ids::CLIENT_CRAFTING_INTERACTION,
        Fact::Type("keen::ecs::ClientCraftingInteraction"),
    ),
    (
        ids::CLIENT_CRAFTING_INTERACTION_WORKSHOP_ID,
        offset(
            "keen::ecs::ClientCraftingInteraction",
            "workshopId",
            "keen::WorkshopId",
        ),
    ),
    (
        ids::WORKSHOP_REGISTRY,
        Fact::Type("keen::WorkshopRegistryResource"),
    ),
    (
        ids::WORKSHOP_REGISTRY_WORKSHOPS,
        offset(
            "keen::WorkshopRegistryResource",
            "workshops",
            "keen::BlobArray<keen::WorkshopResource>",
        ),
    ),
    (ids::WORKSHOP, Fact::Type("keen::WorkshopResource")),
    (
        ids::WORKSHOP_GUID,
        offset("keen::WorkshopResource", "workshopGuid", "keen::Guid"),
    ),
    (
        ids::WORKSHOP_ID,
        offset("keen::WorkshopResource", "workshopId", "keen::WorkshopId"),
    ),
    (
        ids::WORKSHOP_NAME,
        offset("keen::WorkshopResource", "name", "keen::LocaTagId"),
    ),
    (
        ids::WORKSHOP_ITEM,
        offset("keen::WorkshopResource", "item", "keen::ItemId"),
    ),
    (
        ids::WORKSHOP_ITEM_REF,
        offset("keen::WorkshopResource", "itemRef", "keen::ItemReference"),
    ),
    (
        ids::WORKSHOP_NPC,
        offset("keen::WorkshopResource", "isNpc", "keen::bool"),
    ),
    (
        ids::RECIPE_REGISTRY,
        Fact::Type("keen::RecipeRegistryResource"),
    ),
    (
        ids::RECIPE_REGISTRY_RECIPES,
        offset(
            "keen::RecipeRegistryResource",
            "recipes",
            "keen::BlobArray<keen::RecipeInfo>",
        ),
    ),
    (ids::RECIPE, Fact::Type("keen::RecipeInfo")),
    (
        ids::RECIPE_GUID,
        offset("keen::RecipeInfo", "recipeGuid", "keen::Guid"),
    ),
    (
        ids::RECIPE_ID,
        offset("keen::RecipeInfo", "recipeId", "keen::RecipeId"),
    ),
    (
        ids::RECIPE_WORKSHOP_GUID,
        offset("keen::RecipeInfo", "workshopGuid", "keen::Guid"),
    ),
    (
        ids::RECIPE_WORKSHOP,
        offset("keen::RecipeInfo", "workshopId", "keen::WorkshopId"),
    ),
    (
        ids::RECIPE_INPUT,
        offset(
            "keen::RecipeInfo",
            "input",
            "keen::BlobArray<keen::RecipeInputResource>",
        ),
    ),
    (
        ids::RECIPE_OUTPUT,
        offset(
            "keen::RecipeInfo",
            "output",
            "keen::BlobArray<keen::RecipeItemStackResource>",
        ),
    ),
    (
        ids::RECIPE_COMPLETION,
        offset(
            "keen::RecipeInfo",
            "completionRequirementQuery",
            "keen::KnowledgeQueryData",
        ),
    ),
    (
        ids::RECIPE_DURATION,
        offset("keen::RecipeInfo", "craftingDuration", "keen::Time"),
    ),
    (
        ids::RECIPE_REQUIRED_PROPS,
        offset(
            "keen::RecipeInfo",
            "requiredProps",
            "keen::BlobArray<keen::CraftingPropId>",
        ),
    ),
    (
        ids::RECIPE_DEBUG_NAME,
        offset("keen::RecipeInfo", "debugName", "keen::BlobString"),
    ),
    (
        ids::RECIPE_INGREDIENT,
        Fact::Type("keen::RecipeInputResource"),
    ),
    (
        ids::RECIPE_STACK,
        Fact::Type("keen::RecipeItemStackResource"),
    ),
    (
        ids::RECIPE_STACK_ITEM,
        offset("keen::RecipeItemStackResource", "item", "keen::ItemId"),
    ),
    (
        ids::RECIPE_STACK_COUNT,
        offset("keen::RecipeItemStackResource", "count", "keen::uint32"),
    ),
    (ids::UI_IMAGE, Fact::Type("keen::UiTextureResource")),
    (ids::UI_BUNDLE, Fact::Type("keen::FbUiBundle")),
    (
        ids::UI_BUNDLE_CRAFTING_MENU,
        Fact::Path {
            steps: &[
                ("keen::FbUiBundle", "menu"),
                ("keen::FbUiMenu", "crafting"),
                ("keen::FbUiCrafting", "recipes"),
            ],
            holds: "keen::FbUiCraftingRecipeCollection",
        },
    ),
    (
        ids::CRAFTING_MENU_WORKSHOPS,
        offset(
            "keen::FbUiCraftingRecipeCollection",
            "workshops",
            "keen::BlobArray<keen::WorkshopId>",
        ),
    ),
    (
        ids::CRAFTING_MENU_TREES,
        offset(
            "keen::FbUiCraftingRecipeCollection",
            "trees",
            "keen::BlobArray<keen::FbUiCraftingRecipeTree>",
        ),
    ),
    (
        ids::CRAFTING_MENU_MAX_GROUPS,
        offset(
            "keen::FbUiCraftingRecipeCollection",
            "maxRecipeGroupCount",
            "keen::uint32",
        ),
    ),
    (
        ids::CRAFTING_MENU_MAX_SETS,
        offset(
            "keen::FbUiCraftingRecipeCollection",
            "maxRecipeSetCount",
            "keen::uint32",
        ),
    ),
    (
        ids::CRAFTING_MENU_TREE,
        Fact::Type("keen::FbUiCraftingRecipeTree"),
    ),
    (
        ids::CRAFTING_MENU_TREE_GROUPS,
        offset(
            "keen::FbUiCraftingRecipeTree",
            "groups",
            "keen::BlobArray<keen::FbUiCraftingRecipeGroup>",
        ),
    ),
    (
        ids::CRAFTING_MENU_GROUP,
        Fact::Type("keen::FbUiCraftingRecipeGroup"),
    ),
    (
        ids::CRAFTING_MENU_GROUP_ID,
        offset("keen::FbUiCraftingRecipeGroup", "groupId", "keen::Guid"),
    ),
    (
        ids::CRAFTING_MENU_GROUP_SETS,
        offset(
            "keen::FbUiCraftingRecipeGroup",
            "sets",
            "keen::BlobArray<keen::FbUiCraftingRecipeSet>",
        ),
    ),
    (
        ids::CRAFTING_MENU_SET,
        Fact::Type("keen::FbUiCraftingRecipeSet"),
    ),
    (
        ids::CRAFTING_MENU_SET_ENTRIES,
        offset(
            "keen::FbUiCraftingRecipeSet",
            "entries",
            "keen::BlobArray<keen::RecipeId>",
        ),
    ),
    (
        ids::CURRENT_TRANSFORM,
        Fact::Type("keen::ecs::CurrentTransform"),
    ),
    (
        ids::CURRENT_TRANSFORM_TRANSFORM,
        offset(
            "keen::ecs::CurrentTransform",
            "transform",
            "keen::WorldTransform",
        ),
    ),
    (ids::USED_ITEM, Fact::Type("keen::ecs::UsedItem")),
    (ids::DO_NOT_SAVE, Fact::Type("keen::ecs::DoNotSave")),
    (ids::LABEL, Fact::Type("keen::ecs::CustomPlayerString")),
    (
        ids::INVENTORY_SETUP,
        Fact::Type("keen::ecs::InventorySetup"),
    ),
    (
        ids::INVENTORY_SETUP_LINKS,
        offset(
            "keen::ecs::InventorySetup",
            "linksEntities",
            "keen::StaticArray<keen::ecs::EntityId,16>",
        ),
    ),
    (
        ids::INVENTORY_SETUP_CATEGORIES,
        offset(
            "keen::ecs::InventorySetup",
            "linksCategories",
            "keen::StaticArray<keen::InventoryCategory,16>",
        ),
    ),
    (
        ids::INVENTORY_SETUP_INITIALIZED,
        offset("keen::ecs::InventorySetup", "isInitialized", "keen::bool"),
    ),
    (
        ids::INVENTORY_SETUP_GENERIC_SLOTS,
        offset(
            "keen::ecs::InventorySetup",
            "genericSlotCount",
            "keen::uint8",
        ),
    ),
    (
        ids::INVENTORY_SETUP_AVAILABLE_SLOTS,
        offset(
            "keen::ecs::InventorySetup",
            "availableSlotCount",
            "keen::uint8",
        ),
    ),
    (
        ids::INVENTORY_SETUP_OVERWRITE,
        Fact::Type("keen::ecs::InventorySetupOverwrite"),
    ),
    (
        ids::INVENTORY_SETUP_OVERWRITE_GENERIC_SLOTS,
        offset(
            "keen::ecs::InventorySetupOverwrite",
            "genericSlotCount",
            "keen::uint8",
        ),
    ),
    (ids::INVENTORY, Fact::Type("keen::ecs::Inventory")),
    (ids::ITEM_STACK, Fact::Type("keen::ItemStack")),
    (
        ids::ITEM_STACK_COUNT,
        offset("keen::ItemStack", "data", "keen::ItemData"),
    ),
    (
        ids::ITEM_STACK_PIDE,
        // `data` holds the u32 count, then the id of the item's own entity.
        Fact::Offset {
            ty: "keen::ItemStack",
            field: "data",
            plus: 4,
            holds: "keen::ItemData",
        },
    ),
    (ids::DURABILITY, Fact::Type("keen::ecs::NetworkDurability")),
    (
        ids::DURABILITY_MAX,
        offset(
            "keen::ecs::NetworkDurability",
            "durabilityMax",
            "keen::uint16",
        ),
    ),
    (ids::FUEL, Fact::Type("keen::ecs::NetworkFuel")),
    (
        ids::FUEL_MAX,
        offset("keen::ecs::NetworkFuel", "fuelMax", "keen::uint16"),
    ),
    (ids::OWNER, Fact::Type("keen::ecs::OwnerRelationship")),
    (
        ids::INVENTORY_EMPTY_STATE,
        Fact::Type("keen::ecs::InventoryEmptyState"),
    ),
    (
        ids::INVENTORY_EMPTY_STATE_EMPTY,
        offset("keen::ecs::InventoryEmptyState", "isEmpty", "keen::bool"),
    ),
    (ids::DESTROY_ON_LOOT, Fact::Type("keen::ecs::DestroyOnLoot")),
    (ids::LIFETIME, Fact::Type("keen::ecs::LifeTime")),
    (ids::AREA_LOOT, Fact::Type("keen::ecs::AreaLootOverride")),
    (
        ids::AREA_LOOT_EXCLUDE,
        offset("keen::ecs::AreaLootOverride", "exclude", "keen::bool"),
    ),
    (
        ids::INTERACTION_OFFER,
        Fact::Type("keen::ecs::InteractionOfferRuntime"),
    ),
    (
        ids::INTERACTION_OFFER_ACCEPT_TIME,
        offset(
            "keen::ecs::InteractionOfferRuntime",
            "lastAcceptTime",
            "keen::Time",
        ),
    ),
    (
        ids::INTERACTION_OFFER_ACCEPT_GUEST,
        Fact::Path {
            steps: &[
                ("keen::ecs::InteractionOfferRuntime", "lastAcceptData"),
                ("keen::ecs::InteractionAcceptData", "guestId"),
            ],
            holds: "keen::ecs::EntityId",
        },
    ),
    (
        ids::INTERACTION_OFFER_DISABLED,
        offset(
            "keen::ecs::InteractionOfferRuntime",
            "isDisabled",
            "keen::bool",
        ),
    ),
    (ids::PLAYER_INPUT, Fact::Type("keen::ecs::PlayerInput")),
    (
        ids::PLAYER_INPUT_DIGITAL,
        Fact::Path {
            steps: &[
                ("keen::ecs::PlayerInput", "fromClient"),
                ("keen::ecs::ClientPlayerInputData", "digitalInput"),
            ],
            holds: "keen::ecs::PlayerDigitalInput",
        },
    ),
    (
        ids::PLAYER_INPUT_TYPE,
        Fact::Type("keen::ecs::PlayerInputType"),
    ),
    (
        ids::CLIENT_PLAYER_INPUT,
        Fact::Type("keen::ecs::ClientPlayerInput"),
    ),
    (
        ids::CLIENT_PLAYER_INPUT_DIGITAL,
        Fact::Path {
            steps: &[
                ("keen::ecs::ClientPlayerInput", "data"),
                ("keen::ecs::ClientPlayerInputData", "digitalInput"),
            ],
            holds: "keen::ecs::PlayerDigitalInput",
        },
    ),
    (
        ids::SELECTED_TARGETS,
        Fact::Type("keen::ecs::SelectedTargets"),
    ),
    (
        ids::SELECTED_TARGETS_TARGETS,
        offset(
            "keen::ecs::SelectedTargets",
            "targets",
            "keen::StaticArray<keen::ecs::TargetInfo,16>",
        ),
    ),
    (ids::TARGET_INFO, Fact::Type("keen::ecs::TargetInfo")),
    (
        ids::TARGET_INFO_ENTITY,
        offset(
            "keen::ecs::TargetInfo",
            "targetEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (ids::TARGET_TYPE, Fact::Type("keen::ecs::TargetType")),
    (ids::HUD_SCREEN_TYPE, Fact::Type("keen::HudScreenType")),
    (
        ids::HUD_SCREEN_IN_GAME,
        Fact::Constant {
            ty: "keen::HudScreenType",
            name: "InGame",
        },
    ),
    (ids::BOSS, Fact::Type("keen::ecs::Boss")),
    (
        ids::INTERACTION_LOCK,
        Fact::Type("keen::ecs::InteractionLock"),
    ),
    (
        ids::INTERACTION_LOCK_ITEM,
        offset("keen::ecs::InteractionLock", "requiredItem", "keen::ItemId"),
    ),
    (ids::LOOT_SLOT, Fact::Type("keen::LootSlotInfo")),
    (
        ids::LOOT_SLOT_ITEM,
        offset("keen::LootSlotInfo", "itemId", "keen::ItemId"),
    ),
    (
        ids::LOOT_SLOT_COUNT,
        offset("keen::LootSlotInfo", "stackSize", "keen::uint16"),
    ),
    (
        ids::LOOTABLE_ITEMS,
        Fact::Type("keen::LootableItemsResource"),
    ),
    (
        ids::LOOTABLE_ITEMS_ITEMS,
        offset(
            "keen::LootableItemsResource",
            "items",
            "keen::BlobArray<keen::LootableItemInfo>",
        ),
    ),
    (ids::LOOTABLE_ITEM, Fact::Type("keen::LootableItemInfo")),
    (
        ids::LOOTABLE_ITEM_ITEM,
        offset("keen::LootableItemInfo", "itemId", "keen::ItemId"),
    ),
    (
        ids::LOOTABLE_ITEM_STACK_MIN,
        offset("keen::LootableItemInfo", "stackSizeMin", "keen::uint16"),
    ),
    (
        ids::LOOTABLE_ITEM_STACK_MAX,
        offset("keen::LootableItemInfo", "stackSizeMax", "keen::uint16"),
    ),
    (
        ids::LOOTABLE_ITEM_STACK_MAX_SCALED,
        offset(
            "keen::LootableItemInfo",
            "stackSizeMaxScaled",
            "keen::uint16",
        ),
    ),
    (
        ids::LOOT_FILL_TABLE,
        Fact::Type("keen::ecs::DefaultInventoryResource"),
    ),
    (
        ids::LOOT_FILL_TABLE_ROOT,
        offset(
            "keen::ecs::DefaultInventoryResource",
            "rootGroup",
            "keen::ecs::DefaultInventoryGroupResource",
        ),
    ),
    (
        ids::LOOT_FILL_GROUP,
        Fact::Type("keen::ecs::DefaultInventoryGroupResource"),
    ),
    (
        ids::LOOT_FILL_GROUP_STACKS,
        offset(
            "keen::ecs::DefaultInventoryGroupResource",
            "stacks",
            "keen::BlobArray<keen::ecs::ItemStackInfoResource>",
        ),
    ),
    (
        ids::LOOT_FILL_GROUP_GROUPS,
        offset(
            "keen::ecs::DefaultInventoryGroupResource",
            "groups",
            "keen::BlobArray<keen::ecs::DefaultInventoryGroupResource>",
        ),
    ),
    (
        ids::LOOT_FILL_STACK,
        Fact::Type("keen::ecs::ItemStackInfoResource"),
    ),
    (
        ids::LOOT_FILL_STACK_ITEM,
        offset("keen::ecs::ItemStackInfoResource", "item", "keen::ItemId"),
    ),
    (
        ids::LOOT_FILL_STACK_COUNT_MIN,
        offset(
            "keen::ecs::ItemStackInfoResource",
            "countMin",
            "keen::uint32",
        ),
    ),
    (
        ids::LOOT_FILL_STACK_COUNT_MAX,
        offset(
            "keen::ecs::ItemStackInfoResource",
            "countMax",
            "keen::uint32",
        ),
    ),
    (
        ids::LOOT_FILL_STACK_PROBABILITY,
        offset(
            "keen::ecs::ItemStackInfoResource",
            "spawnProbability",
            "keen::float",
        ),
    ),
    (
        ids::LOOT_FILL_STACK_WEIGHT,
        offset("keen::ecs::ItemStackInfoResource", "weight", "keen::float"),
    ),
    (ids::ATTRIBUTE_SET_HEALTH, Fact::Type("keen::ecs::Health")),
    (ids::ATTRIBUTE_SET_STAMINA, Fact::Type("keen::ecs::Stamina")),
    (ids::ATTRIBUTE_SET_MANA, Fact::Type("keen::ecs::Mana")),
    (ids::ATTRIBUTE_SET_LEVEL, Fact::Type("keen::ecs::Level")),
    (
        ids::ATTRIBUTE_SET_EXPERIENCE,
        Fact::Type("keen::ecs::Experience"),
    ),
    (ids::ATTRIBUTE_SET_FUEL, Fact::Type("keen::ecs::Fuel")),
    (
        ids::ATTRIBUTE_SET_SHROUD,
        Fact::Type("keen::ecs::FogResistance"),
    ),
    (
        ids::SKILL_NODES,
        Fact::Type("keen::ecs::UnlockedSkillNodes"),
    ),
    (
        ids::SKILL_NODES_IDS,
        offset(
            "keen::ecs::UnlockedSkillNodes",
            "knownSkillNodes",
            "keen::StaticArray<keen::SkillNodeId,256>",
        ),
    ),
    (
        ids::SKILL_NODES_LEVELS,
        offset(
            "keen::ecs::UnlockedSkillNodes",
            "unlockLevel",
            "keen::StaticArray<keen::SkillUnlockLevel,256>",
        ),
    ),
    (ids::BASE_ACTOR, Fact::Type("keen::ecs::BaseActor")),
    (
        ids::BASE_ACTOR_STATE,
        offset(
            "keen::ecs::BaseActor",
            "currentState",
            "keen::actor::StateMask",
        ),
    ),
    (ids::NPC, Fact::Type("keen::ecs::ActiveNpcState")),
    (ids::ANIMAL, Fact::Type("keen::ecs::Animal")),
    (ids::LOOT_FILL, Fact::Type("keen::ecs::DefaultInventory")),
    (
        ids::LOOT_FILL_DEFAULT_INVENTORY,
        offset(
            "keen::ecs::DefaultInventory",
            "defaultInventory",
            "keen::ecs::DefaultInventoryReference",
        ),
    ),
    (ids::RESOURCE_NODE, Fact::Type("keen::ecs::MiningNode")),
    (ids::GAME_SETTINGS, Fact::Type(GAME_SETTINGS)),
    (
        ids::GAME_SETTINGS_PLAYER_HEALTH_FACTOR,
        offset(GAME_SETTINGS, "playerHealthFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_MANA_FACTOR,
        offset(GAME_SETTINGS, "playerManaFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_STAMINA_FACTOR,
        offset(GAME_SETTINGS, "playerStaminaFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_BODY_HEAT_FACTOR,
        offset(GAME_SETTINGS, "playerBodyHeatFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PLAYER_DIVING_TIME_FACTOR,
        offset(GAME_SETTINGS, "playerDivingTimeFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_DURABILITY,
        offset(GAME_SETTINGS, "enableDurability", "keen::bool"),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_STARVING_DEBUFF,
        offset(GAME_SETTINGS, "enableStarvingDebuff", "keen::bool"),
    ),
    (
        ids::GAME_SETTINGS_FOOD_BUFF_DURATION_FACTOR,
        offset(GAME_SETTINGS, "foodBuffDurationFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_FROM_HUNGER_TO_STARVING,
        offset(GAME_SETTINGS, "fromHungerToStarving", "keen::Time"),
    ),
    (
        ids::GAME_SETTINGS_SHROUD_TIME_FACTOR,
        offset(GAME_SETTINGS, "shroudTimeFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_TOMBSTONE_MODE,
        offset(GAME_SETTINGS, "tombstoneMode", "keen::ecs::TombstoneMode"),
    ),
    (
        ids::GAME_SETTINGS_ENABLE_GLIDER_TURBULENCES,
        offset(GAME_SETTINGS, "enableGliderTurbulences", "keen::bool"),
    ),
    (
        ids::GAME_SETTINGS_WEATHER_FREQUENCY,
        offset(
            GAME_SETTINGS,
            "weatherFrequency",
            "keen::ecs::WeatherFrequency",
        ),
    ),
    (
        ids::GAME_SETTINGS_FISHING_DIFFICULTY,
        offset(
            GAME_SETTINGS,
            "fishingDifficulty",
            "keen::ecs::GenericDifficulty",
        ),
    ),
    (
        ids::GAME_SETTINGS_MINING_DAMAGE_FACTOR,
        offset(GAME_SETTINGS, "miningDamageFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PLANT_GROWTH_SPEED_FACTOR,
        offset(GAME_SETTINGS, "plantGrowthSpeedFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_RESOURCE_DROP_STACK_AMOUNT_FACTOR,
        offset(
            GAME_SETTINGS,
            "resourceDropStackAmountFactor",
            "keen::float",
        ),
    ),
    (
        ids::GAME_SETTINGS_FACTORY_PRODUCTION_SPEED_FACTOR,
        offset(GAME_SETTINGS, "factoryProductionSpeedFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PERK_UPGRADE_RECYCLING_FACTOR,
        offset(GAME_SETTINGS, "perkUpgradeRecyclingFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PERK_COST_FACTOR,
        offset(GAME_SETTINGS, "perkCostFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_EXPERIENCE_COMBAT_FACTOR,
        offset(GAME_SETTINGS, "experienceCombatFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_EXPERIENCE_MINING_FACTOR,
        offset(GAME_SETTINGS, "experienceMiningFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_EXPERIENCE_EXPLORATION_QUESTS_FACTOR,
        offset(
            GAME_SETTINGS,
            "experienceExplorationQuestsFactor",
            "keen::float",
        ),
    ),
    (
        ids::GAME_SETTINGS_RANDOM_SPAWNER_AMOUNT,
        offset(
            GAME_SETTINGS,
            "randomSpawnerAmount",
            "keen::ecs::RandomSpawnerAmount",
        ),
    ),
    (
        ids::GAME_SETTINGS_AGGRO_POOL_AMOUNT,
        offset(
            GAME_SETTINGS,
            "aggroPoolAmount",
            "keen::ecs::AggroPoolAmount",
        ),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_DAMAGE_FACTOR,
        offset(GAME_SETTINGS, "enemyDamageFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_HEALTH_FACTOR,
        offset(GAME_SETTINGS, "enemyHealthFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_STAMINA_FACTOR,
        offset(GAME_SETTINGS, "enemyStaminaFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_ENEMY_PERCEPTION_RANGE_FACTOR,
        offset(GAME_SETTINGS, "enemyPerceptionRangeFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_BOSS_DAMAGE_FACTOR,
        offset(GAME_SETTINGS, "bossDamageFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_BOSS_HEALTH_FACTOR,
        offset(GAME_SETTINGS, "bossHealthFactor", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_THREAT_BONUS,
        offset(GAME_SETTINGS, "threatBonus", "keen::float"),
    ),
    (
        ids::GAME_SETTINGS_PACIFY_ALL_ENEMIES,
        offset(GAME_SETTINGS, "pacifyAllEnemies", "keen::bool"),
    ),
    (
        ids::GAME_SETTINGS_TAMING_STARTLE_REPERCUSSION,
        offset(
            GAME_SETTINGS,
            "tamingStartleRepercussion",
            "keen::ecs::TamingStartleRepercussion",
        ),
    ),
    (
        ids::GAME_SETTINGS_DAY_TIME_DURATION,
        offset(GAME_SETTINGS, "dayTimeDuration", "keen::Time"),
    ),
    (
        ids::GAME_SETTINGS_NIGHT_TIME_DURATION,
        offset(GAME_SETTINGS, "nightTimeDuration", "keen::Time"),
    ),
    (
        ids::GAME_SETTINGS_CURSE_MODIFIER,
        offset(GAME_SETTINGS, "curseModifier", "keen::ecs::CurseModifier"),
    ),
    (
        ids::GAME_SETTINGS_FLAME_LEVEL_FOG_REMOVAL,
        offset(GAME_SETTINGS, "flameLevelFogRemoval", "keen::uint8"),
    ),
    (
        ids::DAMAGE_TYPE_PHYSICAL_CUT,
        enum_constant(DAMAGE_TYPE, "Physical_Cut"),
    ),
    (
        ids::DAMAGE_TYPE_PHYSICAL_PIERCE,
        enum_constant(DAMAGE_TYPE, "Physical_Pierce"),
    ),
    (
        ids::DAMAGE_TYPE_PHYSICAL_BLUNT,
        enum_constant(DAMAGE_TYPE, "Physical_Blunt"),
    ),
    (
        ids::DAMAGE_TYPE_MAGICAL_FIRE,
        enum_constant(DAMAGE_TYPE, "Magical_Fire"),
    ),
    (
        ids::DAMAGE_TYPE_MAGICAL_ICE,
        enum_constant(DAMAGE_TYPE, "Magical_Ice"),
    ),
    (
        ids::DAMAGE_TYPE_MAGICAL_FOG,
        enum_constant(DAMAGE_TYPE, "Magical_Fog"),
    ),
    (
        ids::DAMAGE_TYPE_MAGICAL_LIGHTNING,
        enum_constant(DAMAGE_TYPE, "Magical_Lightning"),
    ),
    (
        ids::DAMAGE_TYPE_HEALING,
        enum_constant(DAMAGE_TYPE, "Healing"),
    ),
    (
        ids::DAMAGE_TYPE_POISON,
        enum_constant(DAMAGE_TYPE, "Poison"),
    ),
    (ids::DAMAGE_TYPE_WOOD, enum_constant(DAMAGE_TYPE, "Wood")),
    (ids::DAMAGE_TYPE_STONE, enum_constant(DAMAGE_TYPE, "Stone")),
    (ids::DAMAGE_TYPE_METAL, enum_constant(DAMAGE_TYPE, "Metal")),
    (
        ids::DAMAGE_TYPE_EXPLOSION,
        enum_constant(DAMAGE_TYPE, "Explosion"),
    ),
    (
        ids::ATTACK_CATEGORY_MELEE,
        enum_constant(ATTACK_CATEGORY, "Melee"),
    ),
    (
        ids::ATTACK_CATEGORY_RANGED,
        enum_constant(ATTACK_CATEGORY, "Ranged"),
    ),
    (
        ids::ATTACK_CATEGORY_MAGIC,
        enum_constant(ATTACK_CATEGORY, "Magic"),
    ),
    (
        ids::ATTACK_CATEGORY_TOOL,
        enum_constant(ATTACK_CATEGORY, "Tool"),
    ),
    (
        ids::ATTACK_CATEGORY_OTHER,
        enum_constant(ATTACK_CATEGORY, "Other"),
    ),
    (
        ids::ATTACK_CATEGORY_AO_E,
        enum_constant(ATTACK_CATEGORY, "AoE"),
    ),
    (
        ids::ATTACK_CATEGORY_SKILL_SHOT,
        enum_constant(ATTACK_CATEGORY, "SkillShot"),
    ),
    (
        ids::EXPERIENCE_SOURCE_NONE,
        enum_constant(EXPERIENCE_SOURCE, "None"),
    ),
    (
        ids::EXPERIENCE_SOURCE_EXPLORATION,
        enum_constant(EXPERIENCE_SOURCE, "Exploration"),
    ),
    (
        ids::EXPERIENCE_SOURCE_LORE,
        enum_constant(EXPERIENCE_SOURCE, "Lore"),
    ),
    (
        ids::EXPERIENCE_SOURCE_QUEST,
        enum_constant(EXPERIENCE_SOURCE, "Quest"),
    ),
    (
        ids::EXPERIENCE_SOURCE_COMBAT,
        enum_constant(EXPERIENCE_SOURCE, "Combat"),
    ),
    (
        ids::EXPERIENCE_SOURCE_MINING,
        enum_constant(EXPERIENCE_SOURCE, "Mining"),
    ),
    (
        ids::EXPERIENCE_SOURCE_ITEM_SET_COMPLETED,
        enum_constant(EXPERIENCE_SOURCE, "ItemSetCompleted"),
    ),
    (
        ids::ALTAR_ACTION_ABANDON,
        enum_constant(ALTAR_ACTION, "Abandon"),
    ),
    (
        ids::ALTAR_ACTION_RECLAIM,
        enum_constant(ALTAR_ACTION, "Reclaim"),
    ),
    (
        ids::ALTAR_ACTION_REMOVE_WATER,
        enum_constant(ALTAR_ACTION, "RemoveWater"),
    ),
    (
        ids::ALTAR_ACTION_SET_AS_PREFERRED_SPAWN_ALTAR,
        enum_constant(ALTAR_ACTION, "SetAsPreferredSpawnAltar"),
    ),
    (
        ids::EQUIPMENT_SLOT_VANITY_HEAD,
        enum_constant(EQUIPMENT_SLOT, "Vanity_Head"),
    ),
    (
        ids::EQUIPMENT_SLOT_VANITY_UPPER_BODY,
        enum_constant(EQUIPMENT_SLOT, "Vanity_UpperBody"),
    ),
    (
        ids::EQUIPMENT_SLOT_VANITY_ARMS,
        enum_constant(EQUIPMENT_SLOT, "Vanity_Arms"),
    ),
    (
        ids::EQUIPMENT_SLOT_VANITY_LOWER_BODY,
        enum_constant(EQUIPMENT_SLOT, "Vanity_LowerBody"),
    ),
    (
        ids::EQUIPMENT_SLOT_VANITY_FEET,
        enum_constant(EQUIPMENT_SLOT, "Vanity_Feet"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARMOUR_HEAD,
        enum_constant(EQUIPMENT_SLOT, "Armour_Head"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARMOUR_UPPER_BODY,
        enum_constant(EQUIPMENT_SLOT, "Armour_UpperBody"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARMOUR_ARMS,
        enum_constant(EQUIPMENT_SLOT, "Armour_Arms"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARMOUR_LOWER_BODY,
        enum_constant(EQUIPMENT_SLOT, "Armour_LowerBody"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARMOUR_FEET,
        enum_constant(EQUIPMENT_SLOT, "Armour_Feet"),
    ),
    (
        ids::EQUIPMENT_SLOT_RING0,
        enum_constant(EQUIPMENT_SLOT, "Ring0"),
    ),
    (
        ids::EQUIPMENT_SLOT_RING1,
        enum_constant(EQUIPMENT_SLOT, "Ring1"),
    ),
    (
        ids::EQUIPMENT_SLOT_RANGED_WEAPON,
        enum_constant(EQUIPMENT_SLOT, "RangedWeapon"),
    ),
    (
        ids::EQUIPMENT_SLOT_SHIELD,
        enum_constant(EQUIPMENT_SLOT, "Shield"),
    ),
    (
        ids::EQUIPMENT_SLOT_ARROW,
        enum_constant(EQUIPMENT_SLOT, "Arrow"),
    ),
    (
        ids::EQUIPMENT_SLOT_SPELL,
        enum_constant(EQUIPMENT_SLOT, "Spell"),
    ),
    (
        ids::EQUIPMENT_SLOT_FISHING_BAIT,
        enum_constant(EQUIPMENT_SLOT, "FishingBait"),
    ),
    (
        ids::EQUIPMENT_SLOT_TERRAIN_MATERIAL,
        enum_constant(EQUIPMENT_SLOT, "TerrainMaterial"),
    ),
    (
        ids::EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_DEFAULT,
        enum_constant(EQUIPMENT_SLOT, "BlueprintMaterial_Default"),
    ),
    (
        ids::EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_ROOF,
        enum_constant(EQUIPMENT_SLOT, "BlueprintMaterial_Roof"),
    ),
    (
        ids::EQUIPMENT_SLOT_OVERGROWTH_MATERIAL,
        enum_constant(EQUIPMENT_SLOT, "OvergrowthMaterial"),
    ),
    (
        ids::EQUIPMENT_SLOT_HOOK_SHOT,
        enum_constant(EQUIPMENT_SLOT, "HookShot"),
    ),
    (
        ids::EQUIPMENT_SLOT_GLIDER,
        enum_constant(EQUIPMENT_SLOT, "Glider"),
    ),
    (
        ids::EQUIPMENT_SLOT_BACKPACK,
        enum_constant(EQUIPMENT_SLOT, "Backpack"),
    ),
    (
        ids::EQUIPMENT_SLOT_MAIN_HAND,
        enum_constant(EQUIPMENT_SLOT, "MainHand"),
    ),
    (
        ids::EQUIPMENT_SLOT_TYPE_BACKPACK,
        enum_constant(EQUIPMENT_SLOT_TYPE, "Backpack"),
    ),
    (
        ids::INVENTORY_ERROR_INVALID_STATE,
        enum_constant(INVENTORY_ERROR, "InvalidState"),
    ),
    (
        ids::INVENTORY_ERROR_OUT_OF_MEMORY,
        enum_constant(INVENTORY_ERROR, "OutOfMemory"),
    ),
    (
        ids::GAME_SETTINGS_PRESET_DEFAULT,
        enum_constant(GAME_SETTINGS_PRESET, "Default"),
    ),
    (
        ids::GAME_SETTINGS_PRESET_RELAXED,
        enum_constant(GAME_SETTINGS_PRESET, "Relaxed"),
    ),
    (
        ids::GAME_SETTINGS_PRESET_HARD,
        enum_constant(GAME_SETTINGS_PRESET, "Hard"),
    ),
    (
        ids::GAME_SETTINGS_PRESET_SURVIVAL,
        enum_constant(GAME_SETTINGS_PRESET, "Survival"),
    ),
    (
        ids::GAME_SETTINGS_PRESET_CUSTOM,
        enum_constant(GAME_SETTINGS_PRESET, "Custom"),
    ),
    (
        ids::TOMBSTONE_MODE_ADD_BACKPACK_MATERIALS,
        enum_constant(TOMBSTONE_MODE, "AddBackpackMaterials"),
    ),
    (
        ids::TOMBSTONE_MODE_EVERYTHING,
        enum_constant(TOMBSTONE_MODE, "Everything"),
    ),
    (
        ids::TOMBSTONE_MODE_NO_TOMBSTONE,
        enum_constant(TOMBSTONE_MODE, "NoTombstone"),
    ),
    (
        ids::WEATHER_FREQUENCY_DISABLED,
        enum_constant(WEATHER_FREQUENCY, "Disabled"),
    ),
    (
        ids::WEATHER_FREQUENCY_RARE,
        enum_constant(WEATHER_FREQUENCY, "Rare"),
    ),
    (
        ids::WEATHER_FREQUENCY_NORMAL,
        enum_constant(WEATHER_FREQUENCY, "Normal"),
    ),
    (
        ids::WEATHER_FREQUENCY_OFTEN,
        enum_constant(WEATHER_FREQUENCY, "Often"),
    ),
    (
        ids::GENERIC_DIFFICULTY_VERY_EASY,
        enum_constant(GENERIC_DIFFICULTY, "VeryEasy"),
    ),
    (
        ids::GENERIC_DIFFICULTY_EASY,
        enum_constant(GENERIC_DIFFICULTY, "Easy"),
    ),
    (
        ids::GENERIC_DIFFICULTY_NORMAL,
        enum_constant(GENERIC_DIFFICULTY, "Normal"),
    ),
    (
        ids::GENERIC_DIFFICULTY_HARD,
        enum_constant(GENERIC_DIFFICULTY, "Hard"),
    ),
    (
        ids::GENERIC_DIFFICULTY_VERY_HARD,
        enum_constant(GENERIC_DIFFICULTY, "VeryHard"),
    ),
    (
        ids::RANDOM_SPAWNER_AMOUNT_FEW,
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Few"),
    ),
    (
        ids::RANDOM_SPAWNER_AMOUNT_NORMAL,
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Normal"),
    ),
    (
        ids::RANDOM_SPAWNER_AMOUNT_MANY,
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Many"),
    ),
    (
        ids::RANDOM_SPAWNER_AMOUNT_EXTREME,
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Extreme"),
    ),
    (
        ids::RANDOM_SPAWNER_AMOUNT_AUTOMATIC,
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Automatic"),
    ),
    (
        ids::AGGRO_POOL_AMOUNT_FEW,
        enum_constant(AGGRO_POOL_AMOUNT, "Few"),
    ),
    (
        ids::AGGRO_POOL_AMOUNT_NORMAL,
        enum_constant(AGGRO_POOL_AMOUNT, "Normal"),
    ),
    (
        ids::AGGRO_POOL_AMOUNT_MANY,
        enum_constant(AGGRO_POOL_AMOUNT, "Many"),
    ),
    (
        ids::AGGRO_POOL_AMOUNT_EXTREME,
        enum_constant(AGGRO_POOL_AMOUNT, "Extreme"),
    ),
    (
        ids::TAMING_STARTLE_REPERCUSSION_KEEP_PROGRESS,
        enum_constant(TAMING_STARTLE_REPERCUSSION, "KeepProgress"),
    ),
    (
        ids::TAMING_STARTLE_REPERCUSSION_LOSE_SOME_PROGRESS,
        enum_constant(TAMING_STARTLE_REPERCUSSION, "LoseSomeProgress"),
    ),
    (
        ids::TAMING_STARTLE_REPERCUSSION_LOSE_ALL_PROGRESS,
        enum_constant(TAMING_STARTLE_REPERCUSSION, "LoseAllProgress"),
    ),
    (
        ids::CURSE_MODIFIER_EASY,
        enum_constant(CURSE_MODIFIER, "Easy"),
    ),
    (
        ids::CURSE_MODIFIER_NORMAL,
        enum_constant(CURSE_MODIFIER, "Normal"),
    ),
    (
        ids::CURSE_MODIFIER_HARD,
        enum_constant(CURSE_MODIFIER, "Hard"),
    ),
    (
        ids::PLAYER_PERMISSIONS_CAN_EDIT_BASE,
        enum_constant(PLAYER_PERMISSIONS, "CanEditBase"),
    ),
    (
        ids::PLAYER_PERMISSIONS_CAN_EDIT_WORLD,
        enum_constant(PLAYER_PERMISSIONS, "CanEditWorld"),
    ),
    (
        ids::TERRAFORMING_REGISTRY_BUILDING_CONFIGS,
        offset(
            "keen::TerraformingEfficiencyRegistryResource",
            "buildingConfigs",
            "keen::BlobArray<keen::BuildingTerraformingConfig>",
        ),
    ),
    (
        ids::BUILDING_TERRAFORMING_CONFIG,
        Fact::Type("keen::BuildingTerraformingConfig"),
    ),
    (
        ids::BUILDING_TERRAFORMING_CONFIG_BUILDING_MATERIAL,
        offset(
            "keen::BuildingTerraformingConfig",
            "buildingMaterial",
            "keen::BuildingMaterialId",
        ),
    ),
    (
        ids::BUILDING_TERRAFORMING_CONFIG_HARDNESS,
        offset(
            "keen::BuildingTerraformingConfig",
            "hardness",
            "keen::HardnessCategory",
        ),
    ),
    (
        ids::PLAYER_WAYPOINT,
        Fact::Type("keen::ecs::PlayerWaypoint"),
    ),
    (
        ids::PLAYER_WAYPOINT_ENABLED,
        offset("keen::ecs::PlayerWaypoint", "enabled", "keen::bool"),
    ),
    (
        ids::PLAYER_WAYPOINT_POSITION,
        offset(
            "keen::ecs::PlayerWaypoint",
            "waypointPosition",
            "keen::WorldPosition",
        ),
    ),
    (ids::MAP_MARKER, Fact::Type("keen::ecs::MapMarker")),
    (
        ids::MAP_MARKER_MARKER_ID,
        offset("keen::ecs::MapMarker", "markerId", "keen::LocaTagReference"),
    ),
    (
        ids::MAP_MARKER_MARKER_TYPE,
        offset(
            "keen::ecs::MapMarker",
            "markerType",
            "keen::MapMarkerTypeReference",
        ),
    ),
    (ids::MAP_MARKER_TYPE, Fact::Type("keen::MapMarkerType")),
    (
        ids::MAP_MARKER_TYPE_MARKER_ID,
        offset("keen::MapMarkerType", "markerId", "keen::MapMarkerTypeId"),
    ),
    (
        ids::MAP_MARKER_TYPE_IS_CUSTOM,
        offset("keen::MapMarkerType", "isCustomMarkerType", "keen::bool"),
    ),
    (
        ids::MAP_MARKER_TYPE_SHOW_ABOVE_FOG,
        offset("keen::MapMarkerType", "showAboveFogOfWar", "keen::bool"),
    ),
    (
        ids::MAP_MARKER_TYPE_SHOW_LABEL,
        offset("keen::MapMarkerType", "showLabel", "keen::bool"),
    ),
    (
        ids::MAP_MARKER_REGISTRY_TYPES,
        Fact::Path {
            steps: &[
                ("keen::MapMarkerRegistryResource", "mapMarkers"),
                ("keen::MapMarkerMap", "values"),
            ],
            holds: "keen::BlobArray<keen::MapMarkerType>",
        },
    ),
    (
        ids::MAP_MARKER_VISIBILITY_TYPE_VISIBLE,
        Fact::Constant {
            ty: "keen::ecs::MapMarkerVisibilityType",
            name: "Visible",
        },
    ),
    (
        ids::GAME_MESSAGE_CHANNEL_VOICE_CHAT,
        Fact::Constant {
            ty: "keen::GameMessageChannelId",
            name: "VoiceChat",
        },
    ),
    (
        ids::MAP_MARKER_REGISTRY,
        Fact::Type("keen::MapMarkerRegistryResource"),
    ),
    (
        ids::SOUND_CONTAINER,
        Fact::Type("keen::SoundContainerResource"),
    ),
    (ids::SOUND, Fact::Type("keen::SoundResource")),
    (
        ids::SOUND_CONTAINER_SOUND_ENTRY,
        Fact::Type("keen::SoundContainerResourceSoundEntry"),
    ),
    (
        ids::MAP_MARKER_TYPE_TYPE_NAME,
        offset("keen::MapMarkerType", "typeName", "keen::LocaTagReference"),
    ),
    (
        ids::MAP_MARKER_TYPE_SORTING_CATEGORY,
        offset(
            "keen::MapMarkerType",
            "sortingCategory",
            "keen::MarkerTypeSorting",
        ),
    ),
    (
        ids::MAP_MARKER_TYPE_IS_FAST_TRAVEL_DESTINATION,
        offset(
            "keen::MapMarkerType",
            "isFastTravelDestination",
            "keen::bool",
        ),
    ),
    (
        ids::MARKER_TYPE_SORTING,
        Fact::Type("keen::MarkerTypeSorting"),
    ),
    (
        ids::MARKER_TYPE_SORTING_NONE,
        enum_constant("keen::MarkerTypeSorting", "None"),
    ),
    (
        ids::MARKER_TYPE_SORTING_FLAME_RELATED,
        enum_constant("keen::MarkerTypeSorting", "FlameRelated"),
    ),
    (
        ids::MARKER_TYPE_SORTING_DUNGEONS,
        enum_constant("keen::MarkerTypeSorting", "Dungeons"),
    ),
    (
        ids::MARKER_TYPE_SORTING_LOCATIONS,
        enum_constant("keen::MarkerTypeSorting", "Locations"),
    ),
    (
        ids::MARKER_TYPE_SORTING_CUSTOM_MARKER,
        enum_constant("keen::MarkerTypeSorting", "CustomMarker"),
    ),
    (
        ids::SCENE_CHUNK,
        Fact::Type("keen::SceneEntityChunkResource"),
    ),
    (
        ids::SCENE_CHUNK_TEMPLATES,
        offset(
            "keen::SceneEntityChunkResource",
            "templates",
            "keen::BlobArray<keen::ecs::TemplateReference>",
        ),
    ),
    (
        ids::SCENE_CHUNK_ENTITIES,
        offset(
            "keen::SceneEntityChunkResource",
            "entities",
            "keen::BlobArray<keen::SceneEntityChunkSpawnEntry>",
        ),
    ),
    (
        ids::SCENE_SPAWN_ENTRY,
        Fact::Type("keen::SceneEntityChunkSpawnEntry"),
    ),
    (
        ids::SCENE_SPAWN_ENTRY_TEMPLATE_INDEX,
        offset(
            "keen::SceneEntityChunkSpawnEntry",
            "templateIndex",
            "keen::uint16",
        ),
    ),
    (
        ids::SCENE_SPAWN_ENTRY_TRANSFORM,
        offset(
            "keen::SceneEntityChunkSpawnEntry",
            "transform",
            "keen::SceneEntityTransform",
        ),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_X0,
        offset("keen::SceneEntityTransform", "positionX0", "keen::uint16"),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_X1,
        offset("keen::SceneEntityTransform", "positionX1", "keen::uint16"),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_Y0,
        offset("keen::SceneEntityTransform", "positionY0", "keen::uint16"),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_Y1,
        offset("keen::SceneEntityTransform", "positionY1", "keen::uint16"),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_Z0,
        offset("keen::SceneEntityTransform", "positionZ0", "keen::uint16"),
    ),
    (
        ids::SCENE_TRANSFORM_POSITION_Z1,
        offset("keen::SceneEntityTransform", "positionZ1", "keen::uint16"),
    ),
    (ids::SCENE_RESOURCE, Fact::Type("keen::SceneResource")),
    (
        ids::SCENE_RESOURCE_HAS_BIOME_MAP,
        offset("keen::SceneResource", "hasBiomeMap", "keen::bool"),
    ),
    (ids::BIOME_MAP, Fact::Type("keen::BiomeMapResource")),
    (
        ids::BIOME_MAP_WORLD_ORIGIN,
        offset("keen::BiomeMapResource", "worldOrigin", "keen::int2"),
    ),
    (
        ids::BIOME_MAP_WORLD_SIZE,
        offset("keen::BiomeMapResource", "worldSize", "keen::uint2"),
    ),
    (
        ids::BIOME_MAP_MAP_SCALE,
        offset("keen::BiomeMapResource", "mapScale", "keen::uint"),
    ),
    (
        ids::DISABLE_RANDOM_LOOT_ON_DESTROY,
        Fact::Type("keen::ecs::DisableRandomLootOnDestroy"),
    ),
    (ids::LEVEL, Fact::Type("keen::ecs::Level")),
    (
        ids::LEVEL_DATA_STORAGE,
        offset(
            "keen::ecs::Level",
            "dataStorage",
            "keen::StaticArray<keen::uint32,2>",
        ),
    ),
    (ids::GROWTH, Fact::Type("keen::ecs::Growth")),
    (
        ids::FACTORY_STATION,
        Fact::Type("keen::ecs::FactoryStation"),
    ),
    (
        ids::FACTORY_STATION_STATE,
        offset(
            "keen::ecs::FactoryStation",
            "state",
            "keen::ecs::FactoryStationState",
        ),
    ),
    (
        ids::FACTORY_STATION_RUNNING_RECIPE,
        offset(
            "keen::ecs::FactoryStation",
            "runningRecipe",
            "keen::RecipeId",
        ),
    ),
    (
        ids::FACTORY_STATION_NEXT_RECIPE,
        offset("keen::ecs::FactoryStation", "nextRecipe", "keen::RecipeId"),
    ),
    (ids::NPC_BED, Fact::Type("keen::ecs::NpcBed")),
    (
        ids::NPC_BED_ASSOCIATED_NPC,
        offset("keen::ecs::NpcBed", "associatedNpc", "keen::NpcId"),
    ),
    (
        ids::STORE_ASSOCIATED_NPC_IDS,
        Fact::Type("keen::ecs::StoreAssociatedNpcIds"),
    ),
    (
        ids::STORE_ASSOCIATED_NPC_IDS_NPC_IDS,
        offset(
            "keen::ecs::StoreAssociatedNpcIds",
            "npcIds",
            "keen::StaticArray<keen::NpcId,2>",
        ),
    ),
    (
        ids::CUSTOM_PLAYER_STRING,
        Fact::Type("keen::ecs::CustomPlayerString"),
    ),
    (
        ids::CUSTOM_PLAYER_STRING_STRING_ID,
        offset(
            "keen::ecs::CustomPlayerString",
            "stringId",
            "keen::CustomStringId",
        ),
    ),
    (
        ids::LOGIC_ENTITY_PLACED_EVENT,
        Fact::Type("keen::ecs::LogicEntityPlacedEvent"),
    ),
    (
        ids::LOGIC_ENTITY_PLACED_EVENT_PLACED_BY_PLAYER_ID,
        offset(
            "keen::ecs::LogicEntityPlacedEvent",
            "placedByPlayerId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::LOGIC_ENTITY_PLACED_EVENT_PLACED_LOGIC_ENTITY_ID,
        offset(
            "keen::ecs::LogicEntityPlacedEvent",
            "placedLogicEntityId",
            "keen::ecs::EntityId",
        ),
    ),
    (
        ids::ITEM_INFO_PLACEMENT_BOX_MIN,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "placementAABBmin"),
            ],
            holds: "keen::float3",
        },
    ),
    (
        ids::ITEM_INFO_PLACEMENT_BOX_MAX,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "placementAABBmax"),
            ],
            holds: "keen::float3",
        },
    ),
    (
        ids::ITEM_FLAG_IS_LOGIC_ENTITY,
        enum_constant("keen::ItemFlag", "IsLogicEntity"),
    ),
    (ids::FUEL_CHARGES, Fact::Type("keen::ecs::Fuel")),
    (
        ids::FUEL_CHARGES_DATA_STORAGE,
        offset(
            "keen::ecs::Fuel",
            "dataStorage",
            "keen::StaticArray<keen::sint32,2>",
        ),
    ),
    (
        ids::LOGIC_GRAPH_NODE_CONNECTED,
        Fact::Type("keen::ecs::LogicGraphNodeConnected"),
    ),
    (
        ids::LOGIC_GRAPH_NODE_CONNECTED_WRITER_GRAPH_ID,
        offset(
            "keen::ecs::LogicGraphNodeConnected",
            "writerGraphId",
            "keen::LogicGraphId",
        ),
    ),
    (
        ids::LOGIC_GRAPH_NODE_CONNECTED_HAS_READER_GRAPHS,
        offset(
            "keen::ecs::LogicGraphNodeConnected",
            "hasReaderGraphs",
            "keen::bool",
        ),
    ),
    (
        ids::LOGIC_GRAPH_TRIGGER,
        Fact::Type("keen::ecs::LogicGraphTrigger"),
    ),
    (
        ids::LOGIC_GRAPH_TRIGGER_TRIGGER_CONFIG,
        offset(
            "keen::ecs::LogicGraphTrigger",
            "triggerConfig",
            "keen::LogicTriggerConfigId",
        ),
    ),
    (
        ids::LOGIC_TRIGGER_SHAPE,
        Fact::Type("keen::ecs::LogicTriggerShape"),
    ),
    (ids::ANIMAL_SAVED, Fact::Type("keen::ecs::AnimalSaved")),
    (
        ids::BASE_ID_COMPONENT,
        Fact::Type("keen::ecs::BaseIdComponent"),
    ),
    (
        ids::ITEM_INFO_REQUIRES_ALTAR_SLOT,
        Fact::Path {
            steps: &[
                ("keen::ItemInfo", "equipment"),
                ("keen::EquipmentSetup", "requiresAltarSlot"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::ACTIVE_NPC_STATE_SAVED,
        Fact::Type("keen::ecs::ActiveNpcStateSaved"),
    ),
    (
        ids::ITEM_FLAG_IS_NPC,
        enum_constant("keen::ItemFlag", "IsNpc"),
    ),
    (
        ids::ITEM_FLAG_IS_ANIMAL,
        enum_constant("keen::ItemFlag", "IsAnimal"),
    ),
    (
        ids::INVENTORY_CRAFTING_STOCK,
        Fact::Type("keen::ecs::InventoryCraftingStock"),
    ),
    (
        ids::FUEL_SETUP_AUTOMATIC_REFILL_ENABLED,
        Fact::Path {
            steps: &[
                ("keen::ecs::FuelSetup", "automaticRefill"),
                ("keen::ecs::FuelRefillSetup", "enabled"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::FUEL_SETUP_RAIN_REFILL_ENABLED,
        Fact::Path {
            steps: &[
                ("keen::ecs::FuelSetup", "rainRefill"),
                ("keen::ecs::FuelRefillSetup", "enabled"),
            ],
            holds: "keen::bool",
        },
    ),
    (
        ids::DYNAMIC_INVENTORY_CRAFTING_STOCK,
        Fact::Type("keen::ecs::DynamicInventoryCraftingStock"),
    ),
    (
        ids::DYNAMIC_INVENTORY_CRAFTING_STOCK_OVERRIDE_IS_SET,
        offset(
            "keen::ecs::DynamicInventoryCraftingStock",
            "canReceiveAreaDepositOverrideIsSet",
            "keen::bool",
        ),
    ),
    (
        ids::DYNAMIC_INVENTORY_CRAFTING_STOCK_OVERRIDE,
        offset(
            "keen::ecs::DynamicInventoryCraftingStock",
            "canReceiveAreaDepositOverride",
            "keen::bool",
        ),
    ),
];

/// The server's game settings, one reflected struct.
const GAME_SETTINGS: &str = "keen::ecs::GameSettings";

/// The bits of a hit's damage type mask.
const DAMAGE_TYPE: &str = "keen::impact::DamageTypeNew";

/// The bits of a hit's attack category mask.
const ATTACK_CATEGORY: &str = "keen::impact::AttackCategory";

/// Where experience came from.
const EXPERIENCE_SOURCE: &str = "keen::ExperienceSourceType";

/// What a player did at a Flame Altar.
const ALTAR_ACTION: &str = "keen::ecs::FlameAltarActionType";

/// A character's equipment slot.
const EQUIPMENT_SLOT: &str = "keen::EquipmentSlot";

/// The slot an item is worn in, as its equipment setup names it.
const EQUIPMENT_SLOT_TYPE: &str = "keen::EquipmentSlotType";

/// The error an inventory operation ends with.
const INVENTORY_ERROR: &str = "keen::ErrorId";

/// The preset a server's game settings came from.
const GAME_SETTINGS_PRESET: &str = "keen::ecs::GameSettingsPreset";

/// What a dead character leaves behind.
const TOMBSTONE_MODE: &str = "keen::ecs::TombstoneMode";

/// How often rain and storms come.
const WEATHER_FREQUENCY: &str = "keen::ecs::WeatherFrequency";

/// A difficulty from very easy to very hard.
const GENERIC_DIFFICULTY: &str = "keen::ecs::GenericDifficulty";

/// How many enemies random spawners bring.
const RANDOM_SPAWNER_AMOUNT: &str = "keen::ecs::RandomSpawnerAmount";

/// How many enemies attack at once.
const AGGRO_POOL_AMOUNT: &str = "keen::ecs::AggroPoolAmount";

/// What startling a tamed animal costs.
const TAMING_STARTLE_REPERCUSSION: &str = "keen::ecs::TamingStartleRepercussion";

/// How hard the shroud's curse hits.
const CURSE_MODIFIER: &str = "keen::ecs::CurseModifier";

/// The bits of a player's permission mask.
const PLAYER_PERMISSIONS: &str = "keen::PlayerPermissions";
