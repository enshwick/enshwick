//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::loot::{Container, LootPass};
use crate::{Load, Off};
use enshwick_inventory::__load::*;

mod drops;

#[enshwick_macros::forward(base)]
#[crate::script(loot_pass)]
impl Load<'_> {
    /// Runs `handler` after every run of the game's loot container check, as
    /// `Load::on_loot_pass` does, with that run's containers as a list.
    ///
    /// # Errors
    ///
    /// As [`Load::on_loot_pass`].
    #[script(name = "on_loot_pass")]
    pub(crate) fn on_loot_pass_script(
        &mut self,
        handler: impl Fn(Vec<Container>) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Runs `handler` after every run of the game's loot container check in
    /// the server's world, on the worker thread that runs it, with that run's
    /// containers. One handler per mod.
    ///
    /// # Errors
    ///
    /// [`Off`] for a second handler, or when the loader or the game lacks a
    /// piece.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_loot_pass(|pass| {
    ///         let mut emptied = 0;
    ///         pass.each(|container| emptied += usize::from(container.is_empty));
    ///         let _ = emptied;
    ///     })
    /// }
    /// ```
    pub fn on_loot_pass<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&LootPass<'_>) + Send + Sync + 'static;
}
