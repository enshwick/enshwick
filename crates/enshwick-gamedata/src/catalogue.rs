//! Engine facts by an id of enshwick's own, answered from the game's
//! reflection.
//!
//! The API asks for a type, a hash or a field offset by id, and the loader
//! and the tools answer it, so a game update that renames or moves something
//! changes this table, not the mods.

mod facts;

pub use facts::FACTS;

use crate::reflect::{Reflection, TypeInfo};
use enshwick::Off;
use enshwick::content::recipes::RecipeKind;
use enshwick::content::{EngineFacts, Guid};

/// How one engine fact is found.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::{Fact, fact};
///
/// assert!(matches!(fact("item_info"), Some(Fact::Type(_))));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fact {
    /// A type, by its reflected name: answers type, size and hash lookups
    /// by the id.
    Type(&'static str),
    /// The offset of `field` in `ty`, plus `plus` bytes into the field.
    Offset {
        /// The reflected type.
        ty: &'static str,
        /// The field in it.
        field: &'static str,
        /// Bytes past the field's start.
        plus: u64,
    },
    /// The sum of the offsets of fields nested one in the next: each pair
    /// is a reflected type and its field, the field holding the next type.
    Path(&'static [(&'static str, &'static str)]),
    /// The value of the constant `name` of the reflected enum `ty`; for a
    /// mask, the index of its bit.
    Constant {
        /// The reflected enum.
        ty: &'static str,
        /// The constant in it.
        name: &'static str,
    },
}

/// The id of the enum an item's equipment slot holds, which
/// [`RECIPE_KINDS`] names constants of.
pub const SLOT_TYPE: &str = "item_slot_type";

/// The id of the offset of the item id in an item's static data.
pub const ITEM_ID: &str = "item_info.id";

/// The id of the offset of the equipment slot in an item's static data.
pub const ITEM_SLOT: &str = "item_info.equipment_slot";

/// The id of the offset of how many of an item one slot holds, a `u16`,
/// in an item's static data.
pub const ITEM_MAX_STACK: &str = "item_info.max_stack";

/// The id of the offset of how long a recipe takes, a `keen::Time` in
/// nanoseconds, in a recipe.
pub const RECIPE_DURATION: &str = "recipe.duration";

/// The id of the offset of an item's flags, a `u32`, in an item's static
/// data.
pub const ITEM_FLAGS: &str = "item_info.flags";

/// The id of the offset of an item's `ItemCategory`, a `u8`, in an item's
/// static data.
pub const ITEM_CATEGORY: &str = "item_info.category";

/// The id of the offset of the template an item places, a GUID, in an
/// item's static data.
pub const ITEM_PLACED_ENTITY: &str = "item_info.placed_entity";

/// The id of the offset of the template of an item's own entity, a GUID, in
/// an item's static data; a backpack's inventory sits there.
pub const ITEM_PID_ENTITY: &str = "item_info.pid_entity";

/// The id of the `Backpack` member of the game's equipment slots.
pub const EQUIPMENT_SLOT_BACKPACK: &str = "equipment_slot.backpack";

/// The id of the flag, a byte, that says an item places building voxels.
pub const ITEM_BUILDING_VOXEL: &str = "item_info.building_voxel";
/// The id of the voxel material an item places, a `u8`: 0x80 and up for a
/// building material.
pub const ITEM_PLACE_VOXEL_MATERIAL_ID: &str = "item_info.place_voxel_material_id";

/// The id of the event the game raises when a building piece is placed.
pub const BUILDING_PLACE_EVENT: &str = "building_place_event";
/// The id of the event the game raises when building voxels are torn down.
pub const BUILDING_TEAR_DOWN_EVENT: &str = "building_tear_down_event";
/// The ids of a placed piece's fields, as the place event holds them:
/// position, orientation, lowest and highest corner, material feedback,
/// tracking item and owner.
pub const BUILDING_PLACE_EVENT_FIELDS: [&str; 7] = [
    "building_place_event.position",
    "building_place_event.orientation",
    "building_place_event.volume_min",
    "building_place_event.volume_max",
    "building_place_event.material",
    "building_place_event.tracking_item_id",
    "building_place_event.owner_id",
];
/// The ids of a torn-down piece's fields, as the tear-down event holds
/// them: position, orientation, lowest and highest corner, material
/// feedback and owner.
pub const BUILDING_TEAR_DOWN_EVENT_FIELDS: [&str; 6] = [
    "building_tear_down_event.position",
    "building_tear_down_event.orientation",
    "building_tear_down_event.volume_min",
    "building_tear_down_event.volume_max",
    "building_tear_down_event.material",
    "building_tear_down_event.owner_id",
];

/// The id of a weapon's damage factor, an `f32`, in an item's static data.
pub const ITEM_DAMAGE_MOD: &str = "item_info.damage_mod";
/// The id of a weapon's attack speed factor, an `f32`.
pub const ITEM_SPEED_MOD: &str = "item_info.speed_mod";
/// The id of the flag that says an item deals damage.
pub const ITEM_DAMAGE_SET: &str = "item_info.damage_set";
/// The id of an item's armour values, `f32` each, as many as
/// [`ARMOR_DISTRIBUTION`] holds.
pub const ITEM_ARMOR: &str = "item_info.armor";
/// The id of the flag that says an item gives armour.
pub const ITEM_ARMOR_SET: &str = "item_info.armor_set";
/// The id of how long an item burns as fuel, a `keen::Time` in nanoseconds.
pub const ITEM_BURN_TIME: &str = "item_info.burn_time";
/// The id of the energy an item gives as fuel, a `u32`.
pub const ITEM_ENERGY_OUTPUT: &str = "item_info.energy_output";
/// The id of the flag that says an item is fuel.
pub const ITEM_FUEL_SET: &str = "item_info.fuel_set";
/// The id of the comfort a placed item gives, a `u32`.
pub const ITEM_COMFORT_AMOUNT: &str = "item_info.comfort_amount";
/// The id of the flag that says an item gives comfort.
pub const ITEM_COMFORT_SET: &str = "item_info.comfort_set";

/// The id of the bit of a player's permission mask that lets the player
/// edit a base.
pub const PERMISSION_CAN_EDIT_BASE: &str = "player_permissions.can_edit_base";
/// The id of the bit of a player's permission mask that lets the player
/// edit the world outside every base.
pub const PERMISSION_CAN_EDIT_WORLD: &str = "player_permissions.can_edit_world";

/// The id of the terraforming registry resource's building configs, an
/// array of [`BUILDING_TERRAFORMING_CONFIG`].
pub const TERRAFORMING_BUILDING_CONFIGS: &str = "terraforming_registry.building_configs";
/// The id of a building material's terraforming config.
pub const BUILDING_TERRAFORMING_CONFIG: &str = "building_terraforming_config";
/// The id of a building config's material, a `u8` counted from 0x80.
pub const BUILDING_CONFIG_MATERIAL: &str = "building_terraforming_config.building_material";
/// The id of a building config's hardness, a `u8`.
pub const BUILDING_CONFIG_HARDNESS: &str = "building_terraforming_config.hardness";

/// The id of the tag that keeps a prop from dropping random loot when it
/// is destroyed.
pub const DISABLE_RANDOM_LOOT_ON_DESTROY: &str = "disable_random_loot_on_destroy";
/// The id of the `Level` attribute and of its `u32` values: the level,
/// then its most.
pub const LEVEL: &str = "level";
/// The id of a `Level`'s values.
pub const LEVEL_DATA: &str = "level.data_storage";
/// The id of a plant's `Growth`.
pub const GROWTH: &str = "growth";
/// The id of a production station and of its state (a `u8`, 0 for no
/// recipe) and running and next recipe (`u32` each, 0 for none).
pub const FACTORY_STATION: &str = "factory_station";
/// The id of a production station's state.
pub const FACTORY_STATE: &str = "factory_station.state";
/// The id of a production station's running recipe.
pub const FACTORY_RUNNING: &str = "factory_station.running_recipe";
/// The id of a production station's next recipe.
pub const FACTORY_NEXT: &str = "factory_station.next_recipe";
/// The id of a survivor bed and of its survivor, a `u32`, 0 for none.
pub const NPC_BED: &str = "npc_bed";
/// The id of a survivor bed's survivor.
pub const NPC_BED_NPC: &str = "npc_bed.associated_npc";
/// The id of the survivors a station keeps, and of their two `u32`s.
pub const STORE_NPCS: &str = "store_associated_npc_ids";
/// The id of a station's two survivors.
pub const STORE_NPCS_IDS: &str = "store_associated_npc_ids.npc_ids";
/// The id of a player's text on a prop, and of its `u64` id, 0 for none.
pub const CUSTOM_PLAYER_STRING: &str = "custom_player_string";
/// The id of a player's text's id.
pub const CUSTOM_PLAYER_STRING_ID: &str = "custom_player_string.string_id";
/// The id of the event that tells who placed a logic prop.
pub const LOGIC_ENTITY_PLACED_EVENT: &str = "logic_entity_placed_event";
/// The ids of that event's fields: who placed it, then the prop.
pub const LOGIC_ENTITY_PLACED_FIELDS: [&str; 2] = [
    "logic_entity_placed_event.placed_by_player_id",
    "logic_entity_placed_event.placed_logic_entity_id",
];
/// The ids of an item's placement box, before it turns: the lowest and
/// the highest corner, `f32` on each axis.
pub const ITEM_PLACEMENT_BOX: [&str; 2] =
    ["item_info.placement_box_min", "item_info.placement_box_max"];
/// The id of the bit of an item's flags that marks a logic prop.
pub const ITEM_FLAG_IS_LOGIC_ENTITY: &str = "item_flag.is_logic_entity";
/// The id of a prop's saved `Fuel`, whose values hold its charges now
/// and most, two `i32`.
pub const FUEL_CHARGES: &str = "fuel_charges";
/// The id of a `Fuel`'s charges.
pub const FUEL_CHARGES_DATA: &str = "fuel_charges.data_storage";
/// The id of a prop's `FuelSetup`.
pub const FUEL_SETUP: &str = "fuel_setup";
/// The id of a `FuelSetup`'s most charges, an `i32`.
pub const FUEL_SETUP_MAX: &str = "fuel_setup.max";
/// The id of the share of its charges a fresh prop starts with, an `f32`.
pub const FUEL_SETUP_INITIAL: &str = "fuel_setup.start";
/// The id of a logic prop's wiring.
pub const LOGIC_CONNECTED: &str = "logic_graph_node_connected";
/// The id of the `u32` graph a logic prop writes to, 0 for none.
pub const LOGIC_CONNECTED_WRITER: &str = "logic_graph_node_connected.writer_graph_id";
/// The id of the byte that tells a logic prop has readers.
pub const LOGIC_CONNECTED_READERS: &str = "logic_graph_node_connected.has_reader_graphs";
/// The id of a logic trigger's settings.
pub const LOGIC_TRIGGER: &str = "logic_graph_trigger";
/// The id of the `u32` settings a logic trigger uses, which a player's
/// change of them replaces.
pub const LOGIC_TRIGGER_CONFIG: &str = "logic_graph_trigger.trigger_config";
/// The id of a logic trigger's area.
pub const LOGIC_TRIGGER_SHAPE: &str = "logic_trigger_shape";
/// The id of an animal's saved state, which marks a creature placed from
/// an item.
pub const ANIMAL_SAVED: &str = "animal_saved";
/// The id of the component that marks a flame altar, which owns a base.
pub const BASE_ID: &str = "base_id_component";
/// The id of the byte of an item that takes an altar slot: the flame
/// altar's item, which founds a base.
pub const ITEM_REQUIRES_ALTAR_SLOT: &str = "item_info.requires_altar_slot";
/// The id of a survivor's saved state, which marks a creature placed from
/// an item.
pub const ACTIVE_NPC_STATE_SAVED: &str = "active_npc_state_saved";
/// The id of the bit of an item's flags that marks a survivor.
pub const ITEM_FLAG_IS_NPC: &str = "item_flag.is_npc";
/// The id of the bit of an item's flags that marks an animal.
pub const ITEM_FLAG_IS_ANIMAL: &str = "item_flag.is_animal";
/// The id of the component that lets Quick Stash put items into a
/// container: magic chests have it, plain boxes do not.
pub const CRAFTING_STOCK: &str = "inventory_crafting_stock";
/// The ids of the bools that make a prop's `Fuel` refill on its own: over
/// time, as a well does, and from rain, as a water barrel does.
pub const FUEL_REFILLS: [&str; 2] = [
    "fuel_setup.automatic_refill_enabled",
    "fuel_setup.rain_refill_enabled",
];
/// The id of a container's per-chest Quick Stash switch a player sets.
pub const STASH_SWITCH: &str = "dynamic_inventory_crafting_stock";
/// The id of the switch's byte that tells a player set it.
pub const STASH_SWITCH_SET: &str = "dynamic_inventory_crafting_stock.override_is_set";
/// The id of the switch's byte: 0 keeps Quick Stash out while it is set.
pub const STASH_SWITCH_ON: &str = "dynamic_inventory_crafting_stock.override";
/// The id of the component that names the item an entity was placed from.
pub const USED_ITEM: &str = "used_item";
/// The id of an entity's place in the world.
pub const CURRENT_TRANSFORM: &str = "current_transform";
/// The id of the transform inside a [`CURRENT_TRANSFORM`].
pub const CURRENT_TRANSFORM_TRANSFORM: &str = "current_transform.transform";
/// The id of a transform's orientation, a quaternion of four `f32`.
pub const WORLD_TRANSFORM_ORIENTATION: &str = "world_transform.orientation";
/// The id of an entity's inventory setup and of its links (`u32`
/// entities), their categories (bytes) and its initialized byte.
pub const INVENTORY_SETUP: &str = "inventory_setup";
/// The id of an inventory setup's linked entities.
pub const INVENTORY_SETUP_LINKS: &str = "inventory_setup.links";
/// The id of an inventory setup's link categories.
pub const INVENTORY_SETUP_CATEGORIES: &str = "inventory_setup.categories";
/// The id of an inventory setup's initialized byte.
pub const INVENTORY_SETUP_INITIALIZED: &str = "inventory_setup.initialized";
/// The id of a sub-inventory's stacks.
pub const INVENTORY: &str = "inventory";
/// The id of a stack in an inventory, its `u32` item first.
pub const ITEM_STACK: &str = "item_stack";
/// The id of a stack's `u32` item entity, its PIDE.
pub const ITEM_STACK_PIDE: &str = "item_stack.pide";
/// The id of the component that relates an entity to its owner, the
/// owner's `u32` entity alone.
pub const OWNER: &str = "owner";
/// The id of a map marker's type, `MapMarker`.
pub const MAP_MARKER: &str = "map_marker";
/// The id of a map marker's label text tag, `markerId`, a GUID.
pub const MAP_MARKER_ID: &str = "map_marker.marker_id";
/// The id of a map marker's type reference, `markerType`, a GUID.
pub const MAP_MARKER_TYPE: &str = "map_marker.marker_type";
/// The id of a marker type in the marker type registry, `MapMarkerType`,
/// whose size is its stride there.
pub const MARKER_TYPE: &str = "map_marker_type";
/// The id of a marker type's id, a `u32`.
pub const MARKER_TYPE_ID: &str = "map_marker_type.marker_id";
/// The id of a marker type's flag for players' own markers, a byte.
pub const MARKER_TYPE_CUSTOM: &str = "map_marker_type.is_custom_marker_type";
/// The id of a marker type's flag for showing above the fog, a byte.
pub const MARKER_TYPE_ABOVE_FOG: &str = "map_marker_type.show_above_fog_of_war";
/// The id of a marker type's flag for showing a label, a byte.
pub const MARKER_TYPE_LABEL: &str = "map_marker_type.show_label";
/// The id of the marker types in the registry resource, a blob array of
/// [`MARKER_TYPE`].
pub const MARKER_REGISTRY_TYPES: &str = "map_marker_registry.types";
/// The id of the visibility of a marker the map shows.
pub const MARKER_VISIBLE: &str = "map_marker_visibility_type.visible";
/// The id of a scene chunk's resource type, which lists what it spawns.
pub const SCENE_CHUNK: &str = "scene_chunk";
/// The id of a scene chunk's template references, a blob array of GUIDs.
pub const SCENE_CHUNK_TEMPLATES: &str = "scene_chunk.templates";
/// The id of a scene chunk's spawn entries, a blob array of
/// [`SCENE_SPAWN_ENTRY`].
pub const SCENE_CHUNK_ENTITIES: &str = "scene_chunk.entities";
/// The id of a scene chunk's spawn entry type, whose size is its stride.
pub const SCENE_SPAWN_ENTRY: &str = "scene_spawn_entry";
/// The id of a spawn entry's template index, a `u16`.
pub const SCENE_SPAWN_TEMPLATE: &str = "scene_spawn_entry.template_index";
/// The id of a spawn entry's packed transform.
pub const SCENE_SPAWN_TRANSFORM: &str = "scene_spawn_entry.transform";
/// The ids of a packed transform's position halves, low then high word of
/// x, y and z, each a `u16`.
pub const SCENE_TRANSFORM_POSITION: [&str; 6] = [
    "scene_transform.position_x0",
    "scene_transform.position_x1",
    "scene_transform.position_y0",
    "scene_transform.position_y1",
    "scene_transform.position_z0",
    "scene_transform.position_z1",
];
/// The id of a template resource's components, a blob array of variants.
pub const TEMPLATE_COMPONENTS: &str = "template.components";

/// The id of the random loot table's type, of which the game has one.
pub const LOOT_RANDOM_TABLE: &str = "lootable_items";

/// The id of a container fill table's type.
pub const LOOT_FILL_TABLE: &str = "loot_fill_table";

/// The id of the armour values' type, whose size counts them.
pub const ARMOR_DISTRIBUTION: &str = "armor_distribution";

/// The kind of the recipes that make an item, by the name of the constant
/// of [`SLOT_TYPE`] the item's equipment slot holds; every other slot is
/// [`RecipeKind::Other`]. A new kind is a row here.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::RecipeKind;
/// use enshwick_gamedata::catalogue::RECIPE_KINDS;
///
/// assert!(RECIPE_KINDS.iter().any(|&(_, kind)| kind == RecipeKind::Roof));
/// ```
pub const RECIPE_KINDS: &[(&str, RecipeKind)] = &[
    ("BlueprintMaterial_Default", RecipeKind::BuildingBlock),
    ("BlueprintMaterial_Roof", RecipeKind::Roof),
    ("OvergrowthMaterial", RecipeKind::Overgrowth),
    ("TerrainMaterial", RecipeKind::Terrain),
];

/// The id of the offset of an item's tags in its static data, a blob array
/// of [`ITEM_TAG_WEIGHT`].
pub const ITEM_TAGS: &str = "item_info.tags";
/// The id of the type of one entry of an item's tags, whose size is its
/// stride.
pub const ITEM_TAG_WEIGHT: &str = "item_tag_weight";
/// The id of the offset of the tag, a GUID, in one entry of an item's tags.
pub const ITEM_TAG_WEIGHT_TAG: &str = "item_tag_weight.tag";

/// The item tag behind the crafting menu's Decorative group.
///
/// The game files a placed prop under that group by this tag on its item.
/// Game data, read from build 410's crafting menu definitions; the dedicated
/// server has no menu, so the item's tag is what both sides share. On server
/// build 399, 685 of the 2536 vanilla recipes make an item with it.
pub const DECORATIVE_ITEM_TAG: Guid = Guid::parse("ee3b266d-83e7-4239-8bdd-8e58fe12e9d1");

/// The kinds an item in a slot of no kind of its own takes from its tags.
///
/// Each row is the name of the constant of [`SLOT_TYPE`] the item's
/// equipment slot holds, the item tag and the kind. A row of
/// [`RECIPE_KINDS`] wins over these. A new kind by tag is a row here.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::RecipeKind;
/// use enshwick_gamedata::catalogue::RECIPE_KIND_TAGS;
///
/// assert!(RECIPE_KIND_TAGS.iter().any(|&(_, _, kind)| kind == RecipeKind::Decorative));
/// ```
pub const RECIPE_KIND_TAGS: &[(&str, Guid, RecipeKind)] =
    &[("BuildTool", DECORATIVE_ITEM_TAG, RecipeKind::Decorative)];

/// The slot value each row of [`RECIPE_KIND_TAGS`] has in `reflection`,
/// with its tag and kind; a constant the build does not reflect is left out.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::recipe_kind_tags;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn tags(reflection: &Reflection<'_>) -> usize {
///     recipe_kind_tags(reflection).len()
/// }
/// ```
#[must_use]
pub fn recipe_kind_tags(reflection: &Reflection<'_>) -> Vec<(u64, Guid, RecipeKind)> {
    let slot = type_name(SLOT_TYPE);
    RECIPE_KIND_TAGS
        .iter()
        .filter_map(|&(name, tag, kind)| Some((reflection.enum_value(slot, name).ok()?, tag, kind)))
        .collect()
}

/// The slot value each row of [`RECIPE_KINDS`] has in `reflection`, with
/// its kind; a constant the build does not reflect is left out.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::recipe_kind_slots;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn slots(reflection: &Reflection<'_>) -> usize {
///     recipe_kind_slots(reflection).len()
/// }
/// ```
#[must_use]
pub fn recipe_kind_slots(reflection: &Reflection<'_>) -> Vec<(u64, RecipeKind)> {
    let slot = type_name(SLOT_TYPE);
    RECIPE_KINDS
        .iter()
        .filter_map(|&(name, kind)| Some((reflection.enum_value(slot, name).ok()?, kind)))
        .collect()
}

const fn offset(ty: &'static str, field: &'static str) -> Fact {
    Fact::Offset { ty, field, plus: 0 }
}

const fn enum_constant(ty: &'static str, name: &'static str) -> Fact {
    Fact::Constant { ty, name }
}

/// The fact named `id`, if the catalogue has one.
///
/// # Examples
///
/// ```
/// assert!(enshwick_gamedata::catalogue::fact("no.such.id").is_none());
/// ```
#[must_use]
pub const fn fact(id: &str) -> Option<Fact> {
    let mut i = 0;
    while i < FACTS.len() {
        let (name, fact) = FACTS[i];
        if same(name, id) {
            return Some(fact);
        }
        i += 1;
    }
    None
}

const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The engine's name for the type `name` stands for: the reflected name of
/// a type id, or `name` itself, which a mod may pass at the low level.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::type_name;
///
/// assert_ne!(type_name("item_info"), "item_info");
/// assert_eq!(type_name("not_an_id"), "not_an_id");
/// ```
#[must_use]
pub fn type_name(name: &str) -> &str {
    match fact(name) {
        Some(Fact::Type(engine)) => engine,
        _ => name,
    }
}

/// The hash the game's reflection gives the type `id` names: FNV-1a over
/// its reflected name, the value resource keys and component lookups carry;
/// `None` when `id` names no type.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::type_hash;
///
/// assert!(type_hash("item_info").is_some());
/// assert_eq!(type_hash("item_info.name"), None);
/// ```
#[must_use]
pub const fn type_hash(id: &str) -> Option<u32> {
    match fact(id) {
        Some(Fact::Type(engine)) => Some(crate::reflect::fnv1a32(engine.as_bytes())),
        _ => None,
    }
}

/// The offset `fact` names, with `field` reading a reflected type's field
/// offset; `None` for a type or a field `field` does not find.
fn offset_of(fact: Fact, field: impl Fn(&str, &str) -> Option<u64>) -> Option<u64> {
    match fact {
        Fact::Offset {
            ty,
            field: name,
            plus,
        } => field(ty, name)?.checked_add(plus),
        Fact::Path(steps) => steps
            .iter()
            .try_fold(0u64, |sum, &(ty, name)| sum.checked_add(field(ty, name)?)),
        Fact::Type(_) | Fact::Constant { .. } => None,
    }
}

/// The type every game event type derives from.
const EVENT_BASE: &str = "keen::ecs::Event";

/// Every game event type `reflection` holds, in no set order: each type
/// that derives from the engine's event base, the bases between included.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::event_types;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn count(reflection: &Reflection<'_>) -> usize {
///     event_types(reflection).count()
/// }
/// ```
pub fn event_types<'r, 'a>(
    reflection: &'r Reflection<'a>,
) -> impl Iterator<Item = &'r TypeInfo<'a>> + 'r {
    reflection.derived_from(EVENT_BASE)
}

/// The value of the offset or enum constant named `id` in `reflection`;
/// `None` when the id names neither or reflection lacks what it names.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::catalogue::constant;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn name_at(reflection: &Reflection<'_>) -> Option<u64> {
///     constant("item_info.name", reflection)
/// }
/// ```
#[must_use]
pub fn constant(id: &str, reflection: &Reflection<'_>) -> Option<u64> {
    match fact(id)? {
        Fact::Constant { ty, name } => reflection.enum_value(ty, name).ok(),
        fact => offset_of(fact, |ty, field| reflection.field_offset(ty, field).ok()),
    }
}

/// The game's reflection answering engine facts by enshwick id, for the
/// loader and for tools that read a game image rather than run in the game.
///
/// # Examples
///
/// ```
/// use enshwick::content::items::ItemLayout;
/// use enshwick_gamedata::catalogue::Reflected;
/// use enshwick_gamedata::reflect::Reflection;
///
/// fn items(reflection: &Reflection<'_>) -> Result<ItemLayout, enshwick::Off> {
///     ItemLayout::resolve(&Reflected(reflection))
/// }
/// ```
#[derive(Debug)]
pub struct Reflected<'r, 'a>(pub &'r Reflection<'a>);

impl EngineFacts for Reflected<'_, '_> {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        constant(id, self.0)
            .and_then(|offset| usize::try_from(offset).ok())
            .ok_or_else(|| Off::new(format!("offset {id} is not reflected")))
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.0
            .type_info(type_name(id))
            .ok()
            .and_then(|info| usize::try_from(info.size()).ok())
            .ok_or_else(|| Off::new(format!("type {id} is not reflected")))
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        type_hash(id).ok_or_else(|| Off::new(format!("{id} names no type")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_listed_once() {
        let mut ids: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before);
    }

    #[test]
    fn the_catalogue_lists_exactly_the_ids_the_api_asks_for_in_its_order() {
        let ours: Vec<&str> = FACTS.iter().map(|&(id, _)| id).collect();
        assert_eq!(enshwick::__private::ENGINE_IDS, ours.as_slice());
    }

    /// `name` in lower case without underscores, to compare an id with the
    /// engine name it stands for.
    fn plain(name: &str) -> String {
        name.chars()
            .filter(|&c| c != '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn every_enum_constant_id_is_named_after_its_engine_constant() {
        for &(id, fact) in FACTS {
            if let Fact::Constant { name, .. } = fact {
                let (_, ours) = id.rsplit_once('.').unwrap();
                assert_eq!(plain(ours), plain(name), "{id}");
            }
        }
    }

    #[test]
    fn every_event_id_is_named_after_its_engine_type_and_field() {
        let events: Vec<(&str, &str)> = FACTS
            .iter()
            .filter_map(|&(id, fact)| match fact {
                Fact::Type(ty) if ty.ends_with("Event") => Some((id, ty)),
                _ => None,
            })
            .collect();
        for &(id, ty) in &events {
            let (_, short) = ty.rsplit_once("::").unwrap();
            assert_eq!(plain(id), plain(short), "{id}");
        }
        for &(id, fact) in FACTS {
            let Some((event, suffix)) = id.split_once('.') else {
                continue;
            };
            if let (true, Fact::Offset { field, .. }) =
                (events.iter().any(|&(e, _)| e == event), fact)
            {
                assert_eq!(plain(suffix), plain(field), "{id}");
            }
        }
    }

    #[test]
    fn every_game_settings_id_is_named_after_its_engine_field() {
        for &(id, fact) in FACTS {
            if let (Some(suffix), Fact::Offset { field, .. }) =
                (id.strip_prefix("game_settings."), fact)
            {
                assert_eq!(plain(suffix), plain(field), "{id}");
            }
        }
    }

    #[test]
    fn an_enum_constant_is_neither_a_type_nor_an_offset() {
        let fact = fact("damage_type.magical_fire");
        assert!(matches!(fact, Some(Fact::Constant { .. })));
        assert_eq!(type_hash("damage_type.magical_fire"), None);
        assert_eq!(offset_of(fact.unwrap(), |_, _| Some(8)), None);
    }

    #[test]
    fn a_type_id_answers_its_engine_name_and_other_names_pass_through() {
        assert_eq!(type_name("ui_ping_event"), "keen::ecs::UiPingEvent");
        assert_eq!(
            type_name("keen::ecs::UiPingEvent"),
            "keen::ecs::UiPingEvent"
        );
        assert_eq!(
            type_name("world_transform.position"),
            "world_transform.position"
        );
    }

    #[test]
    fn a_path_adds_the_offsets_of_its_fields() {
        let path = Fact::Path(&[("A", "b"), ("B", "c"), ("C", "d")]);
        let field = |ty: &str, name: &str| match (ty, name) {
            ("A", "b") => Some(0x10),
            ("B", "c") => Some(0x08),
            ("C", "d") => Some(0x04),
            _ => None,
        };
        assert_eq!(offset_of(path, field), Some(0x1c));
        let broken = Fact::Path(&[("A", "b"), ("B", "missing")]);
        assert_eq!(offset_of(broken, field), None);
    }

    #[test]
    fn an_offset_adds_its_bytes_into_the_field() {
        let fact = Fact::Offset {
            ty: "A",
            field: "b",
            plus: 4,
        };
        assert_eq!(offset_of(fact, |_, _| Some(8)), Some(12));
        assert_eq!(offset_of(Fact::Type("A"), |_, _| Some(8)), None);
    }

    #[test]
    fn a_type_hash_is_fnv1a_of_the_reflected_name() {
        assert_eq!(type_hash("item_info"), Some(0xb5ce_8765));
        assert_eq!(type_hash("item_registry"), Some(0x72ab_39b6));
        assert_eq!(type_hash("item_info.name"), None);
        assert_eq!(type_hash("no.such.id"), None);
    }

    #[test]
    fn every_offset_and_path_names_types_the_engine_spells_out() {
        for &(id, fact) in FACTS {
            match fact {
                Fact::Type(ty) | Fact::Offset { ty, .. } | Fact::Constant { ty, .. } => {
                    assert!(ty.starts_with("keen::"), "{id}");
                }
                Fact::Path(steps) => {
                    assert!(steps.len() > 1, "{id}");
                    assert!(steps.iter().all(|(ty, _)| ty.starts_with("keen::")), "{id}");
                }
            }
        }
    }

    #[test]
    fn each_slot_constant_names_one_kind() {
        let mut names: Vec<&str> = RECIPE_KINDS.iter().map(|&(name, _)| name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(names.len(), before);
        assert!(matches!(fact(SLOT_TYPE), Some(Fact::Type(_))));
        assert!(matches!(fact(ITEM_ID), Some(Fact::Offset { .. })));
        assert!(matches!(fact(ITEM_SLOT), Some(Fact::Path(_))));
    }

    #[test]
    fn a_tag_kind_is_a_kind_no_slot_gives_and_its_layout_is_catalogued() {
        for &(slot, tag, kind) in RECIPE_KIND_TAGS {
            assert!(
                RECIPE_KINDS
                    .iter()
                    .all(|&(name, k)| name != slot && k != kind)
            );
            assert_ne!(kind, RecipeKind::Other);
            assert_ne!(tag, Guid([0; 16]));
        }
        assert!(matches!(fact(ITEM_TAGS), Some(Fact::Offset { .. })));
        assert!(matches!(fact(ITEM_TAG_WEIGHT), Some(Fact::Type(_))));
        assert!(matches!(
            fact(ITEM_TAG_WEIGHT_TAG),
            Some(Fact::Offset { .. })
        ));
    }

    #[test]
    fn the_decorative_tag_reads_as_the_game_data_writes_it() {
        assert_eq!(
            DECORATIVE_ITEM_TAG.to_string(),
            "ee3b266d-83e7-4239-8bdd-8e58fe12e9d1"
        );
    }

    /// With either executable `ENSH_SERVER_EXE` or `ENSH_CLIENT_EXE` names
    /// (skipped without them), an item's tags are a blob array of tag
    /// entries, each holding its tag as a 16-byte GUID, and every slot of
    /// the tag kinds resolves.
    #[test]
    fn the_item_tags_are_reflected_as_guids() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                crate::reflect::Image::parse(&bytes, crate::reflect::Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let Some(Fact::Offset { ty, field, .. }) = fact(ITEM_TAGS) else {
                panic!("{ITEM_TAGS} is not an offset");
            };
            let tags = reflection.type_info(ty).unwrap().fields();
            let tags = tags.iter().find(|f| f.name() == field).unwrap();
            let array = reflection.type_info(tags.type_name().unwrap()).unwrap();
            assert_eq!(array.kind(), crate::reflect::Kind::BlobArray, "{var}");
            assert_eq!(array.inner(), Some(type_name(ITEM_TAG_WEIGHT)), "{var}");
            let Some(Fact::Offset { ty, field, .. }) = fact(ITEM_TAG_WEIGHT_TAG) else {
                panic!("{ITEM_TAG_WEIGHT_TAG} is not an offset");
            };
            let entry = reflection.type_info(ty).unwrap().fields();
            let tag = entry.iter().find(|f| f.name() == field).unwrap();
            let tag = reflection.type_info(tag.type_name().unwrap()).unwrap();
            assert_eq!(tag.size(), 16, "{var}: {}", tag.name());
            assert_eq!(recipe_kind_tags(&reflection).len(), RECIPE_KIND_TAGS.len());
        }
    }

    /// With either executable `ENSH_SERVER_EXE` or `ENSH_CLIENT_EXE` names
    /// (skipped without them), every slot constant of the kinds resolves,
    /// each to a value of its own.
    #[test]
    fn every_recipe_kind_s_slot_is_reflected() {
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let bytes = std::fs::read(path).unwrap();
            let image =
                crate::reflect::Image::parse(&bytes, crate::reflect::Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let slots = recipe_kind_slots(&reflection);
            assert_eq!(slots.len(), RECIPE_KINDS.len(), "{var}");
            let mut values: Vec<u64> = slots.iter().map(|&(value, _)| value).collect();
            values.sort_unstable();
            values.dedup();
            assert_eq!(values.len(), RECIPE_KINDS.len(), "{var}");
        }
    }

    /// With both executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` named
    /// (skipped without them), every API name the game names itself stands
    /// for a type, field or enum constant one of them reflects.
    #[test]
    fn every_api_game_name_is_reflected() {
        let (Ok(server), Ok(client)) = (
            std::env::var("ENSH_SERVER_EXE"),
            std::env::var("ENSH_CLIENT_EXE"),
        ) else {
            return;
        };
        let bytes = [
            std::fs::read(server).unwrap(),
            std::fs::read(client).unwrap(),
        ];
        let images = bytes.each_ref().map(|bytes| {
            crate::reflect::Image::parse(bytes, crate::reflect::Layout::File, None).unwrap()
        });
        let reflections = images.each_ref().map(Reflection::scan);
        for row in crate::game_names::GAME_NAMES {
            let found = reflections.iter().any(|reflection| {
                reflection.type_info(row.game_type).is_ok_and(|info| {
                    row.game_member.is_empty()
                        || info.fields().iter().any(|f| f.name() == row.game_member)
                        || info
                            .enum_values()
                            .iter()
                            .any(|v| v.name() == row.game_member)
                })
            });
            assert!(found, "{row:?}");
        }
    }

    /// With both executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name
    /// (skipped without them), every type and offset of the catalogue
    /// resolves in the reflection of at least one: the client alone has
    /// its interface.
    #[test]
    fn every_fact_resolves_in_a_game_executable() {
        let (Ok(server), Ok(client)) = (
            std::env::var("ENSH_SERVER_EXE"),
            std::env::var("ENSH_CLIENT_EXE"),
        ) else {
            return;
        };
        let bytes = [
            std::fs::read(server).unwrap(),
            std::fs::read(client).unwrap(),
        ];
        let images = bytes.each_ref().map(|bytes| {
            crate::reflect::Image::parse(bytes, crate::reflect::Layout::File, None).unwrap()
        });
        let reflections = images.each_ref().map(Reflection::scan);
        for &(id, fact) in FACTS {
            let found = reflections.iter().any(|reflection| match fact {
                Fact::Type(engine) => reflection.type_info(engine).is_ok(),
                Fact::Offset { .. } | Fact::Path(_) | Fact::Constant { .. } => {
                    constant(id, reflection).is_some()
                }
            });
            assert!(found, "{id}");
        }
        for &(id, fact) in FACTS {
            let Fact::Type(engine) = fact else { continue };
            if id.ends_with("_event") {
                let listed = reflections
                    .iter()
                    .any(|reflection| event_types(reflection).any(|ty| ty.name() == engine));
                assert!(listed, "{id} is not a listed event type");
            }
        }
    }
}
