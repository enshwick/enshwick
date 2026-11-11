//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::changes::Change;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_change)]
impl Load<'_> {
    /// Declares `change` during `init`, from `source`, such as
    /// `items.toml:12`, which the conflict reports name. The loader merges it
    /// with every mod's changes as the [module](crate::content::changes) says, and applies it on
    /// this side; a value players' games read needs the same change on
    /// every side, so such a mod is a join mod. The loader keeps every stack
    /// size above the game's that a world was saved with and never serves
    /// less, and holds the game on a build it would leave vanilla.
    ///
    /// # Errors
    ///
    /// [`Off`] for a value the field does not take, after init, and when the
    /// running build does not reflect the field.
    ///
    /// # Examples
    ///
    /// See the [module](crate::content::changes).
    #[script]
    pub fn change(&mut self, change: &Change, source: &str) -> Result<(), Off>;
}
