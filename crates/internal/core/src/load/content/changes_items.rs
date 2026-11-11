//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::changes::ItemsChange;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_change_items)]
impl Load<'_> {
    /// Declares `change` during `init`, from `source`, as
    /// [`Load::change`] declares a change of one entry; the loader turns it
    /// into one change of each vanilla item the selection picks, read from
    /// the game's data, in this mod's place in load order. Items of mods
    /// are not picked.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field a selection does not change, a value the field
    /// does not take, after init, and when the running build does not
    /// reflect the field.
    ///
    /// # Examples
    ///
    /// See [`ItemsChange`].
    #[script]
    pub fn change_items(&mut self, change: &ItemsChange, source: &str) -> Result<(), Off>;
}
