//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::recipes::*;
use crate::{Load, Off};
use enshwick_content::__load::*;
#[cfg(feature = "unstable")]
use std::sync::Arc;

#[enshwick_macros::forward(base)]
#[crate::script(content_recipes)]
impl Load<'_> {
    /// Adds a recipe to the game, on the server and the client alike: a copy
    /// of the recipe that makes `like`, at the same workshop and unlocked with
    /// it, making `output` instead, beside recipes other mods add. The
    /// client's crafting menu lists it right after the copied recipe (see
    /// [`menu`](super::menu)).
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
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
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::recipes::NewRecipe;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_recipe(NewRecipe {
    ///         id: id!("locker.recipe"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         output: id!("locker").guid(),
    ///         output_count: 1,
    ///         cost: 2,
    ///         workshop: None,
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    #[script]
    pub fn add_recipe(&mut self, recipe: NewRecipe) -> Result<(), Off>;

    /// Adds a recipe as [`add_recipe`](Self::add_recipe) does, asking
    /// `recipe` for it each time the registry is built, so a cost read from
    /// settings that change, such as a world's or a server's join values,
    /// applies through [`rebuild_recipes`]. Every answer keeps the id of the
    /// first.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
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
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::recipes::NewRecipe;
    ///
    /// static COST: AtomicU32 = AtomicU32::new(2);
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_recipe_with(|| NewRecipe {
    ///         id: id!("locker.recipe"),
    ///         like: Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d"),
    ///         output: id!("locker").guid(),
    ///         output_count: 1,
    ///         cost: COST.load(Ordering::Relaxed),
    ///         workshop: None,
    ///     })
    /// }
    /// ```
    #[cfg(feature = "unstable")]
    pub fn add_recipe_with(
        &mut self,
        recipe: impl Fn() -> NewRecipe + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Adds a derived copy of each vanilla recipe `choose` picks: same
    /// workshop and unlock, its output and cost scaled as [`Derived`] says,
    /// beside the original, in the registry and in the client's crafting
    /// menu. `choose` sees only the game's own recipes, never those mods
    /// add. `salt` keeps the copies' GUIDs apart from other mods'; use the
    /// mod id.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
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
    #[cfg(feature = "unstable")]
    #[script]
    pub fn derive_recipes(
        &mut self,
        salt: &str,
        choose: impl Fn(&RecipeView<'_>) -> Option<Derived> + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
