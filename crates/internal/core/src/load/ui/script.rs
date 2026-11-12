//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::{Load, Off};
use enshwick_ui::__internal::{ScriptFrame, ScriptHud};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
#[crate::script(ui)]
impl Load<'_> {
    /// Runs `draw` once per frame of the interface with the frame lent to
    /// it, as [`Load::on_frame`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::on_frame`].
    #[script(name = "on_frame")]
    pub(crate) fn on_frame_script(
        &mut self,
        draw: impl Fn(ScriptFrame) + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Runs `draw` once per frame while the game draws its HUD, with the
    /// HUD lent to it, as [`Load::on_hud`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::on_hud`].
    #[script(name = "on_hud")]
    pub(crate) fn on_hud_script(
        &mut self,
        draw: impl Fn(ScriptHud) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}

#[cfg(feature = "unstable")]
#[enshwick_macros::forward(base)]
#[crate::script(ui_extension)]
impl Load<'_> {
    /// Adds a panel at `edge` of the game's screen `screen` and runs
    /// `draw` once per frame while it is open, with the frame lent to
    /// it, as [`Load::extend_screen`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::extend_screen`].
    #[script(name = "extend_screen")]
    pub(crate) fn extend_screen_script(
        &mut self,
        screen: crate::ui::HudScreenType,
        edge: crate::ui::Edge,
        draw: impl Fn(ScriptFrame) + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Adds a panel that shows only for the screen hosts made from
    /// `hosts`, as [`Load::extend_screen_for`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::extend_screen_for`].
    #[script(name = "extend_screen_for")]
    pub(crate) fn extend_screen_for_script(
        &mut self,
        screen: crate::ui::HudScreenType,
        edge: crate::ui::Edge,
        hosts: Vec<crate::content::Guid>,
        draw: impl Fn(ScriptFrame) + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// Calls `handler` with each change of the game's screen the player
    /// has open, as [`Load::on_screen_changed`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::on_screen_changed`].
    #[script(name = "on_screen_changed")]
    pub(crate) fn on_screen_changed_script(
        &mut self,
        handler: impl Fn(crate::ui::ScreenChanged) + Send + Sync + 'static,
    ) -> Result<(), Off>;
}
