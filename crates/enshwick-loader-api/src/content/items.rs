//! New items: an item cloned from a vanilla one, listed in the item registry.

use super::{Blob, Guid, types};
use crate::{Load, Off};

/// The game's item registry resource.
pub const ITEM_REGISTRY: Guid = Guid::parse("3064d35d-7342-40ca-bdc9-aad58f83bf45");

/// A new item for [`Load::add_item`].
///
/// # Examples
///
/// See [`Load::add_item`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewItem {
    /// The new item's GUID; its game id is [`Guid::id`].
    pub guid: Guid,
    /// The vanilla item it is cloned from: what it looks like and does.
    pub like: Guid,
    /// The tag of its name, given text with [`Load::add_texts`]; `None` keeps
    /// the vanilla item's name.
    pub name: Option<Guid>,
    /// The tag of its description; `None` keeps the vanilla one.
    pub description: Option<Guid>,
    /// The name in the registry's debug list.
    pub debug_name: String,
}

/// Where the fields an item edit touches sit, from the game's reflection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemLayout {
    /// `ItemInfo.itemId`.
    pub id: usize,
    /// `ItemInfo.objectId`.
    pub object: usize,
    /// `ItemInfo.name`.
    pub name: usize,
    /// `ItemInfo.description`.
    pub description: usize,
    /// `ItemRegistryResource.itemRefs`.
    pub refs: usize,
    /// `ItemRegistryResource.dbgNames`.
    pub names: usize,
}

impl ItemLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when a field is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let layout = enshwick_loader_api::content::items::ItemLayout::resolve(load)?;
    ///     assert_eq!(layout.id, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let field = |ty: &str, name: &str| {
            load.field_offset(ty, name)
                .map(|o| usize::try_from(o).unwrap_or(usize::MAX))
        };
        Ok(Self {
            id: field("keen::ItemInfo", "itemId")?,
            object: field("keen::ItemInfo", "objectId")?,
            name: field("keen::ItemInfo", "name")?,
            description: field("keen::ItemInfo", "description")?,
            refs: field("keen::ItemRegistryResource", "itemRefs")?,
            names: field("keen::ItemRegistryResource", "dbgNames")?,
        })
    }
}

/// A GUID reference in a blob.
const REFERENCE: usize = 16;
/// A blob string header.
const BLOB_STRING: usize = 8;

/// `item`'s `ItemInfo`, made from the vanilla one in `like`.
#[must_use]
pub fn clone_item(layout: &ItemLayout, like: &[u8], item: &NewItem) -> Option<Vec<u8>> {
    let mut info = Blob::new(like);
    let mut written = info.write(layout.id, &item.guid.id().to_le_bytes())
        && info.write(layout.object, &item.guid.0);
    if let Some(name) = item.name {
        written &= info.write(layout.name, &name.0);
    }
    if let Some(description) = item.description {
        written &= info.write(layout.description, &description.0);
    }
    written.then(|| info.into_bytes())
}

/// The item registry with `guid` listed after the others, and its debug name
/// beside it.
#[must_use]
pub fn list_item(
    layout: &ItemLayout,
    registry: &[u8],
    guid: Guid,
    debug_name: &str,
) -> Option<Vec<u8>> {
    let mut registry = Blob::new(registry);
    let (_, refs) = registry.array(layout.refs)?;
    let (_, names) = registry.array(layout.names)?;
    registry.grow_array(layout.refs, REFERENCE, 4, &[], &[(guid.0.to_vec(), None)])?;
    // The debug names run beside the references; keep them in step.
    if names == refs {
        let mut text = debug_name.as_bytes().to_vec();
        text.push(0);
        let at = registry.append(&text, 1);
        let first = registry.grow_array(
            layout.names,
            BLOB_STRING,
            4,
            &[0],
            &[(vec![0; BLOB_STRING], None)],
        )?;
        let (start, _) = registry.array(layout.names)?;
        let element = start + usize::try_from(first).ok()? * BLOB_STRING;
        if !registry.set_array(element, at, u32::try_from(debug_name.len()).ok()?) {
            return None;
        }
    }
    Some(registry.into_bytes())
}

impl Load<'_> {
    /// Adds an item to the game, on the server and the client alike: an
    /// `ItemInfo` cloned from a vanilla item, listed in the item registry
    /// beside items other mods add. Players see it once a recipe, loot or a
    /// command gives it to them.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or the item layout is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::content::items::NewItem;
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_item(NewItem {
    ///         guid: Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         name: None,
    ///         description: None,
    ///         debug_name: "Safe".into(),
    ///     })
    /// }
    /// ```
    pub fn add_item(&mut self, item: NewItem) -> Result<(), Off> {
        let layout = ItemLayout::resolve(self)?;
        let (guid, like, debug_name) = (item.guid, item.like, item.debug_name.clone());
        self.serve(
            guid.key(types::ITEM_INFO),
            like.key(types::ITEM_INFO),
            move |base| clone_item(&layout, base, &item),
        )?;
        let registry = ITEM_REGISTRY.key(types::ITEM_REGISTRY);
        self.serve(registry, registry, move |base| {
            list_item(&layout, base, guid, &debug_name)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: ItemLayout = ItemLayout {
        id: 0,
        object: 4,
        name: 0x48,
        description: 0x68,
        refs: 0,
        names: 0x38,
    };
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const NEW: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");

    fn string(blob: &Blob, header: usize) -> Vec<u8> {
        let (at, len) = blob.array(header).unwrap();
        blob.bytes()[at..at + len as usize].to_vec()
    }

    #[test]
    fn a_clone_takes_the_new_identity_and_keeps_the_rest() {
        let mut chest = vec![0u8; 0x80];
        chest[4..20].copy_from_slice(&CHEST.0);
        chest[0x70] = 9;
        let item = NewItem {
            guid: NEW,
            like: CHEST,
            name: Some(NAME),
            description: None,
            debug_name: "New".into(),
        };
        let clone = clone_item(&LAYOUT, &chest, &item).unwrap();
        assert_eq!(&clone[..4], &NEW.id().to_le_bytes());
        assert_eq!(&clone[4..20], &NEW.0);
        assert_eq!(&clone[0x48..0x58], &NAME.0);
        assert_eq!(clone[0x70], 9);
    }

    #[test]
    fn two_mods_can_list_items_one_after_the_other() {
        let mut blob = Blob::new(&[0u8; 0x48]);
        let reference = blob.append(&CHEST.0, 4);
        let text = blob.append(b"Chest\0", 1);
        let name = blob.append(&[0; 8], 4);
        assert!(blob.set_array(name, text, 5));
        assert!(blob.set_array(0, reference, 1));
        assert!(blob.set_array(0x38, name, 1));
        let once = list_item(&LAYOUT, blob.bytes(), NEW, "New").unwrap();
        let twice = Blob::new(&list_item(&LAYOUT, &once, NAME, "Other").unwrap());
        let (refs, count) = twice.array(0).unwrap();
        assert_eq!(count, 3);
        assert_eq!(&twice.bytes()[refs + 32..refs + 48], &NAME.0);
        let (names, _) = twice.array(0x38).unwrap();
        assert_eq!(string(&twice, names), b"Chest");
        assert_eq!(string(&twice, names + 8), b"New");
        assert_eq!(string(&twice, names + 16), b"Other");
    }
}
