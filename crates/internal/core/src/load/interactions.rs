//! `Load`'s methods of a module below core, forwarded to the base handle.

#[cfg(feature = "unstable")]
use crate::ItemId;
use crate::interactions::Interaction;
#[cfg(feature = "unstable")]
use crate::interactions::{ObjectPrompt, Targets};
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The players' prompt targets in the game server, dedicated or hosted
    /// by the client.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] when the game build lacks the target's read.
    ///
    /// # Examples
    ///
    /// See [`Targets`].
    pub fn interaction_targets(&mut self) -> Result<Targets, Off>;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(interaction_prompts)]
impl Load<'_> {
    /// Gives the prompt of every object placed from `item` a game verb, the
    /// mod's own text on clients with Enshwick, or the game's empty action
    /// in place of the object's own, so the mod's action on
    /// `AcceptOfferedInteractionEvent` runs alone. Call it in the mods of
    /// both sides that know the item: the server writes the verb and the
    /// action, a client shows the text. One prompt per item across all mods.
    ///
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, for item 0 or a prompt that changes nothing, for
    /// an item another prompt has, or for a verb or an action on a build
    /// without the server's writes; a build without the client's text site
    /// turns the text off alone.
    ///
    /// # Examples
    ///
    /// See [`ObjectPrompt`](crate::interactions::ObjectPrompt).
    #[script]
    pub fn prompt(&mut self, item: ItemId, prompt: ObjectPrompt) -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
#[crate::script(interactions)]
impl Load<'_> {
    /// Runs `filter` before the game allows a player's interaction with an
    /// entity; returning `false` refuses it the game's own way. It runs in
    /// the server's world only, in the dedicated server or a hosting client,
    /// on the thread of the system that checks the interaction: keep it quick
    /// and never block.
    ///
    /// The game asks whether to offer an interaction for every entity in
    /// each player's reach on every frame, so `filter` runs many times a
    /// frame: it must not allocate, lock or log. Moves, sorts, stock draws,
    /// dismantling and upgrades ask once per action. An interaction without
    /// a player's character or without a target is allowed unasked.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build that lacks a piece.
    ///
    /// # Examples
    ///
    /// See the [module](crate::interactions).
    #[script]
    pub fn on_interaction<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static;
}
