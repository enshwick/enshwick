//! Recipes: a vanilla recipe copied for a new output, with its ingredients
//! scaled, or the recipes in the registry changed in place.

use std::sync::Arc;

use super::menu;
use super::types;
use crate::{Load, Off};

mod add;
mod derive;
mod rules;

pub use add::{
    NewRecipe, RECIPE_REGISTRY, RESERVED_FOR_SINGLE, RecipeLayout, WORKSHOP_RECIPES, add_recipe_to,
    workshop_limit,
};
pub use derive::{
    Derived, RecipeEdit, RecipeView, derive_recipes_in, derived_guid, modify_recipes_in,
};
pub use rules::{Name, RecipeRule, disable, enable, is_disabled};

use add::fnv1a;
use derive::{derived_entries, menu_entry, recipe_guids, vanilla_registry};

impl Load<'_> {
    /// Adds a recipe to the game, on the server and the client alike: a copy
    /// of the recipe that makes `like`, at the same workshop and unlocked with
    /// it, making `output` instead, beside recipes other mods add. The
    /// client's crafting menu lists it right after the copied recipe (see
    /// [`menu`](super::menu)).
    ///
    /// # Errors
    ///
    /// [`Off`] when the recipe layout is not reflected or the loader
    /// refuses the recipe's id (see
    /// [`Load::declare`]); a `like` with no recipe turns the mod off when the
    /// game first reads the registry.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, Id};
    /// use enshwick::content::recipes::NewRecipe;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_recipe(NewRecipe {
    ///         id: Id::new("my-mod:locker.recipe"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         output: Guid::named("my-mod:locker"),
    ///         output_count: 1,
    ///         cost: 2,
    ///         workshop: None,
    ///     })
    /// }
    /// ```
    pub fn add_recipe(&mut self, recipe: NewRecipe) -> Result<(), Off> {
        self.declare(recipe.id)?;
        let layout = RecipeLayout::resolve(self)?;
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY)?;
        self.serve(registry, registry, move |base| {
            add_recipe_to(&layout, base, &recipe, workshop_limit())
        })?;
        menu::serve(self, move |menu, bundle| {
            menu::list_recipes(menu, bundle, &[menu_entry(&layout, &recipe)])
        })
    }

    /// Adds a recipe as [`add_recipe`](Self::add_recipe) does, asking
    /// `recipe` for it each time the registry is built, so a cost read from
    /// settings that change, such as a world's or a server's join values,
    /// applies through [`rebuild_recipes`]. Every answer keeps the id of the
    /// first.
    ///
    /// # Errors
    ///
    /// As for [`add_recipe`](Self::add_recipe).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::atomic::{AtomicU32, Ordering};
    ///
    /// use enshwick::content::{Guid, Id};
    /// use enshwick::content::recipes::NewRecipe;
    ///
    /// static COST: AtomicU32 = AtomicU32::new(2);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_recipe_with(|| NewRecipe {
    ///         id: Id::new("my-mod:locker.recipe"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         output: Guid::named("my-mod:locker"),
    ///         output_count: 1,
    ///         cost: COST.load(Ordering::Relaxed),
    ///         workshop: None,
    ///     })
    /// }
    /// ```
    pub fn add_recipe_with(
        &mut self,
        recipe: impl Fn() -> NewRecipe + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let id = recipe().id;
        self.declare(id)?;
        let layout = RecipeLayout::resolve(self)?;
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY)?;
        let recipe = Arc::new(recipe);
        let built = Arc::clone(&recipe);
        self.serve(registry, registry, move |base| {
            let recipe = NewRecipe { id, ..built() };
            add_recipe_to(&layout, base, &recipe, workshop_limit())
        })?;
        self.allow_rebuild(registry)?;
        menu::serve(self, move |menu, bundle| {
            let recipe = NewRecipe { id, ..recipe() };
            menu::list_recipes(menu, bundle, &[menu_entry(&layout, &recipe)])
        })
    }

    /// Adds a derived copy of each vanilla recipe `choose` picks: same
    /// workshop and unlock, its output and cost scaled as [`Derived`] says,
    /// beside the original, in the registry and in the client's crafting
    /// menu. `choose` sees only the game's own recipes, never those mods
    /// add. `salt` keeps the copies' GUIDs apart from other mods'; use the
    /// mod id.
    ///
    /// # Errors
    ///
    /// [`Off`] when the recipe layout is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::Derived;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.derive_recipes("my-mod", |recipe| {
    ///         recipe
    ///             .debug_name()
    ///             .starts_with("Block_")
    ///             .then_some(Derived { output: 2, cost: 1, workshop: None })
    ///     })
    /// }
    /// ```
    pub fn derive_recipes(
        &mut self,
        salt: &str,
        choose: impl Fn(&RecipeView<'_>) -> Option<Derived> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let layout = RecipeLayout::resolve(self)?;
        let salt = fnv1a(salt.as_bytes());
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY)?;
        let choose = Arc::new(choose);
        let chosen = Arc::clone(&choose);
        self.serve(registry, registry, move |base| {
            let vanilla =
                vanilla_registry().and_then(|registry| recipe_guids(&layout, &registry));
            if vanilla.is_none() {
                crate::warn!(
                    "derived recipes may copy recipes of other mods: the game's recipe registry does not read"
                );
            }
            derive_recipes_in(&layout, base, salt, workshop_limit(), &|view| {
                let added = vanilla
                    .as_ref()
                    .is_some_and(|vanilla| !vanilla.contains(&view.guid()));
                if added { None } else { chosen(view) }
            })
        })?;
        menu::serve(self, move |menu, bundle| {
            let entries = vanilla_registry()
                .and_then(|registry| derived_entries(&layout, &registry, salt, &*choose));
            let Some(entries) = entries else {
                crate::warn!(
                    "derived recipes stay out of the crafting menu: the recipe registry does not read"
                );
                return Some(bundle.to_vec());
            };
            menu::list_recipes(menu, bundle, &entries)
        })
    }

    /// Changes recipes in the registry in place, on the server and the
    /// client alike, so the client's crafting menu shows the new counts:
    /// `modify` is called with each recipe and may scale its output and
    /// ingredient counts. It sees the registry as it reaches this patch,
    /// with the recipes and changes of mods that patched before, and the
    /// mods after see its changes. The mod's log says how many recipes
    /// changed. When values `modify` reads change later, such as a server's
    /// join values, [`rebuild_recipes`] applies them.
    ///
    /// # Errors
    ///
    /// [`Off`] when the recipe layout is not reflected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::recipes::RecipeEdit;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.modify_recipes(|recipe: &mut RecipeEdit<'_>| {
    ///         if recipe.view().debug_name().starts_with("Block_") {
    ///             recipe.scale_output(2);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn modify_recipes(
        &mut self,
        modify: impl Fn(&mut RecipeEdit<'_>) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let layout = RecipeLayout::resolve(self)?;
        let registry = RECIPE_REGISTRY.key(types::RECIPE_REGISTRY)?;
        self.serve(registry, registry, move |base| {
            let (built, changed) = modify_recipes_in(&layout, base, &modify)?;
            crate::info!("{changed} recipes changed");
            Some(built)
        })?;
        self.allow_rebuild(registry)
    }
}

/// Builds the recipe registry again with every mod's recipe changes, from
/// any thread, for a mod whose [`Load::modify_recipes`] or
/// [`Load::add_recipe_with`] reads values that changed, such as a server's
/// join values on a joined client: the counts the game holds change in place. Nothing is done while the game has not
/// built the registry yet. See [`rebuild`](crate::content::rebuild) for
/// what a rebuild can change.
///
/// # Errors
///
/// [`Off`] for a mod that neither modifies recipes nor adds them with
/// [`Load::add_recipe_with`], and when another mod's patch changed the
/// registry's length.
///
/// # Examples
///
/// ```
/// fn settings_changed() {
///     if let Err(off) = enshwick::content::recipes::rebuild_recipes() {
///         enshwick::warn!("recipe counts change at the next start: {off}");
///     }
/// }
/// ```
pub fn rebuild_recipes() -> Result<(), Off> {
    crate::content::rebuild(RECIPE_REGISTRY.key(types::RECIPE_REGISTRY)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::workshops::WorkshopId;
    use crate::content::{Blob, Guid, Id};

    const LAYOUT: RecipeLayout = RecipeLayout {
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
    const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    const LOCKER: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
    const RECIPE_ID: Id = Id::new("test:recipe");
    const RECIPE: Guid = RECIPE_ID.guid();

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn the_copy_costs_more_and_makes_the_new_output() {
        let mut blob = Blob::new(&[0u8; 16]);
        let recipe = blob.append(&[0; 0xa8], 8);
        let mut input = vec![0u8; 48];
        input[0x14..0x18].copy_from_slice(&5u32.to_le_bytes());
        let input_at = blob.append(&input, 8);
        let mut output = vec![0u8; 24];
        output[..16].copy_from_slice(&CHEST.0);
        let output_at = blob.append(&output, 8);
        let name_at = blob.append(b"Chest\0", 1);
        assert!(blob.set_array(recipe + 0x28, input_at, 1));
        assert!(blob.set_array(recipe + 0x30, output_at, 1));
        assert!(blob.set_array(recipe + 0x78, name_at, 5));
        assert!(blob.set_array(8, recipe, 1));

        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: LOCKER,
            output_count: 1,
            cost: 2,
            workshop: None,
        };
        let built =
            Blob::new(&add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).unwrap());
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let copy = recipes + 0xa8;
        assert_eq!(&built.bytes()[copy..copy + 16], &RECIPE.0);
        assert_eq!(u32_at(built.bytes(), copy + 0x10), RECIPE.id());
        assert_eq!(u32_at(built.bytes(), copy + 0x5c), RECIPE.id());
        let (inputs, _) = built.array(copy + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 10);
        let (outputs, _) = built.array(copy + 0x30).unwrap();
        assert_eq!(&built.bytes()[outputs..outputs + 16], &LOCKER.0);
        assert_eq!(u32_at(built.bytes(), outputs + 0x10), LOCKER.id());
        let (name, len) = built.array(copy + 0x78).unwrap();
        assert_eq!(&built.bytes()[name..name + len as usize], b"Chest");
        let (inputs, _) = built.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 5);
    }

    #[test]
    fn derived_copies_double_the_output_of_the_chosen_recipes_only() {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&[0; 0xa8 * 2], 8);
        for (i, (name, made)) in [(&b"Block_A\0"[..], 100u32), (&b"Prop_B\0"[..], 1)]
            .iter()
            .enumerate()
        {
            let at = first + i * 0xa8;
            let mut output = vec![0u8; 24];
            output[0x14..0x18].copy_from_slice(&made.to_le_bytes());
            let output_at = blob.append(&output, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + 0x30, output_at, 1));
            assert!(blob.set_array(at + 0x78, name_at, u32::try_from(name.len() - 1).unwrap()));
        }
        assert!(blob.set_array(8, first, 2));
        let built = derive_recipes_in(&LAYOUT, blob.bytes(), 7, WORKSHOP_RECIPES, &|r| {
            r.debug_name().starts_with("Block_").then_some(Derived {
                output: 2,
                cost: 1,
                workshop: None,
            })
        })
        .unwrap();
        let built = Blob::new(&built);
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 3);
        let copy = RecipeView {
            blob: &built,
            at: recipes + 2 * 0xa8,
            layout: &LAYOUT,
        };
        assert_eq!(copy.debug_name(), "Block_A");
        assert_eq!(copy.output().unwrap().1, 200);
        assert_eq!(copy.guid(), derived_guid(Guid([0; 16]), 7));
        let original = RecipeView {
            blob: &built,
            at: recipes,
            layout: &LAYOUT,
        };
        assert_eq!(original.output().unwrap().1, 100);
    }

    #[test]
    fn a_recipe_can_move_to_another_workshop() {
        let mut blob = Blob::new(&[0u8; 16]);
        let recipe = blob.append(&[0; 0xa8], 8);
        assert!(blob.write(recipe + 0x24, &7u32.to_le_bytes()));
        let mut output = vec![0u8; 24];
        output[..16].copy_from_slice(&CHEST.0);
        let output_at = blob.append(&output, 8);
        assert!(blob.set_array(recipe + 0x30, output_at, 1));
        assert!(blob.set_array(8, recipe, 1));
        let bench = WorkshopId::new(Guid::parse("5b828914-9b48-40f8-b541-72e9b942cee6"));
        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: LOCKER,
            output_count: 1,
            cost: 1,
            workshop: Some(bench),
        };
        let built = add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).unwrap();
        let built = Blob::new(&built);
        let (recipes, _) = built.array(8).unwrap();
        let copy = recipes + 0xa8;
        assert_eq!(&built.bytes()[copy + 0x14..copy + 0x24], &bench.guid().0);
        assert_eq!(u32_at(built.bytes(), copy + 0x24), bench.id());
        assert_eq!(u32_at(built.bytes(), recipes + 0x24), 7);

        // Derived copies move too, and count against the new workshop.
        let derived = derive_recipes_in(&LAYOUT, blob.bytes(), 7, 1, &|_| {
            Some(Derived {
                output: 2,
                cost: 1,
                workshop: Some(bench),
            })
        })
        .unwrap();
        assert_eq!(
            Blob::new(&derived).array(8).unwrap().1,
            1,
            "limit 1, held 0"
        );
        let derived = derive_recipes_in(&LAYOUT, blob.bytes(), 7, RESERVED_FOR_SINGLE + 1, &|_| {
            Some(Derived {
                output: 2,
                cost: 1,
                workshop: Some(bench),
            })
        })
        .unwrap();
        let derived = Blob::new(&derived);
        let (recipes, count) = derived.array(8).unwrap();
        assert_eq!(count, 2);
        assert_eq!(u32_at(derived.bytes(), recipes + 0xa8 + 0x24), bench.id());
    }

    #[test]
    fn modified_recipes_change_in_place_and_stay_one_each() {
        let mut blob = Blob::new(&[0u8; 16]);
        let first = blob.append(&[0; 0xa8 * 2], 8);
        for (i, name) in [&b"Block_A"[..], &b"Prop_Decoration_Chest"[..]]
            .iter()
            .enumerate()
        {
            let at = first + i * 0xa8;
            let mut input = vec![0u8; 48 * 2];
            input[0x14..0x18].copy_from_slice(&3u32.to_le_bytes());
            input[48 + 0x14..48 + 0x18].copy_from_slice(&4u32.to_le_bytes());
            let input_at = blob.append(&input, 8);
            let mut output = vec![0u8; 24];
            output[0x14..0x18].copy_from_slice(&10u32.to_le_bytes());
            let output_at = blob.append(&output, 8);
            let name_at = blob.append(name, 1);
            assert!(blob.set_array(at + 0x28, input_at, 2));
            assert!(blob.set_array(at + 0x30, output_at, 1));
            assert!(blob.set_array(at + 0x78, name_at, u32::try_from(name.len()).unwrap()));
        }
        assert!(blob.set_array(8, first, 2));
        let (built, changed) = modify_recipes_in(&LAYOUT, blob.bytes(), &|recipe| {
            if recipe.view().debug_name().starts_with("Block_") {
                recipe.scale_output(3);
                recipe.scale_inputs(1);
            }
        })
        .unwrap();
        assert_eq!(changed, 1);
        assert_eq!(built.len(), blob.bytes().len(), "in place, nothing added");
        let built = Blob::new(&built);
        let (recipes, count) = built.array(8).unwrap();
        assert_eq!(count, 2);
        let view = |at| RecipeView {
            blob: &built,
            at,
            layout: &LAYOUT,
        };
        assert_eq!(view(recipes).output().unwrap().1, 30);
        assert_eq!(view(recipes + 0xa8).output().unwrap().1, 10);
        let (inputs, _) = built.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(built.bytes(), inputs + 0x14), 3);
        assert_eq!(u32_at(built.bytes(), inputs + 48 + 0x14), 4);

        // A second patch composes: it sees the first one's counts.
        let (again, _) =
            modify_recipes_in(&LAYOUT, built.bytes(), &|recipe| recipe.scale_inputs(2)).unwrap();
        let again = Blob::new(&again);
        let (inputs, _) = again.array(recipes + 0x28).unwrap();
        assert_eq!(u32_at(again.bytes(), inputs + 48 + 0x14), 8);
        assert_eq!(view(recipes).output().unwrap().1, 30);
    }

    #[test]
    fn an_item_without_a_recipe_builds_nothing() {
        let mut blob = Blob::new(&[0u8; 16]);
        assert!(blob.set_array(8, 16, 0));
        let new = NewRecipe {
            id: RECIPE_ID,
            like: CHEST,
            output: LOCKER,
            output_count: 1,
            cost: 1,
            workshop: None,
        };
        assert!(add_recipe_to(&LAYOUT, blob.bytes(), &new, WORKSHOP_RECIPES).is_none());
    }
}
