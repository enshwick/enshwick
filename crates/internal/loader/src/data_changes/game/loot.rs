//! The loader's patches of the loot tables: the random loot table and each
//! container fill table that names a changed item.

use core::ffi::c_void;

use enshwick::abi::{ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::Field;
use enshwick::content::loot_tables::{
    LootLayout, change_fill_table, change_random_loot, fill_items,
};
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, on};
use crate::data_changes::Key;
use crate::host::Loader;

/// The loader's patches of the loot tables the changes reach: the random
/// loot table, and each container fill table that names a changed item.
pub(super) fn loot_patches(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
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
    let (Ok(layout), Some(archive), Some((random, fill))) =
        (LootLayout::resolve(&facts), crate::archive::game(), hashes)
    else {
        crate::log!(
            WARN,
            "loot changes are off: this build does not reflect the loot tables, or the game's archive does not read"
        );
        return Vec::new();
    };
    let items: &'static FxHashMap<u32, Guid> = Box::leak(Box::new(items));
    let mut patches = Vec::new();
    let mut add = |key: ResourceKey, fill: bool| {
        let patch: &'static LootPatch = Box::leak(Box::new(LootPatch {
            layout,
            items,
            fill,
            table: Guid(key.guid),
        }));
        let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
        patches.push((key, build_loot as enshwick::abi::ResourcePatchFn, user));
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
}

/// The loader's patch of a loot table: the base with the values the
/// changes of the mods still on give its items.
unsafe extern "system" fn build_loot(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `LootPatch` `loot_patches` leaked, which lives
    // for the process.
    let patch = unsafe { &*user.cast::<LootPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let sealed = CHANGES.sealed.get()?;
        let merged = sealed.merged();
        let value = |id: enshwick::ItemId, field: Field, base: f64| {
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
        if patch.fill {
            change_fill_table(&patch.layout, &bytes, &value)
        } else {
            change_random_loot(&patch.layout, &bytes, &value)
        }
    }));
    let Ok(Some((built, _))) = built else {
        return Status::REFUSED;
    };
    // The game reads the resource for the life of the process.
    let built: &'static [u8] = Box::leak(built.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local `*const u8`,
    // writable for the call.
    unsafe { out.write(built.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(built.len()) };
    Status::OK
}
