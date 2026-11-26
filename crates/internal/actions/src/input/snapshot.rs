//! The client's state in one copy: its session and world, the open screens
//! and the in-game menu's tab, the local player and the controls it holds.

use super::{Input, InputMask};
use crate::abi::{ClientSnapshot as Raw, Str};
use crate::{Entity, RecipeId, WorldError};
use enshwick_base::ui::Response;
use enshwick_world::__internal::world_status;
use enshwick_world::world::Meter;

mod player;

/// How old the interface's reads in a [`ClientSnapshot`] are.
///
/// # Examples
///
/// ```
/// use enshwick::input::SnapshotState;
///
/// assert_ne!(SnapshotState::Stale, SnapshotState::Never);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SnapshotState {
    /// No frame drew a game since the reads started, when the first mod
    /// opened its input.
    Never,
    /// The reads are off on this build or after a fault; the log and the
    /// Mods screen say why.
    Off,
    /// The last read is older than a second, as while the game loads.
    Stale,
    /// The last read is at most a second old.
    Fresh,
}

impl SnapshotState {
    fn from_raw(state: u32) -> Self {
        match state {
            Raw::FRESH => Self::Fresh,
            Raw::STALE => Self::Stale,
            Raw::OFF => Self::Off,
            _ => Self::Never,
        }
    }

    /// The state's name as the Luau API gives it.
    const fn name(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::Off => "off",
            Self::Stale => "stale",
            Self::Fresh => "fresh",
        }
    }
}

/// The client's state in one copy, from [`Input::client_snapshot`]: one
/// session's world, open screens and menu tab as the interface's last
/// frame read them, and the local player's held controls.
///
/// Each part reads only on a build that has it: an absent part answers
/// `None`, apart from a part that reads nothing now, such as the menu's tab
/// while no menu shows; [`ClientSnapshot::reads`] tells the two apart.
///
/// # Examples
///
/// ```
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let input = load.input()?;
///     load.on_frame(move |frame| {
///         if let Ok(snapshot) = input.client_snapshot()
///             && snapshot.menu_tab() == Some("Backpack")
///         {
///             frame.label("The backpack shows");
///         }
///     })
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ClientSnapshot {
    raw: Raw,
    held: Option<InputMask>,
}

/// A part of a [`ClientSnapshot`] the running build may lack.
///
/// # Examples
///
/// ```
/// use enshwick::input::{ClientSnapshot, SnapshotPart};
///
/// fn tab_unknown(snapshot: &ClientSnapshot) -> bool {
///     !snapshot.reads(SnapshotPart::MenuTab)
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SnapshotPart {
    /// The HUD screen, the screen it returns to and its host.
    Screens,
    /// The in-game menu's tab.
    MenuTab,
    /// The held controls, read in the last second.
    Held,
    /// The local player's entity.
    Player,
    /// The crafting menu's highlighted recipe.
    Recipe,
    /// The page of the loader's own Mods screen that shows, and its mod.
    LoaderScreen,
    /// The loader's dialog that shows, its mod and its buttons.
    Dialog,
    /// The interaction prompt that shows: its target and its verb.
    Prompt,
    /// The character the main menu has selected.
    Character,
    /// Where the local player stands, read in the last second.
    Position,
    /// Where the local player's camera looks, read in the last second.
    Look,
    /// The local player's health and stamina, read in the last second.
    Vitals,
    /// The action bar's selected slot, read in the last second.
    Actionbar,
}

impl SnapshotPart {
    /// Every part, in the order the Luau API lists them.
    const ALL: [Self; 13] = [
        Self::Screens,
        Self::MenuTab,
        Self::Held,
        Self::Player,
        Self::Recipe,
        Self::LoaderScreen,
        Self::Dialog,
        Self::Prompt,
        Self::Character,
        Self::Position,
        Self::Look,
        Self::Vitals,
        Self::Actionbar,
    ];

    const fn bit(self) -> u32 {
        match self {
            Self::Screens => Raw::SCREENS,
            Self::MenuTab => Raw::MENU_TAB,
            Self::Held => Raw::HELD,
            Self::Player => Raw::PLAYER,
            Self::Recipe => Raw::RECIPE,
            Self::LoaderScreen => Raw::LOADER_SCREEN,
            Self::Dialog => Raw::DIALOG,
            Self::Prompt => Raw::PROMPT,
            Self::Character => Raw::CHARACTER,
            Self::Position => Raw::POSITION,
            Self::Look => Raw::LOOK,
            Self::Vitals => Raw::VITALS,
            Self::Actionbar => Raw::ACTIONBAR,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Screens => "screens",
            Self::MenuTab => "menu_tab",
            Self::Held => "held",
            Self::Player => "player",
            Self::Recipe => "recipe",
            Self::LoaderScreen => "loader_screen",
            Self::Dialog => "dialog",
            Self::Prompt => "prompt",
            Self::Character => "character",
            Self::Position => "position",
            Self::Look => "look",
            Self::Vitals => "vitals",
            Self::Actionbar => "actionbar",
        }
    }
}

/// The text of a name the loader wrote, `None` for an empty one.
fn named(name: Str) -> Option<&'static str> {
    // SAFETY: the loader's snapshot names are static text of the process
    // (`HostApi::client_snapshot`), and a fake host's are its test's statics.
    unsafe { name.as_str() }.filter(|name| !name.is_empty())
}

impl ClientSnapshot {
    /// How old the interface's reads are.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::{ClientSnapshot, SnapshotState};
    ///
    /// fn current(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.state() == SnapshotState::Fresh
    /// }
    /// ```
    #[must_use]
    pub fn state(&self) -> SnapshotState {
        SnapshotState::from_raw(self.raw.state)
    }

    /// The loader's count of client sessions, from 1: it grows when the
    /// client's game object, the local player or the world changes, so the
    /// same value in two snapshots names one session; 0 before a frame drew
    /// a game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn same_session(before: &ClientSnapshot, now: &ClientSnapshot) -> bool {
    ///     before.session() != 0 && before.session() == now.session()
    /// }
    /// ```
    #[must_use]
    pub const fn session(&self) -> u64 {
        self.raw.session
    }

    /// Milliseconds since the interface's last read; `None` before the
    /// first.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn paused(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.age_ms().is_some_and(|age| age > 1000)
    /// }
    /// ```
    #[must_use]
    pub fn age_ms(&self) -> Option<u64> {
        (self.raw.age_ms != u64::MAX).then_some(self.raw.age_ms)
    }

    /// The world's name, as [`World::world_key`](crate::world::World) gives
    /// it; `None` while none is known, as on a client joined to a server.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn hosted(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.scene().is_some()
    /// }
    /// ```
    #[must_use]
    pub fn scene(&self) -> Option<&'static str> {
        named(self.raw.scene)
    }

    /// The server a [`CommandAction::Join`](super::CommandAction::Join) of
    /// this loader joins or joined, as `host:query_port`, while the client is
    /// on its way there or in its world; `None` in the menu, in a hosted
    /// world and after a join another way.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn on_server(snapshot: &ClientSnapshot, server: &str) -> bool {
    ///     snapshot.server() == Some(server)
    /// }
    /// ```
    #[must_use]
    pub fn server(&self) -> Option<&'static str> {
        named(self.raw.server)
    }

    /// Whether the running build reads `part`, so its `None` means "none
    /// now" rather than "unread".
    ///
    /// # Examples
    ///
    /// See [`SnapshotPart`].
    #[must_use]
    pub const fn reads(&self, part: SnapshotPart) -> bool {
        self.raw.available & part.bit() != 0
    }

    /// The open HUD screen, a member of the game's `HudScreenType`, as
    /// `ui::HudScreenType::from_name` reads it; `None` while unread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn in_world(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.hud_screen() == Some("InGame")
    /// }
    /// ```
    #[must_use]
    pub fn hud_screen(&self) -> Option<&'static str> {
        named(self.raw.hud_screen)
    }

    /// The screen the open one returns to when it closes; `None` when it
    /// returns to the world or the screens are unread.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn over_backpack(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.return_screen() == Some("Backpack")
    /// }
    /// ```
    #[must_use]
    pub fn return_screen(&self) -> Option<&'static str> {
        named(self.raw.return_screen)
    }

    /// The open screen's host, such as the chest whose screen shows.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn at_station(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.host().is_some()
    /// }
    /// ```
    #[must_use]
    pub fn host(&self) -> Option<Entity> {
        (self.reads(SnapshotPart::Screens) && self.raw.host.0 != 0).then_some(self.raw.host)
    }

    /// The tab the in-game menu shows, as the screen it stands for
    /// (`Social`, `Map`, `Journal`, `PlayerCrafting`, `Backpack`,
    /// `Character` or `SkillTree`), whichever screen opened the menu;
    /// `None` while no menu shows or the tab is unread.
    ///
    /// # Examples
    ///
    /// See the [type](ClientSnapshot).
    #[must_use]
    pub fn menu_tab(&self) -> Option<&'static str> {
        named(self.raw.menu_tab)
    }

    /// The local player's held controls, read in the last second.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::GameAction;
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn blocking(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.held().is_some_and(|held| held.contains(GameAction::Block))
    /// }
    /// ```
    #[must_use]
    pub const fn held(&self) -> Option<InputMask> {
        self.held
    }

    /// The local player's entity.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn has_player(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.player().is_some()
    /// }
    /// ```
    #[must_use]
    pub fn player(&self) -> Option<Entity> {
        self.reads(SnapshotPart::Player).then_some(self.raw.player)
    }

    /// The recipe the crafting menu highlights.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn picked(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.highlighted_recipe().is_some()
    /// }
    /// ```
    #[must_use]
    pub fn highlighted_recipe(&self) -> Option<RecipeId> {
        (self.reads(SnapshotPart::Recipe) && self.raw.highlighted_recipe.0 != 0)
            .then_some(self.raw.highlighted_recipe)
    }

    /// The page of the loader's own Mods screen that shows; `None` while
    /// the screen is closed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::{ClientSnapshot, LoaderPage};
    ///
    /// fn on_the_list(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.loader_screen() == Some(LoaderPage::ModList)
    /// }
    /// ```
    #[must_use]
    pub fn loader_screen(&self) -> Option<LoaderPage> {
        if !self.reads(SnapshotPart::LoaderScreen) {
            return None;
        }
        LoaderPage::ALL
            .iter()
            .find(|(raw, ..)| *raw == self.raw.loader_screen)
            .map(|(_, page, _)| *page)
            .or_else(|| {
                (self.raw.loader_screen != Raw::NO_LOADER_SCREEN)
                    .then_some(LoaderPage::Other(self.raw.loader_screen))
            })
    }

    /// The id of the mod whose page of the loader's Mods screen shows, when
    /// this client loads it; `None` on a page of a mod only the server runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn on_my_page(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.loader_screen_mod() == Some("you.trail")
    /// }
    /// ```
    #[must_use]
    pub fn loader_screen_mod(&self) -> Option<&'static str> {
        self.reads(SnapshotPart::LoaderScreen)
            .then(|| named(self.raw.loader_screen_mod))
            .flatten()
    }

    /// The responses of the buttons of the loader's dialog that shows,
    /// which [`CommandAction::AnswerDialog`](super::CommandAction::AnswerDialog)
    /// gives; empty while none shows, `None` where dialogs do not show in
    /// game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    /// use enshwick::ui::Response;
    ///
    /// fn asks_ok(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.dialog_buttons().is_some_and(|buttons| buttons.contains(&Response::Ok))
    /// }
    /// ```
    #[must_use]
    pub fn dialog_buttons(&self) -> Option<Vec<Response>> {
        self.reads(SnapshotPart::Dialog)
            .then(|| Response::of_buttons(self.raw.dialog_buttons))
    }

    /// The id of the mod whose dialog shows; `None` while none does or for
    /// the loader's own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn mine_shows(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.dialog_mod() == Some("you.trail")
    /// }
    /// ```
    #[must_use]
    pub fn dialog_mod(&self) -> Option<&'static str> {
        self.reads(SnapshotPart::Dialog)
            .then(|| named(self.raw.dialog_mod))
            .flatten()
    }

    /// The number of the loader's dialog that shows, which
    /// [`CommandAction::AnswerDialog`](super::CommandAction::AnswerDialog)
    /// names, so an answer never reaches a dialog that showed after this
    /// snapshot; 0 while none shows or where dialogs do not show in game.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn same_dialog(before: &ClientSnapshot, now: &ClientSnapshot) -> bool {
    ///     before.dialog() != 0 && before.dialog() == now.dialog()
    /// }
    /// ```
    #[must_use]
    pub const fn dialog(&self) -> u32 {
        if self.reads(SnapshotPart::Dialog) {
            self.raw.dialog
        } else {
            0
        }
    }

    /// The entity the interaction prompt that shows targets in the client's
    /// world, such as the chest a press of interact opens; `None` while no
    /// prompt shows.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::Entity;
    /// use enshwick::input::ClientSnapshot;
    ///
    /// fn faces(snapshot: &ClientSnapshot, chest: Entity) -> bool {
    ///     snapshot.prompt_target() == Some(chest)
    /// }
    /// ```
    #[must_use]
    pub fn prompt_target(&self) -> Option<Entity> {
        (self.reads(SnapshotPart::Prompt) && self.raw.prompt_target.0 != 0)
            .then_some(self.raw.prompt_target)
    }

    /// The `LocaTagId` of the verb the shown prompt offers, as the object
    /// gives it before a mod's text takes its place;
    /// [`Verb::from_id`](crate::interactions::Verb::from_id) names the
    /// game's own. `None` while no prompt shows.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::ClientSnapshot;
    /// use enshwick::interactions::Verb;
    ///
    /// fn offers_open(snapshot: &ClientSnapshot) -> bool {
    ///     snapshot.prompt_verb().and_then(Verb::from_id) == Some(Verb::Open)
    /// }
    /// ```
    #[must_use]
    pub fn prompt_verb(&self) -> Option<u32> {
        self.prompt_target().map(|_| self.raw.prompt_verb)
    }

    /// The character the main menu has selected, by its place in the menu's
    /// list from 0, as
    /// [`CommandAction::Host`](super::CommandAction::Host) takes it; `None`
    /// while none is and outside the main menu.
    ///
    /// The place does not say which save storage holds the character: when
    /// the local folder and Steam's cloud both hold one there, `Host` plays
    /// the cloud one, even if the local one is selected.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::{ClientSnapshot, CommandAction};
    ///
    /// fn host_first_world(snapshot: &ClientSnapshot) -> Option<CommandAction> {
    ///     let character = snapshot.selected_character()?;
    ///     Some(CommandAction::Host { world: 0, character, online: false })
    /// }
    /// ```
    #[must_use]
    pub fn selected_character(&self) -> Option<u16> {
        if !self.reads(SnapshotPart::Character) {
            return None;
        }
        u16::try_from(self.raw.character).ok()
    }
}

/// A page of the loader's own Mods screen, from
/// [`ClientSnapshot::loader_screen`].
///
/// # Examples
///
/// ```
/// use enshwick::input::LoaderPage;
///
/// assert_eq!(LoaderPage::ModPage.name(), "mod_page");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LoaderPage {
    /// The list of mods.
    ModList,
    /// A mod's page, with the settings this world gives it.
    ModPage,
    /// The joined server's settings of a mod.
    ModServerSettings,
    /// The joined server's list of mods with settings.
    ServerModList,
    /// The key bindings of a mod the joined server runs and this client
    /// lacks.
    ModBindings,
    /// The mods that differ from a server that refused this client.
    ModMismatch,
    /// A page a newer loader shows that this build of the API does not
    /// name, as its raw value.
    Other(u32),
}

impl LoaderPage {
    const ALL: [(u32, Self, &'static str); 6] = [
        (Raw::MOD_LIST, Self::ModList, "mod_list"),
        (Raw::MOD_PAGE, Self::ModPage, "mod_page"),
        (
            Raw::MOD_SERVER_SETTINGS,
            Self::ModServerSettings,
            "mod_server_settings",
        ),
        (Raw::SERVER_MOD_LIST, Self::ServerModList, "server_mod_list"),
        (Raw::MOD_BINDINGS, Self::ModBindings, "mod_bindings"),
        (Raw::MOD_MISMATCH, Self::ModMismatch, "mod_mismatch"),
    ];

    /// The page's name as the Luau API gives it; `"other"` for every
    /// [`LoaderPage::Other`].
    ///
    /// # Examples
    ///
    /// See [`LoaderPage`].
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, page, _)| *page == self)
            .map_or("other", |(.., name)| name)
    }
}

#[crate::script(client_snapshot)]
impl Input {
    /// The client's state in one copy, for any thread: its session and
    /// world, the open screens and menu tab, the local player and its held
    /// controls.
    ///
    /// The reads start when the mod opens its input, so the client's first
    /// interface step after that answers, in the mod's first frame in a
    /// game. It is [`SnapshotState::Never`] before that step, and while no
    /// frame drew a game since, as in the main menu before the first world.
    /// The reads cost one look at the interface's screen state and menu tab
    /// per frame, which client commands share, and a copy of four words;
    /// the held controls cost their own row, as [`Input::local_held`] says.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server, otherwise the loader's
    /// refusal.
    ///
    /// # Examples
    ///
    /// See [`ClientSnapshot`].
    #[script]
    pub fn client_snapshot(self) -> Result<ClientSnapshot, WorldError> {
        let mut raw = Raw::EMPTY;
        let host = self.0.host;
        world_status((host.api().client_snapshot)(host.handle(), &raw mut raw))?;
        let held = (raw.available & Raw::HELD != 0).then_some(InputMask {
            bits: raw.held,
            actions: &self.0.actions,
        });
        Ok(ClientSnapshot { raw, held })
    }
}

/// The client's state in one copy, as [`Input::client_snapshot`] gives it to Luau.
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[script(name = "ClientSnapshot")]
struct LuauSnapshot {
    /// `"never"`, `"off"`, `"stale"` or `"fresh"`.
    pub state: String,
    /// The session count, as a decimal string.
    pub session: u64,
    /// Milliseconds since the last read, or nil before the first.
    pub age_ms: Option<u64>,
    /// The world's name.
    pub scene: Option<String>,
    /// The parts the build reads: `"screens"`, `"menu_tab"`, `"held"`,
    /// `"player"`, `"recipe"`, `"loader_screen"`, `"dialog"`, `"prompt"`,
    /// `"character"`, `"position"`, `"look"`, `"vitals"` and `"actionbar"`.
    pub reads: Vec<String>,
    /// The open HUD screen.
    pub hud_screen: Option<String>,
    /// The screen the open one returns to.
    pub return_screen: Option<String>,
    /// The open screen's host.
    pub host: Option<Entity>,
    /// The tab the in-game menu shows.
    pub menu_tab: Option<String>,
    /// The held controls' names.
    pub held: Option<Vec<String>>,
    /// The local player's entity.
    pub player: Option<Entity>,
    /// The highlighted recipe.
    pub highlighted_recipe: Option<RecipeId>,
    /// The server a join of this loader joined, as `host:query_port`.
    pub server: Option<String>,
    /// The page of the loader's Mods screen that shows: `"mod_list"`,
    /// `"mod_page"`, `"mod_server_settings"`, `"server_mod_list"`,
    /// `"mod_bindings"` or `"mod_mismatch"`; nil while none does.
    pub loader_screen: Option<String>,
    /// The id of the mod that page names.
    pub loader_screen_mod: Option<String>,
    /// The responses of the buttons of the loader's dialog that shows;
    /// empty while none does, nil where dialogs do not show in game.
    pub dialog_buttons: Option<Vec<Response>>,
    /// The id of the mod whose dialog shows; nil for the loader's own.
    pub dialog_mod: Option<String>,
    /// The number of the dialog that shows, which `answer_dialog` names; 0
    /// while none does.
    pub dialog: u32,
    /// The entity the interaction prompt that shows targets; nil while none
    /// shows.
    pub prompt_target: Option<Entity>,
    /// The `LocaTagId` of that prompt's verb, as the object offers it; Luau
    /// has no `Verb::from_id`, so a script compares the number.
    pub prompt_verb: Option<u32>,
    /// The character the main menu has selected, by its place in the menu's
    /// list from 0, as `host` takes it; nil while none is and outside the
    /// main menu. When both save storages hold a character there, `host`
    /// plays the one in Steam's cloud.
    pub selected_character: Option<u16>,
    /// Where the local player stands, x, y and z in metres.
    pub position: Option<[f64; 3]>,
    /// Where the local player's camera looks, a quaternion x, y, z, w.
    pub look: Option<[f32; 4]>,
    /// The local player's health and its maximum.
    pub health: Option<Meter>,
    /// The local player's stamina and its maximum.
    pub stamina: Option<Meter>,
    /// The action bar's selected slot, from 0.
    pub actionbar_slot: Option<u8>,
}

impl From<ClientSnapshot> for LuauSnapshot {
    fn from(snapshot: ClientSnapshot) -> Self {
        let owned = |name: Option<&str>| name.map(str::to_owned);
        Self {
            state: snapshot.state().name().to_owned(),
            session: snapshot.session(),
            age_ms: snapshot.age_ms(),
            scene: owned(snapshot.scene()),
            reads: SnapshotPart::ALL
                .into_iter()
                .filter(|part| snapshot.reads(*part))
                .map(|part| part.name().to_owned())
                .collect(),
            hud_screen: owned(snapshot.hud_screen()),
            return_screen: owned(snapshot.return_screen()),
            host: snapshot.host(),
            menu_tab: owned(snapshot.menu_tab()),
            held: snapshot
                .held()
                .map(|held| held.iter().map(|action| action.name().to_owned()).collect()),
            player: snapshot.player(),
            highlighted_recipe: snapshot.highlighted_recipe(),
            server: owned(snapshot.server()),
            loader_screen: snapshot.loader_screen().map(|page| page.name().to_owned()),
            loader_screen_mod: owned(snapshot.loader_screen_mod()),
            dialog_buttons: snapshot.dialog_buttons(),
            dialog_mod: owned(snapshot.dialog_mod()),
            dialog: snapshot.dialog(),
            prompt_target: snapshot.prompt_target(),
            prompt_verb: snapshot.prompt_verb(),
            selected_character: snapshot.selected_character(),
            position: snapshot.position(),
            look: snapshot.look(),
            health: snapshot.health(),
            stamina: snapshot.stamina(),
            actionbar_slot: snapshot.actionbar_slot(),
        }
    }
}

impl crate::script::Typed for ClientSnapshot {
    const TY: &'static crate::script::Ty = <LuauSnapshot as crate::script::Typed>::TY;
}

impl crate::script::IntoScript for ClientSnapshot {
    fn into_value(self) -> crate::script::Value {
        LuauSnapshot::from(self).into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static NO_ACTIONS: [Option<crate::controls::GameAction>; super::super::BITS] =
        [None; super::super::BITS];

    #[expect(
        clippy::large_types_passed_by_value,
        reason = "each test hands its literal over once"
    )]
    fn snapshot(raw: Raw) -> ClientSnapshot {
        let held = (raw.available & Raw::HELD != 0).then_some(InputMask {
            bits: raw.held,
            actions: &NO_ACTIONS,
        });
        ClientSnapshot { raw, held }
    }

    #[test]
    fn an_unread_part_answers_none_and_says_so() {
        let empty = snapshot(Raw::EMPTY);
        assert_eq!(empty.state(), SnapshotState::Never);
        assert_eq!(empty.age_ms(), None);
        assert_eq!(
            (empty.hud_screen(), empty.menu_tab(), empty.player()),
            (None, None, None)
        );
        assert!(SnapshotPart::ALL.iter().all(|part| !empty.reads(*part)));
    }

    #[test]
    fn a_read_tab_with_no_menu_open_is_none_but_read() {
        let world = snapshot(Raw {
            state: Raw::FRESH,
            available: Raw::SCREENS | Raw::MENU_TAB,
            age_ms: 16,
            hud_screen: Str::new("InGame"),
            ..Raw::EMPTY
        });
        assert_eq!(world.menu_tab(), None);
        assert!(world.reads(SnapshotPart::MenuTab));
        assert_eq!(world.hud_screen(), Some("InGame"));
        assert_eq!(world.age_ms(), Some(16));
        assert_eq!(world.host(), None);
    }

    #[test]
    fn luau_sees_the_parts_by_name_and_the_state_as_text() {
        let raw = Raw {
            state: Raw::STALE,
            available: Raw::SCREENS | Raw::HELD | Raw::PLAYER,
            session: 4,
            age_ms: 1500,
            scene: Str::new("world1"),
            hud_screen: Str::new("SystemMenu"),
            held: 1,
            player: Entity(12),
            server: Str::new("127.0.0.1:15637"),
            ..Raw::EMPTY
        };
        let luau = LuauSnapshot::from(snapshot(raw));
        assert_eq!(luau.server.as_deref(), Some("127.0.0.1:15637"));
        assert_eq!(luau.state, "stale");
        assert_eq!(luau.reads, ["screens", "held", "player"]);
        assert_eq!(luau.held, Some(Vec::new()), "a bit the build names none of");
        assert_eq!(luau.player, Some(Entity(12)));
        assert_eq!(luau.scene.as_deref(), Some("world1"));
    }

    #[test]
    fn a_read_prompt_names_its_target_and_verb_and_none_while_none_shows() {
        let facing = snapshot(Raw {
            state: Raw::FRESH,
            available: Raw::PROMPT,
            prompt_target: Entity(1052),
            prompt_verb: 0x4e82_e309,
            ..Raw::EMPTY
        });
        assert_eq!(facing.prompt_target(), Some(Entity(1052)));
        assert_eq!(facing.prompt_verb(), Some(0x4e82_e309));
        let nothing = snapshot(Raw {
            available: Raw::PROMPT,
            ..Raw::EMPTY
        });
        assert!(nothing.reads(SnapshotPart::Prompt));
        assert_eq!(
            (nothing.prompt_target(), nothing.prompt_verb()),
            (None, None)
        );
        let unread = snapshot(Raw {
            prompt_target: Entity(1052),
            ..Raw::EMPTY
        });
        assert_eq!(unread.prompt_target(), None, "the build lacks the read");
    }

    #[test]
    fn the_local_player_s_parts_answer_only_while_read() {
        let read = snapshot(Raw {
            state: Raw::FRESH,
            available: Raw::POSITION | Raw::VITALS | Raw::ACTIONBAR,
            position: [1.5, 20.0, -3.0],
            health: 90,
            health_max: 120,
            stamina: 0,
            stamina_max: 100,
            actionbar_slot: 3,
            ..Raw::EMPTY
        });
        assert_eq!(read.position(), Some([1.5, 20.0, -3.0]));
        assert_eq!(
            read.health(),
            Some(Meter {
                current: 90,
                max: 120
            })
        );
        assert_eq!(
            read.stamina(),
            Some(Meter {
                current: 0,
                max: 100
            })
        );
        assert_eq!(read.actionbar_slot(), Some(3));
        assert_eq!(read.look(), None, "the look is unread");
        let luau = LuauSnapshot::from(read);
        assert_eq!(luau.reads, ["position", "vitals", "actionbar"]);
        assert_eq!(
            luau.health,
            Some(Meter {
                current: 90,
                max: 120
            })
        );
        let empty = snapshot(Raw::EMPTY);
        assert_eq!(
            (empty.position(), empty.health(), empty.actionbar_slot()),
            (None, None, None)
        );
    }

    #[test]
    fn the_selected_character_is_its_place_and_none_without_one() {
        let menu = |character| {
            snapshot(Raw {
                available: Raw::CHARACTER,
                character,
                ..Raw::EMPTY
            })
        };
        assert_eq!(menu(2).selected_character(), Some(2));
        assert_eq!(menu(Raw::NO_CHARACTER).selected_character(), None);
        assert!(menu(Raw::NO_CHARACTER).reads(SnapshotPart::Character));
        let luau = LuauSnapshot::from(menu(0));
        assert_eq!(luau.selected_character, Some(0));
        assert_eq!(luau.reads, ["character"]);
        assert_eq!(snapshot(Raw::EMPTY).selected_character(), None);
    }
}
