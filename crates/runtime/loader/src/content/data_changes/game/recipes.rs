//! Recipe changes in the running game: the vanilla recipes a selection
//! reads, and the loader's patches of the recipe registry and of the
//! client's crafting menu for recipes moved to another station.

use core::ffi::c_void;

use enshwick_core::abi::ResourceKey;
use enshwick_core::content::Guid;
use enshwick_core::content::changes::{Entry, Field, RecipeTraits};
use enshwick_core::content::menu::{MenuLayout, UI_BUNDLE, move_recipes};
use enshwick_core::content::recipes::{
    RECIPE_REGISTRY, RecipeChanges, RecipeKind, RecipeLayout, change_recipes_in, recipe_workshops,
};
use enshwick_core::content::workshops::WorkshopId;
use enshwick_gamedata::catalogue;
use rustc_hash::FxHashMap;

use super::{CHANGES, UNREADABLE, on};
use crate::content::data_changes::{Key, Merged};
use crate::content::item_kinds::ItemKindLayout;
use crate::content::served::OwnPatch;
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

/// Every vanilla recipe with the kind of what it makes and its workshop,
/// from the game's archive; none when the build does not reflect the
/// recipes or the items' equipment slots, or the archive does not read.
pub(super) fn vanilla_recipes(loader: &Loader) -> Vec<(Guid, RecipeTraits)> {
    let off = || {
        let what = "the recipe layout a selection reads";
        let _off = loader.fuse_off(&SELECTED_RECIPES_FUSE, what, UNREADABLE);
        Vec::new()
    };
    let facts = catalogue::Reflected(&loader.reflection);
    let (Ok(layout), Some(archive), Some(items)) = (
        RecipeLayout::resolve(&facts),
        crate::engine::archive::game(),
        ItemKindLayout::from_reflection(&loader.reflection),
    ) else {
        return off();
    };
    let Some(index) = archive
        .resource(&RECIPE_REGISTRY.key_by_hash(crate::engine::catalogue::types::RECIPE_REGISTRY))
        .and_then(|registry| crate::content::recipes::Index::read(&layout, &registry))
    else {
        return off();
    };
    let kinds: FxHashMap<u32, RecipeKind> = archive
        .keys_of(crate::engine::catalogue::types::ITEM_INFO)
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
) -> Option<(
    ResourceKey,
    enshwick_core::abi::ResourcePatchFn,
    *mut c_void,
)> {
    let sealed = CHANGES.sealed.get()?;
    let recipes = sealed.merged().entries(Entry::Recipe);
    if recipes.is_empty() {
        return None;
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let layout = match RecipeLayout::resolve(&facts) {
        Ok(layout) => layout,
        Err(off) => {
            let why = format!("does not resolve: {off}");
            let _off = loader.fuse_off(&RECIPES_FUSE, crate::content::recipes::RECIPE_LAYOUT, &why);
            return None;
        }
    };
    let duration = catalogue::constant(catalogue::ids::RECIPE_DURATION, &loader.reflection)
        .and_then(|at| usize::try_from(at).ok());
    crate::log!(INFO, "{} recipes get changed values", recipes.len());
    let key = RECIPE_REGISTRY.key_by_hash(crate::engine::catalogue::types::RECIPE_REGISTRY);
    let (apply, user) = crate::content::served::own_patch(RecipePatch { layout, duration });
    Some((key, apply, user))
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
) -> Option<(
    ResourceKey,
    enshwick_core::abi::ResourcePatchFn,
    *mut c_void,
)> {
    let sealed = CHANGES.sealed.get()?;
    if loader.side != enshwick_core::abi::Side::CLIENT
        || stations(&sealed.merged(), |_| true).is_empty()
    {
        return None;
    }
    let facts = catalogue::Reflected(&loader.reflection);
    let workshops = RecipeLayout::resolve(&facts).ok().and_then(|layout| {
        let registry = crate::engine::archive::game()?.resource(
            &RECIPE_REGISTRY.key_by_hash(crate::engine::catalogue::types::RECIPE_REGISTRY),
        )?;
        recipe_workshops(&layout, &registry)
    });
    let (Ok(layout), Some(workshops)) = (MenuLayout::resolve(&facts), workshops) else {
        let what = "the crafting menu layout";
        let _off = loader.fuse_off(&MOVES_FUSE, what, UNREADABLE);
        return None;
    };
    let key = UI_BUNDLE.key_by_hash(crate::engine::catalogue::types::UI_BUNDLE);
    let (apply, user) = crate::content::served::own_patch(MenuPatch { layout, workshops });
    Some((key, apply, user))
}

/// What the loader's patch of the crafting menu needs.
struct MenuPatch {
    layout: MenuLayout,
    /// Every workshop some vanilla recipe is made at.
    workshops: Vec<WorkshopId>,
}

/// The loader's patch of the crafting menu: the base with every recipe the
/// mods still on move listed at its new station alone.
impl OwnPatch for MenuPatch {
    const SITE: &'static str = "a crafting menu patch";

    fn fuse(&self) -> &Fuse {
        &MOVES_FUSE
    }

    fn build(&self, bytes: Vec<u8>) -> Option<Vec<u8>> {
        moved_recipes(self, &bytes)
    }
}

/// The crafting menu `bytes` with every recipe the mods still on move
/// listed at its new station alone.
fn moved_recipes(patch: &MenuPatch, bytes: &[u8]) -> Option<Vec<u8>> {
    let sealed = CHANGES.sealed.get()?;
    let moves: Vec<(Guid, WorkshopId)> = stations(&sealed.merged(), |m| on(sealed.loader, m))
        .into_iter()
        .filter_map(|(recipe, id)| {
            let workshop = patch.workshops.iter().find(|w| w.id() == id)?;
            Some((recipe, *workshop))
        })
        .collect();
    move_recipes(&patch.layout, bytes, &moves)
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
impl OwnPatch for RecipePatch {
    const SITE: &'static str = "a recipe registry patch";

    fn fuse(&self) -> &Fuse {
        &RECIPES_FUSE
    }

    fn build(&self, bytes: Vec<u8>) -> Option<Vec<u8>> {
        changed_recipes(self, &bytes)
    }
}

/// The recipe registry `bytes` with every recipe change of the mods still
/// on.
fn changed_recipes(patch: &RecipePatch, bytes: &[u8]) -> Option<Vec<u8>> {
    let sealed = CHANGES.sealed.get()?;
    let merged = sealed.merged();
    let live = Live {
        loader: sealed.loader,
        merged: &merged,
    };
    change_recipes_in(&patch.layout, patch.duration, bytes, &live).map(|(built, changed)| {
        if !crate::content::served::scratch() {
            crate::log!(INFO, "{changed} recipes changed by data changes");
        }
        built
    })
}

/// The switch of the changes of selected recipes, which needs the kind and
/// workshop of every vanilla recipe.
static SELECTED_RECIPES_FUSE: Fuse = Fuse::new("changes of selected recipes");
/// The switch of the recipe changes.
static RECIPES_FUSE: Fuse = Fuse::new("recipe changes");
/// The switch of the recipes moved to another station in the client's
/// crafting menu.
static MOVES_FUSE: Fuse = Fuse::new("recipe moves in the crafting menu");
