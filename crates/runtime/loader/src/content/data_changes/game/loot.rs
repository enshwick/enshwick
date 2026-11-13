//! The loader's patches of the loot tables: the random loot table and each
//! container fill table that names a changed item.

use core::ffi::c_void;

use enshwick_core::abi::ResourceKey;
use enshwick_core::content::Guid;
use enshwick_core::content::changes::Field;
use enshwick_core::content::loot_tables::{
    LootLayout, change_fill_table, change_random_loot, fill_items,
};
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, UNREADABLE, on};
use crate::content::data_changes::Key;
use crate::content::served::OwnPatch;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The loader's patches of the loot tables the changes reach: the random
/// loot table, and each container fill table that names a changed item.
pub(super) fn loot_patches(
    loader: &'static Loader,
) -> Vec<(
    ResourceKey,
    enshwick_core::abi::ResourcePatchFn,
    *mut c_void,
)> {
    let Some(sealed) = CHANGES.sealed.get() else {
        return Vec::new();
    };
    let merged = sealed.merged();
    let scoped = merged.table_items();
    let items: FxHashMap<u32, Guid> = merged
        .loot_items()
        .into_iter()
        .chain(scoped.iter().map(|(_, item)| *item))
        .map(|item| (item.fnv1a(), item))
        .collect();
    let alone: Vec<Guid> = scoped.iter().map(|(table, _)| *table).collect();
    let _first = sealed.loot_items.set(merged.loot_items());
    if items.is_empty() {
        return Vec::new();
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let hashes = catalogue::type_hash(catalogue::ids::LOOTABLE_ITEMS)
        .zip(catalogue::type_hash(catalogue::ids::LOOT_FILL_TABLE));
    let (Ok(layout), Some(archive), Some((random, fill))) = (
        LootLayout::resolve(&facts),
        crate::engine::archive::game(),
        hashes,
    ) else {
        let _off = loader.fuse_off(&LOOT_FUSE, "the loot table layout", UNREADABLE);
        return Vec::new();
    };
    let items: &'static FxHashMap<u32, Guid> = Box::leak(Box::new(items));
    let mut patches = Vec::new();
    let mut add = |key: ResourceKey, fill: bool| {
        let (apply, user) = crate::content::served::own_patch(LootPatch {
            layout,
            items,
            fill,
            table: Guid(key.guid),
            fuse: Fuse::new(LOOT_FUSE.feature()),
        });
        patches.push((key, apply, user));
    };
    for key in archive.keys_of(random) {
        add(key, false);
    }
    let mut tables = 0;
    for key in archive.keys_of(fill) {
        let names = alone.contains(&Guid(key.guid))
            || archive.resource(&key).is_some_and(|table| {
                fill_items(&layout, &table)
                    .iter()
                    .any(|id| items.contains_key(&id.get()))
            });
        if names {
            add(key, true);
            tables += 1;
        }
    }
    crate::log!(
        INFO,
        "{} items get changed loot, in {tables} container fill tables",
        items.len()
    );
    patches
}

/// What the loader's patch of a loot table needs: the layout, the changed
/// items by game id, and whether the table is a fill table or the random
/// loot table.
struct LootPatch {
    layout: LootLayout,
    items: &'static FxHashMap<u32, Guid>,
    fill: bool,
    /// The table's GUID: changes of this table alone apply after the
    /// changes of every table.
    table: Guid,
    /// This table's switch alone.
    fuse: Fuse,
}

/// The loader's patch of a loot table: the base with the values the
/// changes of the mods still on give its items.
impl OwnPatch for LootPatch {
    const SITE: &'static str = "a loot table patch";

    fn fuse(&self) -> &Fuse {
        &self.fuse
    }

    fn build(&self, bytes: Vec<u8>) -> Option<Vec<u8>> {
        changed_loot(self, &bytes)
    }
}

fn changed_loot(patch: &LootPatch, bytes: &[u8]) -> Option<Vec<u8>> {
    let sealed = CHANGES.sealed.get()?;
    let merged = sealed.merged();
    let value = |id: enshwick_core::ItemId, field: Field, base: f64| {
        let item = *patch.items.get(&id.get())?;
        let on = |m| on(sealed.loader, m);
        let every = merged.value(Key::new(item, field), base, on);
        let here = Key {
            entry: patch.table,
            field,
            item: Some(item),
        };
        merged.value(here, every.unwrap_or(base), on).or(every)
    };
    let changed = if patch.fill {
        change_fill_table(&patch.layout, bytes, &value)
    } else {
        change_random_loot(&patch.layout, bytes, &value)
    };
    changed.map(|(built, _)| built)
}

/// The switch of the loot changes where the build lacks what they read;
/// each table's patch has a switch of its own besides.
static LOOT_FUSE: Fuse = Fuse::new("loot changes");
