//! Panels a mod adds to the game's own screens, shown and hidden with them.

use std::sync::Mutex;

use crate::__load::*;
use crate::Off;
use crate::abi::{Str, UiExtension};

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

/// The edge of the screen an extension's panel sits at; alone, it places the
/// panel centered along that edge, with no offset.
///
/// # Examples
///
/// See [`Load::extend_screen`](../struct.Load.html#method.extend_screen).
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
    #[cfg(any(test, feature = "fake"))]
    const ALL: [Self; 4] = [Self::Left, Self::Right, Self::Top, Self::Bottom];

    const fn to_raw(self) -> u32 {
        match self {
            Self::Left => UiExtension::LEFT,
            Self::Right => UiExtension::RIGHT,
            Self::Top => UiExtension::TOP,
            Self::Bottom => UiExtension::BOTTOM,
        }
    }

    /// The edge whose raw value is `raw`; `None` for one the ABI lacks.
    #[cfg(any(test, feature = "fake"))]
    fn from_raw(raw: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|edge| edge.to_raw() == raw)
    }
}

/// Where along its edge an extension's panel sits.
///
/// # Examples
///
/// See [`PanelPlacement`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Align {
    /// The start of the edge: the top of the left or right edge, the left
    /// end of the top or bottom edge.
    Start,
    /// The middle of the edge.
    #[default]
    Center,
    /// The end of the edge: the bottom of the left or right edge, the right
    /// end of the top or bottom edge.
    End,
}

impl Align {
    #[cfg(any(test, feature = "fake"))]
    const ALL: [Self; 3] = [Self::Start, Self::Center, Self::End];

    const fn to_raw(self) -> u32 {
        match self {
            Self::Start => UiExtension::START,
            Self::Center => UiExtension::CENTER,
            Self::End => UiExtension::END,
        }
    }

    /// The alignment whose raw value is `raw`; `None` for one the ABI lacks.
    #[cfg(any(test, feature = "fake"))]
    fn from_raw(raw: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|align| align.to_raw() == raw)
    }
}

/// Where an extension's panel sits beside the game's screen, so it can clear
/// that screen's own controls.
///
/// A placement is an edge, where along it, and an offset in layout units.
/// The layout is 1920 units wide and 1080 high, scaled to the window; a
/// panel is 480 units wide. An [`Edge`] alone is a placement centered along
/// that edge.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Align, Edge, PanelPlacement};
///
/// // At the left edge from 38% of the height down, 9% of the width in:
/// // under the Backpack's grid at 16:9.
/// let under_grid = PanelPlacement::new(Edge::Left)
///     .align(Align::Start)
///     .shift(362.0)
///     .inset(125.0);
/// assert_ne!(under_grid, PanelPlacement::from(Edge::Left));
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelPlacement {
    edge: Edge,
    align: Align,
    shift: f32,
    inset: f32,
}

impl PanelPlacement {
    /// A placement at `edge`, centered along it, with no offset.
    ///
    /// # Examples
    ///
    /// See [`PanelPlacement`].
    #[must_use]
    pub const fn new(edge: Edge) -> Self {
        Self {
            edge,
            align: Align::Center,
            shift: 0.0,
            inset: 0.0,
        }
    }

    /// Puts the panel at `align` along its edge.
    ///
    /// # Examples
    ///
    /// See [`PanelPlacement`].
    #[must_use]
    pub const fn align(mut self, align: Align) -> Self {
        self.align = align;
        self
    }

    /// Moves the panel `units` along its edge from where [`PanelPlacement::align`]
    /// puts it: right or down when positive, left or up when negative. A
    /// panel stops at the screen's border; a shift past
    /// [`UiExtension::MAX_OFFSET`] either way is refused at registration.
    ///
    /// # Examples
    ///
    /// See [`PanelPlacement`].
    #[must_use]
    pub const fn shift(mut self, units: f32) -> Self {
        self.shift = units;
        self
    }

    /// Moves the panel `units` away from its edge into the screen, from 0 to
    /// [`UiExtension::MAX_OFFSET`]; another value is refused at
    /// registration.
    ///
    /// # Examples
    ///
    /// See [`PanelPlacement`].
    #[must_use]
    pub const fn inset(mut self, units: f32) -> Self {
        self.inset = units;
        self
    }

    /// The placement of the raw `edge`, `align`, `shift` and `inset` a spec
    /// carries; `None` for an edge or alignment the ABI lacks.
    #[cfg(any(test, feature = "fake"))]
    pub(crate) fn from_raw(edge: u32, align: u32, shift: f32, inset: f32) -> Option<Self> {
        Some(
            Self::new(Edge::from_raw(edge)?)
                .align(Align::from_raw(align)?)
                .shift(shift)
                .inset(inset),
        )
    }
}

impl From<Edge> for PanelPlacement {
    fn from(edge: Edge) -> Self {
        Self::new(edge)
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::extend_screen`.
    #[doc(hidden)]
    fn extend_screen<F>(
        &mut self,
        screen: HudScreenType,
        place: impl Into<PanelPlacement>,
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static;

    /// As `Load::extend_screen_for`.
    #[doc(hidden)]
    fn extend_screen_for<F>(
        &mut self,
        screen: HudScreenType,
        place: impl Into<PanelPlacement>,
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
    fn extend_screen<F>(
        &mut self,
        screen: HudScreenType,
        place: impl Into<PanelPlacement>,
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static,
    {
        self.extend_screen_for(screen, place, &[], draw)
    }

    fn extend_screen_for<F>(
        &mut self,
        screen: HudScreenType,
        place: impl Into<PanelPlacement>,
        hosts: &[crate::content::Guid],
        draw: F,
    ) -> Result<(), Off>
    where
        F: FnMut(&mut super::Frame<'_>) + Send + 'static,
    {
        let place = place.into();
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
        let spec = enshwick_abi::headed!(UiExtension {
            screen: Str::new(screen.name()),
            edge: place.edge.to_raw(),
            reserved: 0,
            hosts: hosts.as_ptr(),
            host_count,
            reserved_hosts: 0,
            align: place.align.to_raw(),
            shift: place.shift,
            inset: place.inset,
            reserved_place: 0,
        });
        crate::load::hand_over(
            Drawer {
                host,
                draw: Mutex::new(draw),
            },
            |user| {
                (host.api().ui_extension)(
                    host.handle(),
                    &raw const spec,
                    Some(draw_frame::<F>),
                    user,
                )
            },
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
/// [`Load::on_screen_changed`](../struct.Load.html#method.on_screen_changed). The name is enshwick's.
///
/// # Examples
///
/// See [`Load::on_screen_changed`](../struct.Load.html#method.on_screen_changed).
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
    fn every_placement_reads_back_from_its_raw_values_and_an_unknown_one_does_not() {
        for edge in Edge::ALL {
            for align in Align::ALL {
                let place = PanelPlacement::new(edge)
                    .align(align)
                    .shift(-7.0)
                    .inset(9.0);
                assert_eq!(
                    PanelPlacement::from_raw(edge.to_raw(), align.to_raw(), -7.0, 9.0),
                    Some(place)
                );
            }
        }
        assert_eq!(
            PanelPlacement::from_raw(UiExtension::BOTTOM + 1, 0, 0.0, 0.0),
            None
        );
        assert_eq!(
            PanelPlacement::from_raw(0, UiExtension::END + 1, 0.0, 0.0),
            None
        );
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
