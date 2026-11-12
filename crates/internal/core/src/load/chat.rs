//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::chat::{ChatLine, ChatVerdict};
use crate::{Load, Off};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(chat)]
impl Load<'_> {
    /// Runs `filter` on every line a player types, inside the game's chat
    /// post on the server's game thread, after the chat mute and in load
    /// order with other mods' filters; it keeps, drops or rewrites the
    /// line. A panic turns this filter off alone, and lines pass it. On a
    /// client it runs while it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] after init or on a build without the game's chat post.
    ///
    /// # Examples
    ///
    /// See the [module](crate::chat).
    #[script]
    pub fn on_chat<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) -> ChatVerdict + Send + Sync + 'static;

    /// Calls `handler` for every line that reached the server's chat, on
    /// the game thread after the step: a player's line as the filters left
    /// it, an emote or a notice. A player's lines reach it at most eight a
    /// second; lines a mod sends never do. A panic in `handler` turns the
    /// mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] after init.
    ///
    /// # Examples
    ///
    /// See the [module](crate::chat).
    #[script]
    pub fn on_chat_said<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: Fn(&ChatLine) + Send + Sync + 'static;
}
