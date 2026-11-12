//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::ui::{Edge, HudScreenType, ScreenChanged};
use crate::{Load, Off};
use enshwick_ui::__load::*;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds a panel at `edge` of the game's screen `screen` and runs `draw`
    /// once per frame while the player has that screen open, on the thread
    /// that draws the interface; `draw` fills the panel with the widgets of
    /// the [`Frame`](crate::ui::Frame) it is handed, and
    /// [`Frame::window`](crate::ui::Frame::window) is refused there. The panel
    /// shows above the game's screen, never changes its layout, and goes
    /// away with it; its buttons are the mod's own, so a press that changes
    /// the world goes to the server as a player action, which the server
    /// checks again. Panels at one edge share one window, which shows only
    /// once a panel draws in it. Client only. A panic in `draw` turns the
    /// mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, after `init`, on a game build without the
    /// screen, or for a screen on which the player is in the world with no
    /// window open ([`HudScreenType::InGame`], [`HudScreenType::NoUi`],
    /// [`HudScreenType::TakeScreenshot`] and the multitools' building
    /// screens), where a panel would cover play.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::{Edge, HudScreenType, Label, Style};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.extend_screen(HudScreenType::StorageChest, Edge::Right, |panel| {
    ///         panel.label(Label::new("Shared with your guild").style(Style::Secondary));
    ///         if panel.button("Sort by name") {
    ///             // Send the server a player action of the mod's own.
    ///         }
    ///     })
    /// }
    /// ```
    pub fn extend_screen<F>(
        &mut self,
        screen: HudScreenType,
        edge: Edge,
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut crate::ui::Frame<'_>) + Send + 'static;

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds a panel as [`Load::extend_screen`] does that shows only while
    /// the open screen's host was made from one of the templates `hosts`,
    /// such as the mod's own survivor or altar kind by its content id's
    /// GUID; empty `hosts` show it for every host. The client reads the
    /// host's template in its last step, so the panel shows from the second
    /// frame of a screen on. A mod that needs what only the server knows of
    /// the host, such as a survivor's NPC id, asks its server part.
    ///
    /// # Errors
    ///
    /// As [`Load::extend_screen`], and [`Off`] for more than
    /// [`UiExtension::MAX_HOSTS`](crate::abi::UiExtension::MAX_HOSTS) templates.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::id;
    /// use enshwick::ui::{Edge, HudScreenType};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let ours = [id!("guard").guid()];
    ///     load.extend_screen_for(HudScreenType::NpcDialog, Edge::Right, &ours, |panel| {
    ///         let _hello = panel.button("Hello");
    ///     })
    /// }
    /// ```
    pub fn extend_screen_for<F>(
        &mut self,
        screen: HudScreenType,
        edge: Edge,
        hosts: &[crate::content::Guid],
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut crate::ui::Frame<'_>) + Send + 'static;

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Calls `handler` on the thread that draws the interface when the
    /// game's screen the player has open changes, as
    /// [`Frame::open_screen`](crate::ui::Frame::open_screen) reads it: a chest,
    /// the map or a survivor's dialog opening or closing. The screens are
    /// compared once per frame, which costs a few bytes compared; the first
    /// frame in a game counts as a change from `None`. Client only. A panic
    /// in `handler` turns the mod off.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server or after `init`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::HudScreenType;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.on_screen_changed(|change| {
    ///         if change.now == Some(HudScreenType::StorageChest) {
    ///             enshwick::info!("a chest opened");
    ///         }
    ///     })
    /// }
    /// ```
    pub fn on_screen_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: FnMut(&ScreenChanged) + Send + 'static;
}
