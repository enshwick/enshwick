//! New workshops: a crafting station cloned from a vanilla one, with a
//! workshop of its own that recipes can target.
//!
//! A workshop is only an id. A placed station crafts at it because its
//! template's `Crafting`, `CraftingInteraction` and `ClientCraftingInteraction`
//! components carry the id, and recipes name it in `RecipeInfo.workshopId`.
//! The workshop registry maps the id to a name and the station item.

use super::items::{self, ItemLayout, NewItem};
use super::recipes::NewRecipe;
use super::{Blob, Guid, types};
use crate::{Load, Off};

/// The game's workshop registry resource.
pub const WORKSHOP_REGISTRY: Guid = Guid::parse("3baba1c2-bc82-4ab7-b20e-16dec28208af");

/// The game's list of every template.
pub const TEMPLATE_COLLECTION: Guid = Guid::parse("377f61dc-c7b8-4e4f-830f-7d2c3e8b7fd8");

/// Vanilla stations to clone: the station item and the template it places.
pub mod stations {
    use super::{Guid, Station};

    /// The Workbench (`Craftingstation_Workbench`).
    pub const WORKBENCH: Station = Station {
        item: Guid::parse("2f04a05f-697d-489b-a32f-76bb3439f5bc"),
        template: Guid::parse("ebf450c3-fb96-4240-a3be-8ebe0713eccd"),
    };
}

/// A vanilla crafting station: its item and the template the item places.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::workshops::stations;
///
/// let workbench = stations::WORKBENCH;
/// assert_ne!(workbench.item, workbench.template);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Station {
    /// The station item.
    pub item: Guid,
    /// The template the item places, its `placedEntity`.
    pub template: Guid,
}

/// Type hashes of the components that make a template a crafting station.
pub mod components {
    /// `ecs::Crafting`: the workshop the entity crafts at.
    pub const CRAFTING: u32 = 0xad77_6575;
    /// `ecs::CraftingInteraction`: the server side of the crafting menu.
    pub const CRAFTING_INTERACTION: u32 = 0x1225_4fdb;
    /// `ecs::ClientCraftingInteraction`: the client side of the crafting menu.
    pub const CLIENT_CRAFTING_INTERACTION: u32 = 0x5bd7_81ae;
}

/// A workshop: recipes made at it name its id.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
/// use enshwick_loader_api::content::workshops::WorkshopId;
///
/// let workbench = WorkshopId::new(Guid::parse("965d3192-0287-49a2-ad51-a716139571a6"));
/// assert_eq!(workbench.id(), 1_302_892_403);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorkshopId(Guid);

impl WorkshopId {
    /// The workshop with GUID `guid`.
    ///
    /// # Examples
    ///
    /// See [`WorkshopId`].
    #[must_use]
    pub const fn new(guid: Guid) -> Self {
        Self(guid)
    }

    /// The workshop's GUID.
    ///
    /// # Examples
    ///
    /// See [`WorkshopId`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.0
    }

    /// The game's id for the workshop: [`Guid::id`] of its GUID.
    ///
    /// # Examples
    ///
    /// See [`WorkshopId`].
    #[must_use]
    pub const fn id(self) -> u32 {
        self.0.id()
    }
}

/// How the new station is crafted: a copy of the vanilla station's recipe.
///
/// # Examples
///
/// See [`Load::add_workshop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StationRecipe {
    /// The recipe's GUID.
    pub guid: Guid,
    /// The vanilla recipe's ingredient counts times this.
    pub cost: u32,
}

/// A new workshop for [`Load::add_workshop`].
///
/// Every GUID is the mod's own and never changes: saves keep the template
/// GUID of each placed station and the item id of each stack.
///
/// # Examples
///
/// See [`Load::add_workshop`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewWorkshop {
    /// The vanilla station cloned: what the new one looks like and does.
    pub like: Station,
    /// The new station item.
    pub item: Guid,
    /// The new station's template.
    pub template: Guid,
    /// The new workshop.
    pub workshop: Guid,
    /// The tag of the station's name, which also names the workshop; `None`
    /// keeps the vanilla names.
    pub name: Option<Guid>,
    /// The tag of the station item's description; `None` keeps the vanilla one.
    pub description: Option<Guid>,
    /// How the station is crafted; `None` adds no recipe.
    pub station_recipe: Option<StationRecipe>,
    /// The item's name in the registry's debug list.
    pub debug_name: String,
}

/// Where the fields a workshop edit touches sit, from the game's reflection.
///
/// # Examples
///
/// See [`clone_template`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkshopLayout {
    /// `ecs::TemplateResource.components`.
    pub components: usize,
    /// `ecs::Crafting.workshop`, `.workshopId` and the component's size.
    pub crafting: [usize; 3],
    /// `ecs::CraftingInteraction.workshop`, `.workshopId` and its size.
    pub interaction: [usize; 3],
    /// `ecs::ClientCraftingInteraction.workshopId` and its size.
    pub client: [usize; 2],
    /// `ItemInfo.equipment` plus `EquipmentSetup.placedEntity`.
    pub placed_entity: usize,
    /// `ecs::TemplateCollectionResource.templates`.
    pub templates: usize,
    /// `WorkshopRegistryResource.workshops`.
    pub workshops: usize,
    /// `WorkshopResource` size.
    pub entry: usize,
    /// `WorkshopResource.workshopGuid`.
    pub entry_guid: usize,
    /// `WorkshopResource.workshopId`.
    pub entry_id: usize,
    /// `WorkshopResource.name`.
    pub entry_name: usize,
    /// `WorkshopResource.item`.
    pub entry_item: usize,
    /// `WorkshopResource.itemRef`.
    pub entry_item_ref: usize,
}

impl WorkshopLayout {
    /// The layout on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when a field or type is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let layout = enshwick_loader_api::content::workshops::WorkshopLayout::resolve(load)?;
    ///     assert_eq!(layout.entry, 56);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let field = |ty: &str, name: &str| {
            load.field_offset(ty, name)
                .map(|o| usize::try_from(o).unwrap_or(usize::MAX))
        };
        let size = |ty: &str| load.type_size(ty).map(|s| s as usize);
        let entry = |name: &str| field("keen::WorkshopResource", name);
        Ok(Self {
            components: field("keen::ecs::TemplateResource", "components")?,
            crafting: [
                field("keen::ecs::Crafting", "workshop")?,
                field("keen::ecs::Crafting", "workshopId")?,
                size("keen::ecs::Crafting")?,
            ],
            interaction: [
                field("keen::ecs::CraftingInteraction", "workshop")?,
                field("keen::ecs::CraftingInteraction", "workshopId")?,
                size("keen::ecs::CraftingInteraction")?,
            ],
            client: [
                field("keen::ecs::ClientCraftingInteraction", "workshopId")?,
                size("keen::ecs::ClientCraftingInteraction")?,
            ],
            placed_entity: field("keen::ItemInfo", "equipment")?
                .saturating_add(field("keen::EquipmentSetup", "placedEntity")?),
            templates: field("keen::ecs::TemplateCollectionResource", "templates")?,
            workshops: field("keen::WorkshopRegistryResource", "workshops")?,
            entry: size("keen::WorkshopResource")?,
            entry_guid: entry("workshopGuid")?,
            entry_id: entry("workshopId")?,
            entry_name: entry("name")?,
            entry_item: entry("item")?,
            entry_item_ref: entry("itemRef")?,
        })
    }
}

/// A `BlobVariant`: type hash, offset from its second word, size.
const VARIANT: usize = 12;
/// A GUID reference in a blob.
const REFERENCE: usize = 16;

/// The station template with its crafting components moved to `workshop`;
/// `None` unless it has all three, each of the reflected size.
/// [`Load::add_workshop`] serves it, with [`clone_station`],
/// [`list_template`] and [`list_workshop`]; call them from [`Load::serve`]
/// builders of your own for a station it does not make.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::types;
/// use enshwick_loader_api::content::workshops::{NewWorkshop, WorkshopLayout, clone_template};
///
/// fn init(
///     load: &mut enshwick_loader_api::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick_loader_api::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let (template, like, workshop) = (new.template, new.like.template, new.workshop);
///     load.serve(template.key(types::TEMPLATE), like.key(types::TEMPLATE), move |base| {
///         clone_template(&layout, base, workshop)
///     })
/// }
/// ```
#[must_use]
pub fn clone_template(layout: &WorkshopLayout, template: &[u8], workshop: Guid) -> Option<Vec<u8>> {
    let mut blob = Blob::new(template);
    let (start, count) = blob.array(layout.components)?;
    let id = workshop.id().to_le_bytes();
    let mut patched = [false; 3];
    for variant in (0..usize::try_from(count).ok()?).map(|i| start + i * VARIANT) {
        let hash = blob.read_u32(variant)?;
        let data = (variant + 4).checked_add(usize::try_from(blob.read_u32(variant + 4)?).ok()?)?;
        let size = usize::try_from(blob.read_u32(variant + 8)?).ok()?;
        let [guid_at, id_at, expected, slot] = match hash {
            components::CRAFTING => [
                layout.crafting[0],
                layout.crafting[1],
                layout.crafting[2],
                0,
            ],
            components::CRAFTING_INTERACTION => [
                layout.interaction[0],
                layout.interaction[1],
                layout.interaction[2],
                1,
            ],
            components::CLIENT_CRAFTING_INTERACTION => {
                [usize::MAX, layout.client[0], layout.client[1], 2]
            }
            _ => continue,
        };
        if size != expected {
            return None;
        }
        let written = (guid_at == usize::MAX || blob.write(data + guid_at, &workshop.0))
            && blob.write(data + id_at, &id);
        if !written {
            return None;
        }
        patched[slot] = true;
    }
    patched.iter().all(|&p| p).then(|| blob.into_bytes())
}

/// The new station item, cloned from the vanilla station in `like`, placing
/// the new template; `None` when the vanilla item does not place
/// `new.like.template`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::items::ItemLayout;
/// use enshwick_loader_api::content::types;
/// use enshwick_loader_api::content::workshops::{NewWorkshop, WorkshopLayout, clone_station};
///
/// fn init(
///     load: &mut enshwick_loader_api::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick_loader_api::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let items = ItemLayout::resolve(load)?;
///     let (item, like) = (new.item.key(types::ITEM_INFO), new.like.item.key(types::ITEM_INFO));
///     load.serve(item, like, move |base| clone_station(&items, &layout, base, &new))
/// }
/// ```
#[must_use]
pub fn clone_station(
    items: &ItemLayout,
    layout: &WorkshopLayout,
    like: &[u8],
    new: &NewWorkshop,
) -> Option<Vec<u8>> {
    let placed = like.get(layout.placed_entity..layout.placed_entity + REFERENCE)?;
    if placed != new.like.template.0 {
        return None;
    }
    let mut item = Blob::new(&items::clone_item(items, like, &item_of(new))?);
    item.write(layout.placed_entity, &new.template.0)
        .then(|| item.into_bytes())
}

/// The template collection with `template` listed after the others.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::types;
/// use enshwick_loader_api::content::workshops::{
///     NewWorkshop, TEMPLATE_COLLECTION, WorkshopLayout, list_template,
/// };
///
/// fn init(
///     load: &mut enshwick_loader_api::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick_loader_api::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let collection = TEMPLATE_COLLECTION.key(types::TEMPLATE_COLLECTION);
///     load.serve(collection, collection, move |base| list_template(&layout, base, new.template))
/// }
/// ```
#[must_use]
pub fn list_template(
    layout: &WorkshopLayout,
    collection: &[u8],
    template: Guid,
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(collection);
    blob.grow_array(
        layout.templates,
        REFERENCE,
        4,
        &[],
        &[(template.0.to_vec(), None)],
    )?;
    Some(blob.into_bytes())
}

/// The workshop registry with the new workshop after the others: a copy of
/// the vanilla station's entry with the new workshop, item and name.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::types;
/// use enshwick_loader_api::content::workshops::{
///     NewWorkshop, WORKSHOP_REGISTRY, WorkshopLayout, list_workshop,
/// };
///
/// fn init(
///     load: &mut enshwick_loader_api::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick_loader_api::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let registry = WORKSHOP_REGISTRY.key(types::WORKSHOP_REGISTRY);
///     load.serve(registry, registry, move |base| list_workshop(&layout, base, &new))
/// }
/// ```
#[must_use]
pub fn list_workshop(
    layout: &WorkshopLayout,
    registry: &[u8],
    new: &NewWorkshop,
) -> Option<Vec<u8>> {
    let mut blob = Blob::new(registry);
    let (start, count) = blob.array(layout.workshops)?;
    let like = new.like.item.id();
    let vanilla = (0..usize::try_from(count).ok()?)
        .map(|i| start + i * layout.entry)
        .find(|&at| blob.read_u32(at + layout.entry_item) == Some(like))?;
    let mut entry = blob.bytes().get(vanilla..vanilla + layout.entry)?.to_vec();
    let mut put = |at: usize, bytes: &[u8]| {
        entry
            .get_mut(at..at + bytes.len())
            .map(|target| target.copy_from_slice(bytes))
    };
    put(layout.entry_guid, &new.workshop.0)?;
    put(layout.entry_id, &new.workshop.id().to_le_bytes())?;
    put(layout.entry_item, &new.item.id().to_le_bytes())?;
    put(layout.entry_item_ref, &new.item.0)?;
    if let Some(name) = new.name {
        put(layout.entry_name, &name.id().to_le_bytes())?;
    }
    blob.grow_array(layout.workshops, layout.entry, 4, &[], &[(entry, None)])?;
    Some(blob.into_bytes())
}

/// The station item as a plain new item.
fn item_of(new: &NewWorkshop) -> NewItem {
    NewItem {
        guid: new.item,
        like: new.like.item,
        name: new.name,
        description: new.description,
        debug_name: new.debug_name.clone(),
    }
}

impl Load<'_> {
    /// Adds a crafting station with a workshop of its own, on the server and
    /// the client alike: the vanilla station's template and item cloned, the
    /// template crafting at the new workshop, the item placing the new
    /// template, both listed beside what other mods add, and optionally a
    /// copy of the vanilla station's recipe that makes the new one. Recipes
    /// target the workshop with [`NewRecipe::workshop`] or
    /// [`Derived::workshop`](super::recipes::Derived::workshop).
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the new one's GUID, so placed stations load as the vanilla station.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or a layout is not reflected; a
    /// station whose components or item do not match turns the mod off when
    /// the game first reads them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::content::workshops::{NewWorkshop, StationRecipe, stations};
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let bench = load.add_workshop(NewWorkshop {
    ///         like: stations::WORKBENCH,
    ///         item: Guid::parse("0851e93d-8883-42e4-a807-c30fd644e7c3"),
    ///         template: Guid::parse("77ff6155-1365-4a4a-8987-507a1bdd32a2"),
    ///         workshop: Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"),
    ///         name: None,
    ///         description: None,
    ///         station_recipe: Some(StationRecipe {
    ///             guid: Guid::parse("94cf68a3-11e0-4c05-a793-526f7e1d7693"),
    ///             cost: 1,
    ///         }),
    ///         debug_name: "Bulk Workbench".into(),
    ///     })?;
    ///     assert_eq!(bench.guid(), Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"));
    ///     Ok(())
    /// }
    /// ```
    pub fn add_workshop(&mut self, new: NewWorkshop) -> Result<WorkshopId, Off> {
        let layout = WorkshopLayout::resolve(self)?;
        let item_layout = ItemLayout::resolve(self)?;
        let workshop = WorkshopId::new(new.workshop);
        let (like, template) = (new.like, new.template);
        self.serve(
            template.key(types::TEMPLATE),
            like.template.key(types::TEMPLATE),
            move |base| clone_template(&layout, base, workshop.guid()),
        )?;
        let collection = TEMPLATE_COLLECTION.key(types::TEMPLATE_COLLECTION);
        self.serve(collection, collection, move |base| {
            list_template(&layout, base, template)
        })?;
        let station = new.clone();
        self.serve(
            new.item.key(types::ITEM_INFO),
            like.item.key(types::ITEM_INFO),
            move |base| clone_station(&item_layout, &layout, base, &station),
        )?;
        let (item, debug_name) = (new.item, new.debug_name.clone());
        let registry = items::ITEM_REGISTRY.key(types::ITEM_REGISTRY);
        self.serve(registry, registry, move |base| {
            items::list_item(&item_layout, base, item, &debug_name)
        })?;
        let recipe = new.station_recipe;
        let registry = WORKSHOP_REGISTRY.key(types::WORKSHOP_REGISTRY);
        self.serve(registry, registry, move |base| {
            list_workshop(&layout, base, &new)
        })?;
        if let Some(recipe) = recipe {
            self.add_recipe(NewRecipe {
                guid: recipe.guid,
                like: like.item,
                output: item,
                output_count: 1,
                cost: recipe.cost,
                workshop: None,
            })?;
        }
        Ok(workshop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYOUT: WorkshopLayout = WorkshopLayout {
        components: 16,
        crafting: [0, 16, 20],
        interaction: [12, 28, 32],
        client: [12, 16],
        placed_entity: 0x3c0,
        templates: 0,
        workshops: 8,
        entry: 56,
        entry_guid: 4,
        entry_id: 20,
        entry_name: 24,
        entry_item: 28,
        entry_item_ref: 32,
    };
    const ITEMS: ItemLayout = ItemLayout {
        id: 0,
        object: 4,
        name: 0x48,
        description: 0x68,
        refs: 0,
        names: 0x38,
    };
    const WORKSHOP: Guid = Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6");
    const ITEM: Guid = Guid::parse("0851e93d-8883-42e4-a807-c30fd644e7c3");
    const TEMPLATE: Guid = Guid::parse("77ff6155-1365-4a4a-8987-507a1bdd32a2");
    const NAME: Guid = Guid::parse("41b222ea-cefa-4774-a040-c43aa9f0b227");
    const OLD: Guid = Guid::parse("965d3192-0287-49a2-ad51-a716139571a6");

    fn new() -> NewWorkshop {
        NewWorkshop {
            like: stations::WORKBENCH,
            item: ITEM,
            template: TEMPLATE,
            workshop: WORKSHOP,
            name: Some(NAME),
            description: None,
            station_recipe: None,
            debug_name: "Bench".into(),
        }
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// A template with a transform-like component and the three crafting
    /// ones, all at the old workshop; returns the blob and each data offset.
    fn station(components: &[(u32, usize)]) -> (Blob, Vec<usize>) {
        let mut blob = Blob::new(&[0u8; 24]);
        let variants = blob.append(&vec![0; components.len() * VARIANT], 4);
        let mut data = Vec::new();
        for (i, &(hash, size)) in components.iter().enumerate() {
            let mut bytes = vec![0xee; size];
            if size >= 20 {
                bytes[..16].copy_from_slice(&OLD.0);
            }
            let at = blob.append(&bytes, 4);
            let variant = variants + i * VARIANT;
            let relative = u32::try_from(at - (variant + 4)).unwrap();
            assert!(blob.write(variant, &hash.to_le_bytes()));
            assert!(blob.write(variant + 4, &relative.to_le_bytes()));
            assert!(blob.write(variant + 8, &u32::try_from(size).unwrap().to_le_bytes()));
            data.push(at);
        }
        let count = u32::try_from(components.len()).unwrap();
        assert!(blob.set_array(16, variants, count));
        (blob, data)
    }

    #[test]
    fn the_template_clone_crafts_at_the_new_workshop_only() {
        let (blob, data) = station(&[
            (0x1234_5678, 24),
            (components::CRAFTING, 20),
            (components::CRAFTING_INTERACTION, 32),
            (components::CLIENT_CRAFTING_INTERACTION, 16),
        ]);
        let clone = clone_template(&LAYOUT, blob.bytes(), WORKSHOP).unwrap();
        let id = WORKSHOP.id();
        assert_eq!(&clone[data[0]..data[0] + 16], &OLD.0, "others stay");
        assert_eq!(&clone[data[1]..data[1] + 16], &WORKSHOP.0);
        assert_eq!(u32_at(&clone, data[1] + 16), id);
        assert_eq!(&clone[data[2] + 12..data[2] + 28], &WORKSHOP.0);
        assert_eq!(u32_at(&clone, data[2] + 28), id);
        assert_eq!(
            &clone[data[2]..data[2] + 12],
            &blob.bytes()[data[2]..data[2] + 12],
            "props in range and comfort stay"
        );
        assert_eq!(u32_at(&clone, data[3] + 12), id);
        assert_eq!(clone.len(), blob.bytes().len());
    }

    #[test]
    fn a_template_missing_a_component_or_of_another_size_is_refused() {
        let (blob, _) = station(&[
            (components::CRAFTING, 20),
            (components::CRAFTING_INTERACTION, 32),
        ]);
        assert!(clone_template(&LAYOUT, blob.bytes(), WORKSHOP).is_none());
        let (blob, _) = station(&[
            (components::CRAFTING, 24),
            (components::CRAFTING_INTERACTION, 32),
            (components::CLIENT_CRAFTING_INTERACTION, 16),
        ]);
        assert!(clone_template(&LAYOUT, blob.bytes(), WORKSHOP).is_none());
    }

    #[test]
    fn the_station_item_places_the_new_template() {
        let mut bench = vec![0u8; 0x400];
        bench[0x3c0..0x3d0].copy_from_slice(&stations::WORKBENCH.template.0);
        let clone = clone_station(&ITEMS, &LAYOUT, &bench, &new()).unwrap();
        assert_eq!(&clone[..4], &ITEM.id().to_le_bytes());
        assert_eq!(&clone[0x3c0..0x3d0], &TEMPLATE.0);
        assert_eq!(&clone[0x48..0x58], &NAME.0);
        // An item placing something else is not the station named.
        bench[0x3c0] ^= 1;
        assert!(clone_station(&ITEMS, &LAYOUT, &bench, &new()).is_none());
    }

    #[test]
    fn the_registries_list_the_new_workshop_and_template_after_the_others() {
        let mut blob = Blob::new(&[0u8; 24]);
        let mut entry = vec![0u8; 56];
        entry[4..20].copy_from_slice(&OLD.0);
        entry[20..24].copy_from_slice(&OLD.id().to_le_bytes());
        entry[24..28].copy_from_slice(&7u32.to_le_bytes());
        entry[28..32].copy_from_slice(&stations::WORKBENCH.item.id().to_le_bytes());
        entry[48..52].copy_from_slice(&3u32.to_le_bytes());
        let at = blob.append(&entry, 4);
        assert!(blob.set_array(8, at, 1));
        let built = Blob::new(&list_workshop(&LAYOUT, blob.bytes(), &new()).unwrap());
        let (start, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let added = &built.bytes()[start + 56..start + 112];
        assert_eq!(&added[4..20], &WORKSHOP.0);
        assert_eq!(u32_at(added, 20), WORKSHOP.id());
        assert_eq!(u32_at(added, 24), NAME.id());
        assert_eq!(u32_at(added, 28), ITEM.id());
        assert_eq!(&added[32..48], &ITEM.0);
        assert_eq!(u32_at(added, 48), 3, "upgrade masks stay");
        assert_eq!(&built.bytes()[start + 4..start + 20], &OLD.0);

        let mut collection = Blob::new(&[0u8; 8]);
        let first = collection.append(&OLD.0, 4);
        assert!(collection.set_array(0, first, 1));
        let listed = Blob::new(&list_template(&LAYOUT, collection.bytes(), TEMPLATE).unwrap());
        let (start, count) = listed.array(0).unwrap();
        assert_eq!(count, 2);
        assert_eq!(&listed.bytes()[start + 16..start + 32], &TEMPLATE.0);
    }

    #[test]
    fn a_registry_without_the_vanilla_station_builds_nothing() {
        let mut blob = Blob::new(&[0u8; 24]);
        assert!(blob.set_array(8, 24, 0));
        assert!(list_workshop(&LAYOUT, blob.bytes(), &new()).is_none());
    }
}
