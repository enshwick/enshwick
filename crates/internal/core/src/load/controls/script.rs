//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};
use enshwick_actions::__internal::{ScriptControl, ScriptFired};
use enshwick_actions::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(controls)]
impl Load<'_> {
    /// Declares `control` with its handler `on`, as [`Load::control`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::control`].
    #[script(name = "control")]
    pub(crate) fn control_script(
        &mut self,
        control: ScriptControl,
        on: impl Fn(ScriptFired) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
