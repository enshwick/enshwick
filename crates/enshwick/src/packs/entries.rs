use serde::Deserialize;
use toml::Spanned;

/// The number format of a tooltip line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(super) enum FormatEntry {
    #[default]
    Number,
    Signed,
    Percent,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct StatEntry {
    /// The text key of the line's label.
    pub(super) label: String,
    pub(super) value: i32,
    #[serde(default)]
    pub(super) format: FormatEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct MeterEntry {
    /// The most it holds, 1 or more.
    pub(super) max: u16,
    /// What a new item holds; `max` when left out.
    pub(super) start: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct IconEntry {
    /// Recolour the vanilla icon toward this colour, red, green, blue.
    pub(super) tint: Option<[u8; 3]>,
    /// How far toward the colour, in percent; 50 when left out.
    pub(super) amount: Option<u8>,
    /// The vanilla icon to recolour; the item's `like` when left out.
    pub(super) like: Option<String>,
    /// A PNG of the pack's own under `icons/`, such as `icons/lantern.png`.
    pub(super) png: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ItemEntry {
    /// The item's name in the pack.
    pub(super) id: String,
    /// The item it is cloned from: a vanilla GUID or a content id.
    pub(super) like: String,
    /// The text key of its name.
    pub(super) name: Option<String>,
    /// The text key of its description.
    pub(super) description: Option<String>,
    /// Its name in the registry's debug list.
    pub(super) debug_name: Option<String>,
    pub(super) meter: Option<MeterEntry>,
    #[serde(default)]
    pub(super) stats: Vec<StatEntry>,
    pub(super) icon: Option<IconEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct RecipeEntry {
    /// The recipe's name in the pack.
    pub(super) id: String,
    /// The item whose recipe is copied.
    pub(super) like: String,
    /// The item it makes.
    pub(super) output: String,
    /// How many it makes; 1 when left out.
    #[serde(default = "one")]
    pub(super) count: u32,
    /// The copied ingredient counts times this; 1 when left out.
    #[serde(default = "one")]
    pub(super) cost: u32,
    /// Where it is made: a workshop of the pack, or a workshop's GUID or
    /// content id; the copied recipe's when left out.
    pub(super) workshop: Option<String>,
}

pub(super) const fn one() -> u32 {
    1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct StationRecipeEntry {
    /// The vanilla station's ingredient counts times this.
    pub(super) cost: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct WorkshopEntry {
    /// The station's name in the pack; it also names its item.
    pub(super) id: String,
    /// The vanilla station cloned: `workbench`.
    pub(super) like: String,
    /// The text key of the station's name, which also names the workshop.
    pub(super) name: Option<String>,
    /// The text key of the station item's description.
    pub(super) description: Option<String>,
    /// How the station is crafted; not at all when left out.
    pub(super) recipe: Option<StationRecipeEntry>,
    /// The item's name in the registry's debug list.
    pub(super) debug_name: Option<String>,
}

/// A new value: a number sets it, `{ times = x }` scales it.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub(super) enum ValueEntry {
    Set(f64),
    Scale(ScaleEntry),
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ScaleEntry {
    /// The factor, from 0 to 100.
    pub(super) times: f64,
}

/// The items a change names: a list of names, or a word that selects
/// vanilla items from the game's data.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub(super) enum ItemsEntry {
    /// `stackable` or `containers`.
    Word(String),
    /// Debug names, GUIDs or content ids.
    Names(Vec<String>),
}

/// A change of vanilla items' values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ItemChangeEntry {
    /// The items changed: debug names, GUIDs or content ids; or
    /// `"stackable"`, every item one slot holds more of, or `"containers"`,
    /// every item that places a container.
    pub(super) items: Option<ItemsEntry>,
    /// Every vanilla item of these categories, by the game's names, such as
    /// `Materials`.
    pub(super) category: Option<Vec<String>>,
    /// How many of the item one inventory slot holds, 1 to 65535.
    pub(super) max_stack: Option<ValueEntry>,
    /// How many slots the container the item places holds, 8 to 128 in
    /// steps of 8.
    pub(super) slots: Option<ValueEntry>,
    /// A weapon's damage factor, from 0 to 100.
    pub(super) damage_mod: Option<ValueEntry>,
    /// A weapon's attack speed factor, from 0 to 10.
    pub(super) speed_mod: Option<ValueEntry>,
    /// Every armour value, scaled: `{ times = x }` only.
    pub(super) armor: Option<ValueEntry>,
    /// How long the item burns as fuel, in seconds.
    pub(super) burn_time: Option<ValueEntry>,
    /// The energy the item gives as fuel.
    pub(super) energy_output: Option<ValueEntry>,
    /// The comfort the placed item gives.
    pub(super) comfort_amount: Option<ValueEntry>,
}

/// A change of vanilla recipes' values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct RecipeChangeEntry {
    /// The recipes changed: GUIDs, content ids or vanilla debug names.
    #[serde(default)]
    pub(super) recipes: Vec<String>,
    /// Every vanilla recipe that makes one of these items.
    #[serde(default)]
    pub(super) output: Vec<String>,
    /// Every vanilla recipe whose output is of these kinds:
    /// `building-block`, `roof`, `overgrowth`, `terrain` or `other`.
    #[serde(default)]
    pub(super) kind: Vec<String>,
    /// Every vanilla recipe made at these crafting stations, named by their
    /// items.
    #[serde(default)]
    pub(super) workshop: Vec<String>,
    /// How many of each item the recipe makes, 1 to 65535.
    pub(super) output_count: Option<ValueEntry>,
    /// How many of each ingredient it takes, 1 to 65535.
    pub(super) cost: Option<ValueEntry>,
    /// How long it takes, in seconds from 0 to 86400.
    pub(super) time: Option<ValueEntry>,
    /// How many of each named ingredient it takes, 0 to 65535: 0 takes the
    /// ingredient out, and one the recipe lacks is added.
    #[serde(default)]
    pub(super) ingredients: std::collections::BTreeMap<String, ValueEntry>,
    /// The crafting station it is made at from the next start, named by the
    /// station's item.
    pub(super) station: Option<String>,
}

/// A change of the game's loot for some items.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct LootChangeEntry {
    /// The items whose loot changes: debug names, GUIDs or content ids.
    pub(super) items: Vec<String>,
    /// The stack size random loot rolls, at least and at most alike.
    pub(super) stack_size: Option<ValueEntry>,
    /// The count container fill tables give, at least and at most alike.
    pub(super) count: Option<ValueEntry>,
    /// The chance container fill tables give the item, from 0 to 1.
    pub(super) spawn_probability: Option<ValueEntry>,
    /// The item's weight among the ones a fill table picks from.
    pub(super) weight: Option<ValueEntry>,
}

/// Where a drop comes from: one name or several.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub(super) enum FromEntry {
    One(String),
    Many(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct DropEntry {
    /// The items, one picked per drop.
    pub(super) items: Vec<String>,
    /// The chance per roll, from 0 to 1.
    pub(super) chance: f64,
    /// `bosses`, `locked-chests`, `corpses`, `chests`, `containers`,
    /// `destroyed`, or a list of the last four.
    pub(super) from: FromEntry,
    /// How many per drop, at least and at most; 1 when left out.
    pub(super) count: Option<[u16; 2]>,
    /// How full each dropped item's meter starts, at least and at most.
    pub(super) meter: Option<[u16; 2]>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ItemsFile {
    #[serde(default)]
    pub(super) item: Vec<Spanned<ItemEntry>>,
    #[serde(default)]
    pub(super) change: Vec<Spanned<ItemChangeEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecipesFile {
    #[serde(default)]
    pub(super) recipe: Vec<Spanned<RecipeEntry>>,
    #[serde(default)]
    pub(super) change: Vec<Spanned<RecipeChangeEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkshopsFile {
    #[serde(default)]
    pub(super) workshop: Vec<Spanned<WorkshopEntry>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LootFile {
    #[serde(default)]
    pub(super) drop: Vec<Spanned<DropEntry>>,
    #[serde(default)]
    pub(super) change: Vec<Spanned<LootChangeEntry>>,
}

/// The JSON Schemas of a pack's files, for editors: `(file, schema)`.
#[cfg(feature = "schema")]
#[must_use]
pub fn schemas() -> Vec<(&'static str, String)> {
    use super::manifest::PackManifest;
    use super::{ITEMS_FILE, LOOT_FILE, MANIFEST_FILE, RECIPES_FILE, WORKSHOPS_FILE};

    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Items {
        item: Option<Vec<ItemEntry>>,
        change: Option<Vec<ItemChangeEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Recipes {
        recipe: Option<Vec<RecipeEntry>>,
        change: Option<Vec<RecipeChangeEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Workshops {
        workshop: Option<Vec<WorkshopEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Loot {
        drop: Option<Vec<DropEntry>>,
        change: Option<Vec<LootChangeEntry>>,
    }
    let json = |schema: schemars::Schema| serde_json::to_string_pretty(&schema).unwrap_or_default();
    vec![
        (MANIFEST_FILE, json(schemars::schema_for!(PackManifest))),
        (ITEMS_FILE, json(schemars::schema_for!(Items))),
        (RECIPES_FILE, json(schemars::schema_for!(Recipes))),
        (WORKSHOPS_FILE, json(schemars::schema_for!(Workshops))),
        (LOOT_FILE, json(schemars::schema_for!(Loot))),
    ]
}
