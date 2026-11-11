//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::resources::{Bytes, Value as Data};
use crate::script::{
    Class, Error, Field, FromScript, Function, IntoScript, Object, Record, Ty, Typed, Value,
};
use crate::{ItemId, Off, RecipeId};
use enshwick_content::__load::*;

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(content_recipe_with)]
impl Load<'_> {
    /// Adds a recipe as `add_recipe` does, asking `recipe` for it each time
    /// the registry is built, so a cost read from settings applies through
    /// `rebuild_recipes`. Every answer keeps the id of the first; an answer
    /// that fails or is nil keeps the first recipe. Unstable.
    #[script(name = "add_recipe_with")]
    pub(crate) fn add_recipe_with_script(
        &mut self,
        recipe: impl Fn() -> Option<super::recipes::NewRecipe> + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
