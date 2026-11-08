//! Vanilla items, workshops and loot tables taken out of play, and a
//! vanilla item answering with another's data.
//!
//! Nothing leaves the game's data and nothing is written to a save:
//! [`Load::remove_item`] keeps the item registered, so the stacks players
//! hold survive and a world opens as vanilla without the mod, while no
//! recipe makes it and no loot gives it. [`Load::remove_workshop`] takes a
//! station's recipes and its own out of play, and leaves placed ones
//! standing. [`Load::remove_loot_table`] serves a table that gives
//! nothing. [`Load::rekey_item`] serves an item under its own id with the
//! data of another. A recipe goes with [`recipes::disable`].
//!
//! ```
//! use enshwick::content::vanilla;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     load.remove_item(vanilla::item("Tool_Torch01")?)?;
//!     load.rekey_item(vanilla::item("Material_T1_Resin")?, vanilla::item("Material_T1_Wood_Logs")?)?;
//!     Ok(())
//! }
//! ```

use crate::content::changes::{self, Change, Field};
use crate::content::recipes::{self, RecipeRule};
use crate::engine_ids as ids;
use crate::resources::{Registry, Value};
use crate::typed::{ItemFlag, Reflected as _};
use crate::{Load, Off};

use super::items::{self, ItemLayout};
use super::{EngineFacts, Guid, types};

/// The game's names of the fields a removal reads and writes (server 399).
const ITEM_ID: &str = "itemId";
const OBJECT_ID: &str = "objectId";
const MAX_STACK: &str = "maxStackSize";
const FLAGS: &str = "flags";
const ITEMS: &str = "items";
const ROOT: &str = "rootGroup";
const STACKS: &str = "stacks";
const GROUPS: &str = "groups";
const LOOT_LABELS: &str = "lootLabels";
const LABELS: &str = "labels";
const CHILD_GROUPS: &str = "childGroups";
/// The bit `ItemFlag::HasPIDE` holds when flags read as a number.
const PIDE_BIT: u64 = 1 << 1;

/// The id an `ItemId` value holds, bare or as its `value` field.
fn id_of(value: &Value) -> Option<u64> {
    match value {
        Value::Uint(id) => Some(*id),
        Value::Struct(_) => value.field("value").and_then(id_of),
        _ => None,
    }
}

/// Whether an item's values give it an entity of its own, which the game
/// makes or clears on load when a re-key changes it.
fn has_entity(item: &Value) -> bool {
    let pide = ItemFlag::HasPIDE.to_value();
    match item.field(FLAGS) {
        Some(Value::Flags(names)) => names
            .iter()
            .any(|name| matches!(&pide, Value::Name(p) if p == name)),
        Some(Value::Uint(raw)) => raw & PIDE_BIT != 0,
        _ => false,
    }
}

/// The random loot table `table` without the entries of the item with game
/// id `item`.
fn without(mut table: Value, item: u32) -> Result<Value, Off> {
    match table.field_mut(ITEMS) {
        Some(Value::List(entries)) => {
            entries.retain(|entry| entry.field(ITEM_ID).and_then(id_of) != Some(u64::from(item)));
            Ok(table)
        }
        _ => Err(Off::new("random loot table: no list of items")),
    }
}

/// The loot table `table`, random or a fill table, giving nothing: a fill
/// table's root group loses its stacks, its groups and the labels it rolls
/// loot by.
fn emptied(mut table: Value) -> Result<Value, Off> {
    if let Some(Value::List(entries)) = table.field_mut(ITEMS) {
        entries.clear();
        return Ok(table);
    }
    let root = table
        .field_mut(ROOT)
        .ok_or_else(|| Off::new("loot table: neither items nor a root group"))?;
    for name in [STACKS, GROUPS] {
        if let Some(Value::List(list)) = root.field_mut(name) {
            list.clear();
        }
    }
    if let Some(labels) = root.field_mut(LOOT_LABELS) {
        for name in [LABELS, CHILD_GROUPS] {
            if let Some(Value::List(list)) = labels.field_mut(name) {
                list.clear();
            }
        }
    }
    Ok(table)
}

/// The item `old` with the data of the item `new`: `old`'s own id and GUID,
/// and at least its stack size.
///
/// # Errors
///
/// [`Off`] when one of the two has an entity of its own and the other not.
fn rekeyed(old: &Value, new: &Value) -> Result<Value, Off> {
    if has_entity(new) != has_entity(old) {
        return Err(Off::new(
            "one item has an entity of its own and the other not, which the game would make or clear in every held stack",
        ));
    }
    let stack = |item: &Value| item.field(MAX_STACK).and_then(id_of).unwrap_or(0);
    let most = stack(old).max(stack(new));
    let mut served = new.clone();
    for name in [ITEM_ID, OBJECT_ID] {
        let own = old
            .field(name)
            .cloned()
            .ok_or_else(|| Off::new(format!("the old item has no {name}")))?;
        *served
            .field_mut(name)
            .ok_or_else(|| Off::new(format!("the new item has no {name}")))? = own;
    }
    if let Some(size) = served.field_mut(MAX_STACK) {
        *size = Value::Uint(most);
    }
    Ok(served)
}

#[crate::script(content_removal)]
impl Load<'_> {
    /// Takes the vanilla item `item` out of play while the mod runs: no
    /// recipe makes it, no crafting or build menu lists it, and no loot
    /// table or starting inventory gives it. The item stays registered, so
    /// the stacks players hold keep it and a world opens as vanilla without
    /// the mod; a mod that wants held copies gone takes them out with the
    /// inventory API.
    ///
    /// Mods' changes of loot chain in load order: a later mod that gives the
    /// item a count gives it back.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, after init, and when the build does not
    /// reflect the loot tables.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn remove_item(&mut self, item: Guid) -> Result<(), Off> {
        let host = self.host();
        let hash = host.hash(ids::LOOTABLE_ITEMS)?;
        let registry = Registry::game()?;
        let ty = registry
            .hashed(hash)
            .ok_or_else(|| Off::new("random loot tables: not a type of this build"))?;
        let tables = host.resource_keys(Some(ty))?;
        recipes::disable(RecipeRule::Output(item.into()))?;
        let source = format!("remove_item {item}");
        self.change(
            &Change::new(item, Field::Count, changes::Value::Set(0.0)),
            &source,
        )?;
        let id = item.fnv1a();
        for key in tables {
            crate::resources::serve_value(host, key, key, move |table| without(table, id))?;
        }
        Ok(())
    }

    /// Takes the vanilla workshop of the station item `station` out of
    /// play while the mod runs: no recipe is made at it, and no recipe
    /// builds the station. Placed stations stay in the world, since they
    /// are entities of the station's template, which the mod leaves alone.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.remove_workshop(vanilla::item("Craftingstation_Workbench")?)
    /// }
    /// ```
    #[script]
    #[expect(
        clippy::unused_self,
        reason = "an init call beside the other removals, where mods and scripts find it"
    )]
    pub fn remove_workshop(&mut self, station: Guid) -> Result<(), Off> {
        recipes::disable(RecipeRule::Workshop(station.into()))?;
        recipes::disable(RecipeRule::Output(station.into()))
    }

    /// Serves the vanilla loot table `table`, random or a container's fill
    /// table, giving nothing while the mod runs. Loot players hold is
    /// plain stacks and stays.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game and for a table the game lacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, table: Guid) -> Result<(), enshwick::Off> {
    ///     load.remove_loot_table(table)
    /// }
    /// ```
    #[script]
    pub fn remove_loot_table(&mut self, table: Guid) -> Result<(), Off> {
        let host = self.host();
        for id in [ids::LOOT_FILL_TABLE, ids::LOOTABLE_ITEMS] {
            let key = table.key_by_hash(host.hash(id)?);
            if host.game_resource(key).is_some() {
                return crate::resources::serve_value(host, key, key, emptied);
            }
        }
        Err(Off::new(format!("loot table {table}: not in the game")))
    }

    /// Serves the vanilla item `old` with the data of the item `new` while
    /// the mod runs, under `old`'s own id and GUID. `new` is a vanilla item
    /// or one this mod added with [`Load::add_item`] before the call. Saves,
    /// recipes and loot that name `old` get `new`'s look and use. The stack
    /// size is at least `old`'s, since the game cuts a held stack above its
    /// item's and saves the cut; saves keep `old`'s id, so a world opens as
    /// vanilla without the mod. `old` takes `new` as the game reads it, with
    /// every mod's changes of `new`; inside that read, a re-key of `new`
    /// back to `old` is skipped, so two items re-keyed to each other swap. Mods that serve `old` chain in load order, and the last
    /// re-key's data wins; a build that fails keeps `old` as it was.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game, for an item the game lacks, and when one of
    /// the two items has an entity of its own (`ItemFlag::HasPIDE`) and the
    /// other not.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[script]
    pub fn rekey_item(&mut self, old: Guid, new: Guid) -> Result<(), Off> {
        let host = self.host();
        let key = old.key(types::ITEM_INFO)?;
        let new_key = new.key(types::ITEM_INFO)?;
        // At init nothing is served yet: the check reads `new` as the game
        // archives it, or as this mod clones it.
        let data = match items::added(new) {
            None => host.game_value(new_key)?,
            Some(item) => {
                let layout = ItemLayout::resolve(self)?;
                let like = host
                    .game_resource(item.like.key(types::ITEM_INFO)?)
                    .ok_or_else(|| {
                        Off::new(format!("its base item {}: not in the game", item.like))
                    })?;
                let bytes = items::served_clone(&layout, &like, &item, new)
                    .ok_or_else(|| Off::new("the mod's item does not clone from its base"))?;
                crate::resources::decode(new_key, &bytes)?
            }
        };
        rekeyed(&host.game_value(key)?, &data)
            .map_err(|off| Off::new(format!("re-key {old} to {new}: {off}")))?;
        // Served from `old` itself, so other mods' patches of it chain and a
        // failed build keeps the vanilla item rather than none; `new` reads
        // as served, with every mod's patches of it.
        crate::resources::serve_value(host, key, key, move |item| {
            let served = host.served_resource(new_key)?;
            rekeyed(&item, &crate::resources::decode(new_key, &served)?)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u8, stack: u64, flags: Value) -> Value {
        Value::Struct(vec![
            (
                ITEM_ID.into(),
                Value::Struct(vec![("value".into(), Value::Uint(id.into()))]),
            ),
            (OBJECT_ID.into(), Value::Guid(Guid([id; 16]))),
            (MAX_STACK.into(), Value::Uint(stack)),
            (FLAGS.into(), flags),
            ("category".into(), Value::Uint(id.into())),
        ])
    }

    fn pide() -> Value {
        Value::Flags(vec![match ItemFlag::HasPIDE.to_value() {
            Value::Name(name) => name,
            _ => unreachable!(),
        }])
    }

    #[test]
    fn a_rekeyed_item_keeps_the_old_id_guid_and_the_larger_stack_and_takes_the_rest() {
        let served = rekeyed(
            &item(1, 50, Value::Flags(vec![])),
            &item(2, 20, Value::Flags(vec![])),
        )
        .unwrap();
        assert_eq!(served.field(ITEM_ID).and_then(id_of), Some(1));
        assert_eq!(served.field(OBJECT_ID), Some(&Value::Guid(Guid([1; 16]))));
        assert_eq!(served.field(MAX_STACK), Some(&Value::Uint(50)));
        assert_eq!(served.field("category"), Some(&Value::Uint(2)));
        let served = rekeyed(&item(1, 50, Value::Uint(0)), &item(2, 99, Value::Uint(0))).unwrap();
        assert_eq!(served.field(MAX_STACK), Some(&Value::Uint(99)));
    }

    #[test]
    fn a_rekey_between_an_item_with_an_entity_and_one_without_is_refused() {
        assert!(rekeyed(&item(1, 1, Value::Flags(vec![])), &item(2, 1, pide())).is_err());
        assert!(
            rekeyed(
                &item(1, 1, Value::Uint(PIDE_BIT)),
                &item(2, 1, Value::Uint(0))
            )
            .is_err()
        );
        assert!(rekeyed(&item(1, 1, Value::Uint(PIDE_BIT)), &item(2, 1, pide())).is_ok());
    }

    #[test]
    fn random_loot_loses_only_the_removed_item() {
        let entry = |id| {
            Value::Struct(vec![(
                ITEM_ID.into(),
                Value::Struct(vec![("value".into(), Value::Uint(id))]),
            )])
        };
        let table = Value::Struct(vec![(
            ITEMS.into(),
            Value::List(vec![entry(1), entry(2), entry(1)]),
        )]);
        let left = without(table, 1).unwrap();
        assert_eq!(left.field(ITEMS), Some(&Value::List(vec![entry(2)])));
        assert!(without(Value::Struct(vec![]), 1).is_err());
    }

    #[test]
    fn two_mods_removals_and_rekeys_chain_in_load_order() {
        let entry = |id| {
            Value::Struct(vec![(
                ITEM_ID.into(),
                Value::Struct(vec![("value".into(), Value::Uint(id))]),
            )])
        };
        let table = Value::Struct(vec![(
            ITEMS.into(),
            Value::List(vec![entry(1), entry(2), entry(3)]),
        )]);
        let first = without(table, 1).unwrap();
        let second = without(first, 3).unwrap();
        assert_eq!(second.field(ITEMS), Some(&Value::List(vec![entry(2)])));
        let old = item(1, 10, Value::Flags(vec![]));
        let first = rekeyed(&old, &item(2, 30, Value::Flags(vec![]))).unwrap();
        let second = rekeyed(&first, &item(3, 5, Value::Flags(vec![]))).unwrap();
        assert_eq!(second.field(ITEM_ID).and_then(id_of), Some(1));
        assert_eq!(second.field(OBJECT_ID), Some(&Value::Guid(Guid([1; 16]))));
        assert_eq!(
            second.field("category"),
            Some(&Value::Uint(3)),
            "the last mod's data"
        );
        assert_eq!(
            second.field(MAX_STACK),
            Some(&Value::Uint(30)),
            "never below what an earlier mod served"
        );
    }

    #[test]
    fn an_emptied_table_gives_nothing_whether_random_or_a_fill_table() {
        let random = Value::Struct(vec![(ITEMS.into(), Value::List(vec![Value::Uint(1)]))]);
        assert_eq!(
            emptied(random).unwrap().field(ITEMS),
            Some(&Value::List(vec![]))
        );
        let labels = Value::Struct(vec![
            (LABELS.into(), Value::List(vec![Value::Uint(3)])),
            (CHILD_GROUPS.into(), Value::List(vec![Value::Uint(4)])),
        ]);
        let group = Value::Struct(vec![
            (STACKS.into(), Value::List(vec![Value::Uint(1)])),
            (GROUPS.into(), Value::List(vec![Value::Uint(2)])),
            (LOOT_LABELS.into(), labels),
        ]);
        let fill = emptied(Value::Struct(vec![(ROOT.into(), group)])).unwrap();
        let root = fill.field(ROOT).unwrap();
        assert_eq!(root.field(STACKS), Some(&Value::List(vec![])));
        assert_eq!(root.field(GROUPS), Some(&Value::List(vec![])));
        let labels = root.field(LOOT_LABELS).unwrap();
        assert_eq!(
            labels.field(LABELS),
            Some(&Value::List(vec![])),
            "no loot by label"
        );
        assert_eq!(labels.field(CHILD_GROUPS), Some(&Value::List(vec![])));
        assert!(emptied(Value::Struct(vec![])).is_err());
    }
}
