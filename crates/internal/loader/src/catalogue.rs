//! The engine catalogue as the loader uses it: the API's ids answered from
//! the running game's reflection, and the hashes of the resource types the
//! loader reads and serves itself.

use enshwick::content::ResourceType;
pub(crate) use enshwick_gamedata::catalogue::{Reflected, constant, event_types, ids, type_name};

use crate::host::Loader;

/// The running game's hash of the type the catalogue id `id` names, as its
/// entities' components carry it; `None` on a build without the type.
pub(crate) fn component_hash(loader: &Loader, id: &str) -> Option<u32> {
    let info = loader.reflection.type_info(type_name(id)).ok()?;
    crate::world::type_hash(info.va())
}

/// The offset the catalogue id `id` names in the running game, in bytes.
pub(crate) fn offset(loader: &Loader, id: &str) -> Option<usize> {
    constant(id, &loader.reflection).and_then(|offset| usize::try_from(offset).ok())
}

/// The size of the type the catalogue id `id` names in the running game.
pub(crate) fn size(loader: &Loader, id: &str) -> Option<usize> {
    let info = loader.reflection.type_info(type_name(id)).ok()?;
    usize::try_from(info.size()).ok()
}

/// The hashes of the resource types the loader handles itself, from the
/// catalogue's reflected names.
pub(crate) mod types {
    use enshwick::content::types as api;

    use super::hash;

    pub(crate) const ITEM_INFO: u32 = hash(api::ITEM_INFO);
    pub(crate) const ITEM_REGISTRY: u32 = hash(api::ITEM_REGISTRY_RESOURCE);
    pub(crate) const RECIPE_REGISTRY: u32 = hash(api::RECIPE_REGISTRY_RESOURCE);
    pub(crate) const TEMPLATE: u32 = hash(api::TEMPLATE_RESOURCE);
    pub(crate) const TEMPLATE_COLLECTION: u32 = hash(api::TEMPLATE_COLLECTION_RESOURCE);
    pub(crate) const WORKSHOP_REGISTRY: u32 = hash(api::WORKSHOP_REGISTRY_RESOURCE);
    pub(crate) const UI_IMAGE: u32 = hash(api::UI_TEXTURE_RESOURCE);
    pub(crate) const UI_BUNDLE: u32 = hash(api::FB_UI_BUNDLE);
}

#[expect(
    clippy::panic,
    reason = "const evaluation: a type the catalogue lacks fails the build"
)]
const fn hash(ty: ResourceType) -> u32 {
    match enshwick_gamedata::catalogue::type_hash(ty.id()) {
        Some(hash) => hash,
        None => panic!("a resource type the catalogue does not name"),
    }
}

#[cfg(test)]
mod tests {
    use enshwick_gamedata::catalogue::fact;

    #[test]
    fn no_id_shadows_a_build_table_id() {
        for table in crate::builds::load() {
            let ids = table.symbols().iter().map(enshwick_gamedata::Symbol::id);
            let mut ids = ids.chain(
                table
                    .constants()
                    .iter()
                    .map(enshwick_gamedata::Constant::id),
            );
            assert!(!ids.any(|id| fact(id).is_some()), "{}", table.file_name());
        }
    }

    #[test]
    fn the_resource_hashes_are_the_game_s() {
        assert_eq!(super::types::ITEM_INFO, 0xb5ce_8765);
        assert_eq!(super::types::UI_IMAGE, 0xf9e8_6997);
        assert_eq!(super::types::UI_BUNDLE, 0xfef9_18b8);
    }
}
