//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::progress::{Comfort, Experience};
use crate::{Load, Off};
use enshwick_knowledge::__internal::ScriptLevels;
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(progress_hooks)]
impl Load<'_> {
    /// Runs `filter` on each comfort a provider registers, as
    /// `Load::on_comfort` does: the comfort it answers registers; nil keeps
    /// the comfort as it was.
    ///
    /// # Errors
    ///
    /// As [`Load::on_comfort`].
    #[script(name = "on_comfort")]
    pub(crate) fn on_comfort_script(
        &mut self,
        filter: impl Fn(Comfort) -> Option<i32> + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Runs `filter` on each tick's experience with the level table, as
    /// `Load::on_experience` does: it returns the rows it changes, each
    /// written over the row of the same character, or nil to keep them all.
    ///
    /// # Errors
    ///
    /// As [`Load::on_experience`].
    #[script(name = "on_experience")]
    pub(crate) fn on_experience_script(
        &mut self,
        filter: impl Fn(Vec<Experience>, ScriptLevels) -> Option<Vec<Experience>>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), Off>;
}
