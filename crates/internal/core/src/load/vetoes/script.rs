//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::game_events::CraftRecipeEvent;
use crate::vetoes::{Placement, Verdict};
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(vetoes_build)]
impl Load<'_> {
    /// Runs `filter` on each player's craft request in the server's world,
    /// as [`Load::on_craft`] does. The filter returns `false` to refuse, nil
    /// or `true` to allow; any other value refuses. It runs on the game
    /// thread, where `World` reads work.
    ///
    /// # Errors
    ///
    /// As [`Load::on_craft`].
    #[script(name = "on_craft")]
    pub(crate) fn craft_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> Verdict + Send + Sync + 'static;

    /// Runs `filter` after the game's cursor check allows a placement, as
    /// [`Load::on_build`] does. The filter returns nil or `true` to allow,
    /// `false` to refuse as the game refuses a placement without building
    /// permission, or a string to refuse and tell the builder why: a key of
    /// the mod's texts or plain words; any other value refuses. It runs on
    /// both sides, on every frame a player aims to build: the server's
    /// answer decides, so `filter` gives the same answer on both sides and
    /// stays quick.
    ///
    /// # Errors
    ///
    /// As [`Load::on_build`].
    #[script(name = "on_build")]
    pub(crate) fn build_filter<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> Verdict + Send + Sync + 'static;
}
