//! Reading a pack's changes of vanilla values.

use std::collections::HashMap;

use crate::content::Guid;
use crate::content::categories::ItemCategoryMask;
use crate::content::changes::{
    Change, Entry, Field, Items, ItemsChange, Recipes, RecipesChange, Value,
};
use crate::content::recipes::RecipeKind;
use crate::content::vanilla::VanillaError;

use super::entries::{
    ItemChangeEntry, ItemsEntry, ItemsFile, LootChangeEntry, LootFile, RecipeChangeEntry,
    RecipesFile, ValueEntry,
};
use super::files::PackError;
use super::pack::Reader;
use super::{ITEMS_FILE, LOOT_FILE, RECIPES_FILE};

/// A field a recipe change sets, with its ingredient and the name the pack
/// gave it, and the value.
type RecipeField = (Field, Option<(Guid, String)>, Value);

/// Changes of every item a selection picks, each with its source.
type Selections = Vec<(ItemsChange, String)>;

/// Changes of every recipe a selection picks, each with its source.
type RecipeSelections = Vec<(RecipesChange, String)>;

/// One `[[change]]` table read: where it starts, the items it names and
/// the value it gives each field.
type ChangeEntry<'a> = (usize, &'a [String], Vec<(Field, Option<ValueEntry>)>);

/// The source of a change: the file and line, then the name the pack wrote
/// unless it is a GUID.
fn source(file: &str, line: usize, name: &str) -> String {
    if Guid::try_parse(name).is_some() {
        format!("{file}:{line}")
    } else {
        format!("{file}:{line} ({name})")
    }
}

/// Whether the item `name` names, `item`, takes `slots`: an item that
/// places a container, never made smaller, where the game's data is at
/// hand; anything without it.
fn container(reader: &Reader<'_>, item: Guid, name: &str, slots: Value) -> Result<(), String> {
    let Some(vanilla) = reader.vanilla else {
        return Ok(());
    };
    match vanilla.container_slots(item) {
        Ok(None) => Err(format!("{name} places no container")),
        Ok(Some(has)) => match slots {
            Value::Set(v) if v < f64::from(has) => Err(format!(
                "slots {v} are fewer than the {has} {name} holds, and a container only grows"
            )),
            _ => Ok(()),
        },
        Err(_) => Ok(()),
    }
}

/// The value a pack entry gives.
fn value(entry: ValueEntry) -> Value {
    match entry {
        ValueEntry::Set(v) => Value::Set(v),
        ValueEntry::Scale(scale) => Value::Times(scale.times),
    }
}

/// The fields an item change gives, with their values.
fn item_fields(e: &ItemChangeEntry) -> Vec<(Field, Option<ValueEntry>)> {
    vec![
        (Field::MaxStackSize, e.max_stack),
        (Field::Slots, e.slots),
        (Field::DamageMod, e.damage_mod),
        (Field::SpeedMod, e.speed_mod),
        (Field::Armor, e.armor),
        (Field::BurnTime, e.burn_time),
        (Field::EnergyOutput, e.energy_output),
        (Field::ComfortAmount, e.comfort_amount),
    ]
}

/// The checked changes of `items_file`, each with its source,
/// `items.toml:<line>`, followed by the name the pack wrote when it is no
/// GUID: `items.toml:12 (Material_T1_Wood_Logs)`; then the changes of
/// every item a selection picks, which the loader resolves from the game's
/// data, with the selection in the source: `items.toml:3 (stackable)`.
pub(super) fn item_changes(
    reader: &mut Reader<'_>,
    items_file: &ItemsFile,
    text: &str,
) -> (Vec<(Change, String)>, Selections) {
    let mut by_name: Vec<ChangeEntry<'_>> = Vec::new();
    let mut selections = Vec::new();
    let mut seen: HashMap<(Items, Field), usize> = HashMap::new();
    for (index, entry) in items_file.change.iter().enumerate() {
        let e = entry.get_ref();
        let at = entry.span().start;
        let number = index + 1;
        let selected = match (&e.items, &e.category) {
            (Some(_), Some(_)) => Err("give items or category, not both".to_owned()),
            (Some(ItemsEntry::Names(list)), None) => {
                by_name.push((at, list.as_slice(), item_fields(e)));
                continue;
            }
            (Some(ItemsEntry::Word(word)), None) => match word.as_str() {
                "stackable" => Ok((Items::Stackable, word.clone())),
                "containers" => Ok((Items::Containers, word.clone())),
                "backpacks" => Ok((Items::Backpacks, word.clone())),
                _ => Err(format!(
                    "items {word:?} is no selection; give a list of items, \"stackable\", \"containers\" or \"backpacks\""
                )),
            },
            (None, Some(categories)) => category(categories).map(|mask| {
                (
                    Items::Category(mask),
                    format!("category {}", categories.join(", ")),
                )
            }),
            (None, None) => Err("give items or category".to_owned()),
        };
        match selected {
            Ok((items, label)) => {
                let line = PackError::at(ITEMS_FILE, text, at, "").line;
                for change in select(reader, text, at, number, items, &item_fields(e)) {
                    if let Some(&first) = seen.get(&(items, change.field)) {
                        reader.error(
                            ITEMS_FILE,
                            text,
                            at,
                            format!(
                                "change {number}: {} of {label} is changed at line {first} already",
                                change.field
                            ),
                        );
                        continue;
                    }
                    seen.insert((items, change.field), line);
                    selections.push((change, format!("{ITEMS_FILE}:{line} ({label})")));
                }
            }
            Err(why) => reader.error(ITEMS_FILE, text, at, format!("change {number}: {why}")),
        }
    }
    let changes = changes_of(reader, ITEMS_FILE, Entry::Item, &by_name, text);
    (changes, selections)
}

/// The categories the game names `names`, or why not.
fn category(names: &[String]) -> Result<ItemCategoryMask, String> {
    if names.is_empty() {
        return Err("category names no item category".to_owned());
    }
    names
        .iter()
        .try_fold(ItemCategoryMask::default(), |mask, name| {
            ItemCategoryMask::named(name)
                .map(|one| mask | one)
                .ok_or_else(|| {
                    let near: Vec<&str> = ItemCategoryMask::NAMES
                        .iter()
                        .map(|(game, _)| *game)
                        .filter(|game| {
                            strsim::levenshtein(&game.to_lowercase(), &name.to_lowercase()) <= 2
                        })
                        .collect();
                    if near.is_empty() {
                        format!("no item category {name:?}")
                    } else {
                        format!("no item category {name:?}; near: {}", near.join(", "))
                    }
                })
        })
}

/// The checked changes of each field `fields` gives through the selection
/// `items`; problems are noted at the change.
fn select(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    number: usize,
    items: Items,
    fields: &[(Field, Option<ValueEntry>)],
) -> Vec<ItemsChange> {
    let given: Vec<ItemsChange> = fields
        .iter()
        .filter_map(|(field, v)| v.map(|v| ItemsChange::new(items, *field, value(v))))
        .collect();
    if given.is_empty() {
        reader.error(
            ITEMS_FILE,
            text,
            at,
            format!("change {number}: changes nothing; give max_stack or slots"),
        );
    }
    given
        .into_iter()
        .filter(|change| match change.check() {
            Ok(()) => true,
            Err(why) => {
                reader.error(ITEMS_FILE, text, at, format!("change {number}: {why}"));
                false
            }
        })
        .collect()
}

/// The checked changes of `loot_file`, each with its source as
/// [`item_changes`] gives it.
pub(super) fn loot_changes(
    reader: &mut Reader<'_>,
    loot_file: &LootFile,
    text: &str,
) -> Vec<(Change, String)> {
    let fields = |e: &LootChangeEntry| {
        vec![
            (Field::StackSize, e.stack_size),
            (Field::Count, e.count),
            (Field::SpawnProbability, e.spawn_probability),
            (Field::Weight, e.weight),
        ]
    };
    let entries: Vec<ChangeEntry<'_>> = loot_file
        .change
        .iter()
        .filter(|entry| entry.get_ref().table.is_empty())
        .map(|entry| {
            let e = entry.get_ref();
            (entry.span().start, e.items.as_slice(), fields(e))
        })
        .collect();
    let mut out = changes_of(reader, LOOT_FILE, Entry::Loot, &entries, text);
    let mut seen: HashMap<(Guid, Guid, Field), usize> = HashMap::new();
    for (index, entry) in loot_file.change.iter().enumerate() {
        let e = entry.get_ref();
        if e.table.is_empty() {
            continue;
        }
        let at = entry.span().start;
        let line = PackError::at(LOOT_FILE, text, at, "").line;
        let number = index + 1;
        for (change, source) in table_changes(reader, text, (at, line, number), e, &fields(e)) {
            let key = (
                change.entry,
                change.item.unwrap_or(change.entry),
                change.field,
            );
            if let Some(&first) = seen.get(&key) {
                reader.error(
                    LOOT_FILE,
                    text,
                    at,
                    format!(
                        "change {number}: {} {source} is changed at line {first} already",
                        change.field
                    ),
                );
                continue;
            }
            seen.insert(key, line);
            out.push((change, source));
        }
    }
    out
}

/// The checked changes change `e` gives the items it names in the fill
/// tables of the containers it names, each with its source; problems are
/// noted at the change, whose start, line and number `at` holds.
fn table_changes(
    reader: &mut Reader<'_>,
    text: &str,
    (at, line, number): (usize, usize, usize),
    e: &LootChangeEntry,
    fields: &[(Field, Option<ValueEntry>)],
) -> Vec<(Change, String)> {
    let file = LOOT_FILE;
    let fail = |reader: &mut Reader<'_>, why: String| {
        reader.error(file, text, at, format!("change {number}: {why}"));
    };
    let given: Vec<(Field, Value)> = fields
        .iter()
        .filter_map(|(field, v)| v.map(|v| (*field, value(v))))
        .collect();
    if given.iter().any(|(field, _)| *field == Field::StackSize) {
        fail(
            reader,
            "stack_size belongs to random loot, which has no tables".to_owned(),
        );
        return Vec::new();
    }
    if given.is_empty() {
        fail(
            reader,
            "changes nothing; give count, spawn_probability, weight".to_owned(),
        );
        return Vec::new();
    }
    if e.items.is_empty() {
        fail(reader, "items names no item".to_owned());
        return Vec::new();
    }
    let mut tables = Vec::new();
    for name in &e.table {
        let found = match reader.vanilla {
            None => Err(format!(
                "table {name:?} needs the game's data at hand to find the container's table"
            )),
            Some(vanilla) => vanilla.loot_table(name).map_err(|why| why.to_string()),
        };
        match found {
            Ok(table) => {
                let label = match table.containers {
                    0 | 1 => format!("in {name}"),
                    n => format!("in {name}, a table {n} containers share"),
                };
                tables.push((table.table, label));
            }
            Err(why) => fail(reader, why),
        }
    }
    let mut out = Vec::new();
    for item_name in &e.items {
        let Some((item, _)) = reader.reference(file, text, at, item_name, "item", "") else {
            continue;
        };
        for (table, label) in &tables {
            for &(field, value) in &given {
                let change = Change::in_table(*table, item, field, value);
                match change.check() {
                    Ok(()) => out.push((change, format!("{file}:{line} ({label})"))),
                    Err(why) => fail(reader, why.to_string()),
                }
            }
        }
    }
    out
}

/// The checked changes of one file's `[[change]]` tables of `kind`: each
/// with where it starts, the items it names and its fields.
fn changes_of(
    reader: &mut Reader<'_>,
    file: &str,
    kind: Entry,
    entries: &[ChangeEntry<'_>],
    text: &str,
) -> Vec<(Change, String)> {
    let mut out = Vec::new();
    let mut seen: HashMap<(Guid, Field), usize> = HashMap::new();
    for (index, (at, items, given)) in entries.iter().enumerate() {
        let at = *at;
        let line = PackError::at(file, text, at, "").line;
        let number = index + 1;
        let fields: Vec<(Field, Value)> = given
            .iter()
            .filter_map(|(field, v)| v.map(|v| (*field, value(v))))
            .collect();
        if items.is_empty() {
            reader.error(
                file,
                text,
                at,
                format!("change {number}: items names no item"),
            );
        }
        if fields.is_empty() {
            let names: Vec<&str> = Field::ALL
                .iter()
                .filter(|f| f.entry() == kind)
                .map(|f| f.name())
                .collect();
            reader.error(
                file,
                text,
                at,
                format!(
                    "change {number}: changes nothing; give {}",
                    names.join(", ")
                ),
            );
        }
        let mut valid = Vec::new();
        for &(field, value) in &fields {
            match field.check(value) {
                Ok(()) => valid.push((field, value)),
                Err(why) => reader.error(file, text, at, format!("change {number}: {why}")),
            }
        }
        for name in *items {
            let Some((item, _)) = reader.reference(file, text, at, name, "item", "") else {
                continue;
            };
            for &(field, value) in &valid {
                if field == Field::Slots
                    && let Err(why) = container(reader, item, name, value)
                {
                    reader.error(file, text, at, format!("change {number}: {why}"));
                    continue;
                }
                if let Some(&first) = seen.get(&(item, field)) {
                    reader.error(
                        file,
                        text,
                        at,
                        format!(
                            "change {number}: {field} of {name} is changed at line {first} already"
                        ),
                    );
                    continue;
                }
                seen.insert((item, field), line);
                out.push((Change::new(item, field, value), source(file, line, name)));
            }
        }
    }
    out
}

/// The vanilla recipe `name` names: a GUID, or a debug name where the
/// game's data is at hand. A recipe a mod or this pack adds is built after
/// the changes of values, so its values are set where it is defined.
fn recipe(reader: &mut Reader<'_>, text: &str, at: usize, name: &str) -> Option<Guid> {
    let file = RECIPES_FILE;
    if let Some(guid) = Guid::try_parse(name) {
        return Some(guid);
    }
    let own = reader.own(name);
    if name.contains(':') || reader.defined.get(&own) == Some(&"recipe") {
        reader.error(
            file,
            text,
            at,
            format!(
                "{name:?} is a recipe a mod or this pack adds: set its values where it is defined"
            ),
        );
        return None;
    }
    let found = match reader.vanilla {
        None => Err(format!(
            "no vanilla recipe {name:?} at hand; name it by its GUID, or check with the game's data"
        )),
        Some(vanilla) => vanilla.recipe(name).map_err(|why| why.to_string()),
    };
    found.map_err(|why| reader.error(file, text, at, why)).ok()
}

/// The vanilla recipes that make the item `name` names.
fn recipes_making(reader: &mut Reader<'_>, text: &str, at: usize, name: &str) -> Vec<Guid> {
    let file = RECIPES_FILE;
    let Some((item, _)) = reader.reference(file, text, at, name, "item", "") else {
        return Vec::new();
    };
    let found = match reader.vanilla {
        None => Err(format!(
            "output {name:?} needs the game's data at hand to find its recipes"
        )),
        Some(vanilla) => vanilla.recipes_making(item).map_err(|why| why.to_string()),
    };
    match found {
        Ok(recipes) if recipes.is_empty() => {
            reader.error(file, text, at, format!("no vanilla recipe makes {name}"));
            Vec::new()
        }
        Ok(recipes) => recipes,
        Err(why) => {
            reader.error(file, text, at, why);
            Vec::new()
        }
    }
}

/// The checked changes of `recipes_file`, each with its source as
/// [`item_changes`] gives it, naming the selector that picked the recipe.
pub(super) fn recipe_changes(
    reader: &mut Reader<'_>,
    recipes_file: &RecipesFile,
    text: &str,
) -> (Vec<(Change, String)>, RecipeSelections) {
    let file = RECIPES_FILE;
    let mut out = Vec::new();
    let mut selections = Vec::new();
    let mut seen: HashMap<(Guid, Field, Option<Guid>), usize> = HashMap::new();
    let mut seen_selected: HashMap<(Recipes, Field), usize> = HashMap::new();
    for (index, entry) in recipes_file.change.iter().enumerate() {
        let at = entry.span().start;
        let line = PackError::at(file, text, at, "").line;
        let e = entry.get_ref();
        let number = index + 1;
        if selected_recipes(
            reader,
            text,
            (at, line, number),
            e,
            &mut seen_selected,
            &mut selections,
        ) {
            continue;
        }
        let valid = recipe_fields(reader, text, at, number, e);
        let mut picked: Vec<(Guid, &str)> = Vec::new();
        for name in &e.recipes {
            if let Some(guid) = recipe(reader, text, at, name) {
                picked.push((guid, name));
            }
        }
        for name in &e.output {
            for guid in recipes_making(reader, text, at, name) {
                picked.push((guid, name));
            }
        }
        for (recipe, name) in picked {
            for (field, item, value) in &valid {
                let key = (recipe, *field, item.as_ref().map(|(guid, _)| *guid));
                if let Some(&first) = seen.get(&key) {
                    let what = match item {
                        Some((_, item)) => format!("{field} {item}"),
                        None => field.to_string(),
                    };
                    reader.error(
                        file,
                        text,
                        at,
                        format!(
                            "change {number}: {what} of {name} is changed at line {first} already"
                        ),
                    );
                    continue;
                }
                seen.insert(key, line);
                let change = match item {
                    Some((item, _)) => Change::ingredient(recipe, *item, *value),
                    None => Change::new(recipe, *field, *value),
                };
                out.push((change, source(file, line, name)));
            }
        }
    }
    (out, selections)
}

/// The checked fields change `number`, `e`, gives the recipes it names;
/// problems are noted at the change.
fn recipe_fields(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    number: usize,
    e: &RecipeChangeEntry,
) -> Vec<RecipeField> {
    let file = RECIPES_FILE;
    let mut fields: Vec<RecipeField> = [
        (Field::Output, e.output_count),
        (Field::Cost, e.cost),
        (Field::CraftingDuration, e.time),
    ]
    .into_iter()
    .filter_map(|(field, v)| v.map(|v| (field, None, value(v))))
    .collect();
    for (name, v) in &e.ingredients {
        if let Some((item, _)) = reader.reference(file, text, at, name, "item", "") {
            fields.push((Field::Ingredient, Some((item, name.clone())), value(*v)));
        }
    }
    let station = e
        .station
        .as_ref()
        .and_then(|name| station(reader, text, at, number, name));
    fields.extend(station.map(|v| (Field::Station, None, v)));
    if e.recipes.is_empty() && e.output.is_empty() {
        reader.error(
            file,
            text,
            at,
            format!("change {number}: names no recipe; give recipes or output"),
        );
    }
    if fields.is_empty() && e.ingredients.is_empty() && e.station.is_none() {
        let names: Vec<&str> = Field::ALL
            .iter()
            .filter(|f| f.entry() == Entry::Recipe && **f != Field::Ingredient)
            .map(|f| f.name())
            .chain(["ingredients"])
            .collect();
        reader.error(
            file,
            text,
            at,
            format!(
                "change {number}: changes nothing; give {}",
                names.join(", ")
            ),
        );
    }
    let mut valid = Vec::new();
    for (field, item, value) in fields {
        match field.check(value) {
            Ok(()) => valid.push((field, item, value)),
            Err(why) => reader.error(file, text, at, format!("change {number}: {why}")),
        }
    }
    valid
}

/// Reads change `e` when it selects recipes by `kind` or `workshop`, or
/// names more than one way to pick them, into `selections`; whether it
/// did. `at` is where the change starts, its line and its number.
fn selected_recipes(
    reader: &mut Reader<'_>,
    text: &str,
    (at, line, number): (usize, usize, usize),
    e: &RecipeChangeEntry,
    seen: &mut HashMap<(Recipes, Field), usize>,
    selections: &mut RecipeSelections,
) -> bool {
    let file = RECIPES_FILE;
    let by_name = !(e.recipes.is_empty() && e.output.is_empty());
    let groups = [by_name, !e.kind.is_empty(), !e.workshop.is_empty()];
    if groups.iter().filter(|g| **g).count() > 1 {
        reader.error(
            file,
            text,
            at,
            format!("change {number}: give one of recipes or output, kind, workshop"),
        );
        return true;
    }
    if by_name || !(groups[1] || groups[2]) {
        return false;
    }
    for (change, label) in select_recipes(reader, text, at, number, e) {
        let key = (change.recipes, change.field);
        if let Some(&first) = seen.get(&key) {
            reader.error(
                file,
                text,
                at,
                format!(
                    "change {number}: {} of {label} is changed at line {first} already",
                    change.field
                ),
            );
            continue;
        }
        seen.insert(key, line);
        selections.push((change, format!("{file}:{line} ({label})")));
    }
    true
}

/// The checked changes of every recipe the `kind` or `workshop` selection
/// of change `number` picks, each with the selection in words; problems are
/// noted at the change.
fn select_recipes(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    number: usize,
    e: &RecipeChangeEntry,
) -> Vec<(RecipesChange, String)> {
    let file = RECIPES_FILE;
    let fail = |reader: &mut Reader<'_>, why: String| {
        reader.error(file, text, at, format!("change {number}: {why}"));
    };
    if !e.ingredients.is_empty() {
        fail(
            reader,
            "ingredients is no field a selection of recipes changes".to_owned(),
        );
        return Vec::new();
    }
    let mut fields: Vec<(Field, Value)> = [
        (Field::Output, e.output_count),
        (Field::Cost, e.cost),
        (Field::CraftingDuration, e.time),
    ]
    .into_iter()
    .filter_map(|(field, v)| v.map(|v| (field, value(v))))
    .collect();
    if let Some(name) = &e.station {
        fields.extend(station(reader, text, at, number, name).map(|v| (Field::Station, v)));
    }
    if fields.is_empty() && e.station.is_none() {
        let names: Vec<&str> = [
            Field::Output,
            Field::Cost,
            Field::CraftingDuration,
            Field::Station,
        ]
        .iter()
        .map(|f| f.name())
        .collect();
        fail(
            reader,
            format!("changes nothing; give {}", names.join(", ")),
        );
        return Vec::new();
    }
    let mut targets: Vec<(Recipes, String)> = Vec::new();
    for name in &e.kind {
        if let Some(kind) = RecipeKind::named(name) {
            targets.push((Recipes::Kind(kind), format!("kind {name}")));
            continue;
        }
        let near: Vec<&str> = RecipeKind::ALL
            .iter()
            .map(|k| k.name())
            .filter(|k| strsim::levenshtein(k, &name.to_lowercase()) <= 2)
            .collect();
        let near = if near.is_empty() {
            String::new()
        } else {
            format!("; near: {}", near.join(", "))
        };
        fail(reader, format!("no recipe kind {name:?}{near}"));
    }
    for name in &e.workshop {
        let Some(vanilla) = reader.vanilla else {
            fail(
                reader,
                format!("workshop {name:?} needs the game's data at hand to find its recipes"),
            );
            continue;
        };
        let station = vanilla.item(name).and_then(|item| vanilla.workshop(item));
        match station {
            Ok(workshop) => targets.push((Recipes::Workshop(workshop), format!("workshop {name}"))),
            Err(VanillaError::NotAStation(_)) => {
                fail(reader, format!("{name} is no crafting station"));
            }
            Err(why) => fail(reader, why.to_string()),
        }
    }
    let mut out = Vec::new();
    for (recipes, label) in targets {
        for &(field, value) in &fields {
            let change = RecipesChange::new(recipes, field, value);
            match change.check() {
                Ok(()) => out.push((change, label.clone())),
                Err(why) => fail(reader, why.to_string()),
            }
        }
    }
    out
}

/// The station value of the crafting station whose item `name` names, from
/// the game's data; problems are noted at change `number`.
fn station(
    reader: &mut Reader<'_>,
    text: &str,
    at: usize,
    number: usize,
    name: &str,
) -> Option<Value> {
    let found = match reader.vanilla {
        None => Err(format!(
            "station {name:?} needs the game's data at hand to find its workshop"
        )),
        Some(vanilla) => match vanilla.item(name).and_then(|item| vanilla.workshop(item)) {
            Ok(workshop) => Ok(Change::station(Guid([0; 16]), workshop).value),
            Err(VanillaError::NotAStation(_)) => Err(format!("{name} is no crafting station")),
            Err(why) => Err(why.to_string()),
        },
    };
    found
        .map_err(|why| reader.error(RECIPES_FILE, text, at, format!("change {number}: {why}")))
        .ok()
}
