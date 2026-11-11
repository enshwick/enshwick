//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::game_events::EventType;
use crate::{Load, Off};

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Every game event type of the running build, read from the game's
    /// reflection, for tools that name what they see: the event types that
    /// exist, not only those the server raises.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot list them.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let types = load.game_event_types()?;
    ///     enshwick::info!("{} game event types on this build", types.len());
    ///     Ok(())
    /// }
    /// ```
    pub fn game_event_types(&self) -> Result<Vec<EventType>, Off>;
}
