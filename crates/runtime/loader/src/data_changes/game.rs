//! Declared changes in the running game: the loader's own patch of each
//! changed item, the conflict reports once every mod ran its init, and the
//! hold a raised stack size sets on the side that keeps the world.

mod containers;
mod holds;
mod items;
mod loot;
mod recipes;

use core::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;
use enshwick_core::abi::{Level, ResourceKey, Status};
use enshwick_core::content::Guid;
use enshwick_core::content::changes::{Change, Entry, Field};
use enshwick_core::content::containers::{ContainerLayout, container_slots};
use enshwick_core::content::loot_tables::LootLayout;
use enshwick_core::content::recipes::{RECIPE_REGISTRY, RecipeLayout};
use enshwick_gamedata::catalogue;
use enshwick_gamedata::reflect::Reflection;
use rustc_hash::FxHashMap;

use crate::host::Loader;
use crate::mod_index::ModIndex;
use enshwick_base::Relock;

use self::containers::container_patches;
pub(crate) use self::holds::world_event;
use self::items::{STATS, build_item, item_offsets};
use self::loot::loot_patches;
use self::recipes::{menu_patch, recipe_patch, stations, vanilla_recipes};
use super::{
    Conflict, Declared, ItemOffsets, Kept, Merged, Pending, Picked, Raised, Selectable,
    TraitOffsets, UNSET, expand, grouped, merge, traits_of,
};
use enshwick_core::abi::ChangeId;
use enshwick_core::content::changes::{ItemTraits, ItemsChange, RecipesChange, Value};

/// The domain's state: changes as mods declare them, what the end of init
/// sealed, and the stack sizes raised so far.
struct Changes {
    declared: Mutex<Vec<Pending>>,
    /// The changes mods set later: owner, number and field, in the order
    /// declared.
    live: Mutex<Vec<(ModIndex, ChangeId, Field)>>,
    sealed: OnceLock<Sealed>,
    raised: Mutex<Option<FxHashMap<ModIndex, Raised>>>,
    /// The stack sizes kept for saved worlds, read when the changes seal.
    kept: Mutex<Option<Kept>>,
    /// Each patched item's game stack size and the size served now.
    stacks: Mutex<Option<FxHashMap<Guid, (u16, u16)>>>,
}

/// What the end of init fixed for the process.
struct Sealed {
    loader: &'static Loader,
    /// Every change in force, in load order.
    declared: Mutex<Vec<Declared>>,
    merged: ArcSwap<Merged>,
    /// The names packs gave the changed entries.
    labels: FxHashMap<Guid, String>,
    /// For each item with changed container slots, the place of its last
    /// such change in load order.
    slots_order: FxHashMap<Guid, usize>,
    /// The resources the loader patches, filled once its patches are made.
    patched: OnceLock<Vec<ResourceKey>>,
    /// The template each item with changed container slots places.
    templates: OnceLock<FxHashMap<Guid, ResourceKey>>,
    /// The items whose loot changed at start.
    loot_items: OnceLock<Vec<Guid>>,
}

impl Sealed {
    /// The merged changes in force now.
    fn merged(&self) -> Arc<Merged> {
        self.merged.load_full()
    }
}

static CHANGES: Changes = Changes {
    declared: Mutex::new(Vec::new()),
    live: Mutex::new(Vec::new()),
    sealed: OnceLock::new(),
    raised: Mutex::new(None),
    kept: Mutex::new(None),
    stacks: Mutex::new(None),
};

/// What the loader's patch of one item needs: the item and where its
/// values sit.
struct ItemPatch {
    item: Guid,
    offsets: &'static ItemOffsets,
}

/// Records mod `owner`'s `change` from `source`; refused once init is over.
pub(crate) fn declare(owner: ModIndex, change: Change, source: String) -> Status {
    if CHANGES.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    CHANGES.declared.relock().push(Pending::One(Declared {
        owner,
        change,
        source,
        live: None,
    }));
    Status::OK
}

/// Records mod `owner`'s `change` of every item a selection picks, from
/// `source`; refused once init is over.
pub(crate) fn declare_items(owner: ModIndex, change: ItemsChange, source: String) -> Status {
    if CHANGES.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    CHANGES.declared.relock().push(Pending::Each {
        owner,
        change,
        source,
        live: None,
    });
    Status::OK
}

/// Records mod `owner`'s change of `target`, whose value it sets later;
/// its number. `TOO_LATE` once init is over.
///
/// # Errors
///
/// The status the entry answers.
pub(crate) fn declare_change(
    owner: ModIndex,
    target: LiveTarget,
    field: Field,
) -> Result<ChangeId, Status> {
    let mut declared = CHANGES.declared.relock();
    if CHANGES.sealed.get().is_some() {
        return Err(Status::TOO_LATE);
    }
    let mut live = CHANGES.live.relock();
    let id = ChangeId(u32::try_from(live.len() + 1).map_err(|_| Status::BAD_ARGS)?);
    let pending = match target {
        LiveTarget::Entry(entry, source) => Pending::One(Declared {
            owner,
            change: Change::new(entry, field, UNSET),
            source,
            live: Some(id),
        }),
        LiveTarget::Items(items, source) => Pending::Each {
            owner,
            change: ItemsChange::new(items, field, UNSET),
            source,
            live: Some(id),
        },
    };
    declared.push(pending);
    live.push((owner, id, field));
    Ok(id)
}

/// What a change a mod sets later reaches, with its source.
pub(crate) enum LiveTarget {
    /// One entry.
    Entry(Guid, String),
    /// Every vanilla item a selection picks.
    Items(enshwick_core::content::changes::Items, String),
}

/// Puts `value` in force for mod `owner`'s change `id`, or takes the
/// mod's value out for `None`, and builds what it reaches again.
pub(crate) fn set_change(owner: ModIndex, id: ChangeId, value: Option<Value>) -> Status {
    let field = CHANGES
        .live
        .relock()
        .iter()
        .find(|(o, i, _)| *o == owner && *i == id)
        .map(|(_, _, field)| *field);
    let Some(field) = field else {
        return Status::BAD_ARGS;
    };
    if value.is_some_and(|value| field.check(value).is_err()) {
        return Status::BAD_ARGS;
    }
    let sealed = {
        let mut pending = CHANGES.declared.relock();
        let Some(sealed) = CHANGES.sealed.get() else {
            super::set_pending(&mut pending, owner, id, value);
            return Status::OK;
        };
        sealed
    };
    let rebuilt = {
        let mut declared = sealed.declared.relock();
        let next = super::set_live(&declared, owner, id, value);
        apply(sealed, &mut declared, next)
    }
    .and_then(|patched| rebuild(&patched));
    match rebuilt {
        Ok(_) => Status::OK,
        Err(why) => {
            crate::log::write(
                Level::WARN,
                &self::id(sealed.loader, owner),
                &format!("{field}: the new value is not in force and takes a restart: {why}"),
            );
            Status::TOO_LATE
        }
    }
}

/// Whether some mod declared a change it sets later, so the resources the
/// changes patch keep their base for a rebuild.
pub(crate) fn has_live() -> bool {
    !CHANGES.live.relock().is_empty()
}

/// Every vanilla item with what a selection reads of it, from the game's
/// archive; none when the build does not reflect a value it needs or the
/// archive does not read.
fn vanilla_items(loader: &Loader) -> Vec<(Guid, ItemTraits)> {
    let items = crate::archive::game().and_then(|archive| items_in(&loader.reflection, archive));
    items.unwrap_or_else(|| {
        crate::log!(
            WARN,
            "changes of selected items are off: this build does not reflect what they read, or the game's archive does not read"
        );
        Vec::new()
    })
}

/// Every item of `archive` with what a selection reads of it, by the
/// offsets `reflection` gives; `None` when it lacks one.
fn items_in(
    reflection: &Reflection<'_>,
    archive: &crate::archive::Archive,
) -> Option<Vec<(Guid, ItemTraits)>> {
    let at = |id: &str| catalogue::constant(id, reflection).and_then(|at| usize::try_from(at).ok());
    let backpack = at(catalogue::ids::ITEM_INFO_PID_ENTITY)
        .zip(at(catalogue::ids::ITEM_INFO_EQUIPMENT_SLOT))
        .zip(
            catalogue::constant(catalogue::ids::EQUIPMENT_SLOT_TYPE_BACKPACK, reflection)
                .and_then(|value| u8::try_from(value).ok()),
        )
        .map(|((pid, slot), value)| (pid, slot, value));
    let offsets = TraitOffsets {
        max_stack: at(catalogue::ids::ITEM_INFO_MAX_STACK)?,
        category: at(catalogue::ids::ITEM_INFO_CATEGORY)?,
        flags: at(catalogue::ids::ITEM_INFO_FLAGS)?,
        placed: at(catalogue::ids::ITEM_INFO_PLACED_ENTITY)?,
        backpack,
    };
    let layout = ContainerLayout::resolve(&catalogue::Reflected(reflection)).ok();
    let slots = |template: Guid| {
        let layout = layout.as_ref()?;
        let data = archive.resource(&template.key_by_hash(crate::catalogue::types::TEMPLATE))?;
        container_slots(layout, &data)
    };
    let mut items: Vec<(Guid, ItemTraits)> = archive
        .keys_of(crate::catalogue::types::ITEM_INFO)
        .into_iter()
        .filter_map(|key| {
            let bytes = archive.resource(&key)?;
            Some((Guid(key.guid), traits_of(&bytes, offsets, slots)?))
        })
        .collect();
    items.sort_unstable_by_key(|(item, _)| item.0);
    Some(items)
}

/// Whether the loader follows the world for changes: some mod declared one,
/// or a pack holds the game and may need its hold released.
pub(crate) fn wanted(loader: &Loader) -> bool {
    let declared = CHANGES.sealed.get().map_or_else(
        || !CHANGES.declared.relock().is_empty(),
        |sealed| !sealed.merged().is_empty(),
    );
    // Kept sizes that do not read may be there: wanted, so nothing cuts them.
    let kept = CHANGES.kept.relock().as_ref().map_or_else(
        || Kept::load(&loader_dir(loader)).map_or(true, |kept| !kept.is_empty()),
        |kept| !kept.is_empty(),
    );
    declared
        || kept
        || enshwick_core::state::holds(&loader.mods_dir)
            .iter()
            .any(|(id, _)| loader.mods.iter().any(|m| m.pack && &m.id == id))
}

/// Whether mod `index` is on.
fn on(loader: &Loader, index: ModIndex) -> bool {
    loader
        .mods
        .get(index.get())
        .is_some_and(|record| !record.off.load(std::sync::atomic::Ordering::Acquire))
}

/// The id of mod `index`.
fn id(loader: &Loader, index: ModIndex) -> String {
    loader
        .mods
        .get(index.get())
        .map_or_else(|| index.to_string(), |record| record.id.clone())
}

/// Whether mod `a` requires mod `b`.
fn requires(loader: &Loader, a: ModIndex, b: ModIndex) -> bool {
    let (Some(a), Some(b)) = (loader.mods.get(a.get()), loader.mods.get(b.get())) else {
        return false;
    };
    a.requires(&b.id)
}

/// Reports each set another mod overrides in both mods' logs and the
/// conflict report.
fn report(loader: &Loader, conflicts: &[Conflict]) {
    let mut lines = Vec::new();
    for (conflict, more) in grouped(conflicts) {
        let mut line = conflict.describe(|m| id(loader, m));
        if more > 0 {
            line = format!("{line}, and the same for {more} more items");
        }
        let level = if conflict.intended {
            line.push_str(", on purpose: it requires that mod");
            Level::INFO
        } else {
            Level::WARN
        };
        for owner in [conflict.winner.owner, conflict.overridden.owner] {
            crate::log::write(level, &id(loader, owner), &line);
        }
        lines.push(line);
    }
    crate::conflicts::set_fields(lines);
}

/// The items whose own values some change of `merged` changes: every item
/// field but the slots of the container it places.
fn edited(merged: &Merged) -> Vec<Guid> {
    merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| merged.keys(*item).iter().any(|k| k.field != Field::Slots))
        .collect()
}

/// Merges every declared change, reports the overridden sets, and returns
/// the loader's patches, one per changed item: its key and the patch with
/// its pointer. Called once, when the served resources are sealed.
pub(crate) fn serve(
    loader: &'static Loader,
) -> Vec<(
    ResourceKey,
    enshwick_core::abi::ResourcePatchFn,
    *mut c_void,
)> {
    let mut open = CHANGES.declared.relock();
    let pending = std::mem::take(&mut *open);
    let (declared, picked) = expand_from_game(loader, pending);
    for p in picked {
        let level = if p.count == 0 {
            Level::WARN
        } else {
            Level::INFO
        };
        crate::log::write(
            level,
            &id(loader, p.owner),
            &format!(
                "{} {}: changes {} vanilla {}",
                p.field,
                p.source,
                p.count,
                if p.field.entry() == Entry::Recipe {
                    "recipes"
                } else {
                    "items"
                }
            ),
        );
    }
    let (merged, conflicts) = merge(&declared, |a, b| requires(loader, a, b));
    report(loader, &conflicts);
    let labels: FxHashMap<Guid, String> = declared
        .iter()
        .filter_map(|d| label(&d.source).map(|l| (d.change.entry, l.to_owned())))
        .collect();
    let offsets = item_offsets(loader);
    let kept = Kept::load(&loader_dir(loader)).unwrap_or_else(|why| {
        crate::log!(WARN, "kept stack sizes do not read: {why}");
        Kept::default()
    });
    let mut items = edited(&merged);
    items.extend(kept.items().map(|(item, _)| item));
    items.sort_unstable_by_key(|item| item.0);
    items.dedup();
    if !kept.is_empty() {
        crate::log!(
            INFO,
            "{} items keep a stack size above the game's, so saved stacks are never cut ({})",
            kept.items().count(),
            super::KEPT_FILE
        );
    }
    *CHANGES.kept.relock() = Some(kept);
    let slots_order: FxHashMap<Guid, usize> = declared
        .iter()
        .enumerate()
        .filter(|(_, d)| d.change.field == Field::Slots)
        .map(|(at, d)| (d.change.entry, at))
        .collect();
    let sealed = CHANGES.sealed.get_or_init(|| Sealed {
        loader,
        declared: Mutex::new(declared),
        merged: ArcSwap::from_pointee(merged),
        labels,
        slots_order,
        patched: OnceLock::new(),
        templates: OnceLock::new(),
        loot_items: OnceLock::new(),
    });
    drop(open);
    let mut patches = Vec::new();
    if !items.is_empty() {
        crate::log!(INFO, "{} items get changed values", items.len());
    }
    let offsets: &'static ItemOffsets = Box::leak(Box::new(offsets));
    for item in items {
        let patch: &'static ItemPatch = Box::leak(Box::new(ItemPatch { item, offsets }));
        let key = item.key_by_hash(crate::catalogue::types::ITEM_INFO);
        let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
        patches.push((key, build_item as enshwick_core::abi::ResourcePatchFn, user));
    }
    if let Some(patch) = recipe_patch(loader) {
        patches.push(patch);
    }
    patches.extend(menu_patch(loader));
    patches.extend(container_patches(loader));
    patches.extend(loot_patches(loader));
    let _first = sealed
        .patched
        .set(patches.iter().map(|(key, _, _)| *key).collect());
    patches
}

/// Expands `pending` over the vanilla items and recipes the game's data
/// holds, reading only what its selections need.
fn expand_from_game(loader: &Loader, pending: Vec<Pending>) -> (Vec<Declared>, Vec<Picked>) {
    let selectable = Selectable {
        items: if pending.iter().any(|p| matches!(p, Pending::Each { .. })) {
            vanilla_items(loader)
        } else {
            Vec::new()
        },
        recipes: if pending.iter().any(|p| matches!(p, Pending::Recipes { .. })) {
            vanilla_recipes(loader)
        } else {
            Vec::new()
        },
    };
    expand(pending, &selectable)
}

/// Records mod `owner`'s `change` of every recipe a selection picks, from
/// `source`; refused once init is over.
pub(crate) fn declare_recipes(owner: ModIndex, change: RecipesChange, source: String) -> Status {
    if CHANGES.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    CHANGES.declared.relock().push(Pending::Recipes {
        owner,
        change,
        source,
    });
    Status::OK
}

/// Puts `next` in force as every change: merges it and reports the
/// overridden sets; the resources the loader patches, for [`rebuild`]
/// once `declared` is unlocked.
///
/// # Errors
///
/// A change that reaches an entry the loader did not patch at start, which
/// takes a restart; `declared` then stays as it was.
fn apply(
    sealed: &Sealed,
    declared: &mut Vec<Declared>,
    next: Vec<Declared>,
) -> Result<Vec<ResourceKey>, ApplyChangesError> {
    let loader = sealed.loader;
    let patched = sealed.patched.get().cloned().unwrap_or_default();
    let (merged, conflicts) = merge(&next, |a, b| requires(loader, a, b));
    if let Some(item) = edited(&merged)
        .into_iter()
        .find(|item| !patched.contains(&item.key_by_hash(crate::catalogue::types::ITEM_INFO)))
    {
        return Err(ApplyChangesError::Values { item });
    }
    let recipes = RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY);
    if !merged.entries(Entry::Recipe).is_empty() && !patched.contains(&recipes) {
        return Err(ApplyChangesError::Recipes);
    }
    let stations = |merged: &Merged| stations(merged, |_| true);
    if stations(&merged) != stations(&sealed.merged()) {
        return Err(ApplyChangesError::Station);
    }
    let templates = sealed.templates.get();
    let slotted = merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| merged.keys(*item).iter().any(|k| k.field == Field::Slots));
    for item in slotted {
        if !templates.is_some_and(|t| t.contains_key(&item)) {
            return Err(ApplyChangesError::Slots { item });
        }
    }
    let looted = sealed.loot_items.get();
    for item in merged.loot_items() {
        if !looted.is_some_and(|items| items.contains(&item)) {
            return Err(ApplyChangesError::Loot { item });
        }
    }
    let fill = catalogue::type_hash(catalogue::ids::LOOT_FILL_TABLE);
    for (table, _) in merged.table_items() {
        let key = fill.map(|hash| table.key_by_hash(hash));
        if !key.is_some_and(|key| patched.contains(&key)) {
            return Err(ApplyChangesError::Table { table });
        }
    }
    report(loader, &conflicts);
    sealed.merged.store(Arc::new(merged));
    *declared = next;
    Ok(patched)
}

/// Builds each of `patched` again in place from its base; how many.
///
/// # Errors
///
/// The resources that did not build again: their base was not kept, or
/// their length changed, such as an ingredient taken out or added.
fn rebuild(patched: &[ResourceKey]) -> Result<usize, ApplyChangesError> {
    let mut failed = Vec::new();
    for key in patched {
        let status = crate::content::rebuild(ModIndex::LOADER, *key);
        if !status.is_ok() {
            failed.push(*key);
        }
    }
    if failed.is_empty() {
        Ok(patched.len())
    } else {
        Err(ApplyChangesError::Rebuild { failed })
    }
}

/// Why a change of values set while the game runs takes a restart.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ApplyChangesError {
    /// An item's values change, which the loader did not patch at start.
    #[error("the values of {item} change for the first time, which takes a restart")]
    Values { item: Guid },
    /// Recipes change, which the loader did not patch at start.
    #[error("recipes change for the first time, which takes a restart")]
    Recipes,
    /// A recipe moves to another station.
    #[error(
        "a recipe moves to another station, which takes a restart: the menus are built at start"
    )]
    Station,
    /// A container's slots change, whose template the loader did not keep.
    #[error("the container slots of {item} change for the first time, which takes a restart")]
    Slots { item: Guid },
    /// An item's loot changes, which the loader did not patch at start.
    #[error("the loot of {item} changes for the first time, which takes a restart")]
    Loot { item: Guid },
    /// A loot table changes, which the loader did not patch at start.
    #[error("the loot table {table} changes for the first time, which takes a restart")]
    Table { table: Guid },
    /// Resources did not build again in place.
    #[error("{} resources do not build again in place: {}", failed.len(), shown(failed))]
    Rebuild { failed: Vec<ResourceKey> },
}

/// `keys` as `<guid>:<type hash>`, joined.
fn shown(keys: &[ResourceKey]) -> String {
    keys.iter()
        .map(|key| format!("{}:{:08x}", Guid(key.guid), key.type_hash))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The pack's own name for an entry, from a source such as
/// `items.toml:12 (Wood_Log)`.
fn label(source: &str) -> Option<&str> {
    let (_, rest) = source.split_once(" (")?;
    rest.strip_suffix(')')
}

/// The loader's own data folder.
fn loader_dir(loader: &Loader) -> std::path::PathBuf {
    loader.mods_dir.join(crate::settings::LOADER_ID)
}

/// Whether the running build reflects what `field` changes.
pub(crate) fn reachable(loader: &Loader, field: Field) -> bool {
    let constant = |id: &str| catalogue::constant(id, &loader.reflection).is_some();
    let recipes = || RecipeLayout::resolve(&catalogue::Reflected(&loader.reflection)).is_ok();
    match field {
        Field::MaxStackSize => constant(catalogue::ids::ITEM_INFO_MAX_STACK),
        Field::Output | Field::Cost | Field::Ingredient | Field::Station => recipes(),
        Field::CraftingDuration => recipes() && constant(catalogue::ids::RECIPE_DURATION),
        Field::StackSize | Field::Count | Field::SpawnProbability | Field::Weight => {
            LootLayout::resolve(&catalogue::Reflected(&loader.reflection)).is_ok()
        }
        Field::Armor => {
            constant(catalogue::ids::ITEM_INFO_ARMOR)
                && constant(catalogue::ids::ITEM_INFO_ARMOR_SET)
                && item_offsets(loader).armor_count > 0
        }
        field if STATS.iter().any(|(f, _, _)| *f == field) => STATS
            .iter()
            .filter(|(f, _, _)| *f == field)
            .all(|&(_, value, set)| constant(value) && constant(set)),
        Field::Slots => {
            ContainerLayout::resolve(&catalogue::Reflected(&loader.reflection)).is_ok()
                && constant(catalogue::ids::ITEM_INFO_PLACED_ENTITY)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On each executable `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name, with
    /// the archive beside it (each skipped without it), the selections by
    /// kind pick vanilla items.
    #[test]
    fn each_kind_of_selection_picks_vanilla_items_on_each_executable() {
        use enshwick_core::content::changes::Items;
        use enshwick_gamedata::reflect::{Image, Layout};
        for var in ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"] {
            let Ok(path) = std::env::var(var) else {
                continue;
            };
            let path = std::path::Path::new(&path);
            let bytes = std::fs::read(path).unwrap();
            let image = Image::parse(&bytes, Layout::File, None).unwrap();
            let reflection = Reflection::scan(&image);
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap();
            // A kept executable alone, without its archive, is skipped.
            let Some(archive) = path
                .parent()
                .and_then(|folder| crate::archive::Archive::open(folder, stem))
            else {
                continue;
            };
            let items = items_in(&reflection, &archive).expect(var);
            for (kind, field) in [
                (Items::Stackable, Field::MaxStackSize),
                (Items::Containers, Field::Slots),
                (Items::Backpacks, Field::Slots),
            ] {
                let picked = items.iter().filter(|(_, t)| kind.picks(field, t)).count();
                assert!(
                    picked > 0,
                    "{var}: {kind:?} picks no item of {}",
                    items.len()
                );
            }
        }
    }

    #[test]
    fn a_label_is_the_name_in_parentheses() {
        assert_eq!(label("items.toml:12 (Wood_Log)"), Some("Wood_Log"));
        assert_eq!(label("items.toml:12"), None);
    }
}
