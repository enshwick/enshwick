//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::ContentRename;
use crate::content::Guid;
use crate::content::renames::*;
use crate::{ItemId, Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_renames)]
impl Load<'_> {
    /// Hands the loader every content rename the mod ever made, oldest
    /// first; call it before adding the renamed content, so an item is
    /// served under its old GUIDs too.
    ///
    /// # Errors
    ///
    /// [`Off`] for content renamed to itself or a second call.
    ///
    /// # Examples
    ///
    /// See [`Renames`].
    #[script]
    pub fn rename_content(&mut self, renames: Renames) -> Result<(), Off>;
}
