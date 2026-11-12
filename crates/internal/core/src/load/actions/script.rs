//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::actions::Actions;
use crate::i18n::Texts;
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(actions)]
impl Load<'_> {
    /// The mod's player actions, with the labels' texts `texts`, as
    /// [`Load::actions`] gives them.
    ///
    /// # Errors
    ///
    /// As [`Load::actions`].
    #[script(name = "actions")]
    pub(crate) fn actions_script(&mut self, texts: Texts) -> Result<Actions, Off>;
}
