//! Panels a mod adds to the game's own screens, shown and hidden with them.

use std::sync::Mutex;

use crate::abi::{Header, Str, UiExtension};
use crate::{Load, Off};

use super::client_only;
use super::frame::{Drawer, draw_frame};
use super::screens::{HudScreenType, NAMES};

impl HudScreenType {
    /// The game's name of the screen, which the loader looks up in the
    /// running build.
    ///
    /// # Examples
    ///
    /// See [`HudScreenType`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// The screen the game names `name`; `None` for a name this enum lacks.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::ui::HudScreenType;
    ///
    /// assert_eq!(HudScreenType::from_name("Map"), Some(HudScreenType::Map));
    /// assert_eq!(HudScreenType::from_name("Map2"), None);
    /// ```
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let index = NAMES.iter().position(|known| *known == name)?;
        Self::ALL.get(index).copied()
    }
}

/// The edge of the screen an extension's panel sits at, centred along it.
///
/// # Examples
///
/// See [`Load::extend_screen`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Edge {
    /// The left edge.
    Left,
    /// The right edge.
    #[default]
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

impl Edge {
    const fn to_raw(self) -> u32 {
        match self {
            Self::Left => UiExtension::LEFT,
            Self::Right => UiExtension::RIGHT,
            Self::Top => UiExtension::TOP,
            Self::Bottom => UiExtension::BOTTOM,
        }
    }
}

impl Load<'_> {
    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Adds a panel at `edge` of the game's screen `screen` and runs `draw`
    /// once per frame while the player has that screen open, on the thread
    /// that draws the interface; `draw` fills the panel with the widgets of
    /// the [`Frame`](super::Frame) it is handed, and
    /// [`Frame::window`](super::Frame::window) is refused there. The panel
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
    /// # extern crate enshwick_core as enshwick;
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
        F: FnMut(&mut super::Frame<'_>) + Send + 'static,
    {
        self.extend_screen_for(screen, edge, &[], draw)
    }

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
    /// [`UiExtension::MAX_HOSTS`] templates.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
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
        F: FnMut(&mut super::Frame<'_>) + Send + 'static,
    {
        let host = self.host();
        client_only(host, "screen extensions")?;
        let host_count = u32::try_from(hosts.len())
            .ok()
            .filter(|&count| count <= UiExtension::MAX_HOSTS)
            .ok_or_else(|| {
                Off::new(format!(
                    "{} screen hosts, past {}",
                    hosts.len(),
                    UiExtension::MAX_HOSTS
                ))
            })?;
        let spec = UiExtension {
            header: Header::of::<UiExtension>(),
            screen: Str::new(screen.name()),
            edge: edge.to_raw(),
            reserved: 0,
            hosts: hosts.as_ptr().cast(),
            host_count,
            reserved_hosts: 0,
        };
        crate::load::hand_over(
            Drawer {
                host,
                draw: Mutex::new(draw),
            },
            |user| (host.api().ui_extension)(host.handle(), &raw const spec, draw_frame::<F>, user),
            "screen extension",
        )
    }

    /// Unstable: needs the `unstable` feature and may change in any release.
    ///
    /// Calls `handler` on the thread that draws the interface when the
    /// game's screen the player has open changes, as
    /// [`Frame::open_screen`](super::Frame::open_screen) reads it: a chest,
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
    /// # extern crate enshwick_core as enshwick;
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
    pub fn on_screen_changed<F>(&mut self, mut handler: F) -> Result<(), Off>
    where
        F: FnMut(&ScreenChanged) + Send + 'static,
    {
        let mut seen: (&'static str, Option<crate::Entity>) = ("", None);
        let mut before = None;
        let mut before_host = None;
        self.on_frame(move |frame| {
            let name = frame.hud_screen_name();
            let host = frame.screen_host();
            if (name, host) == seen {
                return;
            }
            seen = (name, host);
            let now = HudScreenType::from_name(name);
            if (now, host) != (before, before_host) {
                let change = ScreenChanged { before, now, host };
                before = now;
                before_host = host;
                handler(&change);
            }
        })
    }
}

/// The game's screen the player has open changing, from
/// [`Load::on_screen_changed`]. The name is enshwick's.
///
/// # Examples
///
/// See [`Load::on_screen_changed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct ScreenChanged {
    /// The screen open until now, as [`Frame::open_screen`](super::Frame::open_screen)
    /// reads it: `None` outside a game and for a screen [`HudScreenType`]
    /// does not name.
    #[script(default)]
    pub before: Option<HudScreenType>,
    /// The screen open now, read the same way.
    #[script(default)]
    pub now: Option<HudScreenType>,
    /// The entity the screen open now belongs to, as
    /// [`Frame::screen_host`](super::Frame::screen_host) reads it: a
    /// change of host alone, such as from one chest to the next, is a
    /// change too.
    #[script(default)]
    pub host: Option<crate::Entity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_screen_is_named_by_its_own_member() {
        assert_eq!(HudScreenType::InGame.name(), "InGame");
        assert_eq!(HudScreenType::StorageChest.name(), "StorageChest");
        assert_eq!(HudScreenType::NoUi.name(), "NoUi");
        assert_eq!(HudScreenType::YcPlayground.name(), "YcPlayground");
    }

    #[test]
    fn every_screen_sits_at_its_own_place_and_reads_back_from_its_name() {
        for (index, screen) in HudScreenType::ALL.into_iter().enumerate() {
            assert_eq!(screen as usize, index);
            assert_eq!(HudScreenType::from_name(screen.name()), Some(screen));
        }
    }

    #[test]
    fn every_edge_maps_to_its_own_raw_value() {
        let raw = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].map(Edge::to_raw);
        assert_eq!(
            raw,
            [
                UiExtension::LEFT,
                UiExtension::RIGHT,
                UiExtension::TOP,
                UiExtension::BOTTOM
            ]
        );
    }
}
