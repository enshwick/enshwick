//! Vanilla content by the names the game gives it: items by their debug
//! names in the item registry, and crafting stations and their workshops by
//! the station's item, read from the game's archive.
//!
//! [`item`], [`recipe`], [`loot_table`], [`station`] and [`workshop`] name
//! the running game's content, and `experience_values` (`typed`) lists its
//! experience values, so a mod's code holds no vanilla GUID or id;
//! [`deps!`](crate::deps) offers them as `deps::vanilla`. [`Vanilla::game`]
//! reads the running game's data; [`Vanilla::new`] reads any game's through
//! a function of the caller's, as a tool outside the game does.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use crate::Off;
use crate::abi::ResourceKey;

use super::containers::{ContainerLayout, container_slots, template_components};
use super::items::{ITEM_REGISTRY, ItemLayout};
use super::loot_tables::{FillTableLayout, template_table};
use super::recipes::{RECIPE_REGISTRY, RecipeLayout};
use super::workshops::{Station, WORKSHOP_REGISTRY, WorkshopId, WorkshopLayout, stations};
use super::{Blob, EngineFacts, Guid, types};

#[cfg(feature = "typed")]
mod experience;

#[cfg(feature = "typed")]
pub use experience::{ExperienceBalanceValue, experience_values};

mod names;

pub(crate) use names::item_exactly;
pub use names::{VanillaItem, VanillaRecipe, item_by_display_name, items, recipes};

/// Reads one resource of the game's archive.
type Reader = Box<dyn Fn(ResourceKey) -> Option<Vec<u8>> + Send + Sync>;

/// Item debug names to the items that carry them.
type Names = BTreeMap<String, Vec<Guid>>;

/// The most near names a [`VanillaError::Unknown`] suggests.
const NEAR: usize = 5;

/// Why a vanilla name or station was not found.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla::VanillaError;
///
/// let unknown = VanillaError::Unknown { name: "Tool_Torch1".into(), near: vec!["Tool_Torch01".into()] };
/// assert_eq!(unknown.to_string(), "no vanilla item \"Tool_Torch1\"; near: Tool_Torch01");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum VanillaError {
    /// The game's data does not read.
    #[error("the game's {0} does not read")]
    Unreadable(&'static str),
    /// No vanilla item has the name; `near` lists names that look like it.
    #[error("no vanilla item {name:?}{}", near_list(near))]
    Unknown {
        /// The name asked for.
        name: String,
        /// Names that look like it, at most five.
        near: Vec<String>,
    },
    /// Several vanilla items have the name.
    #[error("{} vanilla items are named {name:?}: {}; name one by its GUID", items.len(), guid_list(items))]
    Ambiguous {
        /// The name asked for.
        name: String,
        /// The items that carry it.
        items: Vec<Guid>,
    },
    /// No vanilla recipe has the debug name; `near` lists names that look
    /// like it.
    #[error("no vanilla recipe {name:?}{}", near_list(near))]
    UnknownRecipe {
        /// The name asked for.
        name: String,
        /// Names that look like it, at most five.
        near: Vec<String>,
    },
    /// Several vanilla recipes have the debug name.
    #[error("{} vanilla recipes are named {name:?}: {}; name one by its GUID", recipes.len(), guid_list(recipes))]
    AmbiguousRecipe {
        /// The name asked for.
        name: String,
        /// The recipes that carry it.
        recipes: Vec<Guid>,
    },
    /// No vanilla container template of the name rolls a fill table; `near`
    /// lists names that look like it.
    #[error("no vanilla container {name:?} rolls a loot table{}", near_list(near))]
    NoLootTable {
        /// The name asked for.
        name: String,
        /// Names that look like it, at most five.
        near: Vec<String>,
    },
    /// The item is no crafting station a mod can clone: the game lists no
    /// workshop for it, its workshop is a survivor's, or its template lacks
    /// the crafting components.
    #[error("{0} is no crafting station that can be cloned")]
    NotAStation(Guid),
    /// No vanilla template has the name; `near` lists names that look like
    /// it.
    #[error("no vanilla template {name:?}{}", near_list(near))]
    UnknownTemplate {
        /// The name asked for.
        name: String,
        /// Names that look like it, at most five.
        near: Vec<String>,
    },
    /// Several vanilla templates have the name.
    #[error("{} vanilla templates are named {name:?}: {}; name one by its GUID", templates.len(), guid_list(templates))]
    AmbiguousTemplate {
        /// The name asked for.
        name: String,
        /// The templates that carry it.
        templates: Vec<Guid>,
    },
    /// The template is no enemy: it names no arsenal.
    #[error("{0} is no enemy template")]
    NotAnEnemy(Guid),
}

fn near_list(near: &[String]) -> String {
    if near.is_empty() {
        String::new()
    } else {
        format!("; near: {}", near.join(", "))
    }
}

fn guid_list(items: &[Guid]) -> String {
    items
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The running game's [`Vanilla`], or why it does not read.
fn game() -> Result<&'static Vanilla, VanillaError> {
    Vanilla::game().ok_or(VanillaError::Unreadable("data"))
}

/// The running game's vanilla item with the debug name `name`, such as
/// `Prop_Production_T1_Forge`: what a mod writes in place of the item's GUID,
/// also as `deps::vanilla::item` of [`deps!`](crate::deps).
///
/// # Errors
///
/// As [`Vanilla::item`], and [`VanillaError::Unreadable`] outside the game.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla::{self, VanillaError};
///
/// match vanilla::item("Prop_Production_T1_Forge") {
///     Ok(forge) => enshwick::info!("the forge is {forge}"),
///     Err(VanillaError::Unreadable(_)) => {} // outside the game
///     Err(other) => enshwick::warn!("{other}"),
/// }
/// ```
pub fn item(name: &str) -> Result<Guid, VanillaError> {
    game()?.item(name)
}

/// The running game's vanilla recipe with the debug name `name`.
///
/// # Errors
///
/// As [`Vanilla::recipe`], and [`VanillaError::Unreadable`] outside the
/// game.
///
/// # Examples
///
/// ```
/// let torch = enshwick::content::vanilla::recipe("Tool_Torch01");
/// assert!(torch.is_err(), "no game runs here");
/// ```
pub fn recipe(name: &str) -> Result<Guid, VanillaError> {
    game()?.recipe(name)
}

/// The fill table the running game's vanilla container `name` rolls.
///
/// # Errors
///
/// As [`Vanilla::loot_table`], and [`VanillaError::Unreadable`] outside the
/// game.
///
/// # Examples
///
/// ```
/// let vukah = enshwick::content::vanilla::loot_table("LootContainer_Enemy_Corpse_Vukah");
/// assert!(vukah.is_err(), "no game runs here");
/// ```
pub fn loot_table(name: &str) -> Result<LootTable, VanillaError> {
    game()?.loot_table(name)
}

/// The running game's vanilla crafting station whose item has the debug
/// name `name`, to clone with [`crate::Load::add_workshop`].
///
/// # Errors
///
/// As [`Vanilla::item`] and [`Vanilla::station`], and
/// [`VanillaError::Unreadable`] outside the game.
///
/// # Examples
///
/// ```
/// let forge = enshwick::content::vanilla::station("Prop_Production_T1_Forge");
/// assert!(forge.is_err(), "no game runs here");
/// ```
pub fn station(name: &str) -> Result<Station, VanillaError> {
    let vanilla = game()?;
    vanilla.station(vanilla.item(name)?)
}

/// The workshop of the running game's vanilla crafting station whose item
/// has the debug name `name`, for recipes made at it.
///
/// # Errors
///
/// As [`Vanilla::item`] and [`Vanilla::workshop`], and
/// [`VanillaError::Unreadable`] outside the game.
///
/// # Examples
///
/// ```
/// let forge = enshwick::content::vanilla::workshop("Prop_Production_T1_Forge");
/// assert!(forge.is_err(), "no game runs here");
/// ```
pub fn workshop(name: &str) -> Result<WorkshopId, VanillaError> {
    let vanilla = game()?;
    vanilla.workshop(vanilla.item(name)?)
}

/// The running game's vanilla template named `name`, as the game's data
/// names it, such as `Enemy_Wildbeast_Wolf`: an enemy, a spawner or a prop
/// to copy or change by name.
///
/// # Errors
///
/// As [`Vanilla::template`], and [`VanillaError::Unreadable`] outside the
/// game.
///
/// # Examples
///
/// ```
/// let wolf = enshwick::content::vanilla::template("Enemy_Wildbeast_Wolf");
/// assert!(wolf.is_err(), "no game runs here");
/// ```
pub fn template(name: &str) -> Result<Guid, VanillaError> {
    game()?.template(name)
}

/// A vanilla enemy, from [`enemy`]: its template, faction and arsenal.
///
/// # Examples
///
/// See [`enemy`].
#[cfg(feature = "typed")]
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct Enemy {
    /// The enemy's template.
    pub template: Guid,
    /// Its faction, when the template names one.
    pub faction: Option<crate::typed::FactionType>,
    /// Its arsenal.
    pub arsenal: super::enemies::ArsenalId,
}

/// The running game's vanilla enemy whose template is named `name`, to copy
/// with [`crate::Load::add_enemy`] or to give its arsenal to another.
///
/// # Errors
///
/// As [`template`], and [`VanillaError::NotAnEnemy`] for a template that
/// names no arsenal.
///
/// # Examples
///
/// ```
/// let wolf = enshwick::content::vanilla::enemy("Enemy_Wildbeast_Wolf");
/// assert!(wolf.is_err(), "no game runs here");
/// ```
#[cfg(feature = "typed")]
pub fn enemy(name: &str) -> Result<Enemy, VanillaError> {
    use crate::typed::Named as _;
    use crate::typed::ecs::{EnemyArsenalReference, Faction};

    let template = template(name)?;
    let host = crate::host().ok_or(VanillaError::Unreadable("data"))?;
    let unreadable = |_| VanillaError::Unreadable("enemy template");
    let registry = crate::resources::Registry::game().map_err(unreadable)?;
    let hash = host
        .hash(types::TEMPLATE_RESOURCE.id())
        .map_err(unreadable)?;
    let value = host
        .game_value(template.key_by_hash(hash))
        .map_err(unreadable)?;
    let component = |id: &str| {
        let name = registry.hashed(host.hash(id).ok()?)?.qualified_name();
        match value.field("components")? {
            crate::resources::Value::List(held) => held.iter().find_map(|held| match held {
                crate::resources::Value::Variant(held) if held.type_name() == name => {
                    Some(held.value().clone())
                }
                _ => None,
            }),
            _ => None,
        }
    };
    let read = <EnemyArsenalReference as crate::typed::Reflected>::from_value;
    let arsenal = component(EnemyArsenalReference::ID)
        .and_then(|value| read(&value).ok())
        .ok_or(VanillaError::NotAnEnemy(template))?;
    let faction = component(Faction::ID)
        .and_then(|value| <Faction as crate::typed::Reflected>::from_value(&value).ok())
        .map(|held| held.faction);
    Ok(Enemy {
        template,
        faction,
        arsenal: super::enemies::ArsenalId::new(arsenal.arsenal),
    })
}

/// The game's vanilla content, read on demand.
///
/// # Examples
///
/// ```
/// use enshwick::content::vanilla::Vanilla;
///
/// if let Some(vanilla) = Vanilla::game() {
///     let forge = vanilla.item("Prop_Production_T1_Forge")?;
///     let station = vanilla.station(forge)?;
///     assert_eq!(station.item, forge);
/// }
/// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
/// ```
pub struct Vanilla {
    items: ItemLayout,
    workshops: WorkshopLayout,
    hashes: Hashes,
    read: Reader,
    /// The item names, kept once the registry reads.
    names: OnceLock<Option<Names>>,
    /// Where an item's resource holds the tag of its shown name, when the
    /// build reflects it.
    item_name: Option<usize>,
    /// Every item with its names, kept once the registry reads.
    item_entries: OnceLock<Vec<VanillaItem>>,
    /// The recipe layout and the recipe registry's type hash, when the
    /// build reflects them.
    recipes: Option<(RecipeLayout, u32)>,
    recipe_list: OnceLock<Option<Recipes>>,
    /// The container layout, when the build reflects it.
    containers: Option<ContainerLayout>,
    /// Where templates name the fill table they roll, when the build
    /// reflects it.
    fill: Option<FillTableLayout>,
    fill_tables: OnceLock<Option<FillTables>>,
    /// Where a template names itself, when the build reflects it.
    template_name: Option<usize>,
    template_names: OnceLock<Option<Names>>,
}

/// The fill tables containers roll: each container template's by its name,
/// and how many templates roll each table.
#[derive(Debug, Default)]
struct FillTables {
    by_name: Names,
    shared: BTreeMap<[u8; 16], usize>,
}

/// The fill table a vanilla container rolls, from
/// [`Vanilla::loot_table`].
///
/// # Examples
///
/// See [`Vanilla::loot_table`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct LootTable {
    /// The table's GUID, a `DefaultInventoryResource`.
    pub table: Guid,
    /// How many container templates roll it, this one among them.
    pub containers: usize,
}

/// The vanilla recipes: by debug name, and each with the item it makes.
#[derive(Debug, Default)]
struct Recipes {
    names: Names,
    made: Vec<(Guid, Option<Guid>)>,
}

impl core::fmt::Debug for Vanilla {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Vanilla")
            .field("items", &self.items)
            .field("workshops", &self.workshops)
            .finish_non_exhaustive()
    }
}

/// The hashes of the resource types vanilla content is read from.
#[derive(Clone, Copy, Debug)]
struct Hashes {
    item: u32,
    item_registry: u32,
    template: u32,
    workshop_registry: u32,
    template_collection: u32,
}

impl Hashes {
    fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        Ok(Self {
            item: facts.hash(types::ITEM_INFO.id())?,
            item_registry: facts.hash(types::ITEM_REGISTRY_RESOURCE.id())?,
            template: facts.hash(types::TEMPLATE_RESOURCE.id())?,
            workshop_registry: facts.hash(types::WORKSHOP_REGISTRY_RESOURCE.id())?,
            template_collection: facts.hash(types::TEMPLATE_COLLECTION_RESOURCE.id())?,
        })
    }
}

impl Vanilla {
    /// Vanilla content read through `read`, which returns a resource of the
    /// game's archive by key, with the layouts and types `facts` give.
    ///
    /// # Errors
    ///
    /// The first [`Off`] a fact returns, when the build lacks it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::EngineFacts;
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// fn offline(facts: &impl EngineFacts) -> Result<Vanilla, enshwick::Off> {
    ///     Vanilla::new(facts, |_key| None)
    /// }
    /// ```
    pub fn new(
        facts: &(impl EngineFacts + ?Sized),
        read: impl Fn(ResourceKey) -> Option<Vec<u8>> + Send + Sync + 'static,
    ) -> Result<Self, Off> {
        Ok(Self::with_parts(
            ItemLayout::resolve(facts)?,
            WorkshopLayout::resolve(facts)?,
            Hashes::resolve(facts)?,
            read,
        )
        .with_recipe_facts(facts)
        .with_containers(ContainerLayout::resolve(facts).ok())
        .with_fill_tables(FillTableLayout::resolve(facts).ok())
        .with_template_names(facts.offset(crate::engine_ids::TEMPLATE_NAME).ok())
        .with_item_names(facts.offset(crate::engine_ids::ITEM_INFO_NAME).ok()))
    }

    /// The same, reading template names at `at`.
    fn with_template_names(mut self, at: Option<usize>) -> Self {
        self.template_name = at;
        self
    }

    /// The same, reading the tag of an item's shown name at `at`.
    fn with_item_names(mut self, at: Option<usize>) -> Self {
        self.item_name = at;
        self
    }

    fn with_parts(
        items: ItemLayout,
        workshops: WorkshopLayout,
        hashes: Hashes,
        read: impl Fn(ResourceKey) -> Option<Vec<u8>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            items,
            workshops,
            hashes,
            read: Box::new(read),
            names: OnceLock::new(),
            item_name: None,
            item_entries: OnceLock::new(),
            recipes: None,
            recipe_list: OnceLock::new(),
            containers: None,
            fill: None,
            fill_tables: OnceLock::new(),
            template_name: None,
            template_names: OnceLock::new(),
        }
    }

    /// The vanilla template named `name`, as the template collection lists
    /// it. The first call reads every template's name once, 139 ms on a
    /// dedicated server (build 26083076), more than a Luau script's budget
    /// of 100 ms a second: name templates while the mod starts. A Rust mod's
    /// first call after `init` logs a warning saying so.
    ///
    /// # Errors
    ///
    /// [`VanillaError::UnknownTemplate`] with the names that look like it,
    /// [`VanillaError::AmbiguousTemplate`] when several templates carry it,
    /// and [`VanillaError::Unreadable`] when the templates do not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let _wolf = vanilla.template("Enemy_Wildbeast_Wolf");
    /// }
    /// ```
    pub fn template(&self, name: &str) -> Result<Guid, VanillaError> {
        let names = self
            .template_names
            .get_or_init(|| {
                if crate::__private::initialised() {
                    crate::warn_once!(
                        "vanilla templates named after init: every template's name is read now, on this thread, about 139 ms on a dedicated server; name them in init"
                    );
                }
                self.read_template_names()
            })
            .as_ref()
            .ok_or(VanillaError::Unreadable("templates"))?;
        match names.get(name).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(several) if !several.is_empty() => Err(VanillaError::AmbiguousTemplate {
                name: name.to_owned(),
                templates: several.to_vec(),
            }),
            _ => Err(VanillaError::UnknownTemplate {
                name: name.to_owned(),
                near: near(names, name),
            }),
        }
    }

    fn read_template_names(&self) -> Option<Names> {
        let at = self.template_name?;
        let collection = (self.read)(
            super::templates::TEMPLATE_COLLECTION.key_by_hash(self.hashes.template_collection),
        )?;
        let (start, count) = Blob::new(&collection).array(self.workshops.templates)?;
        let mut names = Names::new();
        for at_guid in (0..usize::try_from(count).ok()?).map(|i| start + i * 16) {
            let guid = Guid(collection.get(at_guid..at_guid + 16)?.try_into().ok()?);
            let Some(template) = (self.read)(guid.key_by_hash(self.hashes.template)) else {
                continue;
            };
            let blob = Blob::new(&template);
            let Some((text, len)) = blob.array(at) else {
                continue;
            };
            let Some(name) = template.get(text..text + usize::try_from(len).ok()?) else {
                continue;
            };
            let name = String::from_utf8_lossy(name)
                .trim_end_matches('\0')
                .to_owned();
            names.entry(name).or_default().push(guid);
        }
        Some(names)
    }

    /// The same, reading recipes through `layout` from the registry of type
    /// `hash`.
    fn with_recipes(mut self, layout: RecipeLayout, hash: u32) -> Self {
        self.recipes = Some((layout, hash));
        self
    }

    /// The same, reading containers through `layout`.
    fn with_containers(mut self, layout: Option<ContainerLayout>) -> Self {
        self.containers = layout;
        self
    }

    /// The same, reading the fill tables containers roll through `layout`.
    fn with_fill_tables(mut self, layout: Option<FillTableLayout>) -> Self {
        self.fill = layout;
        self
    }

    /// The fill table the vanilla container template named `name` rolls,
    /// such as `LootContainer_Enemy_Corpse_Vukah`, with how many templates
    /// share it: a change of the table reaches all of them. The first call
    /// reads every template once.
    ///
    /// # Errors
    ///
    /// [`VanillaError::NoLootTable`] with the names that look like it, and
    /// [`VanillaError::Unreadable`] when the templates do not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let vukah = vanilla.loot_table("LootContainer_Enemy_Corpse_Vukah")?;
    ///     assert!(vukah.containers >= 1);
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn loot_table(&self, name: &str) -> Result<LootTable, VanillaError> {
        let tables = self
            .fill_tables
            .get_or_init(|| self.read_fill_tables())
            .as_ref()
            .ok_or(VanillaError::Unreadable("container templates"))?;
        match tables.by_name.get(name).and_then(|tables| tables.first()) {
            Some(table) => Ok(LootTable {
                table: *table,
                containers: tables.shared.get(&table.0).copied().unwrap_or(1),
            }),
            None => Err(VanillaError::NoLootTable {
                name: name.to_owned(),
                near: near(&tables.by_name, name),
            }),
        }
    }

    fn read_fill_tables(&self) -> Option<FillTables> {
        let layout = self.fill?;
        let collection = (self.read)(
            super::templates::TEMPLATE_COLLECTION.key_by_hash(self.hashes.template_collection),
        )?;
        let blob = Blob::new(&collection);
        let (start, count) = blob.array(self.workshops.templates)?;
        let mut tables = FillTables::default();
        for at in (0..usize::try_from(count).ok()?).map(|i| start + i * 16) {
            let guid: [u8; 16] = collection.get(at..at + 16)?.try_into().ok()?;
            let Some(template) = (self.read)(Guid(guid).key_by_hash(self.hashes.template)) else {
                continue;
            };
            if let Some((name, table)) = template_table(&layout, &template) {
                *tables.shared.entry(table.0).or_default() += 1;
                tables.by_name.entry(name).or_default().push(table);
            }
        }
        Some(tables)
    }

    /// How many slots the container the vanilla item `item` places holds,
    /// or the backpack it is; `None` when it places or carries none.
    ///
    /// # Errors
    ///
    /// [`VanillaError::Unreadable`] when the item's data, the container
    /// layout or the template does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let chest = vanilla.item("Prop_Storage_24_T1_Box")?;
    ///     assert_eq!(vanilla.container_slots(chest)?, Some(24));
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn container_slots(&self, item: Guid) -> Result<Option<u8>, VanillaError> {
        let layout = self
            .containers
            .as_ref()
            .ok_or(VanillaError::Unreadable("container layout"))?;
        let info = (self.read)(item.key_by_hash(self.hashes.item))
            .ok_or(VanillaError::Unreadable("item data"))?;
        let slots = [self.workshops.placed_entity, self.items.pid_entity]
            .into_iter()
            .filter_map(|at| {
                info.get(at..at + 16)
                    .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
                    .map(Guid)
                    .filter(|template| template.0 != [0; 16])
            })
            .find_map(|template| {
                let data = (self.read)(template.key_by_hash(self.hashes.template))?;
                container_slots(layout, &data)
            });
        Ok(slots)
    }

    /// The same, with the recipe layout `facts` give, when they give it.
    fn with_recipe_facts(self, facts: &(impl EngineFacts + ?Sized)) -> Self {
        match (
            RecipeLayout::resolve(facts),
            facts.hash(types::RECIPE_REGISTRY_RESOURCE.id()),
        ) {
            (Ok(layout), Ok(hash)) => self.with_recipes(layout, hash),
            _ => self,
        }
    }

    fn recipe_list(&self) -> Result<&Recipes, VanillaError> {
        self.recipe_list
            .get_or_init(|| {
                let (layout, hash) = self.recipes.as_ref()?;
                let registry = (self.read)(RECIPE_REGISTRY.key_by_hash(*hash))?;
                let mut recipes = Recipes::default();
                for (guid, name, output) in super::recipes::list(layout, &registry)? {
                    recipes.names.entry(name).or_default().push(guid);
                    recipes.made.push((guid, output));
                }
                Some(recipes)
            })
            .as_ref()
            .ok_or(VanillaError::Unreadable("recipe registry"))
    }

    /// The vanilla recipe with the debug name `name`, as the recipe registry
    /// lists it.
    ///
    /// # Errors
    ///
    /// [`VanillaError::UnknownRecipe`] with the names that look like it,
    /// [`VanillaError::AmbiguousRecipe`] when several recipes carry it, and
    /// [`VanillaError::Unreadable`] without the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let _recipe = vanilla.recipe("Tool_Torch01");
    /// }
    /// ```
    pub fn recipe(&self, name: &str) -> Result<Guid, VanillaError> {
        let names = &self.recipe_list()?.names;
        match names.get(name).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(several) if !several.is_empty() => Err(VanillaError::AmbiguousRecipe {
                name: name.to_owned(),
                recipes: several.to_vec(),
            }),
            _ => Err(VanillaError::UnknownRecipe {
                name: name.to_owned(),
                near: near(names, name),
            }),
        }
    }

    /// The vanilla recipes that make `item`, in the registry's order; none
    /// when no recipe does.
    ///
    /// # Errors
    ///
    /// [`VanillaError::Unreadable`] without the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let torch = vanilla.item("Tool_Torch01")?;
    ///     let _recipes = vanilla.recipes_making(torch)?;
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn recipes_making(&self, item: Guid) -> Result<Vec<Guid>, VanillaError> {
        Ok(self
            .recipe_list()?
            .made
            .iter()
            .filter(|(_, output)| *output == Some(item))
            .map(|(recipe, _)| *recipe)
            .collect())
    }

    /// Whether `guid` is a vanilla item's GUID; `false` without the registry.
    pub(crate) fn is_item(&self, guid: Guid) -> bool {
        self.names()
            .is_ok_and(|names| names.values().flatten().any(|item| *item == guid))
    }

    /// Whether `guid` is a vanilla recipe's own GUID; `false` without the
    /// registry.
    pub(crate) fn is_recipe(&self, guid: Guid) -> bool {
        self.recipe_list()
            .is_ok_and(|list| list.made.iter().any(|(recipe, _)| *recipe == guid))
    }

    /// The running game's vanilla content, read from its archive through
    /// [`super::game_resource`]; `None` outside the game or when the build
    /// lacks a layout.
    ///
    /// # Examples
    ///
    /// See [`Vanilla`].
    #[must_use]
    pub fn game() -> Option<&'static Self> {
        static GAME: crate::fresh::Fresh<Option<Vanilla>> = crate::fresh::Fresh::new();
        GAME.get_or_init(|| Self::new(&crate::host()?, super::game_resource).ok())
            .as_ref()
    }

    /// The item names; a registry that does not read yet, as before the
    /// archive is ready, is read again on the next call, and one that reads
    /// but does not fit the layout is kept as unreadable.
    fn names(&self) -> Result<&Names, VanillaError> {
        let unreadable = || VanillaError::Unreadable("item registry");
        if let Some(names) = self.names.get() {
            return names.as_ref().ok_or_else(unreadable);
        }
        let registry = (self.read)(ITEM_REGISTRY.key_by_hash(self.hashes.item_registry))
            .ok_or_else(unreadable)?;
        self.names
            .get_or_init(|| registry_names(&self.items, &registry))
            .as_ref()
            .ok_or_else(unreadable)
    }

    /// The vanilla item the game knows by `id`, [`Guid::fnv1a`] of its
    /// GUID; `None` for none, or without the registry.
    pub(crate) fn item_with_id(&self, id: u32) -> Option<Guid> {
        self.names()
            .ok()?
            .values()
            .flatten()
            .find(|item| item.fnv1a() == id)
            .copied()
    }

    /// The vanilla item with the debug name `name`, as the item registry
    /// lists it, such as `Craftingstation_Workbench`.
    ///
    /// # Errors
    ///
    /// [`VanillaError::Unknown`] with the names that look like it,
    /// [`VanillaError::Ambiguous`] when several items carry it, and
    /// [`VanillaError::Unreadable`] without the registry.
    ///
    /// # Examples
    ///
    /// See [`Vanilla`].
    pub fn item(&self, name: &str) -> Result<Guid, VanillaError> {
        let names = self.names()?;
        match names.get(name).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(several) if !several.is_empty() => Err(VanillaError::Ambiguous {
                name: name.to_owned(),
                items: several.to_vec(),
            }),
            _ => Err(VanillaError::Unknown {
                name: name.to_owned(),
                near: near(names, name),
            }),
        }
    }

    /// The vanilla crafting station whose item is `item`, to clone with
    /// [`crate::Load::add_workshop`].
    ///
    /// # Errors
    ///
    /// [`VanillaError::NotAStation`] when the game lists no workshop for the
    /// item, the workshop is a survivor's, or the template the item places
    /// lacks the crafting components; [`VanillaError::Unreadable`] when the
    /// game's data does not read.
    ///
    /// # Examples
    ///
    /// See [`Vanilla`].
    pub fn station(&self, item: Guid) -> Result<Station, VanillaError> {
        if item == stations::WORKBENCH.item {
            return Ok(stations::WORKBENCH);
        }
        self.workshop_entry(item)?;
        let info = (self.read)(item.key_by_hash(self.hashes.item))
            .ok_or(VanillaError::Unreadable("item data"))?;
        let at = self.workshops.placed_entity;
        let template = info
            .get(at..at + 16)
            .and_then(|bytes| bytes.try_into().ok())
            .map(Guid)
            .ok_or(VanillaError::NotAStation(item))?;
        let data = (self.read)(template.key_by_hash(self.hashes.template))
            .ok_or(VanillaError::NotAStation(item))?;
        let blob = Blob::new(&data);
        let crafts = self.workshops.crafting_types.iter().all(|&ty| {
            template_components(&blob, self.workshops.components, ty)
                .is_some_and(|found| !found.is_empty())
        });
        crafts
            .then_some(Station { item, template })
            .ok_or(VanillaError::NotAStation(item))
    }

    /// The workshop of the vanilla crafting station whose item is `item`,
    /// for recipes made at it.
    ///
    /// # Errors
    ///
    /// As [`Vanilla::station`] for an item without a workshop of its own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let forge = vanilla.item("Prop_Production_T1_Forge")?;
    ///     let _workshop = vanilla.workshop(forge)?;
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn workshop(&self, item: Guid) -> Result<WorkshopId, VanillaError> {
        let entry = self.workshop_entry(item)?;
        let at = self.workshops.entry_guid;
        entry
            .get(at..at + 16)
            .and_then(|bytes| bytes.try_into().ok())
            .map(|guid| WorkshopId::new(Guid(guid)))
            .ok_or(VanillaError::NotAStation(item))
    }

    /// The workshop registry's entry for the station item `item`, when it is
    /// no survivor's.
    fn workshop_entry(&self, item: Guid) -> Result<Vec<u8>, VanillaError> {
        let registry = (self.read)(WORKSHOP_REGISTRY.key_by_hash(self.hashes.workshop_registry))
            .ok_or(VanillaError::Unreadable("workshop registry"))?;
        let layout = &self.workshops;
        let blob = Blob::new(&registry);
        let (start, count) = blob
            .array(layout.workshops)
            .ok_or(VanillaError::Unreadable("workshop registry"))?;
        (0..usize::try_from(count).unwrap_or(0))
            .map(|i| start + i * layout.entry)
            .find(|&at| {
                blob.read_u32(at + layout.entry_item) == Some(item.fnv1a())
                    && registry.get(at + layout.entry_npc) == Some(&0)
            })
            .and_then(|at| registry.get(at..at + layout.entry))
            .map(<[u8]>::to_vec)
            .ok_or(VanillaError::NotAStation(item))
    }
}

/// The debug names of the item registry `registry`, beside its references.
fn registry_names(layout: &ItemLayout, registry: &[u8]) -> Option<Names> {
    let blob = Blob::new(registry);
    let (refs, count) = blob.array(layout.refs)?;
    let (names, names_count) = blob.array(layout.names)?;
    if names_count != count {
        return None;
    }
    let mut out = Names::new();
    for i in 0..usize::try_from(count).ok()? {
        let guid: [u8; 16] = registry
            .get(refs + i * 16..refs + i * 16 + 16)?
            .try_into()
            .ok()?;
        let (text, len) = blob.array(names + i * 8)?;
        let name = registry.get(text..text + usize::try_from(len).ok()?)?;
        let name = core::str::from_utf8(name).ok()?;
        out.entry(name.to_owned()).or_default().push(Guid(guid));
    }
    Some(out)
}

/// Names in `names` that look like `name`, as [`crate::near::near`] finds
/// them.
fn near(names: &Names, name: &str) -> Vec<String> {
    crate::near::near(name, names.keys().map(String::as_str), NEAR)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests;
