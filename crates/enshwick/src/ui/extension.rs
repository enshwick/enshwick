//! Panels a mod adds to the game's own screens, shown and hidden with them.

use std::sync::Mutex;

use crate::abi::{Header, Str, UiExtension};
use crate::{Load, Off};

use super::client_only;
use super::frame::{Drawer, draw_frame};
use super::screens::NAMES;
#[cfg(test)]
use super::screens::OPEN;

/// One of the game's screens, which [`Load::extend_screen`] adds a panel to.
///
/// The members and their names are the game's `HudScreenType`, read from
/// its reflection on client 520; a screen the running build lacks is
/// refused at registration.
///
/// # Examples
///
/// ```
/// use enshwick::ui::HudScreenType;
///
/// assert_eq!(HudScreenType::StorageChest.name(), "StorageChest");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
#[expect(missing_docs, reason = "each member is the game's screen of that name")]
pub enum HudScreenType {
    InGame,
    SystemMenu,
    SettingsMenu,
    InGameMenu,
    Map,
    Journal,
    Character,
    PlayerCrafting,
    Backpack,
    SkillTree,
    SkillTreeRespec,
    FlameAltar,
    AltarUpgrade,
    FlameUpgrade,
    AltarDeactivate,
    AltarReskill,
    AltarItemPickup,
    LoreReading,
    Signpost,
    LogicEditor,
    ReadOnlyLootChest,
    StorageChest,
    Collection,
    FeedingStation,
    StationBasedCrafting,
    NpcDialog,
    NpcCrafting,
    FactoryCrafting,
    BlueprintMultiTool,
    BlueprintMultiToolMaterialCatalog,
    OvergrowthMultiTool,
    DecayMultiTool,
    ScrewdriverMultiToolBuildingMode,
    ScrewdriverMultiToolGraphManager,
    ScrewdriverMultiToolEntityConfig,
    TerraformingSideMenu,
    NpcSummonerMenu,
    UserFeedback,
    TwitchDrops,
    RedeemCommerceProducts,
    CommerceProductChest,
    ItemTinting,
    TextChat,
    Social,
    ShroudForge,
    CharacterCustomization,
    Rename,
    TakeScreenshot,
    InfoPopup,
    NewCommerceUnlock,
    NoUi,
    TechGallery,
    YcPlayground,
}

impl HudScreenType {
    /// Every screen, in the game's order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ui::HudScreenType;
    ///
    /// assert!(HudScreenType::ALL.contains(&HudScreenType::Map));
    /// ```
    pub const ALL: [Self; NAMES.len()] = [
        Self::InGame,
        Self::SystemMenu,
        Self::SettingsMenu,
        Self::InGameMenu,
        Self::Map,
        Self::Journal,
        Self::Character,
        Self::PlayerCrafting,
        Self::Backpack,
        Self::SkillTree,
        Self::SkillTreeRespec,
        Self::FlameAltar,
        Self::AltarUpgrade,
        Self::FlameUpgrade,
        Self::AltarDeactivate,
        Self::AltarReskill,
        Self::AltarItemPickup,
        Self::LoreReading,
        Self::Signpost,
        Self::LogicEditor,
        Self::ReadOnlyLootChest,
        Self::StorageChest,
        Self::Collection,
        Self::FeedingStation,
        Self::StationBasedCrafting,
        Self::NpcDialog,
        Self::NpcCrafting,
        Self::FactoryCrafting,
        Self::BlueprintMultiTool,
        Self::BlueprintMultiToolMaterialCatalog,
        Self::OvergrowthMultiTool,
        Self::DecayMultiTool,
        Self::ScrewdriverMultiToolBuildingMode,
        Self::ScrewdriverMultiToolGraphManager,
        Self::ScrewdriverMultiToolEntityConfig,
        Self::TerraformingSideMenu,
        Self::NpcSummonerMenu,
        Self::UserFeedback,
        Self::TwitchDrops,
        Self::RedeemCommerceProducts,
        Self::CommerceProductChest,
        Self::ItemTinting,
        Self::TextChat,
        Self::Social,
        Self::ShroudForge,
        Self::CharacterCustomization,
        Self::Rename,
        Self::TakeScreenshot,
        Self::InfoPopup,
        Self::NewCommerceUnlock,
        Self::NoUi,
        Self::TechGallery,
        Self::YcPlayground,
    ];

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
        let host = self.host();
        client_only(host, "screen extensions")?;
        let spec = UiExtension {
            header: Header::of::<UiExtension>(),
            screen: Str::new(screen.name()),
            edge: edge.to_raw(),
            reserved: 0,
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
        let mut seen: &'static str = "";
        let mut before = None;
        self.on_frame(move |frame| {
            // SAFETY: the loader keeps the name for the life of the process
            // (`UiFrame`'s `hud_screen`).
            let name: &'static str = unsafe { frame.hud_screen.as_str() }.unwrap_or("");
            if name == seen {
                return;
            }
            seen = name;
            let now = HudScreenType::from_name(name);
            if now != before {
                let change = ScreenChanged { before, now };
                before = now;
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
    /// The screen open until now; `None` outside a game.
    #[script(default)]
    pub before: Option<HudScreenType>,
    /// The screen open now; `None` outside a game.
    #[script(default)]
    pub now: Option<HudScreenType>,
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
        assert!(OPEN.iter().all(|open| NAMES.contains(open)));
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
