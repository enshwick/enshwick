//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::actions::Actions;
use crate::i18n::Texts;
use crate::{Load, Off};
use enshwick_actions::__load::*;

mod script;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The mod's player actions: declare each with [`Actions::add`]. `texts`
    /// holds the labels' texts, as [`Load::texts`] reads them; the
    /// platform's own replies are in it under `actions.*` too, so a mod may
    /// change them. The loader keeps these texts beneath the ones the mod
    /// hands itself (see [`Load::texts`]), before or after: a key of both
    /// reads as the mod handed it, and an action's control
    /// ([`Actions::control`]) reads its label from them in each player's
    /// language, a missing variety falling back as any text does.
    ///
    /// # Errors
    ///
    /// [`Off`] for a second call, or when the loader refuses the actions.
    ///
    /// # Examples
    ///
    /// See the [module](crate::actions).
    pub fn actions(&mut self, texts: Texts) -> Result<Actions, Off>;
}
