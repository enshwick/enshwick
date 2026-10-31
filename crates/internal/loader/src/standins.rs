//! Stand-ins for the items of removed mods. An item stack keeps only its
//! item id, and the game drops a stack whose id it does not know when it
//! loads it. For every item a mod once cloned (recorded in
//! [`crate::clones`]) that no running mod serves now, the loader serves a
//! stand-in under the item's own GUID: a copy of the vanilla item it was
//! cloned from, named "Item of a removed mod", listed in the item registry.
//! The stack keeps its id, count and data, and once the mod is back it
//! serves the item again and the stack is its item as before.

use enshwick::abi::ResourceKey;
use enshwick::content::items::{ItemLayout, NewItem, clone_item, list_item};
use enshwick::content::{Guid, Id};

use crate::catalogue::{Reflected, types};
use crate::host::Loader;

/// The tag of a stand-in's name.
const NAME: Id = Id::new("enshwick:stand-in.name");
/// The tag of a stand-in's description.
const DESCRIPTION: Id = Id::new("enshwick:stand-in.description");
/// Builds a served resource from its base's bytes.
pub(crate) type Builder = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// One stand-in: its key, the vanilla item's key and its builder.
pub(crate) type StandIn = (ResourceKey, ResourceKey, Builder);

/// The recorded items no mod serves: `(item GUID, vanilla GUID)` from
/// `recorded`, leaving out those `served` holds and those a migration
/// renamed (`renamed`, whose new GUID the game reads instead).
#[must_use]
pub(crate) fn orphans(
    recorded: &[([u8; 16], [u8; 16])],
    served: &[ResourceKey],
    renamed: impl Fn(&[u8; 16]) -> bool,
) -> Vec<(Guid, Guid)> {
    recorded
        .iter()
        .filter(|(item, _)| {
            !served
                .iter()
                .any(|key| key.type_hash == types::ITEM_INFO && key.guid == *item)
                && !renamed(item)
        })
        .map(|&(item, vanilla)| (Guid(item), Guid(vanilla)))
        .collect()
}

/// The item layout of the running build, from its reflection.
fn layout(loader: &Loader) -> Option<ItemLayout> {
    ItemLayout::resolve(&Reflected(&loader.reflection)).ok()
}

/// What the loader serves for `orphans`: each stand-in's `(key, base,
/// builder)`, then the builder of the patch that lists them in the item
/// registry; nothing when the build's reflection lacks a field.
pub(crate) fn served(loader: &Loader, orphans: &[(Guid, Guid)]) -> Option<(Vec<StandIn>, Builder)> {
    if orphans.is_empty() {
        return None;
    }
    let Some(layout) = layout(loader) else {
        crate::log!(
            WARN,
            "{} items of removed mods are dropped: the item fields are not reflected on this build",
            orphans.len()
        );
        return None;
    };
    let items = orphans
        .iter()
        .map(|&(item, vanilla)| {
            let mut stand_in = NewItem::new(NAME, vanilla);
            stand_in.name = Some(NAME.guid());
            stand_in.description = Some(DESCRIPTION.guid());
            let build: Builder =
                Box::new(move |base: &[u8]| clone_item(&layout, base, &stand_in, item));
            (
                item.key_by_hash(types::ITEM_INFO),
                vanilla.key_by_hash(types::ITEM_INFO),
                build,
            )
        })
        .collect();
    let listed: Vec<Guid> = orphans.iter().map(|&(item, _)| item).collect();
    let registry: Builder = Box::new(move |base: &[u8]| {
        listed.iter().try_fold(base.to_vec(), |registry, &item| {
            list_item(&layout, &registry, item, &format!("stand-in {item}"))
        })
    });
    for (tag, key) in [
        (NAME, "stand-in.name"),
        (DESCRIPTION, "stand-in.description"),
    ] {
        for (language, text) in crate::texts::translations(key) {
            let _added = crate::content::add_text(tag.guid().fnv1a(), language.0, text);
        }
    }
    Some((items, registry))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: [u8; 16] = [1; 16];
    const RENAMED: [u8; 16] = [2; 16];
    const SERVED: [u8; 16] = [3; 16];
    const CHEST: [u8; 16] = [9; 16];

    #[test]
    fn only_items_no_mod_serves_get_a_stand_in() {
        let recorded = [(ITEM, CHEST), (RENAMED, CHEST), (SERVED, CHEST)];
        let served = [
            Guid(SERVED).key_by_hash(types::ITEM_INFO),
            // Another type under the item's GUID does not count.
            Guid(ITEM).key_by_hash(types::TEMPLATE),
        ];
        let orphans = orphans(&recorded, &served, |guid| *guid == RENAMED);
        assert_eq!(orphans, [(Guid(ITEM), Guid(CHEST))]);
    }

    #[test]
    fn the_tags_are_the_loaders_own() {
        assert_ne!(NAME.guid(), DESCRIPTION.guid());
        assert_eq!(NAME.to_string(), "enshwick:stand-in.name");
    }
}
