//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::ui::{Frame, Ui};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The interface services: dialogs from any thread. Client only.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server.
    ///
    /// # Examples
    ///
    /// See [`Ui`].
    pub fn ui(&mut self) -> Result<Ui, Off>;

    /// Runs `draw` once per frame of the interface, on the thread that draws
    /// it, from the title screen on; it draws with the [`Frame`] it is
    /// handed. Client only. A panic in `draw` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`. When the loader could not link the interface on this game
    /// build, `draw` never runs.
    ///
    /// # Examples
    ///
    /// See the [module](crate::ui).
    pub fn on_frame<F>(&mut self, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut Frame<'_>) + Send + 'static;
}

#[cfg(feature = "unstable")]
mod extension;
mod hud;
mod script;
