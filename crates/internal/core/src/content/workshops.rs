//! New workshops: a crafting station cloned from a vanilla one, with a
//! workshop of its own that recipes can target.
//!
//! A workshop is only an id. A placed station crafts at it because its
//! template's crafting components (the station's own, the server's and the
//! client's side of its menu) carry the id, and recipes name it.
//! The workshop registry maps the id to a name and the station item.

use crate::engine_ids as ids;
use crate::{Load, Off};

use super::items::{self, ItemLayout, NewItem};
use super::recipes::NewRecipe;
use super::templates::{ComponentChange, NewTemplate};
use super::{Blob, EngineFacts, Guid, Id, types};

/// The game's workshop registry resource.
pub const WORKSHOP_REGISTRY: Guid = Guid::parse("3baba1c2-bc82-4ab7-b20e-16dec28208af");

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
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::workshops::stations;
///
/// let workbench = stations::WORKBENCH;
/// assert_ne!(workbench.item, workbench.template);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct Station {
    /// The station item.
    pub item: Guid,
    /// The template the item places.
    pub template: Guid,
}

/// A workshop: recipes made at it name its id.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::Guid;
/// use enshwick::content::workshops::WorkshopId;
///
/// let workbench = WorkshopId::new(Guid::parse("965d3192-0287-49a2-ad51-a716139571a6"));
/// assert_eq!(workbench.id(), 1_302_892_403);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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

    /// The game's id for the workshop: [`Guid::fnv1a`] of its GUID.
    ///
    /// # Examples
    ///
    /// See [`WorkshopId`].
    #[must_use]
    pub const fn id(self) -> u32 {
        self.0.fnv1a()
    }
}

/// A new workshop for [`Load::add_workshop`], named by one id.
///
/// The station item is `id`; the station's other ids derive from it with
/// [`Id::part`]: its template `<id>.template`, its workshop `<id>.workshop`,
/// its recipe `<id>.recipe`, and the tags of its name and description,
/// `<id>.name` and `<id>.description`. So `id` is a plain id, never one
/// derived with [`Id::part`], even when the same id written in full would
/// fit. Every id is the mod's own and never
/// changes: saves keep the template GUID of each placed station and the item
/// id of each stack.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::{Id, id};
/// use enshwick::content::workshops::{NewWorkshop, stations};
///
/// const BENCH: Id = id!("bulk-bench");
///
/// let bench = NewWorkshop::new(BENCH, stations::WORKBENCH);
/// assert_eq!(bench.debug_name, BENCH.to_string());
/// assert_eq!(bench.workshop(), BENCH.part("workshop"));
/// assert_eq!(bench.name(), BENCH.part("name"));
/// assert!(!bench.named && bench.recipe_cost.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct NewWorkshop {
    /// The vanilla station cloned: what the new one looks like and does.
    pub like: Station,
    /// The new station item, whose id the station's other ids derive from.
    pub id: Id,
    /// Whether the station takes the name [`NewWorkshop::name`], which also
    /// names the workshop; `false` keeps the vanilla names. Give the tag its
    /// text with [`Load::add_texts`].
    pub named: bool,
    /// Whether the station item takes the description
    /// [`NewWorkshop::description`]; `false` keeps the vanilla one.
    pub described: bool,
    /// With a cost, the station is crafted by the recipe
    /// [`NewWorkshop::recipe`], a copy of the vanilla station's whose
    /// ingredient counts are times this; `None` adds no recipe.
    pub recipe_cost: Option<u32>,
    /// The item's name in the registry's debug list; the id unless set.
    pub debug_name: String,
}

impl NewWorkshop {
    /// The station `id` cloned from `like`, with the vanilla names and no
    /// recipe.
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub fn new(id: Id, like: Station) -> Self {
        Self {
            like,
            id,
            named: false,
            described: false,
            recipe_cost: None,
            debug_name: id.to_string(),
        }
    }

    /// The new station's template, `<id>.template`.
    ///
    /// # Panics
    ///
    /// When `id` is a part already or too long to take the part; see
    /// [`Id::part`]. [`Load::add_workshop`] refuses such an id first.
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub const fn template(&self) -> Id {
        self.id.part(TEMPLATE)
    }

    /// The new workshop, `<id>.workshop`, which recipes target.
    ///
    /// # Panics
    ///
    /// As [`NewWorkshop::template`].
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub const fn workshop(&self) -> Id {
        self.id.part(WORKSHOP)
    }

    /// The recipe that makes the station, `<id>.recipe`.
    ///
    /// # Panics
    ///
    /// As [`NewWorkshop::template`].
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub const fn recipe(&self) -> Id {
        self.id.part(RECIPE)
    }

    /// The tag of the station's name, `<id>.name`.
    ///
    /// # Panics
    ///
    /// As [`NewWorkshop::template`].
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub const fn name(&self) -> Id {
        self.id.part(NAME)
    }

    /// The tag of the station item's description, `<id>.description`.
    ///
    /// # Panics
    ///
    /// As [`NewWorkshop::template`].
    ///
    /// # Examples
    ///
    /// See [`NewWorkshop`].
    #[must_use]
    pub const fn description(&self) -> Id {
        self.id.part(DESCRIPTION)
    }

    /// Whether every derived id fits; the reason when one does not.
    pub(crate) fn check(&self) -> Result<(), Off> {
        for part in [TEMPLATE, WORKSHOP, RECIPE, NAME, DESCRIPTION] {
            if self.id.try_part(part).is_none() {
                return Err(Off::new(format!(
                    "workshop {}: the id takes no part `.{part}`; name the station by a plain id \
                     of at most {} characters after the mod's",
                    self.id,
                    128 - 1 - DESCRIPTION.len(),
                )));
            }
        }
        Ok(())
    }
}

/// The parts of a station's derived ids.
const TEMPLATE: &str = "template";
const WORKSHOP: &str = "workshop";
const RECIPE: &str = "recipe";
const NAME: &str = super::NAME_PART;
const DESCRIPTION: &str = "description";

/// Where the fields a workshop edit touches sit on the running build,
/// resolved by the loader.
///
/// # Examples
///
/// See [`list_workshop`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkshopLayout {
    /// A template's components.
    pub components: usize,
    /// The hashes of the three crafting component types: the workshop the
    /// entity crafts at, then the server and the client side of its menu.
    pub crafting_types: [u32; 3],
    /// The template an item places, inside the item data.
    pub placed_entity: usize,
    /// The template collection's list of templates.
    pub templates: usize,
    /// The workshop registry's list of workshops.
    pub workshops: usize,
    /// One workshop entry's size.
    pub entry: usize,
    /// The entry's workshop GUID.
    pub entry_guid: usize,
    /// The entry's workshop id.
    pub entry_id: usize,
    /// The entry's name tag.
    pub entry_name: usize,
    /// The entry's station item id.
    pub entry_item: usize,
    /// The entry's station item GUID.
    pub entry_item_ref: usize,
    /// The entry's flag set for a survivor's workshop.
    pub entry_npc: usize,
}

impl WorkshopLayout {
    /// The layout `facts` give: the running game's through [`Load`], a game
    /// image's in a tool.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let layout = enshwick::content::workshops::WorkshopLayout::resolve(load)?;
    ///     assert_eq!(layout.entry, 56);
    ///     Ok(())
    /// }
    /// ```
    pub fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        let at = |id: &str| facts.offset(id);
        Ok(Self {
            components: at(ids::TEMPLATE_COMPONENTS)?,
            crafting_types: [
                facts.hash(ids::CRAFTING)?,
                facts.hash(ids::CRAFTING_INTERACTION)?,
                facts.hash(ids::CLIENT_CRAFTING_INTERACTION)?,
            ],
            placed_entity: at(ids::ITEM_INFO_PLACED_ENTITY)?,
            templates: at(ids::TEMPLATE_COLLECTION_TEMPLATES)?,
            workshops: at(ids::WORKSHOP_REGISTRY_WORKSHOPS)?,
            entry: facts.size(ids::WORKSHOP)?,
            entry_guid: at(ids::WORKSHOP_GUID)?,
            entry_id: at(ids::WORKSHOP_ID)?,
            entry_name: at(ids::WORKSHOP_NAME)?,
            entry_item: at(ids::WORKSHOP_ITEM)?,
            entry_item_ref: at(ids::WORKSHOP_ITEM_REF)?,
            entry_npc: at(ids::WORKSHOP_NPC)?,
        })
    }
}

/// A GUID reference in a blob.
const REFERENCE: usize = 16;

/// The changes that make a station template craft at `workshop`: its
/// crafting component and both sides of its menu name the workshop, at the
/// offsets `facts` give. A template without all three refuses them.
///
/// # Errors
///
/// [`Off`] when the build lacks an offset.
///
/// # Examples
///
/// ```
/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
/// use enshwick::content::Guid;
/// use enshwick::content::workshops::{WorkshopId, at_workshop};
///
/// let host = enshwick::test::FakeHost::new().workshops();
/// let changes = at_workshop(&host, WorkshopId::new(Guid::named("you.mod:bench.workshop")))?;
/// assert_eq!(changes[0].component(), "crafting");
/// # Ok::<(), enshwick::Off>(())
/// # }
/// ```
pub fn at_workshop(
    facts: &(impl EngineFacts + ?Sized),
    workshop: WorkshopId,
) -> Result<Vec<ComponentChange>, Off> {
    let (guid, id) = (workshop.guid().0, workshop.id().to_le_bytes());
    let edit = |component, guid_at: Option<usize>, id_at: usize| {
        ComponentChange::edit_bytes(component, move |bytes| {
            guid_at.is_none_or(|at| put(bytes, at, &guid)) && put(bytes, id_at, &id)
        })
    };
    Ok(vec![
        edit(
            ids::CRAFTING,
            Some(facts.offset(ids::CRAFTING_WORKSHOP)?),
            facts.offset(ids::CRAFTING_WORKSHOP_ID)?,
        ),
        edit(
            ids::CRAFTING_INTERACTION,
            Some(facts.offset(ids::CRAFTING_INTERACTION_WORKSHOP)?),
            facts.offset(ids::CRAFTING_INTERACTION_WORKSHOP_ID)?,
        ),
        edit(
            ids::CLIENT_CRAFTING_INTERACTION,
            None,
            facts.offset(ids::CLIENT_CRAFTING_INTERACTION_WORKSHOP_ID)?,
        ),
    ])
}

/// Writes `data` into `bytes` at `at`; false when it does not fit.
pub(crate) fn put(bytes: &mut [u8], at: usize, data: &[u8]) -> bool {
    at.checked_add(data.len())
        .and_then(|end| bytes.get_mut(at..end))
        .map(|target| target.copy_from_slice(data))
        .is_some()
}

/// The new station item, cloned from the vanilla station in `like`, placing
/// the new template; `None` when the vanilla item does not place
/// `new.like.template`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::items::ItemLayout;
/// use enshwick::content::types;
/// use enshwick::content::workshops::{NewWorkshop, WorkshopLayout, clone_station};
///
/// fn init(
///     load: &mut enshwick::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let items = ItemLayout::resolve(load)?;
///     let (item, like) = (types::ITEM_INFO.key(new.id)?, types::ITEM_INFO.key(new.like.item)?);
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
    let mut item = Blob::new(&items::clone_item(
        items,
        like,
        &item_of(new)?,
        new.id.guid(),
    )?);
    item.write(layout.placed_entity, &new.id.try_part(TEMPLATE)?.guid().0)
        .then(|| item.into_bytes())
}

/// The workshop registry with the new workshop after the others: a copy of
/// the vanilla station's entry with the new workshop, item and name.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::types;
/// use enshwick::content::workshops::{
///     NewWorkshop, WORKSHOP_REGISTRY, WorkshopLayout, list_workshop,
/// };
///
/// fn init(
///     load: &mut enshwick::Load<'_>,
///     new: NewWorkshop,
/// ) -> Result<(), enshwick::Off> {
///     let layout = WorkshopLayout::resolve(load)?;
///     let registry = types::WORKSHOP_REGISTRY_RESOURCE.key(WORKSHOP_REGISTRY)?;
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
    let like = new.like.item.fnv1a();
    let vanilla = (0..usize::try_from(count).ok()?)
        .map(|i| start + i * layout.entry)
        .find(|&at| blob.read_u32(at + layout.entry_item) == Some(like))?;
    let mut entry = blob.bytes().get(vanilla..vanilla + layout.entry)?.to_vec();
    let mut put = |at: usize, bytes: &[u8]| {
        entry
            .get_mut(at..at + bytes.len())
            .map(|target| target.copy_from_slice(bytes))
    };
    let workshop = new.id.try_part(WORKSHOP)?.guid();
    put(layout.entry_guid, &workshop.0)?;
    put(layout.entry_id, &workshop.fnv1a().to_le_bytes())?;
    put(
        layout.entry_item,
        &crate::ItemId::from_guid(new.id.guid()).get().to_le_bytes(),
    )?;
    put(layout.entry_item_ref, &new.id.guid().0)?;
    if new.named {
        let name = new.id.try_part(NAME)?.guid();
        put(layout.entry_name, &name.fnv1a().to_le_bytes())?;
    }
    blob.grow_array(layout.workshops, layout.entry, 4, &[], &[(entry, None)])?;
    Some(blob.into_bytes())
}

/// The station item as a plain new item; `None` when its tags do not fit.
fn item_of(new: &NewWorkshop) -> Option<NewItem> {
    let tag = |on: bool, part| {
        if on {
            new.id.try_part(part).map(|tag| Some(tag.guid()))
        } else {
            Some(None)
        }
    };
    let (name, description) = (tag(new.named, NAME)?, tag(new.described, DESCRIPTION)?);
    Some(NewItem {
        name,
        description,
        debug_name: new.debug_name.clone(),
        ..NewItem::new(new.id, new.like.item)
    })
}

#[crate::script(content_workshops)]
impl Load<'_> {
    /// Adds a crafting station with a workshop of its own, on the server and
    /// the client alike: the vanilla station's template and item cloned, the
    /// template crafting at the new workshop, the item placing the new
    /// template, both listed beside what other mods add, a crafting menu
    /// for the workshop on the client, and optionally a copy of the vanilla
    /// station's recipe that makes the new one. Recipes
    /// target the workshop with [`NewRecipe::workshop`] or
    /// [`Derived::workshop`](super::recipes::Derived::workshop).
    ///
    /// When the mod is removed, the loader serves the vanilla template under
    /// the new one's GUID, so placed stations load as the vanilla station.
    ///
    /// # Errors
    ///
    /// [`Off`] when a layout is not reflected, the id takes no derived part
    /// (see [`Id::try_part`]), the vanilla station's template lacks a
    /// crafting component (see [`at_workshop`]) or the loader refuses one of
    /// the ids (see [`Load::declare`]); a station item that does not place
    /// the station's template turns the mod off when the game first reads it.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Id, id};
    /// use enshwick::content::workshops::{NewWorkshop, stations};
    ///
    /// const BENCH: Id = id!("bench");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let bench = load.add_workshop(NewWorkshop {
    ///         recipe_cost: Some(1),
    ///         ..NewWorkshop::new(BENCH, stations::WORKBENCH)
    ///     })?;
    ///     assert_eq!(bench.guid(), BENCH.part("workshop").guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_workshop(&mut self, new: NewWorkshop) -> Result<WorkshopId, Off> {
        new.check()?;
        let (template, recipe) = (new.template(), new.recipe());
        for id in [new.id, new.workshop()] {
            self.declare(id)?;
        }
        let layout = WorkshopLayout::resolve(self)?;
        let item_layout = ItemLayout::resolve(self)?;
        let workshop = WorkshopId::new(new.workshop().guid());
        let like = new.like;
        self.add_template(NewTemplate {
            changes: at_workshop(self, workshop)?,
            ..NewTemplate::new(template, like.template)
        })?;
        let station = new.clone();
        self.serve(
            types::ITEM_INFO.key(new.id.guid())?,
            types::ITEM_INFO.key(like.item)?,
            move |base| clone_station(&item_layout, &layout, base, &station),
        )?;
        let (item, debug_name) = (new.id.guid(), new.debug_name.clone());
        let registry = types::ITEM_REGISTRY_RESOURCE.key(items::ITEM_REGISTRY)?;
        self.serve(registry, registry, move |base| {
            items::list_item(&item_layout, base, item, &debug_name)
        })?;
        let cost = new.recipe_cost;
        let registry = types::WORKSHOP_REGISTRY_RESOURCE.key(WORKSHOP_REGISTRY)?;
        self.serve(registry, registry, move |base| {
            list_workshop(&layout, base, &new)
        })?;
        super::menu::serve(self, move |menu, bundle| {
            super::menu::list_workshop(menu, bundle, workshop)
        })?;
        if let Some(cost) = cost {
            self.push_recipe(NewRecipe {
                id: recipe,
                like: like.item,
                output: item,
                output_count: 1,
                cost,
                workshop: None,
            })?;
        }
        Ok(workshop)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const LAYOUT: WorkshopLayout = WorkshopLayout {
        components: 16,
        crafting_types: [CRAFTING, INTERACTION, CLIENT],
        placed_entity: 0x3c0,
        templates: 0,
        workshops: 8,
        entry: 56,
        entry_guid: 4,
        entry_id: 20,
        entry_name: 24,
        entry_item: 28,
        entry_item_ref: 32,
        entry_npc: 0,
    };
    const ITEMS: ItemLayout = ItemLayout {
        id: 0,
        object: 4,
        name: 0x48,
        description: 0x68,
        flags: 0x78,
        pid_entity: 0x80,
        ui_values: 0x90,
        icon_image: 0x58,
        ui_value: [0x28, 0, 4, 8, 0x24],
        refs: 0,
        names: 0x38,
    };
    pub(crate) const CRAFTING: u32 = 0xc1;
    pub(crate) const INTERACTION: u32 = 0xc2;
    pub(crate) const CLIENT: u32 = 0xc3;
    const ITEM_ID: Id = Id::new("test:bench");
    const WORKSHOP: Guid = Guid::named("test:bench.workshop");
    const ITEM: Guid = ITEM_ID.guid();
    const TEMPLATE: Guid = Guid::named("test:bench.template");
    const NAME: Guid = Guid::named("test:bench.name");
    const OLD: Guid = Guid::parse("965d3192-0287-49a2-ad51-a716139571a6");

    fn new() -> NewWorkshop {
        NewWorkshop {
            named: true,
            debug_name: "Bench".into(),
            ..NewWorkshop::new(ITEM_ID, stations::WORKBENCH)
        }
    }

    #[test]
    fn a_workshop_derives_its_ids_from_the_station_s() {
        let new = new();
        assert_eq!(new.template().guid(), TEMPLATE);
        assert_eq!(new.workshop().guid(), WORKSHOP);
        assert_eq!(new.recipe(), Id::new("test:bench.recipe"));
        assert_eq!(new.name().guid(), NAME);
        assert_eq!(new.description(), Id::new("test:bench.description"));
        assert!(new.check().is_ok());
    }

    #[test]
    fn a_station_id_without_room_for_its_parts_is_refused() {
        let derived = NewWorkshop::new(ITEM_ID.part("old"), stations::WORKBENCH);
        assert!(derived.check().is_err());
        let long: &'static str = Box::leak(format!("test:{}", "b".repeat(120)).into_boxed_str());
        let too_long = NewWorkshop::new(Id::new(long), stations::WORKBENCH);
        assert!(too_long.check().is_err());
        assert!(list_workshop(&LAYOUT, &[0; 64], &too_long).is_none());
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    /// A `BlobVariant`: type hash, offset from its second word, size.
    const VARIANT: usize = 12;

    /// A template with the components `components`, by type hash and size,
    /// at the old workshop; returns the blob and each data offset.
    pub(crate) fn station(components: &[(u32, usize)]) -> (Blob, Vec<usize>) {
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
    fn a_write_inside_the_bytes_lands_and_one_past_their_end_is_refused() {
        let mut bytes = [0u8; 4];
        assert!(put(&mut bytes, 0, &[1, 2]) && !put(&mut bytes, 3, &[1, 2]));
        assert_eq!(bytes, [1, 2, 0, 0]);
    }

    #[test]
    fn the_station_item_places_the_new_template() {
        let mut bench = vec![0u8; 0x400];
        bench[0x3c0..0x3d0].copy_from_slice(&stations::WORKBENCH.template.0);
        let clone = clone_station(&ITEMS, &LAYOUT, &bench, &new()).unwrap();
        assert_eq!(&clone[..4], &ITEM.fnv1a().to_le_bytes());
        assert_eq!(&clone[0x3c0..0x3d0], &TEMPLATE.0);
        assert_eq!(&clone[0x48..0x58], &NAME.0);
        // An item placing something else is not the station named.
        bench[0x3c0] ^= 1;
        assert!(clone_station(&ITEMS, &LAYOUT, &bench, &new()).is_none());
    }

    #[test]
    fn the_registry_lists_the_new_workshop_after_the_others() {
        let mut blob = Blob::new(&[0u8; 24]);
        let mut entry = vec![0u8; 56];
        entry[4..20].copy_from_slice(&OLD.0);
        entry[20..24].copy_from_slice(&OLD.fnv1a().to_le_bytes());
        entry[24..28].copy_from_slice(&7u32.to_le_bytes());
        entry[28..32].copy_from_slice(&stations::WORKBENCH.item.fnv1a().to_le_bytes());
        entry[48..52].copy_from_slice(&3u32.to_le_bytes());
        let at = blob.append(&entry, 4);
        assert!(blob.set_array(8, at, 1));
        let built = Blob::new(&list_workshop(&LAYOUT, blob.bytes(), &new()).unwrap());
        let (start, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let added = &built.bytes()[start + 56..start + 112];
        assert_eq!(&added[4..20], &WORKSHOP.0);
        assert_eq!(u32_at(added, 20), WORKSHOP.fnv1a());
        assert_eq!(u32_at(added, 24), NAME.fnv1a());
        assert_eq!(u32_at(added, 28), ITEM.fnv1a());
        assert_eq!(&added[32..48], &ITEM.0);
        assert_eq!(u32_at(added, 48), 3, "upgrade masks stay");
        assert_eq!(&built.bytes()[start + 4..start + 20], &OLD.0);
    }

    #[test]
    fn a_registry_without_the_vanilla_station_builds_nothing() {
        let mut blob = Blob::new(&[0u8; 24]);
        assert!(blob.set_array(8, 24, 0));
        assert!(list_workshop(&LAYOUT, blob.bytes(), &new()).is_none());
    }
}
