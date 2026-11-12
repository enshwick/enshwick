//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::game_events::CraftRecipeEvent;
use crate::vetoes::{BuildVerdict, Placement};
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(vetoes)]
impl Load<'_> {
    /// Runs `filter` on each player's craft request in the server's world,
    /// before the crafting system reads it; returning `false` refuses it,
    /// and nothing is made or taken. It runs on the game thread, where
    /// `World` reads work, in the dedicated server or
    /// a hosting client.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the craft event.
    ///
    /// # Examples
    ///
    /// See the [module](crate::vetoes).
    #[script]
    pub fn on_craft<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&CraftRecipeEvent) -> bool + Send + Sync + 'static;

    /// Runs `filter` after the game's cursor check allows a placement; a
    /// refusal refuses it as the game refuses a placement without building
    /// permission, and [`BuildVerdict::RefuseBecause`] tells the builder why:
    /// a client running the mod shows the reason under the build cursor,
    /// and the server sends it as one notice, at most once a second per
    /// player and again only after that player's refusals stopped for a
    /// second. It runs on both sides, on the cursor systems'
    /// worker threads: the server's answer decides, and a client running the
    /// mod shows the cursor as not allowed. The cursor check runs on every
    /// frame a player aims to build, so `filter` must not allocate, lock or
    /// log, and gives the same answer on both sides. A `bool` answer allows
    /// or refuses without a reason.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the cursor check.
    ///
    /// # Examples
    ///
    /// See the [module](crate::vetoes).
    pub fn on_build<F, V>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Placement) -> V + Send + Sync + 'static,
        V: Into<BuildVerdict>;

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
        F: Fn(&Placement) -> BuildVerdict + Send + Sync + 'static;
}
