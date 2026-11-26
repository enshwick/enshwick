//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::vetoes::Verdict;
use crate::world::Join;
use crate::{Load, Off};
use enshwick_world::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(world_join)]
impl Load<'_> {
    /// Runs `rule` for each account that joins the server, before its player
    /// enters the world; a refusal (`false` or a [`Verdict`] that refuses)
    /// refuses it, and its player reads the game's own "the host denies you
    /// access", as for an account on the server's ban list, which stays as
    /// it was. The first rule that refuses decides, in load order. It runs on
    /// the thread the server reads joins on, so it answers at once from what
    /// the mod holds. A rule that panics turns its mod off and lets the
    /// account in. A client runs it while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the rule, as after init, or on a game
    /// build without the join path, so the mod can choose another way.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::PlayerKey;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     // Only the accounts on this mod's list join.
    ///     let allowed = [PlayerKey::new(0x5c86_aa43_5230_614e)];
    ///     load.on_join(move |join| join.player.is_some_and(|key| allowed.contains(&key)))
    /// }
    /// ```
    pub fn on_join<F, V>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> V + Send + Sync + 'static,
        V: Into<Verdict>;

    /// Runs `rule` for each account that joins the server, before its player
    /// enters the world; `false` or a string refuses it, nil or `true` lets
    /// it in, and any other value refuses. A refused player reads the game's
    /// own "the host denies you access". The first rule that refuses
    /// decides, in load order. It runs on the thread the server reads joins
    /// on, so it answers at once from what the mod holds. A client runs it
    /// while it hosts a game.
    ///
    /// # Errors
    ///
    /// As [`Load::on_join`].
    #[script(name = "on_join")]
    pub(crate) fn join_filter<F>(&mut self, rule: F) -> Result<(), Off>
    where
        F: Fn(&Join) -> Verdict + Send + Sync + 'static;
}
