//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{ChangeOp, DataChange, DataSelect, Header, Str};
use crate::content::Guid;
use crate::content::recipes::RecipeKind;
use crate::content::workshops::WorkshopId;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_change_recipes)]
impl Load<'_> {
    /// Declares `change` during `init`, from `source`, as
    /// [`Load::change`] declares a change of one recipe; the loader turns it
    /// into one change of each vanilla recipe the selection picks, read from
    /// the game's data, in this mod's place in load order. Recipes of mods
    /// are not picked, and a recipe the mod changes by name keeps that
    /// change of the field alone.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field a selection of recipes does not change, a value
    /// the field does not take, after init, and when the running build does
    /// not reflect the field.
    ///
    /// # Examples
    ///
    /// See [`RecipesChange`].
    #[script]
    pub fn change_recipes(&mut self, change: &RecipesChange, source: &str) -> Result<(), Off>;
}
