//! Panels a mod adds to the game's own screens, shown and hidden with them.

use std::sync::Mutex;

use crate::__load::*;
use crate::Off;
use crate::abi::{Header, Str, UiExtension};

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

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::extend_screen`.
    #[doc(hidden)]
    fn extend_screen<F>(&mut self, screen: HudScreenType, edge: Edge, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static;

    /// As `Load::extend_screen_for`.
    #[doc(hidden)]
    fn extend_screen_for<F>(
        &mut self,
        screen: HudScreenType,
        edge: Edge,
        hosts: &[crate::content::Guid],
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static;

    /// As `Load::on_screen_changed`.
    #[doc(hidden)]
    fn on_screen_changed<F>(&mut self, handler: F) -> Result<(), Off>
    where
        F: FnMut(&ScreenChanged) + Send + 'static;
}

impl LoadMethods for crate::BaseLoad {
    fn extend_screen<F>(&mut self, screen: HudScreenType, edge: Edge, draw: F) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static,
    {
        self.extend_screen_for(screen, edge, &[], draw)
    }

    fn extend_screen_for<F>(
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

    fn on_screen_changed<F>(&mut self, mut handler: F) -> Result<(), Off>
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
