//! The engine catalogue as the loader uses it: the API's ids answered from
//! the running game's reflection, and the hashes of the resource types the
//! loader reads and serves itself.

use enshwick::content::ResourceType;
pub(crate) use enshwick_gamedata::catalogue::{Reflected, constant, type_name};

/// The hashes of the resource types the loader handles itself, from the
/// catalogue's reflected names.
pub(crate) mod types {
    use enshwick::content::types as api;

    use super::hash;

    pub(crate) const ITEM_INFO: u32 = hash(api::ITEM_INFO);
    pub(crate) const ITEM_REGISTRY: u32 = hash(api::ITEM_REGISTRY);
    pub(crate) const RECIPE_REGISTRY: u32 = hash(api::RECIPE_REGISTRY);
    pub(crate) const TEMPLATE: u32 = hash(api::TEMPLATE);
    pub(crate) const TEMPLATE_COLLECTION: u32 = hash(api::TEMPLATE_COLLECTION);
    pub(crate) const WORKSHOP_REGISTRY: u32 = hash(api::WORKSHOP_REGISTRY);
    pub(crate) const UI_IMAGE: u32 = hash(api::UI_IMAGE);
    pub(crate) const UI_BUNDLE: u32 = hash(api::UI_BUNDLE);
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
