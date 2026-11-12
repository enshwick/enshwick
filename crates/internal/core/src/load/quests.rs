//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::Load;
use crate::quests::Quests;
use enshwick_knowledge::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The [`Quests`] of the running game; every call of one mod gives the
    /// same, which reads the journal once.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> enshwick::quests::Quests {
    ///     load.quests()
    /// }
    /// ```
    #[must_use]
    pub fn quests(&mut self) -> Quests;
}
