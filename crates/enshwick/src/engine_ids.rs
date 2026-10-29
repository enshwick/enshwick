//! The ids the API asks the loader for engine facts by; the loader maps each
//! to the game's names and offsets, so no engine name is compiled into a mod.

/// Declares each id as a constant and lists them all in [`ALL`].
macro_rules! ids {
    ($($(#[$meta:meta])* $name:ident = $id:literal;)*) => {
        $($(#[$meta])* #[doc(hidden)] pub const $name: &str = $id;)*

        /// Every id, for the loader's test that each one resolves.
        pub const ALL: &[&str] = &[$($id),*];
    };
}

ids! {
    HIT_EVENT = "hit_event";
    HIT_EVENT_ROOT_SOURCE_ID = "hit_event.root_source_id";
    HIT_EVENT_TARGET_ID = "hit_event.target_id";
    HIT_EVENT_SOURCE_ID = "hit_event.source_id";
    HIT_EVENT_ROOT_TARGET_ID = "hit_event.root_target_id";
    HIT_EVENT_WEAPON_PIDE_ID = "hit_event.weapon_pide_id";
    HIT_HEALTH_CHANGE = "hit_event.health_change";
    HIT_STAMINA_CHANGE = "hit_event.stamina_change";
    HIT_DAMAGE_TYPES = "hit_event.damage_types";
    HIT_EVENT_ATTACK_CATEGORY = "hit_event.attack_category";
    HIT_TARGET_FACTION = "hit_event.target_faction";
    ENTITY_DIED_EVENT = "entity_died_event";
    ENTITY_DIED_EVENT_DEAD_ENTITY_ID = "entity_died_event.dead_entity_id";
    HIT_HISTORY = "hit_history";
    HIT_HISTORY_LAST_INDEX = "hit_history.last_index";
    HIT_HISTORY_ENTRIES = "hit_history.entries";
    HIT_HISTORY_END = "hit_history.end";
    EXPERIENCE_CREDIT = "experience_credit";
    EXPERIENCE_CREDIT_LAST_PLAYER = "experience_credit.last_player";
    LEVEL_UP_EVENT = "level_up_event";
    LEVEL_UP_EVENT_PLAYER_ENTITY_ID = "level_up_event.player_entity_id";
    LEVEL_UP_EVENT_LEVEL = "level_up_event.level";
    XP_GAINED_EVENT = "xp_gained_event";
    XP_GAINED_EVENT_PLAYER_ENTITY_ID = "xp_gained_event.player_entity_id";
    XP_GAINED_EVENT_AMOUNT = "xp_gained_event.amount";
    XP_GAINED_EVENT_SOURCE = "xp_gained_event.source";
    CRAFT_RECIPE_EVENT = "craft_recipe_event";
    CRAFT_RECIPE_EVENT_CRAFTING_OPERATOR_ID = "craft_recipe_event.crafting_operator_id";
    CRAFT_RECIPE_EVENT_CRAFTING_STATION_ID = "craft_recipe_event.crafting_station_id";
    CRAFT_RECIPE_EVENT_RECIPE_ID = "craft_recipe_event.recipe_id";
    CRAFT_RECIPE_EVENT_RECIPE_AMOUNT = "craft_recipe_event.recipe_amount";
    CRAFT_RECIPE_SUCCESS_EVENT = "craft_recipe_success_event";
    CRAFT_RECIPE_SUCCESS_EVENT_WORKSHOP_ID = "craft_recipe_success_event.workshop_id";
    CRAFT_RECIPE_SUCCESS_EVENT_RECIPE_ID = "craft_recipe_success_event.recipe_id";
    ACCEPT_OFFERED_INTERACTION_EVENT = "accept_offered_interaction_event";
    ACCEPT_OFFERED_INTERACTION_EVENT_GUEST_ENTITY_ID = "accept_offered_interaction_event.guest_entity_id";
    ACCEPT_OFFERED_INTERACTION_EVENT_HOST_ENTITY_ID = "accept_offered_interaction_event.host_entity_id";
    ACCEPT_OFFERED_INTERACTION_EVENT_OFFER_ID = "accept_offered_interaction_event.offer_id";
    /// The id of the event the game raises when a building piece is placed.
    BUILDING_PLACE_EVENT = "building_place_event";
    BUILDING_PLACE_EVENT_OWNER_ID = "building_place_event.owner_id";
    BUILDING_PLACE_EVENT_POSITION = "building_place_event.position";
    BUILDING_PLACE_EVENT_ORIENTATION = "building_place_event.orientation";
    BUILDING_PLACE_EVENT_VOLUME_MIN = "building_place_event.volume_min";
    BUILDING_PLACE_EVENT_VOLUME_MAX = "building_place_event.volume_max";
    BUILDING_PLACE_EVENT_MATERIAL = "building_place_event.material";
    BUILDING_PLACE_EVENT_TRACKING_ITEM_ID = "building_place_event.tracking_item_id";
    /// The id of the event the game raises when building voxels are torn down.
    BUILDING_TEAR_DOWN_EVENT = "building_tear_down_event";
    BUILDING_TEAR_DOWN_EVENT_OWNER_ID = "building_tear_down_event.owner_id";
    BUILDING_TEAR_DOWN_EVENT_POSITION = "building_tear_down_event.position";
    BUILDING_TEAR_DOWN_EVENT_ORIENTATION = "building_tear_down_event.orientation";
    BUILDING_TEAR_DOWN_EVENT_VOLUME_MIN = "building_tear_down_event.volume_min";
    BUILDING_TEAR_DOWN_EVENT_VOLUME_MAX = "building_tear_down_event.volume_max";
    BUILDING_TEAR_DOWN_EVENT_MATERIAL = "building_tear_down_event.material";
    BUILDING_PICK_UP_EVENT = "building_pick_up_event";
    BUILDING_PICK_UP_EVENT_OWNER_ID = "building_pick_up_event.owner_id";
    BUILDING_PICK_UP_EVENT_POSITION = "building_pick_up_event.position";
    BUILDING_PICK_UP_EVENT_VOLUME_MIN = "building_pick_up_event.volume_min";
    BUILDING_PICK_UP_EVENT_VOLUME_MAX = "building_pick_up_event.volume_max";
    BUILDING_PICK_UP_EVENT_MATERIAL = "building_pick_up_event.material";
    BUILDING_PICK_UP_EVENT_PICKED_ENTITY_ID = "building_pick_up_event.picked_entity_id";
    FLAME_ALTAR_EVENT = "flame_altar_event";
    FLAME_ALTAR_EVENT_PLAYER_ENTITY_ID = "flame_altar_event.player_entity_id";
    FLAME_ALTAR_EVENT_BASE_ENTITY_ID = "flame_altar_event.base_entity_id";
    FLAME_ALTAR_EVENT_TYPE = "flame_altar_event.type";
    LOOT_INTERACTION_EVENT = "loot_interaction_event";
    LOOT_INTERACTION_EVENT_GUEST_ID = "loot_interaction_event.guest_id";
    LOOT_INTERACTION_EVENT_HOST_ID = "loot_interaction_event.host_id";
    LOOT_INTERACTION_EVENT_LOOT_ALL = "loot_interaction_event.loot_all";
    HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT = "handle_equipment_slot_changed_event";
    HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_PLAYER_ENTITY_ID = "handle_equipment_slot_changed_event.player_entity_id";
    HANDLE_EQUIPMENT_SLOT_CHANGED_EVENT_EQUIPMENT_SLOT = "handle_equipment_slot_changed_event.equipment_slot";
    REVIVED_PLAYER_EVENT = "revived_player_event";
    REVIVED_PLAYER_EVENT_TARGET_ENTITY_ID = "revived_player_event.target_entity_id";
    REVIVED_PLAYER_EVENT_SOURCE_ENTITY_ID = "revived_player_event.source_entity_id";
    /// The id of a transform's position, three 32.32 fixed-point `i64`.
    WORLD_TRANSFORM_POSITION = "world_transform.position";
    /// The id of a transform's orientation, a quaternion of four `f32`.
    WORLD_TRANSFORM_ORIENTATION = "world_transform.orientation";
    WORLD_TRANSFORM_SCALE = "world_transform.scale";
    NPC_COMMAND_EVENT = "npc_command_event";
    NPC_COMMAND_EVENT_TYPE = "npc_command_event.type";
    NPC_COMMAND_EVENT_NPC_ID = "npc_command_event.npc_id";
    NPC_COMMAND_EVENT_TARGET_TRANSFORM = "npc_command_event.target_transform";
    SET_TOGGLE_STATE_EVENT = "set_toggle_state_event";
    SET_TOGGLE_STATE_EVENT_SENDER_ID = "set_toggle_state_event.sender_id";
    SET_TOGGLE_STATE_EVENT_SIGNAL_ID = "set_toggle_state_event.signal_id";
    SET_TOGGLE_STATE_EVENT_NEW_STATE = "set_toggle_state_event.new_state";
    ADD_THREAT_TARGET_EVENT = "add_threat_target_event";
    ADD_THREAT_TARGET_EVENT_ATTACKER_ID = "add_threat_target_event.attacker_id";
    ADD_THREAT_TARGET_EVENT_TARGET_ID = "add_threat_target_event.target_id";
    ADD_THREAT_TARGET_EVENT_THREAT_VALUE = "add_threat_target_event.threat_value";
    UI_PING_EVENT = "ui_ping_event";
    UI_PING_EVENT_SENDER_ENTITY_ID = "ui_ping_event.sender_entity_id";
    UI_PING_POSITION = "ui_ping_event.position";
    ITEM_INFO = "item_info";
    /// The id of the offset of the item id in an item's static data.
    ITEM_INFO_ID = "item_info.id";
    ITEM_INFO_OBJECT = "item_info.object";
    ITEM_INFO_NAME = "item_info.name";
    ITEM_INFO_DESCRIPTION = "item_info.description";
    /// The id of the offset of an item's flags, a `u32`, in an item's static
    /// data.
    ITEM_INFO_FLAGS = "item_info.flags";
    /// The id of the offset of an item's `ItemCategory`, a `u8`, in an item's
    /// static data.
    ITEM_INFO_CATEGORY = "item_info.category";
    /// The id of the offset of how many of an item one slot holds, a `u16`,
    /// in an item's static data.
    ITEM_INFO_MAX_STACK = "item_info.max_stack";
    /// The id of the offset of the template of an item's own entity, a GUID, in
    /// an item's static data; a backpack's inventory sits there.
    ITEM_INFO_PID_ENTITY = "item_info.pid_entity";
    ITEM_INFO_UI_VALUES = "item_info.ui_values";
    ITEM_INFO_ICON_IMAGE = "item_info.icon_image";
    /// The id of the offset of the template an item places, a GUID, in an
    /// item's static data.
    ITEM_INFO_PLACED_ENTITY = "item_info.placed_entity";
    /// The id of the flag, a byte, that says an item places building voxels.
    ITEM_INFO_BUILDING_VOXEL = "item_info.building_voxel";
    /// The id of the voxel material an item places, a `u8`: 0x80 and up for a
    /// building material.
    ITEM_INFO_PLACE_VOXEL_MATERIAL_ID = "item_info.place_voxel_material_id";
    /// The id of the offset of the equipment slot in an item's static data.
    ITEM_INFO_EQUIPMENT_SLOT = "item_info.equipment_slot";
    /// The id of the offset of an item's tags in its static data, a blob array
    /// of [`ITEM_TAG_WEIGHT`].
    ITEM_INFO_TAGS = "item_info.tags";
    /// The id of a weapon's damage factor, an `f32`, in an item's static data.
    ITEM_INFO_DAMAGE_MOD = "item_info.damage_mod";
    /// The id of a weapon's attack speed factor, an `f32`.
    ITEM_INFO_SPEED_MOD = "item_info.speed_mod";
    /// The id of the flag that says an item deals damage.
    ITEM_INFO_DAMAGE_SET = "item_info.damage_set";
    /// The id of an item's armour values, `f32` each, as many as
    /// [`ARMOR_DISTRIBUTION`] holds.
    ITEM_INFO_ARMOR = "item_info.armor";
    /// The id of the flag that says an item gives armour.
    ITEM_INFO_ARMOR_SET = "item_info.armor_set";
    /// The id of how long an item burns as fuel, a `keen::Time` in nanoseconds.
    ITEM_INFO_BURN_TIME = "item_info.burn_time";
    /// The id of the energy an item gives as fuel, a `u32`.
    ITEM_INFO_ENERGY_OUTPUT = "item_info.energy_output";
    /// The id of the flag that says an item is fuel.
    ITEM_INFO_FUEL_SET = "item_info.fuel_set";
    /// The id of the comfort a placed item gives, a `u32`.
    ITEM_INFO_COMFORT_AMOUNT = "item_info.comfort_amount";
    /// The id of the flag that says an item gives comfort.
    ITEM_INFO_COMFORT_SET = "item_info.comfort_set";
    /// The id of the armour values' type, whose size counts them.
    ARMOR_DISTRIBUTION = "armor_distribution";
    /// The id of the enum an item's equipment slot holds, which
    /// [`RECIPE_KINDS`] names constants of.
    ITEM_SLOT_TYPE = "item_slot_type";
    /// The id of the type of one entry of an item's tags, whose size is its
    /// stride.
    ITEM_TAG_WEIGHT = "item_tag_weight";
    /// The id of the offset of the tag, a GUID, in one entry of an item's tags.
    ITEM_TAG_WEIGHT_TAG = "item_tag_weight.tag";
    ITEM_STAT = "item_stat";
    ITEM_STAT_LABEL = "item_stat.label";
    ITEM_STAT_FORMAT = "item_stat.format";
    ITEM_STAT_VALUE = "item_stat.value";
    ITEM_STAT_SIGNED = "item_stat.signed";
    ITEM_REGISTRY = "item_registry";
    ITEM_REGISTRY_REFS = "item_registry.refs";
    ITEM_REGISTRY_NAMES = "item_registry.names";
    TEMPLATE = "template";
    /// The id of a template resource's components, a blob array of variants.
    TEMPLATE_COMPONENTS = "template.components";
    TEMPLATE_NAME = "template.name";
    TEMPLATE_COLLECTION = "template_collection";
    TEMPLATE_COLLECTION_TEMPLATES = "template_collection.templates";
    /// The id of a prop's `FuelSetup`.
    FUEL_SETUP = "fuel_setup";
    /// The id of a `FuelSetup`'s most charges, an `i32`.
    FUEL_SETUP_MAX = "fuel_setup.max";
    /// The id of the share of its charges a fresh prop starts with, an `f32`.
    FUEL_SETUP_START = "fuel_setup.start";
    CRAFTING = "crafting";
    CRAFTING_WORKSHOP = "crafting.workshop";
    CRAFTING_WORKSHOP_ID = "crafting.workshop_id";
    CRAFTING_INTERACTION = "crafting_interaction";
    CRAFTING_INTERACTION_WORKSHOP = "crafting_interaction.workshop";
    CRAFTING_INTERACTION_WORKSHOP_ID = "crafting_interaction.workshop_id";
    CLIENT_CRAFTING_INTERACTION = "client_crafting_interaction";
    CLIENT_CRAFTING_INTERACTION_WORKSHOP_ID = "client_crafting_interaction.workshop_id";
    WORKSHOP_REGISTRY = "workshop_registry";
    WORKSHOP_REGISTRY_WORKSHOPS = "workshop_registry.workshops";
    WORKSHOP = "workshop";
    WORKSHOP_GUID = "workshop.guid";
    WORKSHOP_ID = "workshop.id";
    WORKSHOP_NAME = "workshop.name";
    WORKSHOP_ITEM = "workshop.item";
    WORKSHOP_ITEM_REF = "workshop.item_ref";
    WORKSHOP_NPC = "workshop.npc";
    RECIPE_REGISTRY = "recipe_registry";
    RECIPE_REGISTRY_RECIPES = "recipe_registry.recipes";
    RECIPE = "recipe";
    RECIPE_GUID = "recipe.guid";
    RECIPE_ID = "recipe.id";
    RECIPE_WORKSHOP_GUID = "recipe.workshop_guid";
    RECIPE_WORKSHOP = "recipe.workshop";
    RECIPE_INPUT = "recipe.input";
    RECIPE_OUTPUT = "recipe.output";
    RECIPE_COMPLETION = "recipe.completion";
    /// The id of the offset of how long a recipe takes, a `keen::Time` in
    /// nanoseconds, in a recipe.
    RECIPE_DURATION = "recipe.duration";
    RECIPE_REQUIRED_PROPS = "recipe.required_props";
    RECIPE_DEBUG_NAME = "recipe.debug_name";
    RECIPE_INGREDIENT = "recipe_ingredient";
    RECIPE_STACK = "recipe_stack";
    RECIPE_STACK_ITEM = "recipe_stack.item";
    RECIPE_STACK_COUNT = "recipe_stack.count";
    UI_IMAGE = "ui_image";
    UI_BUNDLE = "ui_bundle";
    UI_BUNDLE_CRAFTING_MENU = "ui_bundle.crafting_menu";
    CRAFTING_MENU_WORKSHOPS = "crafting_menu.workshops";
    CRAFTING_MENU_TREES = "crafting_menu.trees";
    CRAFTING_MENU_MAX_GROUPS = "crafting_menu.max_groups";
    CRAFTING_MENU_MAX_SETS = "crafting_menu.max_sets";
    CRAFTING_MENU_TREE = "crafting_menu_tree";
    CRAFTING_MENU_TREE_GROUPS = "crafting_menu_tree.groups";
    CRAFTING_MENU_GROUP = "crafting_menu_group";
    CRAFTING_MENU_GROUP_ID = "crafting_menu_group.id";
    CRAFTING_MENU_GROUP_SETS = "crafting_menu_group.sets";
    CRAFTING_MENU_SET = "crafting_menu_set";
    CRAFTING_MENU_SET_ENTRIES = "crafting_menu_set.entries";
    /// The id of an entity's place in the world.
    CURRENT_TRANSFORM = "current_transform";
    /// The id of the transform inside a [`CURRENT_TRANSFORM`].
    CURRENT_TRANSFORM_TRANSFORM = "current_transform.transform";
    /// The id of the component that names the item an entity was placed from.
    USED_ITEM = "used_item";
    LABEL = "label";
    /// The id of an entity's inventory setup and of its links (`u32`
    /// entities), their categories (bytes) and its initialized byte.
    INVENTORY_SETUP = "inventory_setup";
    /// The id of an inventory setup's linked entities.
    INVENTORY_SETUP_LINKS = "inventory_setup.links";
    /// The id of an inventory setup's link categories.
    INVENTORY_SETUP_CATEGORIES = "inventory_setup.categories";
    /// The id of an inventory setup's initialized byte.
    INVENTORY_SETUP_INITIALIZED = "inventory_setup.initialized";
    INVENTORY_SETUP_GENERIC_SLOTS = "inventory_setup.generic_slots";
    INVENTORY_SETUP_AVAILABLE_SLOTS = "inventory_setup.available_slots";
    INVENTORY_SETUP_OVERWRITE = "inventory_setup_overwrite";
    INVENTORY_SETUP_OVERWRITE_GENERIC_SLOTS = "inventory_setup_overwrite.generic_slots";
    /// The id of a sub-inventory's stacks.
    INVENTORY = "inventory";
    /// The id of a stack in an inventory, its `u32` item first.
    ITEM_STACK = "item_stack";
    ITEM_STACK_COUNT = "item_stack.count";
    /// The id of a stack's `u32` item entity, its PIDE.
    ITEM_STACK_PIDE = "item_stack.pide";
    DURABILITY = "durability";
    DURABILITY_MAX = "durability.max";
    FUEL = "fuel";
    FUEL_MAX = "fuel.max";
    /// The id of the component that relates an entity to its owner, the
    /// owner's `u32` entity alone.
    OWNER = "owner";
    INVENTORY_EMPTY_STATE = "inventory_empty_state";
    INVENTORY_EMPTY_STATE_EMPTY = "inventory_empty_state.empty";
    DESTROY_ON_LOOT = "destroy_on_loot";
    LIFETIME = "lifetime";
    AREA_LOOT = "area_loot";
    AREA_LOOT_EXCLUDE = "area_loot.exclude";
    INTERACTION_OFFER = "interaction_offer";
    INTERACTION_OFFER_ACCEPT_TIME = "interaction_offer.accept_time";
    INTERACTION_OFFER_ACCEPT_GUEST = "interaction_offer.accept_guest";
    INTERACTION_OFFER_DISABLED = "interaction_offer.disabled";
    PLAYER_INPUT = "player_input";
    BOSS = "boss";
    INTERACTION_LOCK = "interaction_lock";
    INTERACTION_LOCK_ITEM = "interaction_lock.item";
    LOOT_SLOT = "loot_slot";
    LOOT_SLOT_ITEM = "loot_slot.item";
    LOOT_SLOT_COUNT = "loot_slot.count";
    /// The id of the random loot table's type, of which the game has one.
    LOOTABLE_ITEMS = "lootable_items";
    LOOTABLE_ITEMS_ITEMS = "lootable_items.items";
    LOOTABLE_ITEM = "lootable_item";
    LOOTABLE_ITEM_ITEM = "lootable_item.item";
    LOOTABLE_ITEM_STACK_MIN = "lootable_item.stack_min";
    LOOTABLE_ITEM_STACK_MAX = "lootable_item.stack_max";
    LOOTABLE_ITEM_STACK_MAX_SCALED = "lootable_item.stack_max_scaled";
    /// The id of a container fill table's type.
    LOOT_FILL_TABLE = "loot_fill_table";
    LOOT_FILL_TABLE_ROOT = "loot_fill_table.root";
    LOOT_FILL_GROUP = "loot_fill_group";
    LOOT_FILL_GROUP_STACKS = "loot_fill_group.stacks";
    LOOT_FILL_GROUP_GROUPS = "loot_fill_group.groups";
    LOOT_FILL_STACK = "loot_fill_stack";
    LOOT_FILL_STACK_ITEM = "loot_fill_stack.item";
    LOOT_FILL_STACK_COUNT_MIN = "loot_fill_stack.count_min";
    LOOT_FILL_STACK_COUNT_MAX = "loot_fill_stack.count_max";
    LOOT_FILL_STACK_PROBABILITY = "loot_fill_stack.probability";
    LOOT_FILL_STACK_WEIGHT = "loot_fill_stack.weight";
    ATTRIBUTE_SET_HEALTH = "attribute_set.health";
    ATTRIBUTE_SET_STAMINA = "attribute_set.stamina";
    ATTRIBUTE_SET_MANA = "attribute_set.mana";
    ATTRIBUTE_SET_LEVEL = "attribute_set.level";
    ATTRIBUTE_SET_EXPERIENCE = "attribute_set.experience";
    ATTRIBUTE_SET_FUEL = "attribute_set.fuel";
    ATTRIBUTE_SET_SHROUD = "attribute_set.shroud";
    SKILL_NODES = "skill_nodes";
    SKILL_NODES_IDS = "skill_nodes.ids";
    SKILL_NODES_LEVELS = "skill_nodes.levels";
    BASE_ACTOR = "base_actor";
    BASE_ACTOR_STATE = "base_actor.state";
    NPC = "npc";
    ANIMAL = "animal";
    LOOT_FILL = "loot_fill";
    LOOT_FILL_DEFAULT_INVENTORY = "loot_fill.table";
    RESOURCE_NODE = "resource_node";
    GAME_SETTINGS = "game_settings";
    GAME_SETTINGS_PLAYER_HEALTH_FACTOR = "game_settings.player_health_factor";
    GAME_SETTINGS_PLAYER_MANA_FACTOR = "game_settings.player_mana_factor";
    GAME_SETTINGS_PLAYER_STAMINA_FACTOR = "game_settings.player_stamina_factor";
    GAME_SETTINGS_PLAYER_BODY_HEAT_FACTOR = "game_settings.player_body_heat_factor";
    GAME_SETTINGS_PLAYER_DIVING_TIME_FACTOR = "game_settings.player_diving_time_factor";
    GAME_SETTINGS_ENABLE_DURABILITY = "game_settings.enable_durability";
    GAME_SETTINGS_ENABLE_STARVING_DEBUFF = "game_settings.enable_starving_debuff";
    GAME_SETTINGS_FOOD_BUFF_DURATION_FACTOR = "game_settings.food_buff_duration_factor";
    GAME_SETTINGS_FROM_HUNGER_TO_STARVING = "game_settings.from_hunger_to_starving";
    GAME_SETTINGS_SHROUD_TIME_FACTOR = "game_settings.shroud_time_factor";
    GAME_SETTINGS_TOMBSTONE_MODE = "game_settings.tombstone_mode";
    GAME_SETTINGS_ENABLE_GLIDER_TURBULENCES = "game_settings.enable_glider_turbulences";
    GAME_SETTINGS_WEATHER_FREQUENCY = "game_settings.weather_frequency";
    GAME_SETTINGS_FISHING_DIFFICULTY = "game_settings.fishing_difficulty";
    GAME_SETTINGS_MINING_DAMAGE_FACTOR = "game_settings.mining_damage_factor";
    GAME_SETTINGS_PLANT_GROWTH_SPEED_FACTOR = "game_settings.plant_growth_speed_factor";
    GAME_SETTINGS_RESOURCE_DROP_STACK_AMOUNT_FACTOR = "game_settings.resource_drop_stack_amount_factor";
    GAME_SETTINGS_FACTORY_PRODUCTION_SPEED_FACTOR = "game_settings.factory_production_speed_factor";
    GAME_SETTINGS_PERK_UPGRADE_RECYCLING_FACTOR = "game_settings.perk_upgrade_recycling_factor";
    GAME_SETTINGS_PERK_COST_FACTOR = "game_settings.perk_cost_factor";
    GAME_SETTINGS_EXPERIENCE_COMBAT_FACTOR = "game_settings.experience_combat_factor";
    GAME_SETTINGS_EXPERIENCE_MINING_FACTOR = "game_settings.experience_mining_factor";
    GAME_SETTINGS_EXPERIENCE_EXPLORATION_QUESTS_FACTOR = "game_settings.experience_exploration_quests_factor";
    GAME_SETTINGS_RANDOM_SPAWNER_AMOUNT = "game_settings.random_spawner_amount";
    GAME_SETTINGS_AGGRO_POOL_AMOUNT = "game_settings.aggro_pool_amount";
    GAME_SETTINGS_ENEMY_DAMAGE_FACTOR = "game_settings.enemy_damage_factor";
    GAME_SETTINGS_ENEMY_HEALTH_FACTOR = "game_settings.enemy_health_factor";
    GAME_SETTINGS_ENEMY_STAMINA_FACTOR = "game_settings.enemy_stamina_factor";
    GAME_SETTINGS_ENEMY_PERCEPTION_RANGE_FACTOR = "game_settings.enemy_perception_range_factor";
    GAME_SETTINGS_BOSS_DAMAGE_FACTOR = "game_settings.boss_damage_factor";
    GAME_SETTINGS_BOSS_HEALTH_FACTOR = "game_settings.boss_health_factor";
    GAME_SETTINGS_THREAT_BONUS = "game_settings.threat_bonus";
    GAME_SETTINGS_PACIFY_ALL_ENEMIES = "game_settings.pacify_all_enemies";
    GAME_SETTINGS_TAMING_STARTLE_REPERCUSSION = "game_settings.taming_startle_repercussion";
    GAME_SETTINGS_DAY_TIME_DURATION = "game_settings.day_time_duration";
    GAME_SETTINGS_NIGHT_TIME_DURATION = "game_settings.night_time_duration";
    GAME_SETTINGS_CURSE_MODIFIER = "game_settings.curse_modifier";
    GAME_SETTINGS_FLAME_LEVEL_FOG_REMOVAL = "game_settings.flame_level_fog_removal";
    DAMAGE_TYPE_PHYSICAL_CUT = "damage_type.physical_cut";
    DAMAGE_TYPE_PHYSICAL_PIERCE = "damage_type.physical_pierce";
    DAMAGE_TYPE_PHYSICAL_BLUNT = "damage_type.physical_blunt";
    DAMAGE_TYPE_MAGICAL_FIRE = "damage_type.magical_fire";
    DAMAGE_TYPE_MAGICAL_ICE = "damage_type.magical_ice";
    DAMAGE_TYPE_MAGICAL_FOG = "damage_type.magical_fog";
    DAMAGE_TYPE_MAGICAL_LIGHTNING = "damage_type.magical_lightning";
    DAMAGE_TYPE_HEALING = "damage_type.healing";
    DAMAGE_TYPE_POISON = "damage_type.poison";
    DAMAGE_TYPE_WOOD = "damage_type.wood";
    DAMAGE_TYPE_STONE = "damage_type.stone";
    DAMAGE_TYPE_METAL = "damage_type.metal";
    DAMAGE_TYPE_EXPLOSION = "damage_type.explosion";
    ATTACK_CATEGORY_MELEE = "attack_category.melee";
    ATTACK_CATEGORY_RANGED = "attack_category.ranged";
    ATTACK_CATEGORY_MAGIC = "attack_category.magic";
    ATTACK_CATEGORY_TOOL = "attack_category.tool";
    ATTACK_CATEGORY_OTHER = "attack_category.other";
    ATTACK_CATEGORY_AO_E = "attack_category.ao_e";
    ATTACK_CATEGORY_SKILL_SHOT = "attack_category.skill_shot";
    EXPERIENCE_SOURCE_NONE = "experience_source.none";
    EXPERIENCE_SOURCE_EXPLORATION = "experience_source.exploration";
    EXPERIENCE_SOURCE_LORE = "experience_source.lore";
    EXPERIENCE_SOURCE_QUEST = "experience_source.quest";
    EXPERIENCE_SOURCE_COMBAT = "experience_source.combat";
    EXPERIENCE_SOURCE_MINING = "experience_source.mining";
    EXPERIENCE_SOURCE_ITEM_SET_COMPLETED = "experience_source.item_set_completed";
    ALTAR_ACTION_ABANDON = "altar_action.abandon";
    ALTAR_ACTION_RECLAIM = "altar_action.reclaim";
    ALTAR_ACTION_REMOVE_WATER = "altar_action.remove_water";
    ALTAR_ACTION_SET_AS_PREFERRED_SPAWN_ALTAR = "altar_action.set_as_preferred_spawn_altar";
    EQUIPMENT_SLOT_VANITY_HEAD = "equipment_slot.vanity_head";
    EQUIPMENT_SLOT_VANITY_UPPER_BODY = "equipment_slot.vanity_upper_body";
    EQUIPMENT_SLOT_VANITY_ARMS = "equipment_slot.vanity_arms";
    EQUIPMENT_SLOT_VANITY_LOWER_BODY = "equipment_slot.vanity_lower_body";
    EQUIPMENT_SLOT_VANITY_FEET = "equipment_slot.vanity_feet";
    EQUIPMENT_SLOT_ARMOUR_HEAD = "equipment_slot.armour_head";
    EQUIPMENT_SLOT_ARMOUR_UPPER_BODY = "equipment_slot.armour_upper_body";
    EQUIPMENT_SLOT_ARMOUR_ARMS = "equipment_slot.armour_arms";
    EQUIPMENT_SLOT_ARMOUR_LOWER_BODY = "equipment_slot.armour_lower_body";
    EQUIPMENT_SLOT_ARMOUR_FEET = "equipment_slot.armour_feet";
    EQUIPMENT_SLOT_RING0 = "equipment_slot.ring0";
    EQUIPMENT_SLOT_RING1 = "equipment_slot.ring1";
    EQUIPMENT_SLOT_RANGED_WEAPON = "equipment_slot.ranged_weapon";
    EQUIPMENT_SLOT_SHIELD = "equipment_slot.shield";
    EQUIPMENT_SLOT_ARROW = "equipment_slot.arrow";
    EQUIPMENT_SLOT_SPELL = "equipment_slot.spell";
    EQUIPMENT_SLOT_FISHING_BAIT = "equipment_slot.fishing_bait";
    EQUIPMENT_SLOT_TERRAIN_MATERIAL = "equipment_slot.terrain_material";
    EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_DEFAULT = "equipment_slot.blueprint_material_default";
    EQUIPMENT_SLOT_BLUEPRINT_MATERIAL_ROOF = "equipment_slot.blueprint_material_roof";
    EQUIPMENT_SLOT_OVERGROWTH_MATERIAL = "equipment_slot.overgrowth_material";
    EQUIPMENT_SLOT_HOOK_SHOT = "equipment_slot.hook_shot";
    EQUIPMENT_SLOT_GLIDER = "equipment_slot.glider";
    /// The id of the `Backpack` member of the game's equipment slots.
    EQUIPMENT_SLOT_BACKPACK = "equipment_slot.backpack";
    EQUIPMENT_SLOT_MAIN_HAND = "equipment_slot.main_hand";
    INVENTORY_ERROR_INVALID_STATE = "inventory_error.invalid_state";
    INVENTORY_ERROR_OUT_OF_MEMORY = "inventory_error.out_of_memory";
    GAME_SETTINGS_PRESET_DEFAULT = "game_settings_preset.default";
    GAME_SETTINGS_PRESET_RELAXED = "game_settings_preset.relaxed";
    GAME_SETTINGS_PRESET_HARD = "game_settings_preset.hard";
    GAME_SETTINGS_PRESET_SURVIVAL = "game_settings_preset.survival";
    GAME_SETTINGS_PRESET_CUSTOM = "game_settings_preset.custom";
    TOMBSTONE_MODE_ADD_BACKPACK_MATERIALS = "tombstone_mode.add_backpack_materials";
    TOMBSTONE_MODE_EVERYTHING = "tombstone_mode.everything";
    TOMBSTONE_MODE_NO_TOMBSTONE = "tombstone_mode.no_tombstone";
    WEATHER_FREQUENCY_DISABLED = "weather_frequency.disabled";
    WEATHER_FREQUENCY_RARE = "weather_frequency.rare";
    WEATHER_FREQUENCY_NORMAL = "weather_frequency.normal";
    WEATHER_FREQUENCY_OFTEN = "weather_frequency.often";
    GENERIC_DIFFICULTY_VERY_EASY = "generic_difficulty.very_easy";
    GENERIC_DIFFICULTY_EASY = "generic_difficulty.easy";
    GENERIC_DIFFICULTY_NORMAL = "generic_difficulty.normal";
    GENERIC_DIFFICULTY_HARD = "generic_difficulty.hard";
    GENERIC_DIFFICULTY_VERY_HARD = "generic_difficulty.very_hard";
    RANDOM_SPAWNER_AMOUNT_FEW = "random_spawner_amount.few";
    RANDOM_SPAWNER_AMOUNT_NORMAL = "random_spawner_amount.normal";
    RANDOM_SPAWNER_AMOUNT_MANY = "random_spawner_amount.many";
    RANDOM_SPAWNER_AMOUNT_EXTREME = "random_spawner_amount.extreme";
    RANDOM_SPAWNER_AMOUNT_AUTOMATIC = "random_spawner_amount.automatic";
    AGGRO_POOL_AMOUNT_FEW = "aggro_pool_amount.few";
    AGGRO_POOL_AMOUNT_NORMAL = "aggro_pool_amount.normal";
    AGGRO_POOL_AMOUNT_MANY = "aggro_pool_amount.many";
    AGGRO_POOL_AMOUNT_EXTREME = "aggro_pool_amount.extreme";
    TAMING_STARTLE_REPERCUSSION_KEEP_PROGRESS = "taming_startle_repercussion.keep_progress";
    TAMING_STARTLE_REPERCUSSION_LOSE_SOME_PROGRESS = "taming_startle_repercussion.lose_some_progress";
    TAMING_STARTLE_REPERCUSSION_LOSE_ALL_PROGRESS = "taming_startle_repercussion.lose_all_progress";
    CURSE_MODIFIER_EASY = "curse_modifier.easy";
    CURSE_MODIFIER_NORMAL = "curse_modifier.normal";
    CURSE_MODIFIER_HARD = "curse_modifier.hard";
    /// The id of the bit of a player's permission mask that lets the player
    /// edit a base.
    PLAYER_PERMISSIONS_CAN_EDIT_BASE = "player_permissions.can_edit_base";
    /// The id of the bit of a player's permission mask that lets the player
    /// edit the world outside every base.
    PLAYER_PERMISSIONS_CAN_EDIT_WORLD = "player_permissions.can_edit_world";
    /// The id of the terraforming registry resource's building configs, an
    /// array of [`BUILDING_TERRAFORMING_CONFIG`].
    TERRAFORMING_REGISTRY_BUILDING_CONFIGS = "terraforming_registry.building_configs";
    /// The id of a building material's terraforming config.
    BUILDING_TERRAFORMING_CONFIG = "building_terraforming_config";
    /// The id of a building config's material, a `u8` counted from 0x80.
    BUILDING_TERRAFORMING_CONFIG_BUILDING_MATERIAL = "building_terraforming_config.building_material";
    /// The id of a building config's hardness, a `u8`.
    BUILDING_TERRAFORMING_CONFIG_HARDNESS = "building_terraforming_config.hardness";
    PLAYER_WAYPOINT = "player_waypoint";
    PLAYER_WAYPOINT_ENABLED = "player_waypoint.enabled";
    PLAYER_WAYPOINT_POSITION = "player_waypoint.waypoint_position";
    /// The id of a map marker's type, `MapMarker`.
    MAP_MARKER = "map_marker";
    /// The id of a map marker's label text tag, `markerId`, a GUID.
    MAP_MARKER_MARKER_ID = "map_marker.marker_id";
    /// The id of a map marker's type reference, `markerType`, a GUID.
    MAP_MARKER_MARKER_TYPE = "map_marker.marker_type";
    /// The id of a marker type in the marker type registry, `MapMarkerType`,
    /// whose size is its stride there.
    MAP_MARKER_TYPE = "map_marker_type";
    /// The id of a marker type's id, a `u32`.
    MAP_MARKER_TYPE_MARKER_ID = "map_marker_type.marker_id";
    /// The id of a marker type's flag for players' own markers, a byte.
    MAP_MARKER_TYPE_IS_CUSTOM = "map_marker_type.is_custom_marker_type";
    /// The id of a marker type's flag for showing above the fog, a byte.
    MAP_MARKER_TYPE_SHOW_ABOVE_FOG = "map_marker_type.show_above_fog_of_war";
    /// The id of a marker type's flag for showing a label, a byte.
    MAP_MARKER_TYPE_SHOW_LABEL = "map_marker_type.show_label";
    /// The id of the marker types in the registry resource, a blob array of
    /// [`MAP_MARKER_TYPE`].
    MAP_MARKER_REGISTRY_TYPES = "map_marker_registry.types";
    /// The id of the visibility of a marker the map shows.
    MAP_MARKER_VISIBILITY_TYPE_VISIBLE = "map_marker_visibility_type.visible";
    /// The id of the marker type registry's resource type, of which the
    /// client's archive holds one.
    MAP_MARKER_REGISTRY = "map_marker_registry";
    /// The id of a marker type's name text tag, a GUID.
    MAP_MARKER_TYPE_TYPE_NAME = "map_marker_type.type_name";
    /// The id of a marker type's sorting, a byte of [`MARKER_TYPE_SORTING`].
    MAP_MARKER_TYPE_SORTING_CATEGORY = "map_marker_type.sorting_category";
    /// The id of a marker type's flag for fast travel destinations, a byte.
    MAP_MARKER_TYPE_IS_FAST_TRAVEL_DESTINATION = "map_marker_type.is_fast_travel_destination";
    /// The id of the enum that sorts marker types on the map.
    MARKER_TYPE_SORTING = "marker_type_sorting";
    /// The id of the sorting of marker types in no group.
    MARKER_TYPE_SORTING_NONE = "marker_type_sorting.none";
    /// The id of the sorting of the Flame's marker types.
    MARKER_TYPE_SORTING_FLAME_RELATED = "marker_type_sorting.flame_related";
    /// The id of the sorting of dungeons' marker types.
    MARKER_TYPE_SORTING_DUNGEONS = "marker_type_sorting.dungeons";
    /// The id of the sorting of locations' marker types.
    MARKER_TYPE_SORTING_LOCATIONS = "marker_type_sorting.locations";
    /// The id of the sorting of players' own marker types.
    MARKER_TYPE_SORTING_CUSTOM_MARKER = "marker_type_sorting.custom_marker";
    /// The id of a scene chunk's resource type, which lists what it spawns.
    SCENE_CHUNK = "scene_chunk";
    /// The id of a scene chunk's template references, a blob array of GUIDs.
    SCENE_CHUNK_TEMPLATES = "scene_chunk.templates";
    /// The id of a scene chunk's spawn entries, a blob array of
    /// [`SCENE_SPAWN_ENTRY`].
    SCENE_CHUNK_ENTITIES = "scene_chunk.entities";
    /// The id of a scene chunk's spawn entry type, whose size is its stride.
    SCENE_SPAWN_ENTRY = "scene_spawn_entry";
    /// The id of a spawn entry's template index, a `u16`.
    SCENE_SPAWN_ENTRY_TEMPLATE_INDEX = "scene_spawn_entry.template_index";
    /// The id of a spawn entry's packed transform.
    SCENE_SPAWN_ENTRY_TRANSFORM = "scene_spawn_entry.transform";
    SCENE_TRANSFORM_POSITION_X0 = "scene_transform.position_x0";
    SCENE_TRANSFORM_POSITION_X1 = "scene_transform.position_x1";
    SCENE_TRANSFORM_POSITION_Y0 = "scene_transform.position_y0";
    SCENE_TRANSFORM_POSITION_Y1 = "scene_transform.position_y1";
    SCENE_TRANSFORM_POSITION_Z0 = "scene_transform.position_z0";
    SCENE_TRANSFORM_POSITION_Z1 = "scene_transform.position_z1";
    /// The id of a scene's resource type, keyed by the scene's GUID.
    SCENE_RESOURCE = "scene_resource";
    /// The id of a scene resource's flag for a biome map, a byte.
    SCENE_RESOURCE_HAS_BIOME_MAP = "scene_resource.has_biome_map";
    /// The id of a scene's biome map resource type, keyed by the scene's GUID.
    BIOME_MAP = "biome_map";
    /// The id of a biome map's world origin, two `i32`, x then z.
    BIOME_MAP_WORLD_ORIGIN = "biome_map.world_origin";
    /// The id of a biome map's world size in cells, two `u32`, x then z.
    BIOME_MAP_WORLD_SIZE = "biome_map.world_size";
    /// The id of a biome map's metres per cell, a `u32`.
    BIOME_MAP_MAP_SCALE = "biome_map.map_scale";
    /// The id of the tag that keeps a prop from dropping random loot when it
    /// is destroyed.
    DISABLE_RANDOM_LOOT_ON_DESTROY = "disable_random_loot_on_destroy";
    /// The id of the `Level` attribute and of its `u32` values: the level,
    /// then its most.
    LEVEL = "level";
    /// The id of a `Level`'s values.
    LEVEL_DATA_STORAGE = "level.data_storage";
    /// The id of a plant's `Growth`.
    GROWTH = "growth";
    /// The id of a production station and of its state (a `u8`, 0 for no
    /// recipe) and running and next recipe (`u32` each, 0 for none).
    FACTORY_STATION = "factory_station";
    /// The id of a production station's state.
    FACTORY_STATION_STATE = "factory_station.state";
    /// The id of a production station's running recipe.
    FACTORY_STATION_RUNNING_RECIPE = "factory_station.running_recipe";
    /// The id of a production station's next recipe.
    FACTORY_STATION_NEXT_RECIPE = "factory_station.next_recipe";
    /// The id of a survivor bed and of its survivor, a `u32`, 0 for none.
    NPC_BED = "npc_bed";
    /// The id of a survivor bed's survivor.
    NPC_BED_ASSOCIATED_NPC = "npc_bed.associated_npc";
    /// The id of the survivors a station keeps, and of their two `u32`s.
    STORE_ASSOCIATED_NPC_IDS = "store_associated_npc_ids";
    /// The id of a station's two survivors.
    STORE_ASSOCIATED_NPC_IDS_NPC_IDS = "store_associated_npc_ids.npc_ids";
    /// The id of a player's text on a prop, and of its `u64` id, 0 for none.
    CUSTOM_PLAYER_STRING = "custom_player_string";
    /// The id of a player's text's id.
    CUSTOM_PLAYER_STRING_STRING_ID = "custom_player_string.string_id";
    /// The id of the event that tells who placed a logic prop.
    LOGIC_ENTITY_PLACED_EVENT = "logic_entity_placed_event";
    LOGIC_ENTITY_PLACED_EVENT_PLACED_BY_PLAYER_ID = "logic_entity_placed_event.placed_by_player_id";
    LOGIC_ENTITY_PLACED_EVENT_PLACED_LOGIC_ENTITY_ID = "logic_entity_placed_event.placed_logic_entity_id";
    ITEM_INFO_PLACEMENT_BOX_MIN = "item_info.placement_box_min";
    ITEM_INFO_PLACEMENT_BOX_MAX = "item_info.placement_box_max";
    /// The id of the bit of an item's flags that marks a logic prop.
    ITEM_FLAG_IS_LOGIC_ENTITY = "item_flag.is_logic_entity";
    /// The id of a prop's saved `Fuel`, whose values hold its charges now
    /// and most, two `i32`.
    FUEL_CHARGES = "fuel_charges";
    /// The id of a `Fuel`'s charges.
    FUEL_CHARGES_DATA_STORAGE = "fuel_charges.data_storage";
    /// The id of a logic prop's wiring.
    LOGIC_GRAPH_NODE_CONNECTED = "logic_graph_node_connected";
    /// The id of the `u32` graph a logic prop writes to, 0 for none.
    LOGIC_GRAPH_NODE_CONNECTED_WRITER_GRAPH_ID = "logic_graph_node_connected.writer_graph_id";
    /// The id of the byte that tells a logic prop has readers.
    LOGIC_GRAPH_NODE_CONNECTED_HAS_READER_GRAPHS = "logic_graph_node_connected.has_reader_graphs";
    /// The id of a logic trigger's settings.
    LOGIC_GRAPH_TRIGGER = "logic_graph_trigger";
    /// The id of the `u32` settings a logic trigger uses, which a player's
    /// change of them replaces.
    LOGIC_GRAPH_TRIGGER_TRIGGER_CONFIG = "logic_graph_trigger.trigger_config";
    /// The id of a logic trigger's area.
    LOGIC_TRIGGER_SHAPE = "logic_trigger_shape";
    /// The id of an animal's saved state, which marks a creature placed from
    /// an item.
    ANIMAL_SAVED = "animal_saved";
    /// The id of the component that marks a flame altar, which owns a base.
    BASE_ID_COMPONENT = "base_id_component";
    /// The id of the byte of an item that takes an altar slot: the flame
    /// altar's item, which founds a base.
    ITEM_INFO_REQUIRES_ALTAR_SLOT = "item_info.requires_altar_slot";
    /// The id of a survivor's saved state, which marks a creature placed from
    /// an item.
    ACTIVE_NPC_STATE_SAVED = "active_npc_state_saved";
    /// The id of the bit of an item's flags that marks a survivor.
    ITEM_FLAG_IS_NPC = "item_flag.is_npc";
    /// The id of the bit of an item's flags that marks an animal.
    ITEM_FLAG_IS_ANIMAL = "item_flag.is_animal";
    /// The id of the component that lets Quick Stash put items into a
    /// container: magic chests have it, plain boxes do not.
    INVENTORY_CRAFTING_STOCK = "inventory_crafting_stock";
    FUEL_SETUP_AUTOMATIC_REFILL_ENABLED = "fuel_setup.automatic_refill_enabled";
    FUEL_SETUP_RAIN_REFILL_ENABLED = "fuel_setup.rain_refill_enabled";
    /// The id of a container's per-chest Quick Stash switch a player sets.
    DYNAMIC_INVENTORY_CRAFTING_STOCK = "dynamic_inventory_crafting_stock";
    /// The id of the switch's byte that tells a player set it.
    DYNAMIC_INVENTORY_CRAFTING_STOCK_OVERRIDE_IS_SET = "dynamic_inventory_crafting_stock.override_is_set";
    /// The id of the switch's byte: 0 keeps Quick Stash out while it is set.
    DYNAMIC_INVENTORY_CRAFTING_STOCK_OVERRIDE = "dynamic_inventory_crafting_stock.override";
}
