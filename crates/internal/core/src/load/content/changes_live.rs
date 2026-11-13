//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::content::changes::{ChangeHandle, Field, Target};
use crate::{Load, Off};
use enshwick_content::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(content_live)]
impl Load<'_> {
    /// Declares during `init` a change of `field` of `target`, from
    /// `source`, whose value the mod sets later through the handle. The
    /// loader builds what it reaches from the first read on, so a set after
    /// init needs no restart. A field players' games read needs the same
    /// value on every side: only a mod or pack of side both declares one, and
    /// it sets the value from a synced setting.
    ///
    /// # Errors
    ///
    /// [`Off`] for a field the target does not take (an ingredient, whose
    /// change alters a recipe's length, or a field a selection does not
    /// reach), for a field clients read in a mod or pack of side server, after
    /// init, and when the running build does not reflect the field.
    ///
    /// # Examples
    ///
    /// See [`ChangeHandle`].
    #[script]
    pub fn declare_change(
        &mut self,
        target: Target,
        field: Field,
        source: &str,
    ) -> Result<ChangeHandle, Off>;
}
