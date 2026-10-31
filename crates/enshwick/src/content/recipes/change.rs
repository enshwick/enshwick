//! Changes of vanilla recipes' values in the recipe registry, as the loader
//! applies the merged changes of every mod.

use crate::content::changes::Field;
use crate::content::{Blob, Guid};

use super::RecipeLayout;

/// Nanoseconds in a second: the game keeps a recipe's time in nanoseconds.
const NANOS: f64 = 1e9;

/// What changes in recipes, asked recipe by recipe.
///
/// # Examples
///
/// See [`change_recipes_in`], whose example implements it.
pub trait RecipeChanges {
    /// What of `recipe` changes: each field, with its ingredient for
    /// [`Field::Ingredient`].
    fn of(&self, recipe: Guid) -> Vec<(Field, Option<Guid>)>;

    /// The value in force of `field` (of the ingredient `item`) of `recipe`,
    /// from `base`; `None` when nothing changes it.
    fn value(&self, recipe: Guid, field: Field, item: Option<Guid>, base: f64) -> Option<f64>;
}

/// The recipe registry with every change `changes` holds applied, and how
/// many recipes changed. `duration` is where a recipe keeps its time, when
/// the build reflects it.
///
/// Counts change in place. An ingredient that takes any item of a kind keeps
/// its count. An ingredient set to 0 is taken out, and one the recipe lacks
/// is added with nothing but its item and count. Either moves the
/// ingredient list to the end of the registry, so only a first build takes
/// it.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::content::changes::Field;
/// use enshwick::content::recipes::{RecipeChanges, RecipeLayout, change_recipes_in};
///
/// struct Nothing;
///
/// impl RecipeChanges for Nothing {
///     fn of(&self, _: Guid) -> Vec<(Field, Option<Guid>)> {
///         Vec::new()
///     }
///     fn value(&self, _: Guid, _: Field, _: Option<Guid>, _: f64) -> Option<f64> {
///         None
///     }
/// }
///
/// fn unchanged(layout: &RecipeLayout, registry: &[u8]) -> Option<usize> {
///     change_recipes_in(layout, None, registry, &Nothing).map(|(_, changed)| changed)
/// }
/// ```
#[must_use]
pub fn change_recipes_in(
    layout: &RecipeLayout,
    duration: Option<usize>,
    registry: &[u8],
    changes: &dyn RecipeChanges,
) -> Option<(Vec<u8>, usize)> {
    let mut blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    let stations = workshop_guids(layout, &blob, start, usize::try_from(count).ok()?)?;
    let mut edited = 0;
    for index in 0..usize::try_from(count).ok()? {
        let at = start + index * layout.recipe;
        let guid = Guid(
            blob.bytes()
                .get(at + layout.guid..at + layout.guid + 16)?
                .try_into()
                .ok()?,
        );
        let keys = changes.of(guid);
        if keys.is_empty() {
            continue;
        }
        let mut touched = false;
        if keys.iter().any(|(field, _)| *field == Field::Output) {
            touched |= scale_outputs(layout, &mut blob, at, guid, changes)?;
        }
        touched |= change_inputs(layout, &mut blob, at, guid, &keys, changes)?;
        if let Some(duration) = duration
            && keys
                .iter()
                .any(|(field, _)| *field == Field::CraftingDuration)
        {
            touched |= change_time(&mut blob, at + duration, guid, changes)?;
        }
        if keys.iter().any(|(field, _)| *field == Field::Station) {
            touched |= change_station(layout, &mut blob, at, guid, &stations, changes)?;
        }
        edited += usize::from(touched);
    }
    Some((blob.into_bytes(), edited))
}

/// The count at `at` of an item stack, and the count `value` gives it.
fn count_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A resolved count as the `u32` the registry keeps.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "Field::resolve keeps a count whole and from 0 to 65535"
)]
fn whole(value: f64) -> u32 {
    value as u32
}

/// Changes the count of each item the recipe at `at` makes.
fn scale_outputs(
    layout: &RecipeLayout,
    blob: &mut Blob,
    at: usize,
    recipe: Guid,
    changes: &dyn RecipeChanges,
) -> Option<bool> {
    let (outputs, count) = blob.array(at + layout.output)?;
    let mut touched = false;
    for i in 0..usize::try_from(count).ok()? {
        let field = outputs + i * layout.stack_size + layout.stack_count;
        let made = count_at(blob.bytes(), field)?;
        let Some(new) = changes.value(recipe, Field::Output, None, f64::from(made)) else {
            continue;
        };
        let new = whole(new);
        if new != made {
            touched |= blob.write(field, &new.to_le_bytes());
        }
    }
    Some(touched)
}

/// Whether an ingredient names an item: one that takes any item of a kind
/// leaves its item stack empty and keeps its count elsewhere.
fn names_item(input: &[u8]) -> bool {
    input
        .get(..16)
        .is_some_and(|guid| guid.iter().any(|&b| b != 0))
}

/// Changes the ingredients of the recipe at `at`: every count of an
/// ingredient that names an item, then each named ingredient, taken out at
/// 0 and added when missing.
fn change_inputs(
    layout: &RecipeLayout,
    blob: &mut Blob,
    at: usize,
    recipe: Guid,
    keys: &[(Field, Option<Guid>)],
    changes: &dyn RecipeChanges,
) -> Option<bool> {
    let size = layout.input_size.max(1);
    let (start, count) = blob.array(at + layout.input)?;
    let length = usize::try_from(count).ok()? * size;
    let before: Vec<Vec<u8>> = blob
        .bytes()
        .get(start..start + length)?
        .chunks(size)
        .map(<[u8]>::to_vec)
        .collect();
    let mut inputs = before.clone();
    let count_field = layout.stack_count..layout.stack_count + 4;
    let set = |input: &mut Vec<u8>, count: u32| -> Option<()> {
        input
            .get_mut(count_field.clone())?
            .copy_from_slice(&count.to_le_bytes());
        Some(())
    };
    if keys.iter().any(|(field, _)| *field == Field::Cost) {
        for input in inputs.iter_mut().filter(|input| names_item(input)) {
            let taken = count_at(input, layout.stack_count)?;
            if let Some(new) = changes.value(recipe, Field::Cost, None, f64::from(taken)) {
                set(input, whole(new))?;
            }
        }
    }
    for item in keys
        .iter()
        .filter(|(field, _)| *field == Field::Ingredient)
        .filter_map(|(_, item)| *item)
    {
        let found = inputs.iter().position(|i| i.get(..16) == Some(&item.0[..]));
        let base = match found {
            Some(i) => count_at(&inputs[i], layout.stack_count)?,
            None => 0,
        };
        let Some(new) = changes.value(recipe, Field::Ingredient, Some(item), f64::from(base))
        else {
            continue;
        };
        match (found, whole(new)) {
            (Some(i), 0) => {
                inputs.remove(i);
            }
            (Some(i), new) => set(&mut inputs[i], new)?,
            (None, 0) => {}
            (None, new) => {
                let mut added = vec![0u8; size];
                added.get_mut(..16)?.copy_from_slice(&item.0);
                added
                    .get_mut(layout.stack_item..layout.stack_item + 4)?
                    .copy_from_slice(&item.fnv1a().to_le_bytes());
                set(&mut added, new)?;
                inputs.push(added);
            }
        }
    }
    if inputs == before {
        return Some(false);
    }
    if inputs.len() == before.len() {
        let bytes = inputs.concat();
        return Some(blob.write(start, &bytes));
    }
    let bytes = inputs.concat();
    let moved = blob.append(&bytes, 8);
    let count = u32::try_from(inputs.len()).ok()?;
    Some(blob.set_array(at + layout.input, moved, count))
}

/// Changes the time of a recipe, kept at `at` in nanoseconds.
fn change_time(
    blob: &mut Blob,
    at: usize,
    recipe: Guid,
    changes: &dyn RecipeChanges,
) -> Option<bool> {
    let nanos = i64::from_le_bytes(blob.bytes().get(at..at + 8)?.try_into().ok()?);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a recipe's time is at most a day, far below f64's whole range"
    )]
    let seconds = nanos as f64 / NANOS;
    let Some(new) = changes.value(recipe, Field::CraftingDuration, None, seconds) else {
        return Some(false);
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Field::resolve keeps a time from 0 to 86400 seconds"
    )]
    let new = (new * NANOS).round() as i64;
    Some(new != nanos && blob.write(at, &new.to_le_bytes()))
}

/// Every workshop some recipe of the registry `registry` is made at, each
/// once; `None` when it does not read with `layout`.
///
/// # Examples
///
/// ```
/// use enshwick::content::recipes::{RecipeLayout, recipe_workshops};
///
/// fn stations(layout: &RecipeLayout, registry: &[u8]) -> usize {
///     recipe_workshops(layout, registry).map_or(0, |workshops| workshops.len())
/// }
/// ```
#[must_use]
pub fn recipe_workshops(
    layout: &RecipeLayout,
    registry: &[u8],
) -> Option<Vec<crate::content::workshops::WorkshopId>> {
    let blob = Blob::new(registry);
    let (start, count) = blob.array(layout.recipes)?;
    let found = workshop_guids(layout, &blob, start, usize::try_from(count).ok()?)?;
    Some(
        found
            .into_iter()
            .map(|(_, guid)| crate::content::workshops::WorkshopId::new(Guid(guid)))
            .collect(),
    )
}

/// The GUID of every workshop some of the `count` recipes from `start` are
/// made at, by the workshop's game id.
fn workshop_guids(
    layout: &RecipeLayout,
    blob: &Blob,
    start: usize,
    count: usize,
) -> Option<Vec<(u32, [u8; 16])>> {
    let mut found: Vec<(u32, [u8; 16])> = Vec::new();
    for index in 0..count {
        let at = start + index * layout.recipe;
        let id = blob.read_u32(at + layout.workshop)?;
        if found.iter().any(|(known, _)| *known == id) {
            continue;
        }
        let guid = blob
            .bytes()
            .get(at + layout.workshop_guid..at + layout.workshop_guid + 16)?
            .try_into()
            .ok()?;
        found.push((id, guid));
    }
    Some(found)
}

/// Moves the recipe at `at` to the workshop its station change names, one
/// of `stations`; a workshop no recipe is made at leaves it where it is,
/// with a warning.
fn change_station(
    layout: &RecipeLayout,
    blob: &mut Blob,
    at: usize,
    recipe: Guid,
    stations: &[(u32, [u8; 16])],
    changes: &dyn RecipeChanges,
) -> Option<bool> {
    let current = blob.read_u32(at + layout.workshop)?;
    let Some(new) = changes.value(recipe, Field::Station, None, f64::from(current)) else {
        return Some(false);
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Field::resolve keeps a station a whole number from 0 to u32::MAX"
    )]
    let new = new as u32;
    if new == current {
        return Some(false);
    }
    let Some((_, guid)) = stations.iter().find(|(id, _)| *id == new) else {
        crate::warn!(
            "recipe {recipe} stays at its workshop: no recipe is made at workshop {new}, so the game has no such station"
        );
        return Some(false);
    };
    Some(
        blob.write(at + layout.workshop_guid, guid)
            && blob.write(at + layout.workshop, &new.to_le_bytes()),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::content::changes::Value;

    const LAYOUT: RecipeLayout = RecipeLayout {
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
    };
    const DURATION: usize = 0x38;
    const TORCH: Guid = Guid([1; 16]);
    const WOOD: Guid = Guid([2; 16]);
    const RESIN: Guid = Guid([3; 16]);
    const STONE: Guid = Guid([4; 16]);

    struct Table(HashMap<(Field, Option<Guid>), Vec<Value>>);

    impl RecipeChanges for Table {
        fn of(&self, recipe: Guid) -> Vec<(Field, Option<Guid>)> {
            if recipe == TORCH {
                self.0.keys().copied().collect()
            } else {
                Vec::new()
            }
        }

        fn value(&self, recipe: Guid, field: Field, item: Option<Guid>, base: f64) -> Option<f64> {
            (recipe == TORCH)
                .then(|| self.0.get(&(field, item)))
                .flatten()
                .map(|values| field.resolve(base, values.iter().copied()))
        }
    }

    fn stack(item: Guid, count: u32, size: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; size];
        bytes[..16].copy_from_slice(&item.0);
        bytes[0x10..0x14].copy_from_slice(&item.fnv1a().to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&count.to_le_bytes());
        bytes
    }

    /// A registry with one recipe for a torch: 2 wood and 1 resin, 60 s.
    fn registry() -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 16]);
        let mut recipe = vec![0u8; 0xa8];
        recipe[..16].copy_from_slice(&TORCH.0);
        recipe[DURATION..DURATION + 8].copy_from_slice(&60_000_000_000_i64.to_le_bytes());
        let at = blob.append(&recipe, 8);
        let inputs = [stack(WOOD, 2, 48), stack(RESIN, 1, 48)].concat();
        let inputs_at = blob.append(&inputs, 8);
        let output_at = blob.append(&stack(TORCH, 1, 24), 8);
        assert!(blob.set_array(at + 0x28, inputs_at, 2));
        assert!(blob.set_array(at + 0x30, output_at, 1));
        assert!(blob.set_array(8, at, 1));
        blob.into_bytes()
    }

    fn apply(changes: &[((Field, Option<Guid>), Value)]) -> (Blob, usize) {
        let mut table: HashMap<_, Vec<Value>> = HashMap::new();
        for (key, value) in changes {
            table.entry(*key).or_default().push(*value);
        }
        let (bytes, edited) =
            change_recipes_in(&LAYOUT, Some(DURATION), &registry(), &Table(table)).unwrap();
        (Blob::new(&bytes), edited)
    }

    /// The ingredients of the one recipe: item and count.
    fn inputs(blob: &Blob) -> Vec<(Guid, u32)> {
        let (recipes, _) = blob.array(8).unwrap();
        let (at, count) = blob.array(recipes + 0x28).unwrap();
        (0..count as usize)
            .map(|i| {
                let input = &blob.bytes()[at + i * 48..at + (i + 1) * 48];
                (
                    Guid(input[..16].try_into().unwrap()),
                    count_at(input, 0x14).unwrap(),
                )
            })
            .collect()
    }

    fn output(blob: &Blob) -> u32 {
        let (recipes, _) = blob.array(8).unwrap();
        let (at, _) = blob.array(recipes + 0x30).unwrap();
        count_at(blob.bytes(), at + 0x14).unwrap()
    }

    fn seconds(blob: &Blob) -> i64 {
        let (recipes, _) = blob.array(8).unwrap();
        let at = recipes + DURATION;
        i64::from_le_bytes(blob.bytes()[at..at + 8].try_into().unwrap()) / 1_000_000_000
    }

    #[test]
    fn a_station_change_moves_the_recipe_to_a_workshop_another_recipe_names() {
        let kiln = [9u8; 16];
        let kiln_id = Guid(kiln).fnv1a();
        let mut blob = Blob::new(&registry());
        let (first, _) = blob.array(8).unwrap();
        let mut other = vec![0u8; 0xa8];
        other[..16].copy_from_slice(&WOOD.0);
        other[0x14..0x24].copy_from_slice(&kiln);
        other[0x24..0x28].copy_from_slice(&kiln_id.to_le_bytes());
        let mut both = blob.bytes()[first..first + 0xa8].to_vec();
        both.extend_from_slice(&other);
        let at = blob.append(&both, 8);
        assert!(blob.set_array(8, at, 2));
        let registry = blob.into_bytes();
        let moved = |to: u32| {
            let mut table: HashMap<_, Vec<Value>> = HashMap::new();
            table.insert((Field::Station, None), vec![Value::Set(f64::from(to))]);
            let (bytes, edited) =
                change_recipes_in(&LAYOUT, Some(DURATION), &registry, &Table(table)).unwrap();
            let blob = Blob::new(&bytes);
            let (recipes, _) = blob.array(8).unwrap();
            let guid: [u8; 16] = bytes[recipes + 0x14..recipes + 0x24].try_into().unwrap();
            (edited, guid, blob.read_u32(recipes + 0x24).unwrap())
        };
        assert_eq!(moved(kiln_id), (1, kiln, kiln_id));
        assert_eq!(moved(kiln_id ^ 1), (0, [0; 16], 0));
    }

    #[test]
    fn a_recipe_nobody_changes_stays_as_it_is() {
        let (blob, changed) = apply(&[]);
        assert_eq!(changed, 0);
        assert_eq!(blob.bytes(), registry());
    }

    #[test]
    fn the_output_and_every_ingredient_scale() {
        let (blob, changed) = apply(&[
            ((Field::Output, None), Value::Set(3.0)),
            ((Field::Cost, None), Value::Times(0.5)),
        ]);
        assert_eq!(changed, 1);
        assert_eq!(output(&blob), 3);
        assert_eq!(inputs(&blob), [(WOOD, 1), (RESIN, 1)]);
    }

    #[test]
    fn an_ingredient_set_to_zero_is_taken_out_and_a_missing_one_is_added() {
        let (blob, _) = apply(&[
            ((Field::Ingredient, Some(RESIN)), Value::Set(0.0)),
            ((Field::Ingredient, Some(STONE)), Value::Set(4.0)),
            ((Field::Ingredient, Some(WOOD)), Value::Times(3.0)),
        ]);
        let mut found = inputs(&blob);
        found.sort_by_key(|(item, _)| item.0);
        assert_eq!(found, [(WOOD, 6), (STONE, 4)]);
    }

    #[test]
    fn an_ingredient_named_after_the_cost_wins_over_it() {
        let (blob, _) = apply(&[
            ((Field::Cost, None), Value::Set(5.0)),
            ((Field::Ingredient, Some(WOOD)), Value::Set(1.0)),
        ]);
        assert_eq!(inputs(&blob), [(WOOD, 1), (RESIN, 5)]);
    }

    #[test]
    fn an_ingredient_of_a_kind_keeps_its_count_and_an_added_one_names_only_its_item() {
        let mut blob = Blob::new(&registry());
        let (recipes, _) = blob.array(8).unwrap();
        let (at, _) = blob.array(recipes + 0x28).unwrap();
        let mut kind = vec![0u8; 48];
        kind[24 + 20..24 + 24].copy_from_slice(&7u32.to_le_bytes());
        assert!(blob.write(at + 48, &kind));
        let bytes = blob.into_bytes();
        let changes = Table(HashMap::from([
            ((Field::Cost, None), vec![Value::Times(2.0)]),
            ((Field::Ingredient, Some(STONE)), vec![Value::Set(1.0)]),
        ]));
        let (built, _) = change_recipes_in(&LAYOUT, None, &bytes, &changes).unwrap();
        let blob = Blob::new(&built);
        let (recipes, _) = blob.array(8).unwrap();
        let (at, count) = blob.array(recipes + 0x28).unwrap();
        assert_eq!(count, 3);
        let input = |i: usize| &blob.bytes()[at + i * 48..at + (i + 1) * 48];
        assert_eq!(count_at(input(0), 0x14), Some(4));
        assert_eq!(input(1), &kind[..]);
        assert_eq!(&input(2)[..16], &STONE.0);
        assert!(input(2)[24..].iter().all(|&b| b == 0));
    }

    #[test]
    fn the_time_changes_in_seconds() {
        let (blob, _) = apply(&[((Field::CraftingDuration, None), Value::Times(0.5))]);
        assert_eq!(seconds(&blob), 30);
        let (blob, _) = apply(&[((Field::CraftingDuration, None), Value::Set(0.0))]);
        assert_eq!(seconds(&blob), 0);
    }
}
