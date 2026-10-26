//! Vanilla content by the names the game gives it: items by their debug
//! names in the item registry, and crafting stations and their workshops by
//! the station's item, read from the game's archive.
//!
//! [`Vanilla::game`] reads the running game's data; [`Vanilla::new`] reads
//! any game's through a function of the caller's, as a tool outside the
//! game does.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use super::items::{ITEM_REGISTRY, ItemLayout};
use super::recipes::{RECIPE_REGISTRY, RecipeLayout};
use super::workshops::{
    Station, WORKSHOP_REGISTRY, WorkshopId, WorkshopLayout, clone_template, stations,
};
use super::{Blob, EngineFacts, Guid, types};
use crate::Off;
use crate::abi::ResourceKey;

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
    /// The item is no crafting station a mod can clone: the game lists no
    /// workshop for it, its workshop is a survivor's, or its template lacks
    /// the crafting components.
    #[error("{0} is no crafting station that can be cloned")]
    NotAStation(Guid),
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
    names: OnceLock<Option<Names>>,
    /// The recipe layout and the recipe registry's type hash, when the
    /// build reflects them.
    recipes: Option<(RecipeLayout, u32)>,
    recipe_list: OnceLock<Option<Recipes>>,
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
}

impl Hashes {
    fn resolve(facts: &(impl EngineFacts + ?Sized)) -> Result<Self, Off> {
        Ok(Self {
            item: facts.hash(types::ITEM_INFO.id())?,
            item_registry: facts.hash(types::ITEM_REGISTRY.id())?,
            template: facts.hash(types::TEMPLATE.id())?,
            workshop_registry: facts.hash(types::WORKSHOP_REGISTRY.id())?,
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
        .with_recipe_facts(facts))
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
            recipes: None,
            recipe_list: OnceLock::new(),
        }
    }

    /// The same, reading recipes through `layout` from the registry of type
    /// `hash`.
    fn with_recipes(mut self, layout: RecipeLayout, hash: u32) -> Self {
        self.recipes = Some((layout, hash));
        self
    }

    /// The same, with the recipe layout `facts` give, when they give it.
    fn with_recipe_facts(self, facts: &(impl EngineFacts + ?Sized)) -> Self {
        match (
            RecipeLayout::resolve(facts),
            facts.hash(types::RECIPE_REGISTRY.id()),
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

    fn names(&self) -> Result<&Names, VanillaError> {
        self.names
            .get_or_init(|| {
                let registry = (self.read)(ITEM_REGISTRY.key_by_hash(self.hashes.item_registry))?;
                registry_names(&self.items, &registry)
            })
            .as_ref()
            .ok_or(VanillaError::Unreadable("item registry"))
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
        clone_template(&self.workshops, &data, Guid([0xee; 16]))
            .map(|_| Station { item, template })
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
                blob.read_u32(at + layout.entry_item) == Some(item.id())
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

/// Names in `names` that look like `name`: the same but for case first,
/// then those that hold it, then those a few edits away, ignoring case.
fn near(names: &Names, name: &str) -> Vec<String> {
    let wanted = name.to_ascii_lowercase();
    let mut scored: Vec<(usize, &String)> = names
        .keys()
        .filter_map(|n| {
            let lower = n.to_ascii_lowercase();
            let score = if lower == wanted {
                0
            } else if !wanted.is_empty() && lower.contains(&wanted) {
                1
            } else {
                let edits = strsim::levenshtein(&lower, &wanted);
                if edits > 2 {
                    return None;
                }
                1 + edits
            };
            Some((score, n))
        })
        .collect();
    scored.sort();
    scored
        .into_iter()
        .take(NEAR)
        .map(|(_, n)| n.clone())
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const HASHES: Hashes = Hashes {
        item: 1,
        item_registry: 2,
        template: 3,
        workshop_registry: 4,
    };

    /// The item layout of build 1269579, as far as the registry goes.
    pub(crate) const ITEMS: ItemLayout = ItemLayout {
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
        names: 8,
    };

    /// A registry listing `items` with their names.
    pub(crate) fn registry(items: &[(Guid, &str)]) -> Vec<u8> {
        let mut blob = Blob::new(&[0u8; 16]);
        let refs: Vec<(Vec<u8>, Option<usize>)> =
            items.iter().map(|(g, _)| (g.0.to_vec(), None)).collect();
        blob.grow_array(0, 16, 4, &[], &refs).unwrap();
        for (_, name) in items {
            let at = blob.append(name.as_bytes(), 1);
            let first = blob
                .grow_array(8, 8, 4, &[0], &[(vec![0; 8], None)])
                .unwrap();
            let (start, _) = blob.array(8).unwrap();
            let element = start + usize::try_from(first).unwrap() * 8;
            assert!(blob.set_array(element, at, u32::try_from(name.len()).unwrap()));
        }
        blob.into_bytes()
    }

    #[test]
    fn items_are_found_by_name_and_near_names_suggested() {
        let torch = Guid([1; 16]);
        let (one, two) = (Guid([2; 16]), Guid([3; 16]));
        let data = registry(&[
            (torch, "Tool_Torch01"),
            (one, "Twin"),
            (two, "Twin"),
            (Guid([4; 16]), "Material_T2_Wood_Planks"),
            (Guid([5; 16]), "Material_T1_Wood_Logs"),
        ]);
        let names = registry_names(&ITEMS, &data).unwrap();
        assert_eq!(names.get("Tool_Torch01"), Some(&vec![torch]));
        assert_eq!(near(&names, "tool_torch01"), ["Tool_Torch01"]);
        assert_eq!(
            near(&names, "wood"),
            ["Material_T1_Wood_Logs", "Material_T2_Wood_Planks"]
        );
        assert!(near(&names, "nothing").is_empty());
    }

    /// A game with the items `named`, a forge station of its own workshop,
    /// a survivor's workshop and an item that places a plain template.
    pub(crate) fn game(named: &[(Guid, &str)]) -> Vanilla {
        let data = game_data(named);
        Vanilla::with_parts(
            ITEMS,
            super::super::workshops::tests::LAYOUT,
            HASHES,
            move |key| data.get(&key).cloned(),
        )
    }

    /// The recipe registry's type hash in these tests.
    const RECIPE_REGISTRY_HASH: u32 = 5;

    /// The recipe layout of build 1269579, as far as these tests read it.
    const RECIPES: RecipeLayout = RecipeLayout {
        recipes: 8,
        recipe: 0xa8,
        guid: 0,
        id: 0x10,
        workshop_guid: 0x14,
        workshop: 0x24,
        input: 0x28,
        output: 0x30,
        completion: 0x5c,
        relative: [0x28, 0x30, 0x40, 0x78],
        input_size: 48,
        stack_size: 24,
        stack_item: 0x10,
        stack_count: 0x14,
    };

    /// [`game`] with a recipe registry: each recipe's GUID, debug name and
    /// the item it makes.
    pub(crate) fn game_with_recipes(
        named: &[(Guid, &str)],
        recipes: &[(Guid, &str, Guid)],
    ) -> Vanilla {
        let mut blob = Blob::new(&[0u8; 16]);
        let at = blob.append(&vec![0u8; 0xa8 * recipes.len()], 8);
        for (i, (guid, name, output)) in recipes.iter().enumerate() {
            let recipe = at + i * 0xa8;
            let mut stack = vec![0u8; 24];
            stack[..16].copy_from_slice(&output.0);
            let output_at = blob.append(&stack, 8);
            let name_at = blob.append(name.as_bytes(), 1);
            assert!(blob.write(recipe, &guid.0));
            assert!(blob.set_array(recipe + 0x30, output_at, 1));
            assert!(blob.set_array(recipe + 0x78, name_at, u32::try_from(name.len()).unwrap()));
        }
        assert!(blob.set_array(8, at, u32::try_from(recipes.len()).unwrap()));
        let mut data = game_data(named);
        data.insert(
            RECIPE_REGISTRY.key_by_hash(RECIPE_REGISTRY_HASH),
            blob.into_bytes(),
        );
        Vanilla::with_parts(
            ITEMS,
            super::super::workshops::tests::LAYOUT,
            HASHES,
            move |key| data.get(&key).cloned(),
        )
        .with_recipes(RECIPES, RECIPE_REGISTRY_HASH)
    }

    /// The archive of [`game`], by key.
    fn game_data(named: &[(Guid, &str)]) -> std::collections::HashMap<ResourceKey, Vec<u8>> {
        use super::super::workshops::tests::{CLIENT, CRAFTING, INTERACTION, station};

        let (forge, survivor, prop) = (Guid([5; 16]), Guid([6; 16]), Guid([7; 16]));
        let (template, plain, workshop) = (Guid([8; 16]), Guid([9; 16]), Guid([10; 16]));
        let mut workshops = Blob::new(&[0u8; 24]);
        let mut entries = Vec::new();
        for (item, npc) in [(forge, 0u8), (survivor, 1), (prop, 0)] {
            let mut entry = vec![0u8; 56];
            entry[0] = npc;
            entry[4..20].copy_from_slice(&workshop.0);
            entry[28..32].copy_from_slice(&item.id().to_le_bytes());
            entries.extend_from_slice(&entry);
        }
        let at = workshops.append(&entries, 4);
        assert!(workshops.set_array(8, at, 3));
        let placing = |template: Guid| {
            let mut info = vec![0u8; 0x400];
            info[0x3c0..0x3d0].copy_from_slice(&template.0);
            info
        };
        let (crafting, _) = station(&[(CRAFTING, 20), (INTERACTION, 32), (CLIENT, 16)]);
        let (other, _) = station(&[(0x1234_5678, 24)]);
        let mut data = std::collections::HashMap::new();
        data.insert(
            ITEM_REGISTRY.key_by_hash(HASHES.item_registry),
            registry(named),
        );
        data.insert(
            WORKSHOP_REGISTRY.key_by_hash(HASHES.workshop_registry),
            workshops.into_bytes(),
        );
        for (item, places) in [(forge, template), (survivor, template), (prop, plain)] {
            data.insert(item.key_by_hash(HASHES.item), placing(places));
        }
        data.insert(template.key_by_hash(HASHES.template), crafting.into_bytes());
        data.insert(plain.key_by_hash(HASHES.template), other.into_bytes());
        data
    }

    #[test]
    fn a_recipe_is_found_by_its_name_and_by_what_it_makes() {
        let (torch, recipe) = (Guid([1; 16]), Guid([3; 16]));
        let vanilla = game_with_recipes(
            &[(torch, "Tool_Torch01")],
            &[(recipe, "Tool_Torch01_T0_PK", torch)],
        );
        assert_eq!(vanilla.recipe("Tool_Torch01_T0_PK"), Ok(recipe));
        assert_eq!(
            vanilla
                .recipe("tool_torch01_t0_pk")
                .unwrap_err()
                .to_string(),
            "no vanilla recipe \"tool_torch01_t0_pk\"; near: Tool_Torch01_T0_PK"
        );
        assert_eq!(vanilla.recipes_making(torch), Ok(vec![recipe]));
        assert_eq!(vanilla.recipes_making(Guid([9; 16])), Ok(Vec::new()));
        assert_eq!(
            game(&[]).recipe("Tool_Torch01_T0_PK"),
            Err(VanillaError::Unreadable("recipe registry"))
        );
    }

    #[test]
    fn a_name_finds_its_item_or_explains_why_not() {
        let torch = Guid([1; 16]);
        let vanilla = game(&[
            (torch, "Tool_Torch01"),
            (Guid([2; 16]), "Twin"),
            (Guid([3; 16]), "Twin"),
        ]);
        assert_eq!(vanilla.item("Tool_Torch01"), Ok(torch));
        let unknown = vanilla.item("torch").unwrap_err();
        assert_eq!(
            unknown.to_string(),
            "no vanilla item \"torch\"; near: Tool_Torch01"
        );
        let ambiguous = vanilla.item("Twin").unwrap_err().to_string();
        assert!(
            ambiguous.starts_with("2 vanilla items are named \"Twin\""),
            "{ambiguous}"
        );
        let empty = Vanilla::with_parts(
            ITEMS,
            super::super::workshops::tests::LAYOUT,
            HASHES,
            |_| None,
        );
        assert_eq!(
            empty.item("Tool_Torch01"),
            Err(VanillaError::Unreadable("item registry"))
        );
    }

    #[test]
    fn only_a_crafting_station_of_its_own_can_be_cloned() {
        let vanilla = game(&[]);
        let station = vanilla.station(Guid([5; 16])).unwrap();
        assert_eq!(station.template, Guid([8; 16]));
        assert_eq!(
            vanilla.workshop(Guid([5; 16])).map(WorkshopId::guid),
            Ok(Guid([10; 16]))
        );
        for (item, why) in [
            (6, "a survivor's"),
            (7, "no crafting components"),
            (11, "no workshop"),
        ] {
            assert_eq!(
                vanilla.station(Guid([item; 16])),
                Err(VanillaError::NotAStation(Guid([item; 16]))),
                "{why}"
            );
        }
        assert_eq!(
            vanilla.station(stations::WORKBENCH.item),
            Ok(stations::WORKBENCH)
        );
    }
}
