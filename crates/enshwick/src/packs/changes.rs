//! Reading a pack's changes of vanilla values.

use std::collections::HashMap;

use super::entries::{ItemsFile, LootFile, RecipesFile, ValueEntry};
use super::files::PackError;
use super::pack::Reader;
use super::{ITEMS_FILE, LOOT_FILE, RECIPES_FILE};
use crate::content::Guid;
use crate::content::changes::{Change, Entry, Field, Value};

/// A field a recipe change sets, with its ingredient and the name the pack
/// gave it, and the value.
type RecipeField = (Field, Option<(Guid, String)>, Value);

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

/// The checked changes of `items_file`, each with its source,
/// `items.toml:<line>`, followed by the name the pack wrote when it is no
/// GUID: `items.toml:12 (Material_T1_Wood_Logs)`.
pub(super) fn item_changes(
    reader: &mut Reader<'_>,
    items_file: &ItemsFile,
    text: &str,
) -> Vec<(Change, String)> {
    let entries: Vec<ChangeEntry<'_>> = items_file
        .change
        .iter()
        .map(|entry| {
            let e = entry.get_ref();
            let fields = vec![
                (Field::MaxStack, e.max_stack),
                (Field::Slots, e.slots),
                (Field::DamageMod, e.damage_mod),
                (Field::SpeedMod, e.speed_mod),
                (Field::Armor, e.armor),
                (Field::BurnTime, e.burn_time),
                (Field::EnergyOutput, e.energy_output),
                (Field::ComfortAmount, e.comfort_amount),
            ];
            (entry.span().start, e.items.as_slice(), fields)
        })
        .collect();
    changes_of(reader, ITEMS_FILE, Entry::Item, &entries, text)
}

/// The checked changes of `loot_file`, each with its source as
/// [`item_changes`] gives it.
pub(super) fn loot_changes(
    reader: &mut Reader<'_>,
    loot_file: &LootFile,
    text: &str,
) -> Vec<(Change, String)> {
    let entries: Vec<ChangeEntry<'_>> = loot_file
        .change
        .iter()
        .map(|entry| {
            let e = entry.get_ref();
            let fields = vec![
                (Field::StackSize, e.stack_size),
                (Field::Count, e.count),
                (Field::SpawnProbability, e.spawn_probability),
                (Field::Weight, e.weight),
            ];
            (entry.span().start, e.items.as_slice(), fields)
        })
        .collect();
    changes_of(reader, LOOT_FILE, Entry::Loot, &entries, text)
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
) -> Vec<(Change, String)> {
    let file = RECIPES_FILE;
    let mut out = Vec::new();
    let mut seen: HashMap<(Guid, Field, Option<Guid>), usize> = HashMap::new();
    for (index, entry) in recipes_file.change.iter().enumerate() {
        let at = entry.span().start;
        let line = PackError::at(file, text, at, "").line;
        let e = entry.get_ref();
        let number = index + 1;
        let mut fields: Vec<RecipeField> = [
            (Field::Output, e.output_count),
            (Field::Cost, e.cost),
            (Field::Time, e.time),
        ]
        .into_iter()
        .filter_map(|(field, v)| v.map(|v| (field, None, value(v))))
        .collect();
        for (name, v) in &e.ingredients {
            if let Some((item, _)) = reader.reference(file, text, at, name, "item", "") {
                fields.push((Field::Ingredient, Some((item, name.clone())), value(*v)));
            }
        }
        if e.recipes.is_empty() && e.output.is_empty() {
            reader.error(
                file,
                text,
                at,
                format!("change {number}: names no recipe; give recipes or output"),
            );
        }
        if fields.is_empty() && e.ingredients.is_empty() {
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
    out
}
