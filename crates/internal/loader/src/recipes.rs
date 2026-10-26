//! Disabled recipes: the server's recipe file, a world's own and mods' rules
//! name recipes nobody crafts. They leave every crafting menu and the server
//! refuses to craft them.
//!
//! `mod-configs/enshwick.recipes.toml` holds the rules of every world this
//! process runs, and `mod-data/enshwick/worlds/<key>/recipes.toml` a world's
//! own, applied over it; mods add rules through the API, under both files.
//! Each file disables recipes by name, by the item they make or by the
//! workshop they are made at, and enables some back.
//!
//! The game keeps the recipes of each workshop in a list it asks for by
//! workshop id, for the menus of stations, of survivors and of crafting by
//! hand (workshop 0). The loader's hook on that lookup hands out the list
//! without the disabled recipes, and its screen of craft requests empties a
//! request for one. A client joined to a server takes the server's list over
//! the loader's messages.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use enshwick::abi::RecipeRuleKind;
use enshwick::content::recipes::RecipeLayout;
use enshwick::content::{Blob, Guid};

/// The server's recipe file, in `mod-configs`.
pub const FILE: &str = "enshwick.recipes.toml";
/// A world's own recipe file, in its folder of the loader's data.
pub const WORLD_FILE: &str = "recipes.toml";
/// The workshop of crafting by hand.
pub const HAND: &str = "hand";
/// The most near names a problem suggests.
const NEAR: usize = 5;

/// The recipe file the loader writes on first start.
pub const TEMPLATE: &str = r#"# Recipes nobody crafts in this game. They leave every crafting menu (by
# hand, at stations and at survivors), and the server refuses to craft them;
# items that exist stay. A world's own mod-data/enshwick/worlds/<key>/recipes.toml
# applies over this file. Changes apply within a second.
#
# recipes names a recipe by its debug name (Tool_Torch01_T0_PK), a mod's
# recipe by its content id (you.lanterns:lantern.recipe), any recipe by its
# GUID.
# outputs disables every recipe that makes an item, named the same ways.
# workshops disables every recipe made at a workshop: "hand", or the debug
# name of the station's or the survivor's item (Craftingstation_Workbench,
# NPC_Crafter_Blacksmith01). [enable] takes recipes back from [disable] and
# from mods. The console line enshwick recipes find <text> finds the names.

[disable]
recipes = []
outputs = []
workshops = []

[enable]
recipes = []
outputs = []
workshops = []
"#;

/// What a rule names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// One recipe.
    Recipe,
    /// Every recipe that makes an item.
    Output,
    /// Every recipe made at a workshop.
    Workshop,
}

impl Kind {
    /// The kind of an ABI rule.
    #[must_use]
    pub fn from_abi(kind: RecipeRuleKind) -> Option<Self> {
        match kind {
            RecipeRuleKind::RECIPE => Some(Self::Recipe),
            RecipeRuleKind::OUTPUT => Some(Self::Output),
            RecipeRuleKind::WORKSHOP => Some(Self::Workshop),
            _ => None,
        }
    }

    /// The kind a command word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "recipe" | "recipes" => Some(Self::Recipe),
            "output" | "outputs" => Some(Self::Output),
            "workshop" | "workshops" => Some(Self::Workshop),
            _ => None,
        }
    }

    /// The key of the kind's list in a recipe file.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Recipe => "recipes",
            Self::Output => "outputs",
            Self::Workshop => "workshops",
        }
    }
}

/// Where a rule comes from, for its problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A line of a recipe file.
    File {
        /// The file, as the log names it.
        file: String,
        /// Its line, from 1.
        line: usize,
    },
    /// A mod's call.
    Mod(String),
}

impl core::fmt::Display for Source {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::File { file, line } => write!(f, "{file}:{line}"),
            Self::Mod(id) => write!(f, "mod {id}"),
        }
    }
}

/// One rule: what it names and where it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    /// What it names.
    pub kind: Kind,
    /// The name, as written.
    pub name: String,
    /// Where it comes from.
    pub source: Source,
}

/// A recipe file: what it disables, and what it enables back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RuleFile {
    /// Rules that disable.
    pub disable: Vec<Rule>,
    /// Rules that enable back.
    pub enable: Vec<Rule>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileForm {
    disable: ListsForm,
    enable: ListsForm,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ListsForm {
    recipes: Vec<toml::Spanned<String>>,
    outputs: Vec<toml::Spanned<String>>,
    workshops: Vec<toml::Spanned<String>>,
}

/// The line of byte `at` in `text`, from 1.
fn line_of(text: &str, at: usize) -> usize {
    text.get(..at)
        .map_or(0, |before| before.matches('\n').count())
        + 1
}

impl RuleFile {
    /// Reads the recipe file `text`, named `file` in its rules' problems.
    ///
    /// # Errors
    ///
    /// The problem with its line, when the file is no recipe file.
    pub fn parse(file: &str, text: &str) -> Result<Self, String> {
        let form: FileForm = toml::from_str(text).map_err(|e| {
            let line = e.span().map_or(1, |span| line_of(text, span.start));
            format!("{file}:{line}: {}", e.message().trim_end())
        })?;
        let rules = |lists: ListsForm| -> Vec<Rule> {
            [
                (Kind::Recipe, lists.recipes),
                (Kind::Output, lists.outputs),
                (Kind::Workshop, lists.workshops),
            ]
            .into_iter()
            .flat_map(|(kind, names)| {
                names.into_iter().map(move |name| Rule {
                    kind,
                    source: Source::File {
                        file: file.to_owned(),
                        line: line_of(text, name.span().start),
                    },
                    name: name.into_inner().trim().to_owned(),
                })
            })
            .collect()
        };
        Ok(Self {
            disable: rules(form.disable),
            enable: rules(form.enable),
        })
    }
}

/// One recipe of the registry, as rules find it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recipe {
    /// Its GUID.
    pub guid: Guid,
    /// Its game id.
    pub id: u32,
    /// Its debug name.
    pub name: String,
    /// The game ids of the items it makes.
    pub outputs: Vec<u32>,
    /// The game id of its workshop, 0 for crafting by hand.
    pub workshop: u32,
}

/// The recipe registry as rules find it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    recipes: Vec<Recipe>,
    by_name: HashMap<String, Vec<usize>>,
}

impl Index {
    /// The index of `recipes`.
    #[must_use]
    pub fn new(recipes: Vec<Recipe>) -> Self {
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, recipe) in recipes.iter().enumerate() {
            by_name.entry(recipe.name.clone()).or_default().push(at);
        }
        Self { recipes, by_name }
    }

    /// Reads the recipe registry `registry` of layout `layout`; `None` when
    /// it does not read as one.
    #[must_use]
    pub fn read(layout: &RecipeLayout, registry: &[u8]) -> Option<Self> {
        let blob = Blob::new(registry);
        let (start, count) = blob.array(layout.recipes)?;
        let count = usize::try_from(count).ok()?;
        let mut recipes = Vec::with_capacity(count.min(registry.len() / layout.recipe.max(1)));
        for at in (0..count).map(|i| start + i * layout.recipe) {
            let guid: [u8; 16] = registry
                .get(at + layout.guid..at + layout.guid + 16)?
                .try_into()
                .ok()?;
            let name = blob
                .array(at + layout.relative[3])
                .and_then(|(text, len)| registry.get(text..text + usize::try_from(len).ok()?))
                .map(|bytes| {
                    String::from_utf8_lossy(bytes)
                        .trim_end_matches('\0')
                        .to_owned()
                })
                .unwrap_or_default();
            let (outputs, made) = blob.array(at + layout.output).unwrap_or((0, 0));
            let outputs = (0..usize::try_from(made).ok()?)
                .map_while(|i| blob.read_u32(outputs + i * layout.stack_size + layout.stack_item))
                .collect();
            recipes.push(Recipe {
                guid: Guid(guid),
                id: blob.read_u32(at + layout.id)?,
                name,
                outputs,
                workshop: blob.read_u32(at + layout.workshop)?,
            });
        }
        Some(Self::new(recipes))
    }

    /// The index with each recipe a content id names called by that id:
    /// a mod's recipe is a copy of a vanilla one, debug name included.
    #[must_use]
    pub fn with_content(mut self, content: &HashMap<u32, String>) -> Self {
        for recipe in &mut self.recipes {
            if let Some(name) = content.get(&recipe.id) {
                recipe.name.clone_from(name);
            }
        }
        Self::new(self.recipes)
    }

    /// Every recipe, in registry order.
    #[must_use]
    pub fn recipes(&self) -> &[Recipe] {
        &self.recipes
    }

    /// The recipe with game id `id`.
    #[must_use]
    pub fn by_id(&self, id: u32) -> Option<&Recipe> {
        self.recipes.iter().find(|r| r.id == id)
    }
}

/// Names of the game's items and workshops, for rules and replies.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    /// Item debug names to the items that carry them.
    items: BTreeMap<String, Vec<Guid>>,
    /// Item game ids to their debug names.
    item_labels: HashMap<u32, String>,
    /// Item game ids to the workshop each belongs to.
    workshops: HashMap<u32, u32>,
    /// Workshop game ids to the item each belongs to.
    workshop_items: HashMap<u32, u32>,
}

impl Names {
    /// Names from items (debug name and GUID) and workshops (workshop id
    /// and the game id of its item).
    #[must_use]
    pub fn new(
        items: impl IntoIterator<Item = (String, Guid)>,
        workshops: impl IntoIterator<Item = (u32, u32)>,
    ) -> Self {
        let mut names = Self::default();
        for (name, guid) in items {
            names.item_labels.insert(guid.id(), name.clone());
            names.items.entry(name).or_default().push(guid);
        }
        for (workshop, item) in workshops {
            names.workshops.insert(item, workshop);
            names.workshop_items.insert(workshop, item);
        }
        names
    }

    /// The names with each item the game's data lacks, a mod's, called by
    /// the content id that names it.
    #[must_use]
    pub fn with_content(mut self, content: &HashMap<u32, String>) -> Self {
        for (&id, name) in content {
            self.item_labels.entry(id).or_insert_with(|| name.clone());
        }
        self
    }

    /// The debug name of the item with game id `id`.
    #[must_use]
    pub fn item_name(&self, id: u32) -> Option<&str> {
        self.item_labels.get(&id).map(String::as_str)
    }

    /// A workshop in words: `hand`, its item's debug name, or its id.
    #[must_use]
    pub fn workshop_name(&self, workshop: u32) -> String {
        if workshop == 0 {
            return HAND.to_owned();
        }
        self.workshop_items
            .get(&workshop)
            .and_then(|item| self.item_name(*item))
            .map_or_else(|| workshop.to_string(), str::to_owned)
    }

    /// The item with debug name `name`.
    fn item(&self, name: &str) -> Result<Guid, String> {
        match self.items.get(name).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(several) if !several.is_empty() => Err(format!(
                "{} items are named {name:?}: {}; name one by its GUID",
                several.len(),
                list(several)
            )),
            _ => Err(format!(
                "no item {name:?}{}",
                near_list(&near(self.items.keys().map(String::as_str), name))
            )),
        }
    }
}

fn list(guids: &[Guid]) -> String {
    guids
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn near_list(near: &[&str]) -> String {
    if near.is_empty() {
        String::new()
    } else {
        format!("; near: {}", near.join(", "))
    }
}

/// Names of `names` that look like `name`: the same but for case first,
/// then those that hold it, then those a few edits away, ignoring case.
fn near<'a>(names: impl Iterator<Item = &'a str>, name: &str) -> Vec<&'a str> {
    let wanted = name.to_ascii_lowercase();
    let mut scored: Vec<(usize, &str)> = names
        .filter_map(|n| {
            let lower = n.to_ascii_lowercase();
            let score = if lower == wanted {
                0
            } else if !wanted.is_empty() && lower.contains(&wanted) {
                1
            } else {
                let edits = strsim::levenshtein(&lower, &wanted);
                if edits > 2 {
                    return None;
                }
                1 + edits
            };
            Some((score, n))
        })
        .collect();
    scored.sort_unstable();
    scored.dedup();
    scored.into_iter().take(NEAR).map(|(_, n)| n).collect()
}

/// A name as a reference: a GUID, a content id's GUID, or a debug name.
enum Reference<'a> {
    Guid(Guid),
    Name(&'a str),
}

fn reference(name: &str) -> Reference<'_> {
    Guid::try_parse(name)
        .or_else(|| Guid::try_named(name))
        .map_or(Reference::Name(name), Reference::Guid)
}

/// The game ids of the recipes `rule` names.
///
/// # Errors
///
/// What is wrong with the name, with the names that look like it.
pub fn resolve(rule: &Rule, index: &Index, names: &Names) -> Result<Vec<u32>, String> {
    let name = rule.name.as_str();
    if name.is_empty() {
        return Err("an empty name".to_owned());
    }
    match rule.kind {
        Kind::Recipe => match reference(name) {
            Reference::Guid(guid) => index
                .recipes
                .iter()
                .find(|r| r.guid == guid || r.id == guid.id())
                .map(|r| vec![r.id])
                .ok_or_else(|| format!("no recipe {name}")),
            Reference::Name(name) => match index.by_name.get(name).map(Vec::as_slice) {
                Some([one]) => Ok(vec![index.recipes[*one].id]),
                Some(several) if !several.is_empty() => {
                    let guids: Vec<Guid> = several.iter().map(|&i| index.recipes[i].guid).collect();
                    Err(format!(
                        "{} recipes are named {name:?}: {}; name one by its GUID",
                        several.len(),
                        list(&guids)
                    ))
                }
                _ => Err(format!(
                    "no recipe {name:?}{}",
                    near_list(&near(index.by_name.keys().map(String::as_str), name))
                )),
            },
        },
        Kind::Output => {
            let item = match reference(name) {
                Reference::Guid(guid) => guid,
                Reference::Name(name) => names.item(name)?,
            };
            let found: Vec<u32> = index
                .recipes
                .iter()
                .filter(|r| r.outputs.contains(&item.id()))
                .map(|r| r.id)
                .collect();
            if found.is_empty() {
                Err(format!("no recipe makes {name}"))
            } else {
                Ok(found)
            }
        }
        Kind::Workshop => {
            let workshop = if name.eq_ignore_ascii_case(HAND) {
                0
            } else {
                match reference(name) {
                    Reference::Guid(guid) => guid.id(),
                    Reference::Name(name) => {
                        let item = names.item(name)?;
                        *names
                            .workshops
                            .get(&item.id())
                            .ok_or_else(|| format!("{name} has no workshop"))?
                    }
                }
            };
            let found: Vec<u32> = index
                .recipes
                .iter()
                .filter(|r| r.workshop == workshop)
                .map(|r| r.id)
                .collect();
            if found.is_empty() {
                Err(format!("no recipe is made at {name}"))
            } else {
                Ok(found)
            }
        }
    }
}

/// The recipes disabled by mods' rules `mods`, then by each of `files` in
/// turn: a file disables what its `[disable]` names and enables back what
/// its `[enable]` names, over everything before it. The problems, one line
/// each with where the rule comes from.
#[must_use]
pub fn compose(
    index: &Index,
    names: &Names,
    mods: &[Rule],
    files: &[&RuleFile],
) -> (BTreeSet<u32>, Vec<String>) {
    let mut problems = Vec::new();
    let mut gather = |rules: &[Rule]| -> BTreeSet<u32> {
        let mut ids = BTreeSet::new();
        for rule in rules {
            match resolve(rule, index, names) {
                Ok(found) => ids.extend(found),
                Err(why) => problems.push(format!("{}: {why}", rule.source)),
            }
        }
        ids
    };
    let mut disabled = gather(mods);
    for file in files {
        disabled.extend(gather(&file.disable));
        for id in gather(&file.enable) {
            disabled.remove(&id);
        }
    }
    (disabled, problems)
}

/// The recipes whose debug name, or whose output's, holds `text`, ignoring
/// case.
#[must_use]
pub fn find<'a>(index: &'a Index, names: &Names, text: &str) -> Vec<&'a Recipe> {
    let wanted = text.to_ascii_lowercase();
    index
        .recipes
        .iter()
        .filter(|r| {
            r.name.to_ascii_lowercase().contains(&wanted)
                || r.outputs.iter().any(|&o| {
                    names
                        .item_name(o)
                        .is_some_and(|n| n.to_ascii_lowercase().contains(&wanted))
                })
        })
        .collect()
}

/// A recipe in words: name, GUID, workshop and what it makes.
#[must_use]
pub fn describe(recipe: &Recipe, names: &Names) -> String {
    let made: Vec<&str> = recipe
        .outputs
        .iter()
        .filter_map(|&o| names.item_name(o))
        .collect();
    format!(
        "{} ({}) at {}{}",
        recipe.name,
        recipe.guid,
        names.workshop_name(recipe.workshop),
        if made.is_empty() {
            String::new()
        } else {
            format!(", makes {}", made.join(", "))
        }
    )
}

/// `text` with `name` added to the `[disable]` (`disable`) or `[enable]`
/// list of `kind`, and taken out of the other; comments kept.
///
/// # Errors
///
/// When `text` is no TOML or its lists are no lists.
pub fn edit_file(text: &str, kind: Kind, name: &str, disable: bool) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| format!("{e}"))?;
    let (to, from) = if disable {
        ("disable", "enable")
    } else {
        ("enable", "disable")
    };
    if let Some(list) = doc
        .get_mut(from)
        .and_then(|t| t.get_mut(kind.key()))
        .and_then(toml_edit::Item::as_array_mut)
    {
        list.retain(|v| v.as_str() != Some(name));
    }
    let table = doc
        .entry(to)
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| format!("[{to}] is no table"))?;
    let list = table
        .entry(kind.key())
        .or_insert(toml_edit::value(toml_edit::Array::new()))
        .as_array_mut()
        .ok_or_else(|| format!("{to}.{} is no list", kind.key()))?;
    if !list.iter().any(|v| v.as_str() == Some(name)) {
        list.push(name);
    }
    Ok(doc.to_string())
}

/// Splits ids into the messages a client takes: the first and last flags
/// and up to `per` ids each, at least one message.
#[must_use]
pub fn parts(ids: &[u32], per: usize) -> Vec<(bool, bool, Vec<u32>)> {
    let chunks: Vec<&[u32]> = if ids.is_empty() {
        vec![&[]]
    } else {
        ids.chunks(per.max(1)).collect()
    };
    let last = chunks.len() - 1;
    chunks
        .into_iter()
        .enumerate()
        .map(|(i, chunk)| (i == 0, i == last, chunk.to_vec()))
        .collect()
}

#[cfg(windows)]
pub(crate) use live::{
    command, filter, hello, init, is_disabled, joining, left, poll, received, registry_built, rule,
    world_changed,
};

#[cfg(all(windows, test))]
pub(crate) use live::disable_for_test;

#[cfg(windows)]
mod live;

#[cfg(test)]
mod tests {
    use super::*;

    const TORCH: Guid = Guid::parse("7c583d87-f300-4d17-a27e-9a77fbbfaeba");
    const BENCH_ITEM: Guid = Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc");
    const BENCH: u32 = 1_302_892_403;

    fn recipe(name: &str, id: u32, output: Guid, workshop: u32) -> Recipe {
        Recipe {
            guid: Guid::named(&format!("test:{}", name.to_ascii_lowercase())),
            id,
            name: name.to_owned(),
            outputs: vec![output.id()],
            workshop,
        }
    }

    fn index() -> Index {
        Index::new(vec![
            recipe("Tool_Torch01_T0_PK", 1, TORCH, 0),
            recipe("Tool_Torch01_T0_bench", 2, TORCH, BENCH),
            recipe("Block_Wood", 3, BENCH_ITEM, BENCH),
            recipe("Twin", 4, BENCH_ITEM, BENCH),
            recipe("Twin", 5, BENCH_ITEM, BENCH),
        ])
    }

    fn names() -> Names {
        Names::new(
            [
                ("Tool_Torch01_T0".to_owned(), TORCH),
                ("Craftingstation_Workbench".to_owned(), BENCH_ITEM),
            ],
            [(BENCH, BENCH_ITEM.id())],
        )
    }

    fn rule(kind: Kind, name: &str) -> Rule {
        Rule {
            kind,
            name: name.to_owned(),
            source: Source::Mod("test".to_owned()),
        }
    }

    #[test]
    fn a_file_names_each_rule_with_its_line() {
        let text = "[disable]\nrecipes = [\n  \"A\",\n  \"B\",\n]\nworkshops = [\"hand\"]\n\n[enable]\noutputs = [\"C\"]\n";
        let file = RuleFile::parse("r.toml", text).unwrap();
        let lines: Vec<(Kind, &str, String)> = file
            .disable
            .iter()
            .chain(&file.enable)
            .map(|r| (r.kind, r.name.as_str(), r.source.to_string()))
            .collect();
        assert_eq!(
            lines,
            [
                (Kind::Recipe, "A", "r.toml:3".to_owned()),
                (Kind::Recipe, "B", "r.toml:4".to_owned()),
                (Kind::Workshop, "hand", "r.toml:6".to_owned()),
                (Kind::Output, "C", "r.toml:9".to_owned()),
            ]
        );
    }

    #[test]
    fn a_broken_file_says_where() {
        let why = RuleFile::parse("r.toml", "[disable]\nrecipe = [\"A\"]\n").unwrap_err();
        assert!(why.starts_with("r.toml:2: "), "{why}");
        assert!(RuleFile::parse("r.toml", "").unwrap().disable.is_empty());
        assert!(RuleFile::parse("r.toml", TEMPLATE).is_ok());
    }

    #[test]
    fn the_content_id_example_in_the_template_names_a_full_mod_id() {
        let example = TEMPLATE
            .split_once("content id (")
            .and_then(|(_, rest)| rest.split_once(')'))
            .unwrap()
            .0;
        let (owner, name) = example.split_once(':').unwrap();
        assert!(enshwick::abi::is_valid_id(owner), "{example}");
        assert!(!name.is_empty());
    }

    #[test]
    fn rules_name_recipes_by_name_guid_output_and_workshop() {
        let (index, names) = (index(), names());
        let ids = |kind: Kind, name: &str| resolve(&rule(kind, name), &index, &names);
        assert_eq!(ids(Kind::Recipe, "Block_Wood"), Ok(vec![3]));
        assert_eq!(
            ids(Kind::Recipe, &index.recipes[0].guid.to_string()),
            Ok(vec![1])
        );
        assert_eq!(ids(Kind::Recipe, "test:block_wood"), Ok(vec![3]));
        assert_eq!(ids(Kind::Output, "Tool_Torch01_T0"), Ok(vec![1, 2]));
        assert_eq!(ids(Kind::Output, &TORCH.to_string()), Ok(vec![1, 2]));
        assert_eq!(ids(Kind::Workshop, "hand"), Ok(vec![1]));
        assert_eq!(
            ids(Kind::Workshop, "Craftingstation_Workbench"),
            Ok(vec![2, 3, 4, 5])
        );
    }

    #[test]
    fn unknown_and_ambiguous_names_say_what_to_write() {
        let (index, names) = (index(), names());
        let why = |kind: Kind, name: &str| resolve(&rule(kind, name), &index, &names).unwrap_err();
        assert_eq!(
            why(Kind::Recipe, "Block_Wod"),
            "no recipe \"Block_Wod\"; near: Block_Wood"
        );
        assert!(why(Kind::Recipe, "Twin").starts_with("2 recipes are named \"Twin\": "));
        assert_eq!(
            why(Kind::Output, "Tool_Torch"),
            "no item \"Tool_Torch\"; near: Tool_Torch01_T0"
        );
        assert_eq!(
            why(Kind::Workshop, "Tool_Torch01_T0"),
            "Tool_Torch01_T0 has no workshop"
        );
        assert_eq!(why(Kind::Recipe, ""), "an empty name");
    }

    #[test]
    fn files_apply_in_turn_over_mods_and_enable_wins_in_its_file() {
        let (index, names) = (index(), names());
        let server = RuleFile::parse(
            "server.toml",
            "[disable]\nworkshops = [\"Craftingstation_Workbench\"]\n[enable]\nrecipes = [\"Block_Wood\"]\n",
        )
        .unwrap();
        let world = RuleFile::parse(
            "world.toml",
            "[disable]\nrecipes = [\"Block_Wood\", \"Nope\"]\n[enable]\nrecipes = [\"Tool_Torch01_T0_PK\"]\n",
        )
        .unwrap();
        let mods = [rule(Kind::Recipe, "Tool_Torch01_T0_PK")];
        let (disabled, problems) = compose(&index, &names, &mods, &[&server]);
        assert_eq!(disabled.into_iter().collect::<Vec<_>>(), [1, 2, 4, 5]);
        assert!(problems.is_empty());
        let (disabled, problems) = compose(&index, &names, &mods, &[&server, &world]);
        assert_eq!(disabled.into_iter().collect::<Vec<_>>(), [2, 3, 4, 5]);
        assert_eq!(problems, ["world.toml:2: no recipe \"Nope\""]);
    }

    #[test]
    fn find_looks_at_recipe_and_output_names() {
        let (index, names) = (index(), names());
        let found: Vec<u32> = find(&index, &names, "torch").iter().map(|r| r.id).collect();
        assert_eq!(found, [1, 2]);
        let found: Vec<u32> = find(&index, &names, "WORKBENCH")
            .iter()
            .map(|r| r.id)
            .collect();
        assert_eq!(found, [3, 4, 5]);
        assert_eq!(
            describe(&index.recipes[1], &names),
            format!(
                "Tool_Torch01_T0_bench ({}) at Craftingstation_Workbench, makes Tool_Torch01_T0",
                index.recipes[1].guid
            )
        );
    }

    #[test]
    fn a_mods_copy_goes_by_its_content_id_and_leaves_the_vanilla_name_alone() {
        let lantern = Guid::named("you.lanterns:lantern");
        let copy = Guid::named("you.lanterns:lantern.recipe");
        let mut recipes = index().recipes;
        recipes.push(Recipe {
            guid: copy,
            id: copy.id(),
            name: "Block_Wood".to_owned(),
            outputs: vec![lantern.id()],
            workshop: BENCH,
        });
        let content: HashMap<u32, String> = [
            (copy.id(), "you.lanterns:lantern.recipe".to_owned()),
            (lantern.id(), "you.lanterns:lantern".to_owned()),
        ]
        .into();
        let index = Index::new(recipes).with_content(&content);
        let names = names().with_content(&content);
        assert_eq!(
            resolve(&rule(Kind::Recipe, "Block_Wood"), &index, &names),
            Ok(vec![3])
        );
        assert_eq!(
            resolve(
                &rule(Kind::Recipe, "you.lanterns:lantern.recipe"),
                &index,
                &names
            ),
            Ok(vec![copy.id()])
        );
        let found = find(&index, &names, "lantern");
        assert_eq!(found.len(), 1);
        assert_eq!(
            describe(found[0], &names),
            format!(
                "you.lanterns:lantern.recipe ({copy}) at Craftingstation_Workbench, makes you.lanterns:lantern"
            )
        );
    }

    #[test]
    fn an_edit_moves_a_name_between_the_lists_and_keeps_comments() {
        let text = edit_file(TEMPLATE, Kind::Output, "Torch", true).unwrap();
        assert!(text.starts_with("# Recipes nobody crafts"));
        let file = RuleFile::parse("r", &text).unwrap();
        assert_eq!(file.disable.len(), 1);
        let text = edit_file(&text, Kind::Output, "Torch", false).unwrap();
        let file = RuleFile::parse("r", &text).unwrap();
        assert!(file.disable.is_empty());
        assert_eq!(file.enable[0].name, "Torch");
        let fresh = edit_file("", Kind::Recipe, "A", true).unwrap();
        assert_eq!(RuleFile::parse("r", &fresh).unwrap().disable[0].name, "A");
    }

    #[test]
    fn ids_go_in_parts_marked_first_and_last() {
        assert_eq!(parts(&[], 2), [(true, true, vec![])]);
        assert_eq!(
            parts(&[1, 2, 3], 2),
            [(true, false, vec![1, 2]), (false, true, vec![3])]
        );
    }

    fn layout() -> RecipeLayout {
        RecipeLayout {
            recipes: 8,
            recipe: 0xa8,
            guid: 0,
            id: 0x10,
            workshop_guid: 0x14,
            workshop: 0x24,
            input: 0x28,
            output: 0x30,
            completion: 0x5c,
            relative: [0x28, 0x30, 0x40, 0x78],
            input_size: 48,
            stack_size: 24,
            stack_item: 0x10,
            stack_count: 0x14,
        }
    }

    #[test]
    fn an_index_of_a_registry_with_a_corrupt_count_reads_as_none() {
        let mut registry = vec![0u8; 16];
        registry[8..12].copy_from_slice(&8i32.to_le_bytes());
        registry[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(Index::read(&layout(), &registry), None);
    }

    #[test]
    fn an_index_reads_a_registry() {
        use enshwick::content::Blob;
        let layout = layout();
        let mut base = vec![0u8; 16];
        base[8..12].copy_from_slice(&(-8i32).to_le_bytes());
        let mut blob = Blob::new(&base);
        let mut entry = vec![0u8; 0xa8];
        entry[..16].copy_from_slice(&TORCH.0);
        entry[0x10..0x14].copy_from_slice(&7u32.to_le_bytes());
        entry[0x24..0x28].copy_from_slice(&BENCH.to_le_bytes());
        let at = blob.append(&entry, 8);
        assert!(blob.set_array(8, at, 1));
        let mut stack = vec![0u8; 24];
        stack[0x10..0x14].copy_from_slice(&TORCH.id().to_le_bytes());
        let stack_at = blob.append(&stack, 8);
        assert!(blob.set_array(at + 0x30, stack_at, 1));
        let name = blob.append(b"Torch\0", 1);
        assert!(blob.set_array(at + 0x78, name, 5));
        let index = Index::read(&layout, blob.bytes()).unwrap();
        assert_eq!(
            index.recipes(),
            [Recipe {
                guid: TORCH,
                id: 7,
                name: "Torch".to_owned(),
                outputs: vec![TORCH.id()],
                workshop: BENCH,
            }]
        );
        assert_eq!(index.by_id(7).map(|r| r.workshop), Some(BENCH));
    }
}
