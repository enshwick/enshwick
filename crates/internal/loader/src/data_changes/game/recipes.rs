//! Recipe changes in the running game: the vanilla recipes a selection
//! reads, and the loader's patches of the recipe registry and of the
//! client's crafting menu for recipes moved to another station.

use core::ffi::c_void;

use enshwick::abi::{ResourceKey, Status};
use enshwick::content::Guid;
use enshwick::content::changes::{Entry, Field, RecipeTraits};
use enshwick::content::menu::{MenuLayout, UI_BUNDLE, move_recipes};
use enshwick::content::recipes::{
    RECIPE_REGISTRY, RecipeChanges, RecipeKind, RecipeLayout, change_recipes_in, recipe_workshops,
};
use enshwick::content::workshops::WorkshopId;
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, on};
use crate::data_changes::{Key, Merged};
use crate::host::Loader;
use crate::item_kinds::ItemKindLayout;
use crate::mod_index::ModIndex;

/// Every vanilla recipe with the kind of what it makes and its workshop,
/// from the game's archive; none when the build does not reflect the
/// recipes or the items' equipment slots, or the archive does not read.
pub(super) fn vanilla_recipes(loader: &Loader) -> Vec<(Guid, RecipeTraits)> {
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

/// The loader's patch of the recipe registry, when some mod changes a
/// recipe and the build reflects recipes.
pub(super) fn recipe_patch(
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
pub(super) fn stations(merged: &Merged, on: impl Fn(ModIndex) -> bool + Copy) -> Vec<(Guid, u32)> {
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
pub(super) fn menu_patch(
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
