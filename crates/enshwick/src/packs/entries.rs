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

/// A change of vanilla items' values.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ItemChangeEntry {
    /// The items changed: debug names, GUIDs or content ids.
    pub(super) items: Vec<String>,
    /// How many of the item one inventory slot holds, 1 to 65535.
    pub(super) max_stack: Option<ValueEntry>,
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
