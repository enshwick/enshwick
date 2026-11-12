//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::controls::{Control, Fired};
use crate::{Load, Off};
use enshwick_actions::__load::*;

mod script;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Declares `control` with its handler, which runs on the client's
    /// interface thread for a key, and on the server's game thread for a
    /// [linked](Control::link) game action; a client hosting a game may run
    /// both at once.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed name, a name declared before, or when the
    /// loader refuses the control.
    ///
    /// # Examples
    ///
    /// See the [module](crate::controls).
    pub fn control<F>(&mut self, control: Control, on: F) -> Result<(), Off>
    where
        F: Fn(&Fired<'_>) + Send + Sync + 'static;
}
