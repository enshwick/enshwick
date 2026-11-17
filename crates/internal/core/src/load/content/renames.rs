//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::renames::Renames;
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_renames)]
impl Load<'_> {
    /// Hands the loader every content rename the mod ever made, oldest
    /// first; call it after [`Load::declare`] of each content id renamed to
    /// and before adding the renamed content, so an item is served under its
    /// old GUIDs too.
    ///
    /// # Errors
    ///
    /// [`Off`] for content renamed to itself, a second call, or a rename to
    /// a GUID that is none of the mod's declared content ids, the game's
    /// content or the old GUID of a later rename.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    #[script]
    pub fn rename_content(&mut self, renames: Renames) -> Result<(), Off>;
}
