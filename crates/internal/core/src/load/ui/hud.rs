//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::ui::Hud;
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Runs `draw` once per frame while the game draws its HUD, on the
    /// thread that draws it; what it draws hides with the HUD, in menus and
    /// in screenshot mode. Client only. A panic in `draw` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`.
    ///
    /// # Examples
    ///
    /// See [`Hud`].
    pub fn on_hud<F>(&mut self, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut Hud<'_>) + Send + 'static;
}
