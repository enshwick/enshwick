//! The loader's own patches among the served resources: item stand-ins,
//! terrain materials, buff types, changes of vanilla values and recipe
//! rates, each chained where its readers need it.

use core::ffi::c_void;
use std::sync::atomic::Ordering;

use enshwick_core::abi::{ResourceKey, ResourcePatchFn};
use enshwick_core::content::Guid;

use super::{OwnPatch, Patch, Served, own_patch, recipe_registry};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;

/// Adds the loader's stand-ins for the items of removed mods to `served`,
/// and the patch that lists them to the item registry; how many.
pub(super) fn serve_stand_ins(
    served: &mut Vec<Served>,
    items: Vec<crate::content::standins::StandIn>,
    registry: Builder,
) -> usize {
    let patch = |builder| BuilderPatch::new(builder, STAND_INS).patch();
    let count = items.len();
    for (key, base, builder) in items {
        served.push(Served::new(key, base, vec![patch(builder)], false));
    }
    let key = enshwick_core::content::items::ITEM_REGISTRY
        .key_by_hash(crate::engine::catalogue::types::ITEM_REGISTRY);
    match served.iter_mut().find(|s| s.key == key) {
        Some(listed) => listed.patches.push(patch(registry)),
        None => served.push(Served::new(key, key, vec![patch(registry)], false)),
    }
    count
}

/// Adds the loader's own patches to `served` in the order they chain:
/// terrain materials, buff types, changes of vanilla values, then recipe
/// rates; `materials` are the mods' terrain materials and `orphans` the
/// recorded ones no mod adds now. How many keys.
pub(super) fn serve_own(
    served: &mut Vec<Served>,
    loader: &'static Loader,
    materials: &'static [crate::content::terrain_materials::Material],
    orphans: &[(Guid, Guid)],
) -> usize {
    use crate::content::{buff_types, data_changes, rate_links, terrain_materials};

    serve_terrain(served, terrain_materials::serve(loader, materials, orphans))
        + serve_buffs(served, buff_types::serve(loader, buff_types::sealed()))
        + serve_changes(
            served,
            data_changes::serve(loader),
            data_changes::has_live(),
        )
        + serve_rates(served, rate_links::recipe_patch(loader))
}

/// Adds the loader's patches for terrain materials to `served`, before
/// every mod's patch on each key, so a mod's patch of the rules sees the
/// new ones; the loader's changes of vanilla values, served after, go
/// first again, so a material copies the rules as changed. Each key keeps
/// its base, since a joining client builds it again from its host's table.
/// How many keys.
fn serve_terrain(served: &mut Vec<Served>, patches: Vec<(ResourceKey, Builder)>) -> usize {
    let count = patches.len();
    for (key, builder) in patches {
        let patch = BuilderPatch::new(builder, crate::content::terrain_materials::FEATURE).patch();
        match served.iter_mut().find(|s| s.key == key) {
            Some(entry) => {
                entry.patches.insert(0, patch);
                entry.keeps_base.store(true, Ordering::Relaxed);
            }
            None => served.push(Served::new(key, key, vec![patch], true)),
        }
    }
    count
}

/// Adds the loader's patch of the buff collection with every mod's buff
/// types to `served`, before every mod's patch of it, so a mod's patch sees
/// the new types; the loader's changes of vanilla values, served after, go
/// first again, so a type copies the values as changed. How many keys.
fn serve_buffs(served: &mut Vec<Served>, patch: Option<(ResourceKey, Builder)>) -> usize {
    let Some((key, builder)) = patch else {
        return 0;
    };
    let patch = BuilderPatch::new(builder, crate::content::buff_types::FEATURE).patch();
    match served.iter_mut().find(|s| s.key == key) {
        Some(entry) => entry.patches.insert(0, patch),
        None => served.push(Served::new(key, key, vec![patch], false)),
    }
    1
}

/// Adds the loader's patch for the recipe output rates to `served`, last on
/// the recipe registry, after every mod's and every data change, and keeps
/// the registry's base, since a changed factor rebuilds it; how many keys.
pub(super) fn serve_rates(served: &mut Vec<Served>, patch: Option<ResourcePatchFn>) -> usize {
    let Some(apply) = patch else {
        return 0;
    };
    let key = recipe_registry();
    let patch = Patch::new(ModIndex::LOADER, apply, 0);
    match served.iter_mut().find(|s| s.key == key) {
        Some(entry) => {
            entry.patches.push(patch);
            entry.keeps_base.store(true, Ordering::Relaxed);
        }
        None => served.push(Served::new(key, key, vec![patch], true)),
    }
    1
}

/// Adds the loader's patches of changed vanilla values to `served`: first
/// on a vanilla key, so mods patch the changed values; last on a mod's new
/// resource, after the mod built it. How many keys.
pub(super) fn serve_changes(
    served: &mut Vec<Served>,
    patches: Vec<(ResourceKey, ResourcePatchFn, *mut c_void)>,
    keep: bool,
) -> usize {
    let count = patches.len();
    for (key, apply, user) in patches {
        let patch = Patch::new(ModIndex::LOADER, apply, user.expose_provenance());
        match served.iter_mut().find(|s| s.key == key) {
            Some(entry) if crate::content::data_changes::goes_first(&key, &entry.base) => {
                entry.patches.insert(0, patch);
                entry.keeps_base.fetch_or(keep, Ordering::Relaxed);
            }
            Some(entry) => {
                entry.patches.push(patch);
                entry.keeps_base.fetch_or(keep, Ordering::Relaxed);
            }
            None => served.push(Served::new(key, key, vec![patch], keep)),
        }
    }
    count
}

/// Builds a served resource from its base's bytes, which it may hand back
/// unchanged without a copy.
pub(crate) type Builder = Box<dyn Fn(Vec<u8>) -> Option<Vec<u8>> + Send + Sync>;

/// One of the loader's own patches that a [`Builder`] makes: a stand-in,
/// the item registry that lists them, or a resource terrain materials
/// change.
pub(super) struct BuilderPatch {
    builder: Builder,
    /// This patch's switch alone: a build that panics is refused, and the
    /// other keys keep building, so what they keep stays in the save.
    fuse: Fuse,
}

impl BuilderPatch {
    /// The patch `builder` makes, under a switch of its own that names
    /// `feature`.
    pub(super) fn new(builder: Builder, feature: &'static str) -> Self {
        Self {
            builder,
            fuse: Fuse::new(feature),
        }
    }

    /// The patch as the loader's on a served key, leaked for the process.
    fn patch(self) -> Patch {
        let (apply, user) = own_patch(self);
        Patch::new(ModIndex::LOADER, apply, user.expose_provenance())
    }
}

impl OwnPatch for BuilderPatch {
    const SITE: &'static str = "a loader build";

    fn fuse(&self) -> &Fuse {
        &self.fuse
    }

    fn build(&self, base: Vec<u8>) -> Option<Vec<u8>> {
        (self.builder)(base)
    }
}

/// The feature every stand-in's switch names.
pub(super) const STAND_INS: &str = "item stand-ins";
