//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::vetoes::Verdict;
use crate::world::Rename;
use crate::{Load, Off};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(world_rename)]
impl Load<'_> {
    /// Runs `filter` before the server applies a player's rename of any
    /// entity, on its game thread; a refusal (`false` or a [`Verdict`] that
    /// refuses) refuses the rename. The game itself checks no permission for
    /// renames. A client runs it while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the filter, as after init.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Only administrators rename anything.
    ///     load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
    /// }
    /// ```
    pub fn on_rename<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// Runs `filter` before the server applies a player's rename of any
    /// entity, on its game thread; `false` or a string refuses the rename,
    /// nil or `true` allows it, and any other value refuses. The game itself
    /// checks no permission for renames. A client runs it while it hosts a
    /// game.
    ///
    /// # Errors
    ///
    /// As [`Load::on_rename`].
    #[script(name = "on_rename")]
    pub(crate) fn rename_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> Verdict + Send + Sync + 'static;
}
