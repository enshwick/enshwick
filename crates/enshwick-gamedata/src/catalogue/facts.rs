//! The catalogue's table: each id the API asks for with the engine fact
//! that answers it. It stays one table past the usual file size, since a
//! test matches it row by row with the API's list of ids.

use super::{Fact, enum_constant, offset};

/// Every fact the API asks for, by id, in the order the API lists its ids.
pub const FACTS: &[(&str, Fact)] = &[
    ("hit_event", Fact::Type("keen::ecs::HitEvent")),
    (
        "hit_event.root_source_id",
        offset("keen::ecs::BaseHitEvent", "rootSourceId"),
    ),
    (
        "hit_event.target_id",
        offset("keen::ecs::BaseHitEvent", "targetId"),
    ),
    (
        "hit_event.source_id",
        offset("keen::ecs::HitEvent", "sourceId"),
    ),
    (
        "hit_event.root_target_id",
        offset("keen::ecs::HitEvent", "rootTargetId"),
    ),
    (
        "hit_event.weapon_pide_id",
        offset("keen::ecs::HitEvent", "weaponPideId"),
    ),
    (
        "hit_event.health_change",
        offset("keen::ecs::HitEvent", "healthChange"),
    ),
    (
        "hit_event.stamina_change",
        offset("keen::ecs::HitEvent", "staminaChange"),
    ),
    (
        "hit_event.damage_types",
        offset("keen::ecs::HitEvent", "damageTypes"),
    ),
    (
        "hit_event.attack_category",
        offset("keen::ecs::HitEvent", "attackCategory"),
    ),
    (
        "hit_event.target_faction",
        offset("keen::ecs::HitEvent", "targetFaction"),
    ),
    (
        "entity_died_event",
        Fact::Type("keen::ecs::EntityDiedEvent"),
    ),
    (
        "entity_died_event.dead_entity_id",
        offset("keen::ecs::EntityDiedEvent", "deadEntityId"),
    ),
    ("hit_history", Fact::Type("keen::ecs::HitHistory")),
    (
        "hit_history.last_index",
        offset("keen::ecs::HitHistory", "lastIndex"),
    ),
    (
        "hit_history.entries",
        offset("keen::ecs::HitHistory", "history"),
    ),
    (
        "hit_history.end",
        offset("keen::ecs::HitHistory", "targetTransform"),
    ),
    (
        "experience_credit",
        Fact::Type("keen::ecs::ExperienceSource"),
    ),
    (
        "experience_credit.last_player",
        offset("keen::ecs::ExperienceSource", "lastHitPlayerId"),
    ),
    ("level_up_event", Fact::Type("keen::ecs::LevelUpEvent")),
    (
        "level_up_event.player_entity_id",
        offset("keen::ecs::LevelUpEvent", "playerEntityId"),
    ),
    (
        "level_up_event.level",
        offset("keen::ecs::LevelUpEvent", "level"),
    ),
    ("xp_gained_event", Fact::Type("keen::ecs::XPGainedEvent")),
    (
        "xp_gained_event.player_entity_id",
        offset("keen::ecs::XPGainedEvent", "playerEntityId"),
    ),
    (
        "xp_gained_event.amount",
        offset("keen::ecs::XPGainedEvent", "amount"),
    ),
    (
        "xp_gained_event.source",
        offset("keen::ecs::XPGainedEvent", "source"),
    ),
    (
        "craft_recipe_event",
        Fact::Type("keen::ecs::CraftRecipeEvent"),
    ),
    (
        "craft_recipe_event.crafting_operator_id",
        offset("keen::ecs::CraftRecipeEvent", "craftingOperatorId"),
    ),
    (
        "craft_recipe_event.crafting_station_id",
        offset("keen::ecs::CraftRecipeEvent", "craftingStationId"),
    ),
    (
        "craft_recipe_event.recipe_id",
        offset("keen::ecs::CraftRecipeEvent", "recipeId"),
    ),
    (
        "craft_recipe_event.recipe_amount",
        offset("keen::ecs::CraftRecipeEvent", "recipeAmount"),
    ),
    (
        "craft_recipe_success_event",
        Fact::Type("keen::ecs::CraftRecipeSuccessEvent"),
    ),
    (
        "craft_recipe_success_event.workshop_id",
        offset("keen::ecs::CraftRecipeSuccessEvent", "workshopId"),
    ),
    (
        "craft_recipe_success_event.recipe_id",
        offset("keen::ecs::CraftRecipeSuccessEvent", "recipeId"),
    ),
    (
        "accept_offered_interaction_event",
        Fact::Type("keen::ecs::AcceptOfferedInteractionEvent"),
    ),
    (
        "accept_offered_interaction_event.guest_entity_id",
        offset("keen::ecs::AcceptOfferedInteractionEvent", "guestEntityId"),
    ),
    (
        "accept_offered_interaction_event.host_entity_id",
        offset("keen::ecs::AcceptOfferedInteractionEvent", "hostEntityId"),
    ),
    (
        "accept_offered_interaction_event.offer_id",
        offset("keen::ecs::AcceptOfferedInteractionEvent", "offerId"),
    ),
    (
        "building_place_event",
        Fact::Type("keen::ecs::BuildingPlaceEvent"),
    ),
    (
        "building_place_event.owner_id",
        offset("keen::ecs::BuildingPlaceEvent", "ownerId"),
    ),
    (
        "building_place_event.position",
        offset("keen::ecs::BuildingPlaceEvent", "position"),
    ),
    (
        "building_place_event.orientation",
        offset("keen::ecs::BuildingPlaceEvent", "orientation"),
    ),
    (
        "building_place_event.volume_min",
        offset("keen::ecs::BuildingPlaceEvent", "volumeMin"),
    ),
    (
        "building_place_event.volume_max",
        offset("keen::ecs::BuildingPlaceEvent", "volumeMax"),
    ),
    (
        "building_place_event.material",
        offset("keen::ecs::BuildingPlaceEvent", "material"),
    ),
    (
        "building_place_event.tracking_item_id",
        offset("keen::ecs::BuildingPlaceEvent", "trackingItemId"),
    ),
    (
        "building_tear_down_event",
        Fact::Type("keen::ecs::BuildingTearDownEvent"),
    ),
    (
        "building_tear_down_event.owner_id",
        offset("keen::ecs::BuildingTearDownEvent", "ownerId"),
    ),
    (
        "building_tear_down_event.position",
        offset("keen::ecs::BuildingTearDownEvent", "position"),
    ),
    (
        "building_tear_down_event.orientation",
        offset("keen::ecs::BuildingTearDownEvent", "orientation"),
    ),
    (
        "building_tear_down_event.volume_min",
        offset("keen::ecs::BuildingTearDownEvent", "volumeMin"),
    ),
    (
        "building_tear_down_event.volume_max",
        offset("keen::ecs::BuildingTearDownEvent", "volumeMax"),
    ),
    (
        "building_tear_down_event.material",
        offset("keen::ecs::BuildingTearDownEvent", "material"),
    ),
    (
        "building_pick_up_event",
        Fact::Type("keen::ecs::BuildingPickUpEvent"),
    ),
    (
        "building_pick_up_event.owner_id",
        offset("keen::ecs::BuildingPickUpEvent", "ownerId"),
    ),
    (
        "building_pick_up_event.position",
        offset("keen::ecs::BuildingPickUpEvent", "position"),
    ),
    (
        "building_pick_up_event.volume_min",
        offset("keen::ecs::BuildingPickUpEvent", "volumeMin"),
    ),
    (
        "building_pick_up_event.volume_max",
        offset("keen::ecs::BuildingPickUpEvent", "volumeMax"),
    ),
    (
        "building_pick_up_event.material",
        offset("keen::ecs::BuildingPickUpEvent", "material"),
    ),
    (
        "building_pick_up_event.picked_entity_id",
        offset("keen::ecs::BuildingPickUpEvent", "pickedEntityId"),
    ),
    (
        "flame_altar_event",
        Fact::Type("keen::ecs::FlameAltarEvent"),
    ),
    (
        "flame_altar_event.player_entity_id",
        offset("keen::ecs::FlameAltarEvent", "playerEntityId"),
    ),
    (
        "flame_altar_event.base_entity_id",
        offset("keen::ecs::FlameAltarEvent", "baseEntityId"),
    ),
    (
        "flame_altar_event.type",
        offset("keen::ecs::FlameAltarEvent", "type"),
    ),
    (
        "loot_interaction_event",
        Fact::Type("keen::ecs::LootInteractionEvent"),
    ),
    (
        "loot_interaction_event.guest_id",
        offset("keen::ecs::LootInteractionEvent", "guestId"),
    ),
    (
        "loot_interaction_event.host_id",
        offset("keen::ecs::LootInteractionEvent", "hostId"),
    ),
    (
        "loot_interaction_event.loot_all",
        offset("keen::ecs::LootInteractionEvent", "lootAll"),
    ),
    (
        "handle_equipment_slot_changed_event",
        Fact::Type("keen::ecs::HandleEquipmentSlotChangedEvent"),
    ),
    (
        "handle_equipment_slot_changed_event.player_entity_id",
        offset(
            "keen::ecs::HandleEquipmentSlotChangedEvent",
            "playerEntityId",
        ),
    ),
    (
        "handle_equipment_slot_changed_event.equipment_slot",
        offset(
            "keen::ecs::HandleEquipmentSlotChangedEvent",
            "equipmentSlot",
        ),
    ),
    (
        "revived_player_event",
        Fact::Type("keen::ecs::RevivedPlayerEvent"),
    ),
    (
        "revived_player_event.target_entity_id",
        offset("keen::ecs::RevivedPlayerEvent", "targetEntityId"),
    ),
    (
        "revived_player_event.source_entity_id",
        offset("keen::ecs::RevivedPlayerEvent", "sourceEntityId"),
    ),
    (
        "world_transform.position",
        offset("keen::WorldTransform", "position"),
    ),
    (
        "world_transform.orientation",
        offset("keen::WorldTransform", "orientation"),
    ),
    (
        "world_transform.scale",
        offset("keen::WorldTransform", "scale"),
    ),
    (
        "npc_command_event",
        Fact::Type("keen::ecs::NpcCommandEvent"),
    ),
    (
        "npc_command_event.type",
        offset("keen::ecs::NpcCommandEvent", "type"),
    ),
    (
        "npc_command_event.npc_id",
        offset("keen::ecs::NpcCommandEvent", "npcId"),
    ),
    (
        "npc_command_event.target_transform",
        offset("keen::ecs::NpcCommandEvent", "targetTransform"),
    ),
    (
        "set_toggle_state_event",
        Fact::Type("keen::ecs::SetToggleStateEvent"),
    ),
    (
        "set_toggle_state_event.sender_id",
        offset("keen::ecs::SetToggleStateEvent", "senderId"),
    ),
    (
        "set_toggle_state_event.signal_id",
        offset("keen::ecs::SetToggleStateEvent", "signalId"),
    ),
    (
        "set_toggle_state_event.new_state",
        offset("keen::ecs::SetToggleStateEvent", "newState"),
    ),
    (
        "add_threat_target_event",
        Fact::Type("keen::ecs::AddThreatTargetEvent"),
    ),
    (
        "add_threat_target_event.attacker_id",
        offset("keen::ecs::AddThreatTargetEvent", "attackerId"),
    ),
    (
        "add_threat_target_event.target_id",
        offset("keen::ecs::AddThreatTargetEvent", "targetId"),
    ),
    (
        "add_threat_target_event.threat_value",
        offset("keen::ecs::AddThreatTargetEvent", "threatValue"),
    ),
    ("ui_ping_event", Fact::Type("keen::ecs::UiPingEvent")),
    (
        "ui_ping_event.sender_entity_id",
        offset("keen::ecs::UiPingEvent", "senderEntityId"),
    ),
    (
        "ui_ping_event.position",
        offset("keen::ecs::UiPingEvent", "position"),
    ),
    ("item_info", Fact::Type("keen::ItemInfo")),
    ("item_info.id", offset("keen::ItemInfo", "itemId")),
    ("item_info.object", offset("keen::ItemInfo", "objectId")),
    ("item_info.name", offset("keen::ItemInfo", "name")),
    (
        "item_info.description",
        offset("keen::ItemInfo", "description"),
    ),
    ("item_info.flags", offset("keen::ItemInfo", "flags")),
    ("item_info.category", offset("keen::ItemInfo", "category")),
    (
        "item_info.max_stack",
        offset("keen::ItemInfo", "maxStackSize"),
    ),
    (
        "item_info.pid_entity",
        offset("keen::ItemInfo", "pidEntity"),
    ),
    ("item_info.ui_values", offset("keen::ItemInfo", "uiValues")),
    (
        "item_info.icon_image",
        offset("keen::ItemInfo", "iconImage"),
    ),
    (
        "item_info.placed_entity",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "placedEntity"),
        ]),
    ),
    (
        "item_info.building_voxel",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "voxelData"),
            ("keen::ChangeVoxelData", "isBuildingVoxel"),
        ]),
    ),
    (
        "item_info.place_voxel_material_id",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "voxelData"),
            ("keen::ChangeVoxelData", "placeVoxelMaterialId"),
        ]),
    ),
    (
        "item_info.equipment_slot",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "slot"),
        ]),
    ),
    (
        "item_info.damage_mod",
        Fact::Path(&[
            ("keen::ItemInfo", "damageSetup"),
            ("keen::ItemDamageSetup", "dmgMod"),
        ]),
    ),
    (
        "item_info.speed_mod",
        Fact::Path(&[
            ("keen::ItemInfo", "damageSetup"),
            ("keen::ItemDamageSetup", "speedMod"),
        ]),
    ),
    (
        "item_info.damage_set",
        Fact::Path(&[
            ("keen::ItemInfo", "damageSetup"),
            ("keen::ItemDamageSetup", "isSet"),
        ]),
    ),
    (
        "item_info.armor",
        Fact::Path(&[
            ("keen::ItemInfo", "armorSetup"),
            ("keen::ItemArmorSetup", "distribution"),
        ]),
    ),
    (
        "item_info.armor_set",
        Fact::Path(&[
            ("keen::ItemInfo", "armorSetup"),
            ("keen::ItemArmorSetup", "isSet"),
        ]),
    ),
    (
        "item_info.burn_time",
        Fact::Path(&[
            ("keen::ItemInfo", "fuelSetup"),
            ("keen::ItemFuelData", "burnTime"),
        ]),
    ),
    (
        "item_info.energy_output",
        Fact::Path(&[
            ("keen::ItemInfo", "fuelSetup"),
            ("keen::ItemFuelData", "energyOutput"),
        ]),
    ),
    (
        "item_info.fuel_set",
        Fact::Path(&[
            ("keen::ItemInfo", "fuelSetup"),
            ("keen::ItemFuelData", "isSet"),
        ]),
    ),
    (
        "item_info.comfort_amount",
        Fact::Path(&[
            ("keen::ItemInfo", "comfortSetup"),
            ("keen::ItemComfortSetup", "comfortAmount"),
        ]),
    ),
    (
        "item_info.comfort_set",
        Fact::Path(&[
            ("keen::ItemInfo", "comfortSetup"),
            ("keen::ItemComfortSetup", "isSet"),
        ]),
    ),
    (
        "armor_distribution",
        Fact::Type("keen::impact::ArmorDistribution"),
    ),
    ("item_slot_type", Fact::Type("keen::EquipmentSlotType")),
    ("item_stat", Fact::Type("keen::ImpactUiValue")),
    ("item_stat.label", offset("keen::ImpactUiValue", "locaId")),
    (
        "item_stat.format",
        offset("keen::ImpactUiValue", "valueFormat"),
    ),
    ("item_stat.value", offset("keen::ImpactUiValue", "value")),
    (
        "item_stat.signed",
        offset("keen::ImpactUiValue", "isSigned"),
    ),
    ("item_registry", Fact::Type("keen::ItemRegistryResource")),
    (
        "item_registry.refs",
        offset("keen::ItemRegistryResource", "itemRefs"),
    ),
    (
        "item_registry.names",
        offset("keen::ItemRegistryResource", "dbgNames"),
    ),
    ("template", Fact::Type("keen::ecs::TemplateResource")),
    (
        "template.components",
        offset("keen::ecs::TemplateResource", "components"),
    ),
    (
        "template_collection",
        Fact::Type("keen::ecs::TemplateCollectionResource"),
    ),
    (
        "template_collection.templates",
        offset("keen::ecs::TemplateCollectionResource", "templates"),
    ),
    ("fuel_setup", Fact::Type("keen::ecs::FuelSetup")),
    (
        "fuel_setup.max",
        offset("keen::ecs::FuelSetup", "maxFuelCharges"),
    ),
    (
        "fuel_setup.start",
        offset("keen::ecs::FuelSetup", "initialFuelPercentage"),
    ),
    ("crafting", Fact::Type("keen::ecs::Crafting")),
    (
        "crafting.workshop",
        offset("keen::ecs::Crafting", "workshop"),
    ),
    (
        "crafting.workshop_id",
        offset("keen::ecs::Crafting", "workshopId"),
    ),
    (
        "crafting_interaction",
        Fact::Type("keen::ecs::CraftingInteraction"),
    ),
    (
        "crafting_interaction.workshop",
        offset("keen::ecs::CraftingInteraction", "workshop"),
    ),
    (
        "crafting_interaction.workshop_id",
        offset("keen::ecs::CraftingInteraction", "workshopId"),
    ),
    (
        "client_crafting_interaction",
        Fact::Type("keen::ecs::ClientCraftingInteraction"),
    ),
    (
        "client_crafting_interaction.workshop_id",
        offset("keen::ecs::ClientCraftingInteraction", "workshopId"),
    ),
    (
        "workshop_registry",
        Fact::Type("keen::WorkshopRegistryResource"),
    ),
    (
        "workshop_registry.workshops",
        offset("keen::WorkshopRegistryResource", "workshops"),
    ),
    ("workshop", Fact::Type("keen::WorkshopResource")),
    (
        "workshop.guid",
        offset("keen::WorkshopResource", "workshopGuid"),
    ),
    (
        "workshop.id",
        offset("keen::WorkshopResource", "workshopId"),
    ),
    ("workshop.name", offset("keen::WorkshopResource", "name")),
    ("workshop.item", offset("keen::WorkshopResource", "item")),
    (
        "workshop.item_ref",
        offset("keen::WorkshopResource", "itemRef"),
    ),
    ("workshop.npc", offset("keen::WorkshopResource", "isNpc")),
    (
        "recipe_registry",
        Fact::Type("keen::RecipeRegistryResource"),
    ),
    (
        "recipe_registry.recipes",
        offset("keen::RecipeRegistryResource", "recipes"),
    ),
    ("recipe", Fact::Type("keen::RecipeInfo")),
    ("recipe.guid", offset("keen::RecipeInfo", "recipeGuid")),
    ("recipe.id", offset("keen::RecipeInfo", "recipeId")),
    (
        "recipe.workshop_guid",
        offset("keen::RecipeInfo", "workshopGuid"),
    ),
    ("recipe.workshop", offset("keen::RecipeInfo", "workshopId")),
    ("recipe.input", offset("keen::RecipeInfo", "input")),
    ("recipe.output", offset("keen::RecipeInfo", "output")),
    (
        "recipe.completion",
        offset("keen::RecipeInfo", "completionRequirementQuery"),
    ),
    (
        "recipe.duration",
        offset("keen::RecipeInfo", "craftingDuration"),
    ),
    (
        "recipe.required_props",
        offset("keen::RecipeInfo", "requiredProps"),
    ),
    ("recipe.debug_name", offset("keen::RecipeInfo", "debugName")),
    ("recipe_ingredient", Fact::Type("keen::RecipeInputResource")),
    ("recipe_stack", Fact::Type("keen::RecipeItemStackResource")),
    (
        "recipe_stack.item",
        offset("keen::RecipeItemStackResource", "item"),
    ),
    (
        "recipe_stack.count",
        offset("keen::RecipeItemStackResource", "count"),
    ),
    ("ui_image", Fact::Type("keen::UiTextureResource")),
    ("ui_bundle", Fact::Type("keen::FbUiBundle")),
    (
        "ui_bundle.crafting_menu",
        Fact::Path(&[
            ("keen::FbUiBundle", "menu"),
            ("keen::FbUiMenu", "crafting"),
            ("keen::FbUiCrafting", "recipes"),
        ]),
    ),
    (
        "crafting_menu.workshops",
        offset("keen::FbUiCraftingRecipeCollection", "workshops"),
    ),
    (
        "crafting_menu.trees",
        offset("keen::FbUiCraftingRecipeCollection", "trees"),
    ),
    (
        "crafting_menu.max_groups",
        offset("keen::FbUiCraftingRecipeCollection", "maxRecipeGroupCount"),
    ),
    (
        "crafting_menu.max_sets",
        offset("keen::FbUiCraftingRecipeCollection", "maxRecipeSetCount"),
    ),
    (
        "crafting_menu_tree",
        Fact::Type("keen::FbUiCraftingRecipeTree"),
    ),
    (
        "crafting_menu_tree.groups",
        offset("keen::FbUiCraftingRecipeTree", "groups"),
    ),
    (
        "crafting_menu_group",
        Fact::Type("keen::FbUiCraftingRecipeGroup"),
    ),
    (
        "crafting_menu_group.id",
        offset("keen::FbUiCraftingRecipeGroup", "groupId"),
    ),
    (
        "crafting_menu_group.sets",
        offset("keen::FbUiCraftingRecipeGroup", "sets"),
    ),
    (
        "crafting_menu_set",
        Fact::Type("keen::FbUiCraftingRecipeSet"),
    ),
    (
        "crafting_menu_set.entries",
        offset("keen::FbUiCraftingRecipeSet", "entries"),
    ),
    (
        "current_transform",
        Fact::Type("keen::ecs::CurrentTransform"),
    ),
    (
        "current_transform.transform",
        offset("keen::ecs::CurrentTransform", "transform"),
    ),
    ("used_item", Fact::Type("keen::ecs::UsedItem")),
    ("label", Fact::Type("keen::ecs::CustomPlayerString")),
    ("inventory_setup", Fact::Type("keen::ecs::InventorySetup")),
    (
        "inventory_setup.links",
        offset("keen::ecs::InventorySetup", "linksEntities"),
    ),
    (
        "inventory_setup.categories",
        offset("keen::ecs::InventorySetup", "linksCategories"),
    ),
    (
        "inventory_setup.initialized",
        offset("keen::ecs::InventorySetup", "isInitialized"),
    ),
    (
        "inventory_setup.generic_slots",
        offset("keen::ecs::InventorySetup", "genericSlotCount"),
    ),
    (
        "inventory_setup.available_slots",
        offset("keen::ecs::InventorySetup", "availableSlotCount"),
    ),
    (
        "inventory_setup_overwrite",
        Fact::Type("keen::ecs::InventorySetupOverwrite"),
    ),
    (
        "inventory_setup_overwrite.generic_slots",
        offset("keen::ecs::InventorySetupOverwrite", "genericSlotCount"),
    ),
    ("inventory", Fact::Type("keen::ecs::Inventory")),
    ("item_stack", Fact::Type("keen::ItemStack")),
    ("item_stack.count", offset("keen::ItemStack", "data")),
    (
        "item_stack.pide",
        // `data` holds the u32 count, then the id of the item's own entity.
        Fact::Offset {
            ty: "keen::ItemStack",
            field: "data",
            plus: 4,
        },
    ),
    ("durability", Fact::Type("keen::ecs::NetworkDurability")),
    (
        "durability.max",
        offset("keen::ecs::NetworkDurability", "durabilityMax"),
    ),
    ("fuel", Fact::Type("keen::ecs::NetworkFuel")),
    ("fuel.max", offset("keen::ecs::NetworkFuel", "fuelMax")),
    ("owner", Fact::Type("keen::ecs::OwnerRelationship")),
    (
        "inventory_empty_state",
        Fact::Type("keen::ecs::InventoryEmptyState"),
    ),
    (
        "inventory_empty_state.empty",
        offset("keen::ecs::InventoryEmptyState", "isEmpty"),
    ),
    ("destroy_on_loot", Fact::Type("keen::ecs::DestroyOnLoot")),
    ("lifetime", Fact::Type("keen::ecs::LifeTime")),
    ("area_loot", Fact::Type("keen::ecs::AreaLootOverride")),
    (
        "area_loot.exclude",
        offset("keen::ecs::AreaLootOverride", "exclude"),
    ),
    (
        "interaction_offer",
        Fact::Type("keen::ecs::InteractionOfferRuntime"),
    ),
    (
        "interaction_offer.accept_time",
        offset("keen::ecs::InteractionOfferRuntime", "lastAcceptTime"),
    ),
    (
        "interaction_offer.accept_guest",
        Fact::Path(&[
            ("keen::ecs::InteractionOfferRuntime", "lastAcceptData"),
            ("keen::ecs::InteractionAcceptData", "guestId"),
        ]),
    ),
    (
        "interaction_offer.disabled",
        offset("keen::ecs::InteractionOfferRuntime", "isDisabled"),
    ),
    ("player_input", Fact::Type("keen::ecs::PlayerInput")),
    ("boss", Fact::Type("keen::ecs::Boss")),
    ("interaction_lock", Fact::Type("keen::ecs::InteractionLock")),
    (
        "interaction_lock.item",
        offset("keen::ecs::InteractionLock", "requiredItem"),
    ),
    ("loot_slot", Fact::Type("keen::LootSlotInfo")),
    ("loot_slot.item", offset("keen::LootSlotInfo", "itemId")),
    ("loot_slot.count", offset("keen::LootSlotInfo", "stackSize")),
    ("lootable_items", Fact::Type("keen::LootableItemsResource")),
    (
        "lootable_items.items",
        offset("keen::LootableItemsResource", "items"),
    ),
    ("lootable_item", Fact::Type("keen::LootableItemInfo")),
    (
        "lootable_item.item",
        offset("keen::LootableItemInfo", "itemId"),
    ),
    (
        "lootable_item.stack_min",
        offset("keen::LootableItemInfo", "stackSizeMin"),
    ),
    (
        "lootable_item.stack_max",
        offset("keen::LootableItemInfo", "stackSizeMax"),
    ),
    (
        "lootable_item.stack_max_scaled",
        offset("keen::LootableItemInfo", "stackSizeMaxScaled"),
    ),
    (
        "loot_fill_table",
        Fact::Type("keen::ecs::DefaultInventoryResource"),
    ),
    (
        "loot_fill_table.root",
        offset("keen::ecs::DefaultInventoryResource", "rootGroup"),
    ),
    (
        "loot_fill_group",
        Fact::Type("keen::ecs::DefaultInventoryGroupResource"),
    ),
    (
        "loot_fill_group.stacks",
        offset("keen::ecs::DefaultInventoryGroupResource", "stacks"),
    ),
    (
        "loot_fill_group.groups",
        offset("keen::ecs::DefaultInventoryGroupResource", "groups"),
    ),
    (
        "loot_fill_stack",
        Fact::Type("keen::ecs::ItemStackInfoResource"),
    ),
    (
        "loot_fill_stack.item",
        offset("keen::ecs::ItemStackInfoResource", "item"),
    ),
    (
        "loot_fill_stack.count_min",
        offset("keen::ecs::ItemStackInfoResource", "countMin"),
    ),
    (
        "loot_fill_stack.count_max",
        offset("keen::ecs::ItemStackInfoResource", "countMax"),
    ),
    (
        "loot_fill_stack.probability",
        offset("keen::ecs::ItemStackInfoResource", "spawnProbability"),
    ),
    (
        "loot_fill_stack.weight",
        offset("keen::ecs::ItemStackInfoResource", "weight"),
    ),
    ("attribute_set.health", Fact::Type("keen::ecs::Health")),
    ("attribute_set.stamina", Fact::Type("keen::ecs::Stamina")),
    ("attribute_set.mana", Fact::Type("keen::ecs::Mana")),
    ("attribute_set.level", Fact::Type("keen::ecs::Level")),
    (
        "attribute_set.experience",
        Fact::Type("keen::ecs::Experience"),
    ),
    ("attribute_set.fuel", Fact::Type("keen::ecs::Fuel")),
    (
        "attribute_set.shroud",
        Fact::Type("keen::ecs::FogResistance"),
    ),
    ("skill_nodes", Fact::Type("keen::ecs::UnlockedSkillNodes")),
    (
        "skill_nodes.ids",
        offset("keen::ecs::UnlockedSkillNodes", "knownSkillNodes"),
    ),
    (
        "skill_nodes.levels",
        offset("keen::ecs::UnlockedSkillNodes", "unlockLevel"),
    ),
    ("base_actor", Fact::Type("keen::ecs::BaseActor")),
    (
        "base_actor.state",
        offset("keen::ecs::BaseActor", "currentState"),
    ),
    ("npc", Fact::Type("keen::ecs::ActiveNpcState")),
    ("animal", Fact::Type("keen::ecs::Animal")),
    ("loot_fill", Fact::Type("keen::ecs::DefaultInventory")),
    ("resource_node", Fact::Type("keen::ecs::MiningNode")),
    ("game_settings", Fact::Type(GAME_SETTINGS)),
    (
        "game_settings.player_health_factor",
        offset(GAME_SETTINGS, "playerHealthFactor"),
    ),
    (
        "game_settings.player_mana_factor",
        offset(GAME_SETTINGS, "playerManaFactor"),
    ),
    (
        "game_settings.player_stamina_factor",
        offset(GAME_SETTINGS, "playerStaminaFactor"),
    ),
    (
        "game_settings.player_body_heat_factor",
        offset(GAME_SETTINGS, "playerBodyHeatFactor"),
    ),
    (
        "game_settings.player_diving_time_factor",
        offset(GAME_SETTINGS, "playerDivingTimeFactor"),
    ),
    (
        "game_settings.enable_durability",
        offset(GAME_SETTINGS, "enableDurability"),
    ),
    (
        "game_settings.enable_starving_debuff",
        offset(GAME_SETTINGS, "enableStarvingDebuff"),
    ),
    (
        "game_settings.food_buff_duration_factor",
        offset(GAME_SETTINGS, "foodBuffDurationFactor"),
    ),
    (
        "game_settings.from_hunger_to_starving",
        offset(GAME_SETTINGS, "fromHungerToStarving"),
    ),
    (
        "game_settings.shroud_time_factor",
        offset(GAME_SETTINGS, "shroudTimeFactor"),
    ),
    (
        "game_settings.tombstone_mode",
        offset(GAME_SETTINGS, "tombstoneMode"),
    ),
    (
        "game_settings.enable_glider_turbulences",
        offset(GAME_SETTINGS, "enableGliderTurbulences"),
    ),
    (
        "game_settings.weather_frequency",
        offset(GAME_SETTINGS, "weatherFrequency"),
    ),
    (
        "game_settings.fishing_difficulty",
        offset(GAME_SETTINGS, "fishingDifficulty"),
    ),
    (
        "game_settings.mining_damage_factor",
        offset(GAME_SETTINGS, "miningDamageFactor"),
    ),
    (
        "game_settings.plant_growth_speed_factor",
        offset(GAME_SETTINGS, "plantGrowthSpeedFactor"),
    ),
    (
        "game_settings.resource_drop_stack_amount_factor",
        offset(GAME_SETTINGS, "resourceDropStackAmountFactor"),
    ),
    (
        "game_settings.factory_production_speed_factor",
        offset(GAME_SETTINGS, "factoryProductionSpeedFactor"),
    ),
    (
        "game_settings.perk_upgrade_recycling_factor",
        offset(GAME_SETTINGS, "perkUpgradeRecyclingFactor"),
    ),
    (
        "game_settings.perk_cost_factor",
        offset(GAME_SETTINGS, "perkCostFactor"),
    ),
    (
        "game_settings.experience_combat_factor",
        offset(GAME_SETTINGS, "experienceCombatFactor"),
    ),
    (
        "game_settings.experience_mining_factor",
        offset(GAME_SETTINGS, "experienceMiningFactor"),
    ),
    (
        "game_settings.experience_exploration_quests_factor",
        offset(GAME_SETTINGS, "experienceExplorationQuestsFactor"),
    ),
    (
        "game_settings.random_spawner_amount",
        offset(GAME_SETTINGS, "randomSpawnerAmount"),
    ),
    (
        "game_settings.aggro_pool_amount",
        offset(GAME_SETTINGS, "aggroPoolAmount"),
    ),
    (
        "game_settings.enemy_damage_factor",
        offset(GAME_SETTINGS, "enemyDamageFactor"),
    ),
    (
        "game_settings.enemy_health_factor",
        offset(GAME_SETTINGS, "enemyHealthFactor"),
    ),
    (
        "game_settings.enemy_stamina_factor",
        offset(GAME_SETTINGS, "enemyStaminaFactor"),
    ),
    (
        "game_settings.enemy_perception_range_factor",
        offset(GAME_SETTINGS, "enemyPerceptionRangeFactor"),
    ),
    (
        "game_settings.boss_damage_factor",
        offset(GAME_SETTINGS, "bossDamageFactor"),
    ),
    (
        "game_settings.boss_health_factor",
        offset(GAME_SETTINGS, "bossHealthFactor"),
    ),
    (
        "game_settings.threat_bonus",
        offset(GAME_SETTINGS, "threatBonus"),
    ),
    (
        "game_settings.pacify_all_enemies",
        offset(GAME_SETTINGS, "pacifyAllEnemies"),
    ),
    (
        "game_settings.taming_startle_repercussion",
        offset(GAME_SETTINGS, "tamingStartleRepercussion"),
    ),
    (
        "game_settings.day_time_duration",
        offset(GAME_SETTINGS, "dayTimeDuration"),
    ),
    (
        "game_settings.night_time_duration",
        offset(GAME_SETTINGS, "nightTimeDuration"),
    ),
    (
        "game_settings.curse_modifier",
        offset(GAME_SETTINGS, "curseModifier"),
    ),
    (
        "game_settings.flame_level_fog_removal",
        offset(GAME_SETTINGS, "flameLevelFogRemoval"),
    ),
    (
        "damage_type.physical_cut",
        enum_constant(DAMAGE_TYPE, "Physical_Cut"),
    ),
    (
        "damage_type.physical_pierce",
        enum_constant(DAMAGE_TYPE, "Physical_Pierce"),
    ),
    (
        "damage_type.physical_blunt",
        enum_constant(DAMAGE_TYPE, "Physical_Blunt"),
    ),
    (
        "damage_type.magical_fire",
        enum_constant(DAMAGE_TYPE, "Magical_Fire"),
    ),
    (
        "damage_type.magical_ice",
        enum_constant(DAMAGE_TYPE, "Magical_Ice"),
    ),
    (
        "damage_type.magical_fog",
        enum_constant(DAMAGE_TYPE, "Magical_Fog"),
    ),
    (
        "damage_type.magical_lightning",
        enum_constant(DAMAGE_TYPE, "Magical_Lightning"),
    ),
    ("damage_type.healing", enum_constant(DAMAGE_TYPE, "Healing")),
    ("damage_type.poison", enum_constant(DAMAGE_TYPE, "Poison")),
    ("damage_type.wood", enum_constant(DAMAGE_TYPE, "Wood")),
    ("damage_type.stone", enum_constant(DAMAGE_TYPE, "Stone")),
    ("damage_type.metal", enum_constant(DAMAGE_TYPE, "Metal")),
    (
        "damage_type.explosion",
        enum_constant(DAMAGE_TYPE, "Explosion"),
    ),
    (
        "attack_category.melee",
        enum_constant(ATTACK_CATEGORY, "Melee"),
    ),
    (
        "attack_category.ranged",
        enum_constant(ATTACK_CATEGORY, "Ranged"),
    ),
    (
        "attack_category.magic",
        enum_constant(ATTACK_CATEGORY, "Magic"),
    ),
    (
        "attack_category.tool",
        enum_constant(ATTACK_CATEGORY, "Tool"),
    ),
    (
        "attack_category.other",
        enum_constant(ATTACK_CATEGORY, "Other"),
    ),
    (
        "attack_category.ao_e",
        enum_constant(ATTACK_CATEGORY, "AoE"),
    ),
    (
        "attack_category.skill_shot",
        enum_constant(ATTACK_CATEGORY, "SkillShot"),
    ),
    (
        "experience_source.none",
        enum_constant(EXPERIENCE_SOURCE, "None"),
    ),
    (
        "experience_source.exploration",
        enum_constant(EXPERIENCE_SOURCE, "Exploration"),
    ),
    (
        "experience_source.lore",
        enum_constant(EXPERIENCE_SOURCE, "Lore"),
    ),
    (
        "experience_source.quest",
        enum_constant(EXPERIENCE_SOURCE, "Quest"),
    ),
    (
        "experience_source.combat",
        enum_constant(EXPERIENCE_SOURCE, "Combat"),
    ),
    (
        "experience_source.mining",
        enum_constant(EXPERIENCE_SOURCE, "Mining"),
    ),
    (
        "experience_source.item_set_completed",
        enum_constant(EXPERIENCE_SOURCE, "ItemSetCompleted"),
    ),
    (
        "altar_action.abandon",
        enum_constant(ALTAR_ACTION, "Abandon"),
    ),
    (
        "altar_action.reclaim",
        enum_constant(ALTAR_ACTION, "Reclaim"),
    ),
    (
        "altar_action.remove_water",
        enum_constant(ALTAR_ACTION, "RemoveWater"),
    ),
    (
        "altar_action.set_as_preferred_spawn_altar",
        enum_constant(ALTAR_ACTION, "SetAsPreferredSpawnAltar"),
    ),
    (
        "equipment_slot.vanity_head",
        enum_constant(EQUIPMENT_SLOT, "Vanity_Head"),
    ),
    (
        "equipment_slot.vanity_upper_body",
        enum_constant(EQUIPMENT_SLOT, "Vanity_UpperBody"),
    ),
    (
        "equipment_slot.vanity_arms",
        enum_constant(EQUIPMENT_SLOT, "Vanity_Arms"),
    ),
    (
        "equipment_slot.vanity_lower_body",
        enum_constant(EQUIPMENT_SLOT, "Vanity_LowerBody"),
    ),
    (
        "equipment_slot.vanity_feet",
        enum_constant(EQUIPMENT_SLOT, "Vanity_Feet"),
    ),
    (
        "equipment_slot.armour_head",
        enum_constant(EQUIPMENT_SLOT, "Armour_Head"),
    ),
    (
        "equipment_slot.armour_upper_body",
        enum_constant(EQUIPMENT_SLOT, "Armour_UpperBody"),
    ),
    (
        "equipment_slot.armour_arms",
        enum_constant(EQUIPMENT_SLOT, "Armour_Arms"),
    ),
    (
        "equipment_slot.armour_lower_body",
        enum_constant(EQUIPMENT_SLOT, "Armour_LowerBody"),
    ),
    (
        "equipment_slot.armour_feet",
        enum_constant(EQUIPMENT_SLOT, "Armour_Feet"),
    ),
    (
        "equipment_slot.ring0",
        enum_constant(EQUIPMENT_SLOT, "Ring0"),
    ),
    (
        "equipment_slot.ring1",
        enum_constant(EQUIPMENT_SLOT, "Ring1"),
    ),
    (
        "equipment_slot.ranged_weapon",
        enum_constant(EQUIPMENT_SLOT, "RangedWeapon"),
    ),
    (
        "equipment_slot.shield",
        enum_constant(EQUIPMENT_SLOT, "Shield"),
    ),
    (
        "equipment_slot.arrow",
        enum_constant(EQUIPMENT_SLOT, "Arrow"),
    ),
    (
        "equipment_slot.spell",
        enum_constant(EQUIPMENT_SLOT, "Spell"),
    ),
    (
        "equipment_slot.fishing_bait",
        enum_constant(EQUIPMENT_SLOT, "FishingBait"),
    ),
    (
        "equipment_slot.terrain_material",
        enum_constant(EQUIPMENT_SLOT, "TerrainMaterial"),
    ),
    (
        "equipment_slot.blueprint_material_default",
        enum_constant(EQUIPMENT_SLOT, "BlueprintMaterial_Default"),
    ),
    (
        "equipment_slot.blueprint_material_roof",
        enum_constant(EQUIPMENT_SLOT, "BlueprintMaterial_Roof"),
    ),
    (
        "equipment_slot.overgrowth_material",
        enum_constant(EQUIPMENT_SLOT, "OvergrowthMaterial"),
    ),
    (
        "equipment_slot.hook_shot",
        enum_constant(EQUIPMENT_SLOT, "HookShot"),
    ),
    (
        "equipment_slot.glider",
        enum_constant(EQUIPMENT_SLOT, "Glider"),
    ),
    (
        "equipment_slot.backpack",
        enum_constant(EQUIPMENT_SLOT, "Backpack"),
    ),
    (
        "equipment_slot.main_hand",
        enum_constant(EQUIPMENT_SLOT, "MainHand"),
    ),
    (
        "inventory_error.invalid_state",
        enum_constant(INVENTORY_ERROR, "InvalidState"),
    ),
    (
        "inventory_error.out_of_memory",
        enum_constant(INVENTORY_ERROR, "OutOfMemory"),
    ),
    (
        "game_settings_preset.default",
        enum_constant(GAME_SETTINGS_PRESET, "Default"),
    ),
    (
        "game_settings_preset.relaxed",
        enum_constant(GAME_SETTINGS_PRESET, "Relaxed"),
    ),
    (
        "game_settings_preset.hard",
        enum_constant(GAME_SETTINGS_PRESET, "Hard"),
    ),
    (
        "game_settings_preset.survival",
        enum_constant(GAME_SETTINGS_PRESET, "Survival"),
    ),
    (
        "game_settings_preset.custom",
        enum_constant(GAME_SETTINGS_PRESET, "Custom"),
    ),
    (
        "tombstone_mode.add_backpack_materials",
        enum_constant(TOMBSTONE_MODE, "AddBackpackMaterials"),
    ),
    (
        "tombstone_mode.everything",
        enum_constant(TOMBSTONE_MODE, "Everything"),
    ),
    (
        "tombstone_mode.no_tombstone",
        enum_constant(TOMBSTONE_MODE, "NoTombstone"),
    ),
    (
        "weather_frequency.disabled",
        enum_constant(WEATHER_FREQUENCY, "Disabled"),
    ),
    (
        "weather_frequency.rare",
        enum_constant(WEATHER_FREQUENCY, "Rare"),
    ),
    (
        "weather_frequency.normal",
        enum_constant(WEATHER_FREQUENCY, "Normal"),
    ),
    (
        "weather_frequency.often",
        enum_constant(WEATHER_FREQUENCY, "Often"),
    ),
    (
        "generic_difficulty.very_easy",
        enum_constant(GENERIC_DIFFICULTY, "VeryEasy"),
    ),
    (
        "generic_difficulty.easy",
        enum_constant(GENERIC_DIFFICULTY, "Easy"),
    ),
    (
        "generic_difficulty.normal",
        enum_constant(GENERIC_DIFFICULTY, "Normal"),
    ),
    (
        "generic_difficulty.hard",
        enum_constant(GENERIC_DIFFICULTY, "Hard"),
    ),
    (
        "generic_difficulty.very_hard",
        enum_constant(GENERIC_DIFFICULTY, "VeryHard"),
    ),
    (
        "random_spawner_amount.few",
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Few"),
    ),
    (
        "random_spawner_amount.normal",
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Normal"),
    ),
    (
        "random_spawner_amount.many",
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Many"),
    ),
    (
        "random_spawner_amount.extreme",
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Extreme"),
    ),
    (
        "random_spawner_amount.automatic",
        enum_constant(RANDOM_SPAWNER_AMOUNT, "Automatic"),
    ),
    (
        "aggro_pool_amount.few",
        enum_constant(AGGRO_POOL_AMOUNT, "Few"),
    ),
    (
        "aggro_pool_amount.normal",
        enum_constant(AGGRO_POOL_AMOUNT, "Normal"),
    ),
    (
        "aggro_pool_amount.many",
        enum_constant(AGGRO_POOL_AMOUNT, "Many"),
    ),
    (
        "aggro_pool_amount.extreme",
        enum_constant(AGGRO_POOL_AMOUNT, "Extreme"),
    ),
    (
        "taming_startle_repercussion.keep_progress",
        enum_constant(TAMING_STARTLE_REPERCUSSION, "KeepProgress"),
    ),
    (
        "taming_startle_repercussion.lose_some_progress",
        enum_constant(TAMING_STARTLE_REPERCUSSION, "LoseSomeProgress"),
    ),
    (
        "taming_startle_repercussion.lose_all_progress",
        enum_constant(TAMING_STARTLE_REPERCUSSION, "LoseAllProgress"),
    ),
    ("curse_modifier.easy", enum_constant(CURSE_MODIFIER, "Easy")),
    (
        "curse_modifier.normal",
        enum_constant(CURSE_MODIFIER, "Normal"),
    ),
    ("curse_modifier.hard", enum_constant(CURSE_MODIFIER, "Hard")),
    (
        "player_permissions.can_edit_base",
        enum_constant(PLAYER_PERMISSIONS, "CanEditBase"),
    ),
    (
        "player_permissions.can_edit_world",
        enum_constant(PLAYER_PERMISSIONS, "CanEditWorld"),
    ),
    (
        "terraforming_registry.building_configs",
        offset(
            "keen::TerraformingEfficiencyRegistryResource",
            "buildingConfigs",
        ),
    ),
    (
        "building_terraforming_config",
        Fact::Type("keen::BuildingTerraformingConfig"),
    ),
    (
        "building_terraforming_config.building_material",
        offset("keen::BuildingTerraformingConfig", "buildingMaterial"),
    ),
    (
        "building_terraforming_config.hardness",
        offset("keen::BuildingTerraformingConfig", "hardness"),
    ),
    (
        "disable_random_loot_on_destroy",
        Fact::Type("keen::ecs::DisableRandomLootOnDestroy"),
    ),
    ("level", Fact::Type("keen::ecs::Level")),
    (
        "level.data_storage",
        offset("keen::ecs::Level", "dataStorage"),
    ),
    ("growth", Fact::Type("keen::ecs::Growth")),
    ("factory_station", Fact::Type("keen::ecs::FactoryStation")),
    (
        "factory_station.state",
        offset("keen::ecs::FactoryStation", "state"),
    ),
    (
        "factory_station.running_recipe",
        offset("keen::ecs::FactoryStation", "runningRecipe"),
    ),
    (
        "factory_station.next_recipe",
        offset("keen::ecs::FactoryStation", "nextRecipe"),
    ),
    ("npc_bed", Fact::Type("keen::ecs::NpcBed")),
    (
        "npc_bed.associated_npc",
        offset("keen::ecs::NpcBed", "associatedNpc"),
    ),
    (
        "store_associated_npc_ids",
        Fact::Type("keen::ecs::StoreAssociatedNpcIds"),
    ),
    (
        "store_associated_npc_ids.npc_ids",
        offset("keen::ecs::StoreAssociatedNpcIds", "npcIds"),
    ),
    (
        "custom_player_string",
        Fact::Type("keen::ecs::CustomPlayerString"),
    ),
    (
        "custom_player_string.string_id",
        offset("keen::ecs::CustomPlayerString", "stringId"),
    ),
    (
        "logic_entity_placed_event",
        Fact::Type("keen::ecs::LogicEntityPlacedEvent"),
    ),
    (
        "logic_entity_placed_event.placed_by_player_id",
        offset("keen::ecs::LogicEntityPlacedEvent", "placedByPlayerId"),
    ),
    (
        "logic_entity_placed_event.placed_logic_entity_id",
        offset("keen::ecs::LogicEntityPlacedEvent", "placedLogicEntityId"),
    ),
    (
        "item_info.placement_box_min",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "placementAABBmin"),
        ]),
    ),
    (
        "item_info.placement_box_max",
        Fact::Path(&[
            ("keen::ItemInfo", "equipment"),
            ("keen::EquipmentSetup", "placementAABBmax"),
        ]),
    ),
    (
        "item_flag.is_logic_entity",
        enum_constant("keen::ItemFlag", "IsLogicEntity"),
    ),
    ("fuel_charges", Fact::Type("keen::ecs::Fuel")),
    (
        "fuel_charges.data_storage",
        offset("keen::ecs::Fuel", "dataStorage"),
    ),
    (
        "logic_graph_node_connected",
        Fact::Type("keen::ecs::LogicGraphNodeConnected"),
    ),
    (
        "logic_graph_node_connected.writer_graph_id",
        offset("keen::ecs::LogicGraphNodeConnected", "writerGraphId"),
    ),
    (
        "logic_graph_node_connected.has_reader_graphs",
        offset("keen::ecs::LogicGraphNodeConnected", "hasReaderGraphs"),
    ),
    (
        "logic_graph_trigger",
        Fact::Type("keen::ecs::LogicGraphTrigger"),
    ),
    (
        "logic_graph_trigger.trigger_config",
        offset("keen::ecs::LogicGraphTrigger", "triggerConfig"),
    ),
    (
        "logic_trigger_shape",
        Fact::Type("keen::ecs::LogicTriggerShape"),
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
