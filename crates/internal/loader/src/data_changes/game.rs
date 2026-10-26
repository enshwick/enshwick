//! Declared changes in the running game: the loader's own patch of each
//! changed item, the conflict reports once every mod ran its init, and the
//! hold a raised stack size sets on the side that keeps the world.

use core::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};

use arc_swap::ArcSwap;

use enshwick::abi::{Event, Level, Relation, ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::{Change, Entry, Field};
use enshwick::content::containers::{ContainerLayout, container_slots, resize_container};
use enshwick::content::recipes::{RECIPE_REGISTRY, RecipeChanges, RecipeLayout, change_recipes_in};
use enshwick::state::Hold;
use rustc_hash::FxHashMap;

use super::{
    Conflict, Declared, HoldStep, Key, Merged, Raised, credited, grown_slots, hold_step, merge,
    template_slots,
};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

/// The domain's state: changes as mods declare them, what the end of init
/// sealed, and the stack sizes raised so far.
struct Changes {
    declared: Mutex<Vec<Declared>>,
    sealed: OnceLock<Sealed>,
    raised: Mutex<Option<FxHashMap<ModIndex, Raised>>>,
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
}

impl Sealed {
    /// The merged changes in force now.
    fn merged(&self) -> Arc<Merged> {
        self.merged.load_full()
    }
}

static CHANGES: Changes = Changes {
    declared: Mutex::new(Vec::new()),
    sealed: OnceLock::new(),
    raised: Mutex::new(None),
};

/// What the loader's patch of one item needs: the item and where its stack
/// size sits.
struct ItemPatch {
    item: Guid,
    max_stack_at: usize,
}

/// Records mod `owner`'s `change` from `source`; refused once init is over.
pub(crate) fn declare(owner: ModIndex, change: Change, source: String) -> Status {
    if CHANGES.sealed.get().is_some() {
        return Status::TOO_LATE;
    }
    CHANGES.declared.relock().push(Declared {
        owner,
        change,
        source,
    });
    Status::OK
}

/// Whether the loader follows the world for changes: some mod declared one,
/// or a pack holds the game and may need its hold released.
pub(crate) fn wanted(loader: &Loader) -> bool {
    let declared = CHANGES.sealed.get().map_or_else(
        || !CHANGES.declared.relock().is_empty(),
        |sealed| !sealed.merged().is_empty(),
    );
    declared
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
    // SAFETY: a record's manifest is the loader's copy, kept for the process
    // with every string and slice in it.
    let Ok(candidate) = (unsafe { crate::manifest::parse(a.manifest) }) else {
        return false;
    };
    candidate
        .dependencies
        .iter()
        .any(|d| d.relation == Relation::REQUIRES && b.owns(&d.id))
}

/// Reports each set another mod overrides in both mods' logs and the
/// conflict report.
fn report(loader: &Loader, conflicts: &[Conflict]) {
    for conflict in conflicts {
        let line = conflict.describe(|m| id(loader, m));
        let level = if conflict.intended {
            Level::INFO
        } else {
            Level::WARN
        };
        for owner in [conflict.winner.owner, conflict.overridden.owner] {
            crate::log::write(level, &id(loader, owner), &line);
        }
        let feature = match conflict.key.item {
            Some(item) => format!("{} {item} of {}", conflict.key.field, conflict.key.entry),
            None => format!("{} of {}", conflict.key.field, conflict.key.entry),
        };
        crate::conflicts::note(
            feature.clone(),
            conflict.overridden.owner,
            crate::conflicts::Mode::Exclusive,
            Status::REFUSED,
        );
        crate::conflicts::note(
            feature,
            conflict.winner.owner,
            crate::conflicts::Mode::Exclusive,
            Status::OK,
        );
    }
}

/// The items whose stack size some change of `merged` changes.
fn stacked(merged: &Merged) -> Vec<Guid> {
    merged
        .entries(Entry::Item)
        .into_iter()
        .filter(|item| {
            merged
                .keys(*item)
                .iter()
                .any(|k| k.field == Field::MaxStackSize)
        })
        .collect()
}

/// Merges every declared change, reports the overridden sets, and returns
/// the loader's patches, one per changed item: its key and the patch with
/// its pointer. Called once, when the served resources are sealed.
pub(crate) fn serve(
    loader: &'static Loader,
) -> Vec<(ResourceKey, enshwick::abi::ResourcePatchFn, *mut c_void)> {
    let declared = std::mem::take(&mut *CHANGES.declared.relock());
    let (merged, conflicts) = merge(&declared, |a, b| requires(loader, a, b));
    report(loader, &conflicts);
    let labels: FxHashMap<Guid, String> = declared
        .iter()
        .filter_map(|d| label(&d.source).map(|l| (d.change.entry, l.to_owned())))
        .collect();
    let max_stack_at = enshwick_gamedata::catalogue::constant(
        enshwick_gamedata::catalogue::ITEM_MAX_STACK,
        &loader.reflection,
    )
    .and_then(|at| usize::try_from(at).ok());
    let items = stacked(&merged);
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
    });
    let mut patches = Vec::new();
    match max_stack_at {
        Some(max_stack_at) => {
            if !items.is_empty() {
                crate::log!(INFO, "{} items get a changed stack size", items.len());
            }
            for item in items {
                let patch: &'static ItemPatch =
                    Box::leak(Box::new(ItemPatch { item, max_stack_at }));
                let key = item.key_by_hash(crate::catalogue::types::ITEM_INFO);
                let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
                patches.push((key, build_item as enshwick::abi::ResourcePatchFn, user));
            }
        }
        None if !items.is_empty() => crate::log!(
            WARN,
            "stack size changes are off: this build does not reflect the items' stack size"
        ),
        None => {}
    }
    if let Some(patch) = recipe_patch(loader) {
        patches.push(patch);
    }
    patches.extend(container_patches(loader));
    let _first = sealed
        .patched
        .set(patches.iter().map(|(key, _, _)| *key).collect());
    patches
}

/// Takes `changes` as mod `owner`'s changes from now on, for a development
/// reload: merges every change again, reports the overridden sets, and
/// builds each resource the loader patches again in place; how many it
/// rebuilt.
///
/// # Errors
///
/// What takes a restart: a change of an entry the loader did not patch at
/// start, or a rebuild that changes a resource's length, such as an
/// ingredient taken out or added.
pub(crate) fn reload(owner: ModIndex, changes: Vec<(Change, String)>) -> Result<usize, String> {
    let sealed = CHANGES
        .sealed
        .get()
        .ok_or_else(|| "no changes were in force at start".to_owned())?;
    let loader = sealed.loader;
    let patched = sealed.patched.get().cloned().unwrap_or_default();
    let mut declared = sealed.declared.relock();
    let next = super::replace_owner(
        &declared,
        owner,
        changes
            .into_iter()
            .map(|(change, source)| Declared {
                owner,
                change,
                source,
            })
            .collect(),
    );
    let (merged, conflicts) = merge(&next, |a, b| requires(loader, a, b));
    if let Some(item) = stacked(&merged)
        .into_iter()
        .find(|item| !patched.contains(&item.key_by_hash(crate::catalogue::types::ITEM_INFO)))
    {
        return Err(format!(
            "the stack size of {item} changes for the first time, which takes a restart"
        ));
    }
    let recipes = RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY);
    if !merged.entries(Entry::Recipe).is_empty() && !patched.contains(&recipes) {
        return Err("recipes change for the first time, which takes a restart".to_owned());
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
    report(loader, &conflicts);
    sealed.merged.store(Arc::new(merged));
    *declared = next;
    drop(declared);
    let mut failed = Vec::new();
    for key in &patched {
        let status = crate::content::rebuild(ModIndex::LOADER, *key);
        if !status.is_ok() {
            failed.push(format!("{}:{:08x}", Guid(key.guid), key.type_hash));
        }
    }
    if failed.is_empty() {
        Ok(patched.len())
    } else {
        Err(format!(
            "{} resources take a restart, as their length changed: {}",
            failed.len(),
            failed.join(", ")
        ))
    }
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
    let facts = enshwick_gamedata::catalogue::Reflected(&loader.reflection);
    let placed = enshwick_gamedata::catalogue::constant(
        enshwick_gamedata::catalogue::ITEM_PLACED_ENTITY,
        &loader.reflection,
    )
    .and_then(|at| usize::try_from(at).ok());
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
        let template = archive
            .resource(&item.key_by_hash(crate::catalogue::types::ITEM_INFO))
            .and_then(|info| info.get(placed..placed + 16)?.try_into().ok())
            .map(Guid)
            .filter(|template| template.0 != [0; 16]);
        let Some(template) = template else {
            crate::log!(
                WARN,
                "container slots of {item} are not changed: it is no vanilla item that places a container"
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
    let facts = enshwick_gamedata::catalogue::Reflected(&loader.reflection);
    let Ok(layout) = RecipeLayout::resolve(&facts) else {
        crate::log!(
            WARN,
            "recipe changes are off: this build does not reflect the recipes"
        );
        return None;
    };
    let duration = enshwick_gamedata::catalogue::constant(
        enshwick_gamedata::catalogue::RECIPE_DURATION,
        &loader.reflection,
    )
    .and_then(|at| usize::try_from(at).ok());
    crate::log!(INFO, "{} recipes get changed values", recipes.len());
    let patch: &'static RecipePatch = Box::leak(Box::new(RecipePatch { layout, duration }));
    let key = RECIPE_REGISTRY.key_by_hash(crate::catalogue::types::RECIPE_REGISTRY);
    let user = core::ptr::from_ref(patch).cast_mut().cast::<c_void>();
    Some((key, build_recipes as enshwick::abi::ResourcePatchFn, user))
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
    let at = patch.max_stack_at;
    let Some(field) = bytes.get_mut(at..at + 2) else {
        crate::log!(WARN, "item {} is too short for its stack size", patch.item);
        return;
    };
    let old = u16::from_le_bytes([field[0], field[1]]);
    let live = |m: ModIndex| on(loader, m);
    let Some(new) = merged.value(
        Key::new(patch.item, Field::MaxStackSize),
        f64::from(old),
        live,
    ) else {
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Field::resolve keeps a stack size whole and from 1 to 65535"
    )]
    let new = new as u16;
    field.copy_from_slice(&new.to_le_bytes());
    if new > old {
        let without = |m: ModIndex| {
            merged
                .value(
                    Key::new(patch.item, Field::MaxStackSize),
                    f64::from(old),
                    |o| live(o) && o != m,
                )
                .unwrap_or(f64::from(old))
        };
        let owners = merged.owners(Key::new(patch.item, Field::MaxStackSize), live);
        let label = labels
            .get(&patch.item)
            .cloned()
            .unwrap_or_else(|| patch.item.to_string());
        let mut raised = CHANGES.raised.relock();
        let raised = raised.get_or_insert_with(FxHashMap::default);
        let alone = |m: ModIndex| {
            merged
                .value(
                    Key::new(patch.item, Field::MaxStackSize),
                    f64::from(old),
                    |o| live(o) && o == m,
                )
                .unwrap_or(f64::from(old))
        };
        for owner in credited(&owners, f64::from(old), f64::from(new), without, alone) {
            raised
                .entry(owner)
                .and_modify(|r| r.note(patch.item))
                .or_insert_with(|| Raised::new(patch.item, label.clone(), old, new));
        }
    }
}

/// On the side that keeps the world, when it loads: sets the hold of every
/// pack that raised a stack size, and releases that of every other pack
/// still on. A mod with code keeps its own hold.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if event != Event::WORLD_LOADED {
        return;
    }
    let raised = CHANGES.raised.relock().clone().unwrap_or_default();
    for (index, record) in loader.mods.iter().enumerate() {
        let index = ModIndex::new(index);
        if !on(loader, index) {
            continue;
        }
        let hold = Hold::new(&loader.data_dir.join(&record.id));
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
    let constant =
        |id: &str| enshwick_gamedata::catalogue::constant(id, &loader.reflection).is_some();
    let recipes = || {
        RecipeLayout::resolve(&enshwick_gamedata::catalogue::Reflected(&loader.reflection)).is_ok()
    };
    match field {
        Field::MaxStackSize => constant(enshwick_gamedata::catalogue::ITEM_MAX_STACK),
        Field::Output | Field::Cost | Field::Ingredient => recipes(),
        Field::CraftingDuration => {
            recipes() && constant(enshwick_gamedata::catalogue::RECIPE_DURATION)
        }
        Field::Slots => {
            ContainerLayout::resolve(&enshwick_gamedata::catalogue::Reflected(&loader.reflection))
                .is_ok()
                && constant(enshwick_gamedata::catalogue::ITEM_PLACED_ENTITY)
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
