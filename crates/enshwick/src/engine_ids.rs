//! The ids the API asks the loader for engine facts by; the loader maps each
//! to the game's names and offsets, so no engine name is compiled into a mod.

/// Declares each id as a constant and lists them all in [`ALL`].
macro_rules! ids {
    ($($name:ident = $id:literal;)*) => {
        $(pub(crate) const $name: &str = $id;)*

        /// Every id, for the loader's test that each one resolves.
        pub const ALL: &[&str] = &[$($id),*];
    };
}

ids! {
    WORLD_TRANSFORM_POSITION = "world_transform.position";
    WORLD_TRANSFORM_ORIENTATION = "world_transform.orientation";
    WORLD_TRANSFORM_SCALE = "world_transform.scale";
    NPC_COMMAND_EVENT = "npc_command_event";
    NPC_COMMAND_KIND = "npc_command_event.kind";
    NPC_COMMAND_NPC = "npc_command_event.npc";
    NPC_COMMAND_TARGET = "npc_command_event.target";
    SET_TOGGLE_STATE_EVENT = "set_toggle_state_event";
    SET_TOGGLE_SENDER = "set_toggle_state_event.sender";
    SET_TOGGLE_SIGNAL = "set_toggle_state_event.signal";
    SET_TOGGLE_STATE = "set_toggle_state_event.state";
    ADD_THREAT_TARGET_EVENT = "add_threat_target_event";
    ADD_THREAT_ATTACKER = "add_threat_target_event.attacker";
    ADD_THREAT_TARGET = "add_threat_target_event.target";
    ADD_THREAT_THREAT = "add_threat_target_event.threat";
    UI_PING_EVENT = "ui_ping_event";
    UI_PING_SENDER = "ui_ping_event.sender";
    UI_PING_POSITION = "ui_ping_event.position";
    ITEM_INFO = "item_info";
    ITEM_INFO_ID = "item_info.id";
    ITEM_INFO_OBJECT = "item_info.object";
    ITEM_INFO_NAME = "item_info.name";
    ITEM_INFO_DESCRIPTION = "item_info.description";
    ITEM_INFO_FLAGS = "item_info.flags";
    ITEM_INFO_PID_ENTITY = "item_info.pid_entity";
    ITEM_INFO_UI_VALUES = "item_info.ui_values";
    ITEM_INFO_ICON_IMAGE = "item_info.icon_image";
    ITEM_INFO_PLACED_ENTITY = "item_info.placed_entity";
    ITEM_STAT = "item_stat";
    ITEM_STAT_LABEL = "item_stat.label";
    ITEM_STAT_FORMAT = "item_stat.format";
    ITEM_STAT_VALUE = "item_stat.value";
    ITEM_STAT_SIGNED = "item_stat.signed";
    ITEM_REGISTRY = "item_registry";
    ITEM_REGISTRY_REFS = "item_registry.refs";
    ITEM_REGISTRY_NAMES = "item_registry.names";
    TEMPLATE = "template";
    TEMPLATE_COMPONENTS = "template.components";
    TEMPLATE_COLLECTION = "template_collection";
    TEMPLATE_COLLECTION_TEMPLATES = "template_collection.templates";
    FUEL_SETUP = "fuel_setup";
    FUEL_SETUP_MAX = "fuel_setup.max";
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
}
