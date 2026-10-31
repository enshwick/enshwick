//! Declared changes in the running game: the loader's own patch of each
//! changed item, the conflict reports once every mod ran its init, and the
//! hold a raised stack size sets on the side that keeps the world.

use core::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;
use enshwick::abi::{Event, Level, ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::{Change, Entry, Field};
use enshwick::content::containers::{ContainerLayout, container_slots, resize_container};
use enshwick::content::loot_tables::{
    LootLayout, change_fill_table, change_random_loot, fill_items,
};
use enshwick::content::menu::{MenuLayout, UI_BUNDLE, move_recipes};
use enshwick::content::recipes::{
    RECIPE_REGISTRY, RecipeChanges, RecipeLayout, change_recipes_in, recipe_workshops,
};
use enshwick::content::workshops::WorkshopId;
use enshwick::state::Hold;
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use crate::host::Loader;
use crate::item_kinds::ItemKindLayout;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

use super::{
    Conflict, Declared, HoldStep, ItemOffsets, Kept, Key, Merged, Pending, Picked, Raised,
    Selectable, TraitOffsets, UNSET, credited, edit_item, expand, grouped, grown_slots,
    hold_reason, hold_step, merge, served, template_slots, traits_of,
};
use enshwick::abi::ChangeId;
use enshwick::content::changes::{ItemTraits, ItemsChange, RecipeTraits, RecipesChange, Value};
use enshwick::content::recipes::RecipeKind;

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

/// What the end of init fixed for the process; the changes themselves
/// change only in a development reload.
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
    Items(enshwick::content::changes::Items, String),
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
    let at = |id: &str| {
        catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let backpack = at(catalogue::ids::ITEM_INFO_PID_ENTITY)
        .zip(at(catalogue::ids::ITEM_INFO_EQUIPMENT_SLOT))
        .zip(
            catalogue::constant(catalogue::ids::EQUIPMENT_SLOT_BACKPACK, &loader.reflection)
                .and_then(|value| u8::try_from(value).ok()),
        )
        .map(|((pid, slot), value)| (pid, slot, value));
    let offsets = (|| {
        Some(TraitOffsets {
            max_stack: at(catalogue::ids::ITEM_INFO_MAX_STACK)?,
            category: at(catalogue::ids::ITEM_INFO_CATEGORY)?,
            flags: at(catalogue::ids::ITEM_INFO_FLAGS)?,
            placed: at(catalogue::ids::ITEM_INFO_PLACED_ENTITY)?,
            backpack,
        })
    })();
    let (Some(offsets), Some(archive)) = (offsets, crate::archive::game()) else {
        crate::log!(
            WARN,
            "changes of selected items are off: this build does not reflect what they read, or the game's archive does not read"
        );
        return Vec::new();
    };
    let layout = ContainerLayout::resolve(&catalogue::Reflected(&loader.reflection)).ok();
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
    items
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
        || enshwick::state::holds(&loader.data_dir)
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

/// Each stat with the catalogue ids of its value and of the flag that says
/// an item has it.
const STATS: [(Field, &str, &str); 6] = [
    (
        Field::DamageMod,
        catalogue::ids::ITEM_INFO_DAMAGE_MOD,
        catalogue::ids::ITEM_INFO_DAMAGE_SET,
    ),
    (
        Field::SpeedMod,
        catalogue::ids::ITEM_INFO_SPEED_MOD,
        catalogue::ids::ITEM_INFO_DAMAGE_SET,
    ),
    (
        Field::Armor,
        catalogue::ids::ITEM_INFO_ARMOR,
        catalogue::ids::ITEM_INFO_ARMOR_SET,
    ),
    (
        Field::BurnTime,
        catalogue::ids::ITEM_INFO_BURN_TIME,
        catalogue::ids::ITEM_INFO_FUEL_SET,
    ),
    (
        Field::EnergyOutput,
        catalogue::ids::ITEM_INFO_ENERGY_OUTPUT,
        catalogue::ids::ITEM_INFO_FUEL_SET,
    ),
    (
        Field::ComfortAmount,
        catalogue::ids::ITEM_INFO_COMFORT_AMOUNT,
        catalogue::ids::ITEM_INFO_COMFORT_SET,
    ),
];

/// Where an item's values sit on this build.
fn item_offsets(loader: &Loader) -> ItemOffsets {
    let at = |id: &str| {
        catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let armor_count = loader
        .reflection
        .type_info(catalogue::type_name(catalogue::ids::ARMOR_DISTRIBUTION))
        .ok()
        .and_then(|info| usize::try_from(info.size() / 4).ok())
        .unwrap_or(0);
    ItemOffsets {
        max_stack: at(catalogue::ids::ITEM_INFO_MAX_STACK),
        stats: STATS
            .iter()
            .filter_map(|&(field, value, set)| Some((field, at(value)?, at(set)?)))
            .collect(),
        armor_count,
    }
}

/// Merges every declared change, reports the overridden sets, and returns
/// the loader's patches, one per changed item: its key and the patch with
/// its pointer. Called once, when the served resources are sealed.
pub(crate) fn serve(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
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
        patches.push((key, build_item as enshwick::abi::ResourcePatchFn, user));
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

/// Takes `changes` and the changes of every item and recipe the selections
/// pick as mod `owner`'s changes from now on, for a development reload:
/// merges every change again, reports the overridden sets, and builds each
/// resource the loader patches again in place; how many it rebuilt.
///
/// # Errors
///
/// What takes a restart: a change of an entry the loader did not patch at
/// start, a recipe's station, or a rebuild that changes a resource's
/// length, such as an ingredient taken out or added.
pub(crate) fn reload(
    owner: ModIndex,
    changes: Vec<(Change, String)>,
    items: Vec<(ItemsChange, String)>,
    recipes: Vec<(RecipesChange, String)>,
) -> Result<usize, String> {
    let sealed = CHANGES
        .sealed
        .get()
        .ok_or_else(|| "no changes were in force at start".to_owned())?;
    let one = changes.into_iter().map(|(change, source)| {
        Pending::One(Declared {
            owner,
            change,
            source,
            live: None,
        })
    });
    let each = items.into_iter().map(|(change, source)| Pending::Each {
        owner,
        change,
        source,
        live: None,
    });
    let of_recipes = recipes
        .into_iter()
        .map(|(change, source)| Pending::Recipes {
            owner,
            change,
            source,
        });
    let pending: Vec<Pending> = one.chain(each).chain(of_recipes).collect();
    let (mine, _) = expand_from_game(sealed.loader, pending);
    let patched = {
        let mut declared = sealed.declared.relock();
        let next = super::replace_owner(&declared, owner, mine);
        apply(sealed, &mut declared, next)?
    };
    rebuild(&patched)
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

/// Every vanilla recipe with the kind of what it makes and its workshop,
/// from the game's archive; none when the build does not reflect the
/// recipes or the items' equipment slots, or the archive does not read.
fn vanilla_recipes(loader: &Loader) -> Vec<(Guid, RecipeTraits)> {
    let off = || {
        crate::log!(
            WARN,
            "changes of selected recipes are off: this build does not reflect what they read, or the game's archive does not read"
        );
        Vec::new()
    };
    let facts = catalogue::Reflected(&loader.reflection);
    let (Ok(layout), Some(archive), Some(items)) = (
        RecipeLayout::resolve(&facts),
        crate::archive::game(),
        ItemKindLayout::from_reflection(&loader.reflection),
    ) else {
        return off();
    };
    let Some(index) = archive
        .resource(&RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY))
        .and_then(|registry| crate::recipes::Index::read(&layout, &registry))
    else {
        return off();
    };
    let kinds: FxHashMap<u32, RecipeKind> = archive
        .keys_of(crate::catalogue::types::ITEM_INFO)
        .into_iter()
        .filter_map(|key| {
            let bytes = archive.resource(&key)?;
            let read = |at: usize, out: &mut [u8]| {
                if let Some(from) = bytes.get(at..at + out.len()) {
                    out.copy_from_slice(from);
                }
            };
            let (id, kind) = items.item(bytes.len(), read)?;
            Some((id, RecipeKind::from(kind)))
        })
        .collect();
    index
        .recipes()
        .iter()
        .map(|recipe| {
            let kind = recipe
                .outputs
                .first()
                .and_then(|item| kinds.get(item))
                .copied()
                .unwrap_or(RecipeKind::Other);
            (
                recipe.guid,
                RecipeTraits {
                    kind,
                    workshop: recipe.workshop,
                },
            )
        })
        .collect()
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
) -> Result<Vec<ResourceKey>, String> {
    let loader = sealed.loader;
    let patched = sealed.patched.get().cloned().unwrap_or_default();
    let (merged, conflicts) = merge(&next, |a, b| requires(loader, a, b));
    if let Some(item) = edited(&merged)
        .into_iter()
        .find(|item| !patched.contains(&item.key_by_hash(crate::catalogue::types::ITEM_INFO)))
    {
        return Err(format!(
            "the values of {item} change for the first time, which takes a restart"
        ));
    }
    let recipes = RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY);
    if !merged.entries(Entry::Recipe).is_empty() && !patched.contains(&recipes) {
        return Err("recipes change for the first time, which takes a restart".to_owned());
    }
    let stations = |merged: &Merged| stations(merged, |_| true);
    if stations(&merged) != stations(&sealed.merged()) {
        return Err(
            "a recipe moves to another station, which takes a restart: the menus are built at start"
                .to_owned(),
        );
    }
    let templates = sealed.templates.get();
    let slotted = merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| merged.keys(*item).iter().any(|k| k.field == Field::Slots));
    for item in slotted {
        if !templates.is_some_and(|t| t.contains_key(&item)) {
            return Err(format!(
                "the container slots of {item} change for the first time, which takes a restart"
            ));
        }
    }
    let looted = sealed.loot_items.get();
    for item in merged.loot_items() {
        if !looted.is_some_and(|items| items.contains(&item)) {
            return Err(format!(
                "the loot of {item} changes for the first time, which takes a restart"
            ));
        }
    }
    let fill = catalogue::type_hash(catalogue::ids::LOOT_FILL_TABLE);
    for (table, _) in merged.table_items() {
        let key = fill.map(|hash| table.key_by_hash(hash));
        if !key.is_some_and(|key| patched.contains(&key)) {
            return Err(format!(
                "the loot table {table} changes for the first time, which takes a restart"
            ));
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
fn rebuild(patched: &[ResourceKey]) -> Result<usize, String> {
    let mut failed = Vec::new();
    for key in patched {
        let status = crate::content::rebuild(ModIndex::LOADER, *key);
        if !status.is_ok() {
            failed.push(format!("{}:{:08x}", Guid(key.guid), key.type_hash));
        }
    }
    if failed.is_empty() {
        Ok(patched.len())
    } else {
        Err(format!(
            "{} resources do not build again in place: {}",
            failed.len(),
            failed.join(", ")
        ))
    }
}

/// The loader's patches of the loot tables the changes reach: the random
/// loot table, and each container fill table that names a changed item.
fn loot_patches(
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

/// The loader's patches of the templates the items with changed container
/// slots place: one per template, for every item that places it.
fn container_patches(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let Some(sealed) = CHANGES.sealed.get() else {
        return Vec::new();
    };
    let merged = sealed.merged();
    let items: Vec<Guid> = merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| merged.keys(*item).iter().any(|k| k.field == Field::Slots))
        .collect();
    if items.is_empty() {
        return Vec::new();
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let at = |id: &str| {
        catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
    };
    let placed = at(catalogue::ids::ITEM_INFO_PLACED_ENTITY);
    let own = at(catalogue::ids::ITEM_INFO_PID_ENTITY);
    let (Ok(layout), Some(placed), Some(archive)) = (
        ContainerLayout::resolve(&facts),
        placed,
        crate::archive::game(),
    ) else {
        crate::log!(
            WARN,
            "container slot changes are off: this build does not reflect containers, or the game's archive does not read"
        );
        return Vec::new();
    };
    let mut templates: Vec<(Guid, Vec<Guid>)> = Vec::new();
    for item in items {
        let info = archive.resource(&item.key_by_hash(crate::catalogue::types::ITEM_INFO));
        let holds = |template: &Guid| {
            archive
                .resource(&template.key_by_hash(crate::catalogue::types::TEMPLATE))
                .is_some_and(|data| container_slots(&layout, &data).is_some())
        };
        let template = info.as_deref().and_then(|info| {
            [Some(placed), own]
                .into_iter()
                .flatten()
                .filter_map(|at| info.get(at..at + 16)?.try_into().ok().map(Guid))
                .filter(|template| template.0 != [0; 16])
                .find(holds)
        });
        let Some(template) = template else {
            crate::log!(
                WARN,
                "container slots of {item} are not changed: it is no vanilla item that places a container or carries one"
            );
            continue;
        };
        match templates.iter_mut().find(|(t, _)| *t == template) {
            Some((_, items)) => items.push(item),
            None => templates.push((template, vec![item])),
        }
    }
    let mut patches = Vec::new();
    let mut placed_by = FxHashMap::default();
    for (template, mut items) in templates {
        items.sort_by_key(|item| sealed.slots_order.get(item).copied().unwrap_or(0));
        crate::log!(
            INFO,
            "container slots change on template {template}, which {} places, and so for every item that places it",
            items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        let patch: &'static ContainerPatch = Box::leak(Box::new(ContainerPatch { items, layout }));
        let key = template.key_by_hash(crate::catalogue::types::TEMPLATE);
        for item in &patch.items {
            placed_by.insert(*item, key);
        }
        let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
        patches.push((key, build_container as enshwick::abi::ResourcePatchFn, user));
    }
    let _first = sealed.templates.set(placed_by);
    patches
}

/// What the loader's patch of a container's template needs: the changed
/// items that place it, in load order, and the layout.
struct ContainerPatch {
    items: Vec<Guid>,
    layout: ContainerLayout,
}

/// The loader's patch of a container's template: the base with the slots
/// the changes of the mods still on give it, never fewer than it has.
unsafe extern "system" fn build_container(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `ContainerPatch` `container_patches` leaked,
    // which lives for the process.
    let patch = unsafe { &*user.cast::<ContainerPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| resized(patch, &bytes)));
    let Ok(Some(built)) = built else {
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

/// The template `bytes` with the slots its changed items give it.
fn resized(patch: &ContainerPatch, bytes: &[u8]) -> Option<Vec<u8>> {
    let sealed = CHANGES.sealed.get()?;
    let Some(old) = container_slots(&patch.layout, bytes) else {
        crate::log!(
            WARN,
            "container slots of {} are not changed: the template has no inventory of its own",
            patch
                .items
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Some(bytes.to_vec());
    };
    let values: Vec<(Guid, f64)> = patch
        .items
        .iter()
        .filter_map(|&item| {
            let key = Key::new(item, Field::Slots);
            let value = sealed
                .merged()
                .value(key, f64::from(old), |m| on(sealed.loader, m))?;
            Some((item, value))
        })
        .collect();
    let (Some(new), disagree) = template_slots(&values) else {
        return Some(bytes.to_vec());
    };
    if disagree {
        let all = values
            .iter()
            .map(|(item, value)| format!("{item} {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        crate::log!(
            WARN,
            "items that place one container ask for different slots ({all}): the last in load order counts"
        );
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Field::resolve keeps slots whole and from 8 to 128"
    )]
    let (new, lowered) = grown_slots(old, new as u8);
    if lowered {
        crate::log!(
            WARN,
            "container slots stay at {old}: a container never shrinks, as an upgraded chest moves every item it holds"
        );
    }
    resize_container(&patch.layout, bytes, new)
}

/// The loader's patch of the recipe registry, when some mod changes a
/// recipe and the build reflects recipes.
fn recipe_patch(
    loader: &'static Loader,
) -> Option<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let sealed = CHANGES.sealed.get()?;
    let recipes = sealed.merged().entries(Entry::Recipe);
    if recipes.is_empty() {
        return None;
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let Ok(layout) = RecipeLayout::resolve(&facts) else {
        crate::log!(
            WARN,
            "recipe changes are off: this build does not reflect the recipes"
        );
        return None;
    };
    let duration = catalogue::constant(catalogue::ids::RECIPE_DURATION, &loader.reflection)
        .and_then(|at| usize::try_from(at).ok());
    crate::log!(INFO, "{} recipes get changed values", recipes.len());
    let patch: &'static RecipePatch = Box::leak(Box::new(RecipePatch { layout, duration }));
    let key = RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY);
    let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
    Some((key, build_recipes as enshwick::abi::ResourcePatchFn, user))
}

/// Every recipe some change of `merged` from a mod `on` lets through moves,
/// with the game id of the workshop it moves to, sorted.
fn stations(merged: &Merged, on: impl Fn(ModIndex) -> bool + Copy) -> Vec<(Guid, u32)> {
    let mut moved: Vec<(Guid, u32)> = merged
        .entries(Entry::Recipe)
        .into_iter()
        .filter_map(|recipe| {
            let id = merged.value(Key::new(recipe, Field::Station), 0.0, on)?;
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "Field::resolve keeps a station a whole number from 0 to u32::MAX"
            )]
            Some((recipe, id as u32))
        })
        .collect();
    moved.sort_unstable_by_key(|(recipe, _)| recipe.0);
    moved
}

/// The loader's patch of the client's crafting menu, when some mod moves a
/// recipe to another station: each moved recipe is listed at its new
/// station alone.
fn menu_patch(
    loader: &'static Loader,
) -> Option<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let sealed = CHANGES.sealed.get()?;
    if loader.side != enshwick::abi::Side::CLIENT || stations(&sealed.merged(), |_| true).is_empty()
    {
        return None;
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let workshops = RecipeLayout::resolve(&facts).ok().and_then(|layout| {
        let registry = crate::archive::game()?
            .resource(&RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY))?;
        recipe_workshops(&layout, &registry)
    });
    let (Ok(layout), Some(workshops)) = (MenuLayout::resolve(&facts), workshops) else {
        crate::log!(
            WARN,
            "recipes moved to other stations stay listed where they were: this build does not reflect the crafting menu"
        );
        return None;
    };
    let patch: &'static MenuPatch = Box::leak(Box::new(MenuPatch { layout, workshops }));
    let key = UI_BUNDLE.key_by_hash(crate::catalogue::types::UI_BUNDLE);
    let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
    Some((key, build_menu as enshwick::abi::ResourcePatchFn, user))
}

/// What the loader's patch of the crafting menu needs.
struct MenuPatch {
    layout: MenuLayout,
    /// Every workshop some vanilla recipe is made at.
    workshops: Vec<WorkshopId>,
}

/// The loader's patch of the crafting menu: the base with every recipe the
/// mods still on move listed at its new station alone.
unsafe extern "system" fn build_menu(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `MenuPatch` `menu_patch` leaked, which lives for
    // the process.
    let patch = unsafe { &*user.cast::<MenuPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let sealed = CHANGES.sealed.get()?;
        let moves: Vec<(Guid, WorkshopId)> = stations(&sealed.merged(), |m| on(sealed.loader, m))
            .into_iter()
            .filter_map(|(recipe, id)| {
                let workshop = patch.workshops.iter().find(|w| w.id() == id)?;
                Some((recipe, *workshop))
            })
            .collect();
        move_recipes(&patch.layout, &bytes, &moves)
    }));
    let Ok(Some(built)) = built else {
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

/// What the loader's patch of the recipe registry needs.
struct RecipePatch {
    layout: RecipeLayout,
    /// Where a recipe keeps its time, when the build reflects it.
    duration: Option<usize>,
}

/// The merged changes as the recipe edit asks for them, with the mods still
/// on.
struct Live<'a> {
    loader: &'a Loader,
    merged: &'a Merged,
}

impl RecipeChanges for Live<'_> {
    fn of(&self, recipe: Guid) -> Vec<(Field, Option<Guid>)> {
        self.merged
            .keys(recipe)
            .into_iter()
            .map(|key| (key.field, key.item))
            .collect()
    }

    fn value(&self, recipe: Guid, field: Field, item: Option<Guid>, base: f64) -> Option<f64> {
        let key = Key {
            entry: recipe,
            field,
            item,
        };
        self.merged.value(key, base, |m| on(self.loader, m))
    }
}

/// The loader's patch of the recipe registry: the base with every recipe
/// change of the mods still on.
unsafe extern "system" fn build_recipes(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `RecipePatch` `recipe_patch` leaked, which lives
    // for the process.
    let patch = unsafe { &*user.cast::<RecipePatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let sealed = CHANGES.sealed.get()?;
        let merged = sealed.merged();
        let live = Live {
            loader: sealed.loader,
            merged: &merged,
        };
        change_recipes_in(&patch.layout, patch.duration, &bytes, &live)
    }));
    let Ok(Some((built, changed))) = built else {
        return Status::REFUSED;
    };
    crate::log!(INFO, "{changed} recipes changed by data changes");
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

/// The pack's own name for an entry, from a source such as
/// `items.toml:12 (Wood_Log)`.
fn label(source: &str) -> Option<&str> {
    let (_, rest) = source.split_once(" (")?;
    rest.strip_suffix(')')
}

/// The loader's patch of an item: the base with its changed values.
unsafe extern "system" fn build_item(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the `ItemPatch` `serve` leaked for this key, which
    // lives for the process.
    let patch = unsafe { &*user.cast::<ItemPatch>() };
    let mut bytes = vec![0; base_len];
    // SAFETY: `ResourcePatchFn` hands `base_len` readable bytes at `base`,
    // unchanged for the call.
    unsafe { enshwick::raw::read_bytes_at(base, 0, &mut bytes) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        changed_item(patch, &mut bytes);
    }));
    if built.is_err() {
        return Status::REFUSED;
    }
    // The game reads the resource for the life of the process.
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    // SAFETY: `run_patches` passes `out` pointing at a local `*const u8`,
    // writable for the call.
    unsafe { out.write(bytes.as_ptr()) };
    // SAFETY: `run_patches` passes `out_len` pointing at a local `usize`,
    // writable for the call.
    unsafe { out_len.write(bytes.len()) };
    Status::OK
}

/// Writes the changed stack size into the item `bytes`, and notes a raise.
fn changed_item(patch: &ItemPatch, bytes: &mut [u8]) {
    let Some(sealed) = CHANGES.sealed.get() else {
        return;
    };
    let (loader, labels) = (sealed.loader, &sealed.labels);
    let merged = sealed.merged();
    let live = |m: ModIndex| on(loader, m);
    let fields: Vec<Field> = merged
        .keys(patch.item)
        .into_iter()
        .map(|key| key.field)
        .collect();
    let vanilla = stack_at(bytes, patch.offsets);
    let edit = edit_item(bytes, patch.offsets, &fields, |field, base| {
        merged.value(Key::new(patch.item, field), base, live)
    });
    keep_stack(patch, bytes, vanilla);
    for field in &edit.lacking {
        crate::log!(
            WARN,
            "{field} of {} is not changed: the item has none",
            patch.item
        );
    }
    let Some((old, new)) = edit.stack.filter(|(old, new)| new > old) else {
        return;
    };
    let key = Key::new(patch.item, Field::MaxStackSize);
    let without = |m: ModIndex| {
        merged
            .value(key, f64::from(old), |o| live(o) && o != m)
            .unwrap_or(f64::from(old))
    };
    let alone = |m: ModIndex| {
        merged
            .value(key, f64::from(old), |o| live(o) && o == m)
            .unwrap_or(f64::from(old))
    };
    let owners = merged.owners(key, live);
    let label = labels
        .get(&patch.item)
        .cloned()
        .unwrap_or_else(|| patch.item.to_string());
    let mut raised = CHANGES.raised.relock();
    let raised = raised.get_or_insert_with(FxHashMap::default);
    for owner in credited(&owners, f64::from(old), f64::from(new), without, alone) {
        raised
            .entry(owner)
            .and_modify(|r| r.note(patch.item))
            .or_insert_with(|| Raised::new(patch.item, label.clone(), old, new));
    }
}

/// The stack size in `bytes`, an item's static data.
fn stack_at(bytes: &[u8], offsets: &ItemOffsets) -> Option<u16> {
    let at = offsets.max_stack?;
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// Serves at least the kept stack size of the patched item in `bytes`,
/// whose game size is `vanilla`, and notes the size served.
fn keep_stack(patch: &ItemPatch, bytes: &mut [u8], vanilla: Option<u16>) {
    let (Some(at), Some(vanilla), Some(merged)) = (
        patch.offsets.max_stack,
        vanilla,
        stack_at(bytes, patch.offsets),
    ) else {
        return;
    };
    let kept = CHANGES
        .kept
        .relock()
        .as_ref()
        .and_then(|kept| kept.of(patch.item));
    let (size, won) = served(merged, kept);
    if won {
        if let Some(slot) = bytes.get_mut(at..at + 2) {
            slot.copy_from_slice(&size.to_le_bytes());
        }
        crate::log!(
            INFO,
            "stacks of {} stay at {size}, not {merged}: a saved world may hold stacks that large",
            patch.item
        );
    }
    CHANGES
        .stacks
        .relock()
        .get_or_insert_with(FxHashMap::default)
        .insert(patch.item, (vanilla, size));
}

/// The loader's own data folder.
fn loader_dir(loader: &Loader) -> std::path::PathBuf {
    loader.data_dir.join(crate::settings::LOADER_ID)
}

/// On the side that keeps the world, when it loads or saves: keeps every
/// stack size in force above the game's, and holds the game while any is
/// kept, so a game the loader leaves vanilla stops before it cuts them.
fn keep_sizes(loader: &Loader) {
    let stacks = CHANGES.stacks.relock().clone().unwrap_or_default();
    let mut kept = CHANGES.kept.relock();
    if kept.is_none() {
        match Kept::load(&loader_dir(loader)) {
            Ok(read) => *kept = Some(read),
            // The file and its hold stay as they are; the next load or save
            // tries again.
            Err(why) => {
                crate::log!(
                    WARN,
                    "kept stack sizes do not read and stay as they are: {why}"
                );
                return;
            }
        }
    }
    let Some(kept) = kept.as_mut() else {
        return;
    };
    let mut grew = false;
    for (item, (vanilla, size)) in stacks {
        grew |= kept.record(item, vanilla, size);
    }
    let dir = loader_dir(loader);
    if grew && let Err(why) = kept.save(&dir) {
        crate::log!(WARN, "kept stack sizes not written: {why}");
    }
    let hold = Hold::new(&dir);
    let result = if kept.is_empty() {
        hold.clear()
    } else {
        hold.set(&hold_reason(kept))
    };
    if let Err(why) = result {
        crate::log!(WARN, "hold not kept: {why}");
    }
}

/// On the side that keeps the world: keeps the stack sizes in force when it
/// loads and saves, and when it loads sets the hold of every pack that
/// raised a stack size on that world, and releases that of every other pack
/// still on. A pack's hold of the whole data folder, from before holds knew
/// worlds, goes once the world is known. A mod with code keeps its own
/// hold.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if event == Event::WORLD_SAVED {
        keep_sizes(loader);
    }
    if event != Event::WORLD_LOADED {
        return;
    }
    keep_sizes(loader);
    let raised = CHANGES.raised.relock().clone().unwrap_or_default();
    let world = crate::saves::world_key(loader);
    for (index, record) in loader.mods.iter().enumerate() {
        let index = ModIndex::new(index);
        if !on(loader, index) {
            continue;
        }
        let dir = loader.data_dir.join(&record.id);
        let hold = match world {
            Some(world) => {
                if record.pack
                    && let Err(why) = Hold::new(&dir).clear()
                {
                    crate::log::write(Level::WARN, &record.id, &format!("hold not kept: {why}"));
                }
                Hold::for_world(&dir, world)
            }
            None => Hold::new(&dir),
        };
        let result = match hold_step(record.pack, raised.get(&index)) {
            HoldStep::Set(reason) => hold.set(&reason),
            HoldStep::Release => hold.clear(),
            HoldStep::Keep => Ok(()),
        };
        if let Err(why) = result {
            crate::log::write(Level::WARN, &record.id, &format!("hold not kept: {why}"));
        }
    }
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

    #[test]
    fn a_label_is_the_name_in_parentheses() {
        assert_eq!(label("items.toml:12 (Wood_Log)"), Some("Wood_Log"));
        assert_eq!(label("items.toml:12"), None);
    }
}
