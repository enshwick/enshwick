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
    /// The functions of the pack's `main.luau` that run on players' actions
    /// with the item.
    pub(super) on: Option<OnEntry>,
}

/// An item's functions in the pack's `main.luau`, one per kind of action,
/// each the name of a global function.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct OnEntry {
    /// Runs when a player eats, drinks or uses the item; `false` refuses it.
    pub(super) consume: Option<String>,
    /// Runs when a player throws the item away; `false` refuses it.
    pub(super) trash: Option<String>,
    /// Runs when a player equips the item; `false` refuses it.
    pub(super) equip: Option<String>,
    /// Runs when a player takes the item off; `false` refuses it.
    pub(super) unequip: Option<String>,
    /// Runs after a player picked the item up.
    pub(super) pickup: Option<String>,
    /// Runs after a player placed the item.
    pub(super) place: Option<String>,
    /// Runs once the game took the item of a use every filter allowed.
    pub(super) consumed: Option<String>,
}

impl OnEntry {
    /// Each named function with its kind's name, as `enshwick::items::Kind`
    /// spells it.
    pub(super) fn functions(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("Consume", &self.consume),
            ("Trash", &self.trash),
            ("Equip", &self.equip),
            ("Unequip", &self.unequip),
            ("Pickup", &self.pickup),
            ("Place", &self.place),
            ("Consumed", &self.consumed),
        ]
        .into_iter()
        .filter_map(|(kind, function)| Some((kind, function.as_deref()?)))
    }
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
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema), serde(untagged))]
pub(super) enum ValueEntry {
    Set(f64),
    Scale(ScaleEntry),
}

impl<'de> Deserialize<'de> for ValueEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match toml::Value::deserialize(deserializer)? {
            table @ toml::Value::Table(_) => super::values::shape(table).map(Self::Scale),
            value => super::values::shape(value).map(Self::Set),
        }
    }
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
#[serde(
    untagged,
    expecting = "`stackable`, `containers`, `backpacks` or a list of item names"
)]
pub(super) enum ItemsEntry {
    /// `stackable`, `containers` or `backpacks`.
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
    /// `"stackable"`, every item one slot holds more of, `"containers"`,
    /// every item that places a container, or `"backpacks"`.
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
    /// The containers whose fill tables alone change, by their templates'
    /// names, such as `LootContainer_Enemy_Corpse_Vukah`; every table when
    /// left out.
    #[serde(default)]
    pub(super) table: Vec<String>,
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
#[serde(
    untagged,
    expecting = "a source, such as `bosses`, or a list of sources"
)]
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
    /// The chance per roll, from 0 to 1, or `{ setting = "<key>" }`.
    pub(super) chance: super::values::NumberEntry<f64>,
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
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Rewards {
        reward: Option<Vec<super::rewards::RewardEntry>>,
    }
    #[derive(schemars::JsonSchema)]
    #[schemars(deny_unknown_fields)]
    #[expect(dead_code, reason = "only its schema is read")]
    struct Sounds {
        sound: Option<Vec<super::sounds::SoundEntry>>,
    }
    let json = |mut schema: schemars::Schema| {
        if let Some(map) = schema.as_object_mut() {
            shown(map);
        }
        serde_json::to_string_pretty(&schema).unwrap_or_default()
    };
    vec![
        (MANIFEST_FILE, json(schemars::schema_for!(PackManifest))),
        (ITEMS_FILE, json(schemars::schema_for!(Items))),
        (RECIPES_FILE, json(schemars::schema_for!(Recipes))),
        (WORKSHOPS_FILE, json(schemars::schema_for!(Workshops))),
        (LOOT_FILE, json(schemars::schema_for!(Loot))),
        (super::REWARDS_FILE, json(schemars::schema_for!(Rewards))),
        (super::SOUNDS_FILE, json(schemars::schema_for!(Sounds))),
    ]
}

/// The fence tokens rustdoc reads as Rust attributes, besides `rust` and the
/// `ignore-<target>`, `edition` and error code ones.
#[cfg(feature = "schema")]
const RUST_FENCE: [&str; 6] = [
    "ignore",
    "should_panic",
    "no_run",
    "compile_fail",
    "standalone_crate",
    "test_harness",
];

/// Leaves out of every description in `map`, nested ones too, the lines
/// rustdoc hides, so an editor shows a description as the docs do.
#[cfg(feature = "schema")]
fn shown(map: &mut serde_json::Map<String, serde_json::Value>) {
    for (key, field) in map.iter_mut() {
        match field {
            serde_json::Value::String(text) if key == "description" => {
                *text = without_hidden(text);
            }
            serde_json::Value::Object(inner) => shown(inner),
            serde_json::Value::Array(items) => items
                .iter_mut()
                .filter_map(serde_json::Value::as_object_mut)
                .for_each(shown),
            _ => {}
        }
    }
}

/// `text` without the lines of its Rust examples that are `#` alone or open
/// with `# `, which rustdoc compiles but does not show.
#[cfg(feature = "schema")]
fn without_hidden(text: &str) -> String {
    let mut rust = None;
    let mut kept = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(info) = trimmed.strip_prefix("```") {
            rust = match rust {
                Some(_) => None,
                None => Some(is_rust(info)),
            };
        } else if rust == Some(true) && (trimmed == "#" || trimmed.starts_with("# ")) {
            continue;
        }
        kept.push(line);
    }
    kept.join("\n")
}

/// Whether a fence's `info` makes its block Rust, the only kind with hidden
/// lines. By rustdoc's rule a fence is Rust without a token of another
/// language, or with `rust` or a Rust attribute before the first such token.
#[cfg(feature = "schema")]
fn is_rust(info: &str) -> bool {
    let (mut other, mut rust) = (false, false);
    for token in info.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
        let error_code = token.len() == 5
            && token.starts_with('E')
            && token.bytes().skip(1).all(|b| b.is_ascii_digit());
        match token {
            "rust" => rust = true,
            _ if token.starts_with("edition") => {}
            _ if RUST_FENCE.contains(&token) || token.starts_with("ignore-") || error_code => {
                rust |= !other;
            }
            _ => other = true,
        }
    }
    rust || !other
}

#[cfg(test)]
#[cfg(feature = "schema")]
mod tests {
    use super::*;

    #[test]
    fn a_description_keeps_what_the_docs_show_of_its_examples() {
        let text = "Reads a pack.\n\n```\n# extern crate enshwick_core as enshwick;\nlet a = 1;\n#\n# Ok::<(), ()>(())\n```\n\n```toml\n# A comment stays.\nid = \"you.a\"\n```";
        assert_eq!(
            without_hidden(text),
            "Reads a pack.\n\n```\nlet a = 1;\n```\n\n```toml\n# A comment stays.\nid = \"you.a\"\n```"
        );
    }

    #[test]
    fn a_fence_with_an_error_code_or_a_target_hides_lines_and_another_language_does_not() {
        for info in [
            "compile_fail,E0428",
            "ignore-windows",
            "no_run,text",
            "ignore",
        ] {
            let text = format!("```{info}\n# hidden\nshown\n```");
            assert_eq!(without_hidden(&text), format!("```{info}\nshown\n```"));
        }
        for info in ["text", "text,no_run", "json"] {
            let text = format!("```{info}\n# kept\n```");
            assert_eq!(without_hidden(&text), text);
        }
    }

    #[test]
    fn every_description_of_a_schema_is_cut_but_a_field_named_description_stays() {
        let mut value = serde_json::json!({
            "description": "A.\n```\n# hidden\nshown\n```",
            "properties": { "description": { "description": "B.\n```rust\n# hidden\n```" } },
            "anyOf": [{ "description": "C.\n```\n# hidden\n```" }],
            "default": "```\n# kept\n```",
        });
        shown(value.as_object_mut().unwrap());
        assert_eq!(value["description"], "A.\n```\nshown\n```");
        assert_eq!(
            value["properties"]["description"]["description"],
            "B.\n```rust\n```"
        );
        assert_eq!(value["anyOf"][0]["description"], "C.\n```\n```");
        assert_eq!(value["default"], "```\n# kept\n```");
    }
}
