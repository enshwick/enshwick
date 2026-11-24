//! The client's own state and control: the local input sampler's status,
//! the client snapshot, and the commands a client mod gives the client.

use crate::abi::{Entity, Header, RecipeId, Str};

/// Cached diagnostics of the client's local gameplay-input sampler.
/// Flags and age are weakly consistent observations, not world or session identity.
/// Reading this does not request sampling. ABI 12.1.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct LocalInputInfo {
    /// Size and version of this struct.
    pub header: Header,
    /// A combination of this type's flag constants; unknown flags are ignored.
    pub flags: u32,
    /// Reserved; 0.
    pub reserved: u32,
    /// Age of the last cached sample in milliseconds; `u64::MAX` means never sampled.
    pub age_ms: u64,
}
impl_headed!(LocalInputInfo);
impl LocalInputInfo {
    /// A mod requested sampling through `local_held`.
    pub const REQUESTED: u32 = 1;
    /// The sampler's existing layout resolved.
    pub const LAYOUT_AVAILABLE: u32 = 2;
    /// The existing sampler link succeeded and its fuse remains on.
    pub const LINK_AVAILABLE: u32 = 4;
    /// The sampler's read fuse remains on; this alone does not mean reads can run.
    pub const READ_ENABLED: u32 = 8;
    /// At least one sample was cached, including a zero mask.
    pub const SAMPLE_PRESENT: u32 = 16;
    /// Requested, available, enabled and sampled within the existing 1000 ms window.
    pub const FRESH: u32 = 32;
    /// Its header set, no flag and never sampled: the value a read starts
    /// from. A read before any sample keeps `age_ms` at `u64::MAX` but sets
    /// the flags of what the loader found.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::LocalInputInfo;
    ///
    /// let never = LocalInputInfo::EMPTY;
    /// assert_eq!((never.flags, never.age_ms), (0, u64::MAX));
    /// ```
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        flags: 0,
        reserved: 0,
        age_ms: u64::MAX,
    };
}

/// The client's state in one copy, which
/// [`HostApi::client_snapshot`](super::HostApi::client_snapshot) writes.
///
/// It holds the
/// session and world, the open screens and the in-game menu's tab, read in
/// the interface's last frame, and the local player's held controls, read
/// in its last step.
///
/// Each group of fields counts only while its bit of `available` is set; a
/// build that lacks what a group reads leaves its bit clear and the rest of
/// the copy whole. Names are the game's own: a screen or tab is a member of
/// its reflected `HudScreenType`, and empty while none is. The names are
/// static text, valid for the process.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientSnapshot, Header};
///
/// let snapshot = ClientSnapshot { header: Header::of::<ClientSnapshot>(), ..ClientSnapshot::EMPTY };
/// assert_eq!(snapshot.state, ClientSnapshot::NEVER);
/// assert_eq!(snapshot.available & ClientSnapshot::SCREENS, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientSnapshot {
    /// Size and version of this struct.
    pub header: Header,
    /// How old the interface's reads are: [`ClientSnapshot::NEVER`] to [`ClientSnapshot::FRESH`].
    pub state: u32,
    /// Which field groups count: [`ClientSnapshot::SCREENS`] and the other group bits.
    pub available: u32,
    /// The loader's count of client sessions, from 1: it grows when the
    /// client's game object, the local player's entity or the world
    /// changes, so a value read twice names one session. 0 before the
    /// interface read a game.
    pub session: u64,
    /// Milliseconds since the interface's last read; `u64::MAX` before
    /// the first.
    pub age_ms: u64,
    /// The world's name, as [`HostApi::world_key`](super::HostApi::world_key)
    /// gives it; empty while none is known, as on a client joined to a
    /// server.
    pub scene: Str,
    /// The open HUD screen; group [`ClientSnapshot::SCREENS`].
    pub hud_screen: Str,
    /// The screen the open one returns to when it closes, empty when it
    /// returns to the world; group [`ClientSnapshot::SCREENS`].
    pub return_screen: Str,
    /// The tab the in-game menu shows, as the screen it stands for
    /// (`Social`, `Map`, `Journal`, `PlayerCrafting`, `Backpack`,
    /// `Character` or `SkillTree`), empty while no menu shows; group
    /// [`ClientSnapshot::MENU_TAB`].
    pub menu_tab: Str,
    /// The local player's held controls, as
    /// [`HostApi::local_held`](super::HostApi::local_held) gives them; group
    /// [`ClientSnapshot::HELD`].
    pub held: u64,
    /// The local player's entity; group [`ClientSnapshot::PLAYER`].
    pub player: Entity,
    /// The open screen's host, such as the chest whose screen shows, or 0;
    /// group [`ClientSnapshot::SCREENS`].
    pub host: Entity,
    /// The recipe the crafting menu highlights, 0 for none; group
    /// [`ClientSnapshot::RECIPE`].
    pub highlighted_recipe: RecipeId,
    /// Zero.
    pub reserved: u32,
    /// The server a [`ClientCommand::JOIN`] of this loader joins or joined,
    /// as `host:query_port`, while the client is on its way there or in its
    /// world; empty in the menu and in a world the client hosts or joined
    /// another way. ABI 12.11.
    pub server: Str,
    /// The mod whose page of the loader's own screen shows, by its id, empty
    /// for none; group [`ClientSnapshot::LOADER_SCREEN`]. ABI 12.23.
    pub loader_screen_mod: Str,
    /// The page of the loader's own Mods screen that shows:
    /// [`ClientSnapshot::NO_LOADER_SCREEN`] to
    /// [`ClientSnapshot::MOD_MISMATCH`]; group
    /// [`ClientSnapshot::LOADER_SCREEN`]. ABI 12.23.
    pub loader_screen: u32,
    /// Zero.
    pub reserved2: u32,
}

impl_headed!(ClientSnapshot, born = 120);

impl ClientSnapshot {
    /// No interface read yet.
    pub const NEVER: u32 = 0;
    /// The interface's reads are off on this build or after a fault.
    pub const OFF: u32 = 1;
    /// The last read is older than a second: no frame drew a game since,
    /// as while loading.
    pub const STALE: u32 = 2;
    /// The last read is at most a second old.
    pub const FRESH: u32 = 3;
    /// [`hud_screen`](Self::hud_screen), [`return_screen`](Self::return_screen) and [`host`](Self::host).
    pub const SCREENS: u32 = 1;
    /// [`menu_tab`](Self::menu_tab).
    pub const MENU_TAB: u32 = 2;
    /// [`held`](Self::held), a read at most a second old.
    pub const HELD: u32 = 4;
    /// [`player`](Self::player).
    pub const PLAYER: u32 = 8;
    /// [`highlighted_recipe`](Self::highlighted_recipe).
    pub const RECIPE: u32 = 16;
    /// [`loader_screen`](Self::loader_screen) and
    /// [`loader_screen_mod`](Self::loader_screen_mod). ABI 12.23.
    pub const LOADER_SCREEN: u32 = 32;
    /// The loader's Mods screen does not show. ABI 12.23.
    pub const NO_LOADER_SCREEN: u32 = 0;
    /// The list of mods shows. ABI 12.23.
    pub const MOD_LIST: u32 = 1;
    /// A mod's page shows, with its settings this world gives it.
    /// ABI 12.23.
    pub const MOD_PAGE: u32 = 2;
    /// The joined server's settings of a mod show. ABI 12.23.
    pub const MOD_SERVER_SETTINGS: u32 = 3;
    /// The joined server's list of mods with settings shows. ABI 12.23.
    pub const SERVER_MOD_LIST: u32 = 4;
    /// The key bindings of a mod the joined server runs and this client
    /// lacks show. ABI 12.23.
    pub const MOD_BINDINGS: u32 = 5;
    /// The mods that differ from a server that refused this client show.
    /// ABI 12.23.
    pub const MOD_MISMATCH: u32 = 6;
    /// Nothing read, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        state: Self::NEVER,
        available: 0,
        session: 0,
        age_ms: u64::MAX,
        scene: Str::EMPTY,
        hud_screen: Str::EMPTY,
        return_screen: Str::EMPTY,
        menu_tab: Str::EMPTY,
        held: 0,
        player: Entity(0),
        host: Entity(0),
        highlighted_recipe: RecipeId(0),
        reserved: 0,
        server: Str::EMPTY,
        loader_screen_mod: Str::EMPTY,
        loader_screen: Self::NO_LOADER_SCREEN,
        reserved2: 0,
    };
}

/// A request to the client's own interface or input, which
/// [`HostApi::client_command`](super::HostApi::client_command) queues.
///
/// The loader runs it in the interface's frame or the input builder's step,
/// never inside a mod's callback, and only while the client is still in
/// `session` on `expected_screen`. Its outcome is what
/// [`HostApi::client_command_status`](super::HostApi::client_command_status)
/// reports; the call itself answers only that it is queued.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientCommand, Str};
///
/// let open = ClientCommand {
///     kind: ClientCommand::OPEN_SCREEN,
///     session: 1,
///     expected_screen: Str::new("InGame"),
///     target: Str::new("Backpack"),
///     ..ClientCommand::EMPTY
/// };
/// assert_eq!(open.hold_ms, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientCommand {
    /// Size and version of this struct.
    pub header: Header,
    /// What to do: [`ClientCommand::OPEN_SCREEN`] to [`ClientCommand::WINDOW_TEXT`].
    pub kind: u32,
    /// A move's hold in milliseconds, 1 to [`ClientCommand::MAX_HOLD_MS`];
    /// a press's, 1 to [`ClientCommand::MAX_PRESS_MS`].
    pub hold_ms: u32,
    /// The session the command belongs to, as
    /// [`ClientSnapshot::session`] names it; a command whose session ended
    /// is cancelled. 0, the session of a snapshot taken before the
    /// interface read a game, takes the session of the first frame that
    /// runs the command.
    pub session: u64,
    /// The HUD screen the client must show, a `HudScreenType` member; empty
    /// for any screen.
    pub expected_screen: Str,
    /// What the command opens or chooses: a screen for
    /// [`ClientCommand::OPEN_SCREEN`], a tab for [`ClientCommand::CHOOSE_TAB`], by the
    /// `HudScreenType` member it stands for; the server, `host:query_port`,
    /// for [`ClientCommand::JOIN`]; the game action's reflected
    /// `PlayerInputType` name, such as `Jump`, for [`ClientCommand::PRESS`].
    pub target: Str,
    /// A look's turn in radians of yaw and pitch, before the player's
    /// camera sensitivity; each at most [`ClientCommand::MAX_LOOK`]. A positive yaw
    /// turns right.
    pub look: [f32; 2],
    /// A move's direction, x right and y forward, of length at most 1.
    pub direction: [f32; 2],
    /// The recipe to highlight.
    pub recipe: RecipeId,
    /// Zero.
    pub reserved: u32,
    /// The command [`ClientCommand::CANCEL`] cancels.
    pub cancels: u64,
    /// The server's password for [`ClientCommand::JOIN`], at most
    /// [`ClientCommand::MAX_PASSWORD`] bytes, empty for none; borrowed for
    /// the call. The loader hands it to the game's own join and never
    /// keeps, logs or returns it. ABI 12.11.
    pub password: Str,
    /// The world position, x, y and z in metres, that
    /// [`ClientCommand::SET_WAYPOINT`] and [`ClientCommand::PING`] mark.
    /// ABI 12.23.
    pub position: [f64; 3],
    /// The world [`ClientCommand::HOST`] plays: its place in the main
    /// menu's list of worlds, from 0, below [`ClientCommand::MAX_SLOTS`].
    /// ABI 12.23.
    pub world: u16,
    /// The character [`ClientCommand::HOST`] plays as: its place in the
    /// main menu's list of characters, from 0, below
    /// [`ClientCommand::MAX_SLOTS`]. ABI 12.23.
    pub character: u16,
    /// 1 when [`ClientCommand::HOST`] opens the world to other players
    /// online, 0 to play it offline. ABI 12.23.
    pub online: u32,
    /// The item of the carried window `target` (`mod:window`) that
    /// [`ClientCommand::WINDOW_PRESS`], [`ClientCommand::WINDOW_PICK`] and
    /// [`ClientCommand::WINDOW_TEXT`] act on, by the id
    /// [`HostApi::client_windows`](super::HostApi::client_windows) lists;
    /// borrowed for the call. ABI 12.24.
    pub item: Str,
    /// The option [`ClientCommand::WINDOW_PICK`] picks, by the name the
    /// window shows, or the text [`ClientCommand::WINDOW_TEXT`] sets, at
    /// most [`ClientCommand::MAX_TEXT`] bytes; borrowed for the call.
    /// ABI 12.24.
    pub text: Str,
}

impl_headed!(ClientCommand, born = 88);

impl ClientCommand {
    /// Opens `target`, one of the screens the game opens from a key in the
    /// world.
    pub const OPEN_SCREEN: u32 = 1;
    /// Closes the open screen, back to the world.
    pub const CLOSE_SCREEN: u32 = 2;
    /// Shows the in-game menu's tab `target` while the menu shows.
    pub const CHOOSE_TAB: u32 = 3;
    /// Cancels the mod's command `cancels`.
    pub const CANCEL: u32 = 4;
    /// Turns the camera by `look` once.
    pub const LOOK: u32 = 5;
    /// Holds `direction` for `hold_ms`.
    pub const MOVE: u32 = 6;
    /// Highlights `recipe` in the open crafting menu, crafting nothing.
    pub const HIGHLIGHT_RECIPE: u32 = 7;
    /// Joins the dedicated server `target`, `host:query_port`, with
    /// `password` from the main menu with a character selected; `session`
    /// is 0. Refused while a join or a world runs. ABI 12.11.
    pub const JOIN: u32 = 8;
    /// Leaves the world back to the main menu. ABI 12.11.
    pub const LEAVE: u32 = 9;
    /// Holds the game action `target` in the local player's input for
    /// `hold_ms`, as its key would. ABI 12.21.
    pub const PRESS: u32 = 10;
    /// Sets the local player's waypoint at `position`, or moves it there.
    /// ABI 12.23.
    pub const SET_WAYPOINT: u32 = 11;
    /// Clears the local player's waypoint. ABI 12.23.
    pub const CLEAR_WAYPOINT: u32 = 12;
    /// Pings `position` for the players of the world. ABI 12.23.
    pub const PING: u32 = 13;
    /// Hosts `world` with `character` from the main menu, offline or
    /// `online`; `session` is 0. Refused while a join, a host or a world
    /// runs. ABI 12.23.
    pub const HOST: u32 = 14;
    /// Opens the loader's Mods screen over the main menu or the pause menu,
    /// whichever shows, at its list of mods. ABI 12.23.
    pub const OPEN_MODS: u32 = 15;
    /// Opens mod `target`'s page on the Mods screen, by its id, opening
    /// the screen first. ABI 12.23.
    pub const OPEN_MOD: u32 = 16;
    /// Presses mod `target`'s settings gear on the Mods screen, by its id,
    /// opening the screen first. ABI 12.23.
    pub const OPEN_MOD_SETTINGS: u32 = 17;
    /// Presses `item` of the carried window `target`, as a click would: a
    /// button, a list's switch or an on-or-off field. ABI 12.24.
    pub const WINDOW_PRESS: u32 = 18;
    /// Picks the option named `text` in the list of `item` of the carried
    /// window `target`. ABI 12.24.
    pub const WINDOW_PICK: u32 = 19;
    /// Sets the text of `item`'s text field in the carried window `target`
    /// to `text`, as typing would. ABI 12.24.
    pub const WINDOW_TEXT: u32 = 20;
    /// The longest text, in bytes, a window's field takes. ABI 12.24.
    pub const MAX_TEXT: usize = 255;
    /// How many worlds and characters the main menu lists at most.
    /// ABI 12.23.
    pub const MAX_SLOTS: u16 = 16;
    /// The longest password, in bytes, the game's join takes.
    pub const MAX_PASSWORD: usize = 0x400;
    /// The longest move.
    pub const MAX_HOLD_MS: u32 = 250;
    /// The longest press. ABI 12.21.
    pub const MAX_PRESS_MS: u32 = 2000;
    /// The largest turn of one look, in radians, a quarter turn.
    pub const MAX_LOOK: f32 = 1.570_796_4;
    /// No command, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: 0,
        hold_ms: 0,
        session: 0,
        expected_screen: Str::EMPTY,
        target: Str::EMPTY,
        look: [0.0; 2],
        direction: [0.0; 2],
        recipe: RecipeId(0),
        reserved: 0,
        cancels: 0,
        password: Str::EMPTY,
        position: [0.0; 3],
        world: 0,
        character: 0,
        online: 0,
        item: Str::EMPTY,
        text: Str::EMPTY,
    };

    /// Whether the command's own bounds hold: a finite look of at most
    /// [`ClientCommand::MAX_LOOK`] each, a finite direction of length above 0 and
    /// at most 1 held 1 to [`ClientCommand::MAX_HOLD_MS`] ms, a press held 1
    /// to [`ClientCommand::MAX_PRESS_MS`] ms, a non-zero recipe, a finite
    /// position to mark, a host's world and character below
    /// [`ClientCommand::MAX_SLOTS`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::ClientCommand;
    ///
    /// let walk = ClientCommand { kind: ClientCommand::MOVE, direction: [0.0, 1.0], hold_ms: 250, ..ClientCommand::EMPTY };
    /// assert!(walk.in_bounds());
    /// assert!(!ClientCommand { hold_ms: 251, ..walk }.in_bounds());
    /// ```
    #[must_use]
    pub fn in_bounds(&self) -> bool {
        match self.kind {
            Self::LOOK => self
                .look
                .iter()
                .all(|turn| turn.is_finite() && turn.abs() <= Self::MAX_LOOK),
            Self::MOVE => {
                let [x, y] = self.direction;
                let length = x.hypot(y);
                // A unit direction a mod normalised may round just past 1.
                length.is_finite()
                    && length > 0.0
                    && length <= 1.0 + 1e-4
                    && (1..=Self::MAX_HOLD_MS).contains(&self.hold_ms)
            }
            Self::PRESS => (1..=Self::MAX_PRESS_MS).contains(&self.hold_ms),
            Self::HIGHLIGHT_RECIPE => self.recipe.0 != 0,
            Self::SET_WAYPOINT | Self::PING => self.position.iter().all(|m| m.is_finite()),
            Self::HOST => {
                self.session == 0
                    && self.world < Self::MAX_SLOTS
                    && self.character < Self::MAX_SLOTS
                    && self.online <= 1
            }
            Self::JOIN => self.session == 0 && self.password.len() <= Self::MAX_PASSWORD,
            Self::WINDOW_PRESS | Self::WINDOW_PICK | Self::WINDOW_TEXT => {
                self.text.len() <= Self::MAX_TEXT
            }
            _ => true,
        }
    }
}

/// Where a [`ClientCommand`] stands, which
/// [`HostApi::client_command_status`](super::HostApi::client_command_status)
/// writes.
///
/// A command is [`ClientCommandStatus::PENDING`] until the game took it
/// ([`ClientCommandStatus::APPLIED`]), then [`ClientCommandStatus::OBSERVED`] once the client's own state
/// shows its effect. It may end [`ClientCommandStatus::CANCELLED`] for `reason`,
/// [`ClientCommandStatus::UNAVAILABLE`] on a build that lacks what it needs, or,
/// for a join, [`ClientCommandStatus::FAILED`]. A join follows its `phase` with the
/// game's own pace: no two-second limit cancels it. Nothing answers a guessed
/// success.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ClientCommandStatus, Header};
///
/// let status = ClientCommandStatus { header: Header::of::<ClientCommandStatus>(), ..ClientCommandStatus::EMPTY };
/// assert_eq!(status.state, ClientCommandStatus::PENDING);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientCommandStatus {
    /// Size and version of this struct.
    pub header: Header,
    /// [`ClientCommandStatus::PENDING`] to [`ClientCommandStatus::UNAVAILABLE`].
    pub state: u32,
    /// Why a cancelled command ended: [`ClientCommandStatus::SESSION_CHANGED`] to
    /// [`ClientCommandStatus::ITEM_OFF`]; 0 otherwise.
    pub reason: u32,
    /// The command's kind.
    pub kind: u32,
    /// Zero.
    pub reserved: u32,
    /// Milliseconds since the command was queued.
    pub age_ms: u64,
    /// Where the client stands on a [`ClientCommand::JOIN`],
    /// [`ClientCommand::HOST`] or [`ClientCommand::LEAVE`]: [`ClientCommandStatus::PHASE_QUEUED`] to
    /// [`ClientCommandStatus::PHASE_LEFT`]; 0 for other kinds. ABI 12.11.
    pub phase: u32,
    /// Why a [`ClientCommandStatus::FAILED`] join failed, as far as the
    /// client can tell: [`ClientCommandStatus::WRONG_PASSWORD`] to
    /// [`ClientCommandStatus::UNKNOWN`]; 0 otherwise. ABI 12.11.
    pub failure: u32,
}

impl_headed!(ClientCommandStatus, born = 32);

impl ClientCommandStatus {
    /// Queued; the game has not taken it yet.
    pub const PENDING: u32 = 0;
    /// The game took it: its queue holds the screen change, the menu its
    /// tab, or the input builder the look or move.
    pub const APPLIED: u32 = 1;
    /// The client's own state shows the effect: the screen or tab shows,
    /// the look was written whole, the move's hold ran out, the recipe is
    /// highlighted.
    pub const OBSERVED: u32 = 2;
    /// It ended without its effect, for `reason`.
    pub const CANCELLED: u32 = 3;
    /// The running build lacks what it needs, or the target is not there,
    /// such as a recipe the open menu does not list.
    pub const UNAVAILABLE: u32 = 4;
    /// The client's session changed.
    pub const SESSION_CHANGED: u32 = 1;
    /// Another screen than the expected one shows.
    pub const OTHER_SCREEN: u32 = 2;
    /// The player pressed a control or moved the stick or mouse.
    pub const PLAYER_INPUT: u32 = 3;
    /// The mod cancelled it.
    pub const BY_MOD: u32 = 4;
    /// Two seconds passed without its effect.
    pub const TIMED_OUT: u32 = 5;
    /// The game blocks the player's input now.
    pub const INPUT_BLOCKED: u32 = 6;
    /// The mod or the feature turned off.
    pub const MOD_OFF: u32 = 7;
    /// The window shows the item off, as a Run button while a field is
    /// empty or an action the player may not run now, or the item has no
    /// option of that name or takes no such text. ABI 12.24.
    pub const ITEM_OFF: u32 = 8;
    /// The game ended a join or a host without reaching the world, for
    /// `failure`.
    /// ABI 12.11.
    pub const FAILED: u32 = 5;
    /// Waiting to start: the loader asks the server for its address.
    pub const PHASE_QUEUED: u32 = 1;
    /// The main menu took the join and starts play.
    pub const PHASE_MENU: u32 = 2;
    /// Play started; the client connects to the server.
    pub const PHASE_CONNECTING: u32 = 3;
    /// Connected; the client loads the world.
    pub const PHASE_LOADING: u32 = 4;
    /// The client is in the server's world.
    pub const PHASE_IN_WORLD: u32 = 5;
    /// The client left play, back to the main menu.
    pub const PHASE_LEFT: u32 = 6;
    /// The server refused the password, or asks for one and none was given.
    pub const WRONG_PASSWORD: u32 = 1;
    /// The server is full.
    pub const FULL: u32 = 2;
    /// The server was not found.
    pub const NOT_FOUND: u32 = 3;
    /// The server is reserved for others.
    pub const RESERVED: u32 = 4;
    /// The server runs another game version.
    pub const VERSION_MISMATCH: u32 = 5;
    /// The loader's join check or the server refused the client.
    pub const REFUSED: u32 = 6;
    /// The game did not start or finish the join in time.
    pub const JOIN_TIMED_OUT: u32 = 7;
    /// The server's query port did not answer with its address.
    pub const UNREACHABLE: u32 = 8;
    /// The join failed for a reason the client does not show.
    pub const UNKNOWN: u32 = 9;
    /// A command queued now, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        state: Self::PENDING,
        reason: 0,
        kind: 0,
        reserved: 0,
        age_ms: 0,
        phase: 0,
        failure: 0,
    };
}

/// An item of a window the server's mods show this client, one entry of
/// [`HostApi::client_windows`](super::HostApi::client_windows), which a
/// [`ClientCommand::WINDOW_PRESS`], [`ClientCommand::WINDOW_PICK`] or
/// [`ClientCommand::WINDOW_TEXT`] names by `window` and `item`.
///
/// The strings are borrowed for the call. ABI 12.24.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ClientWindowItem;
///
/// let item = ClientWindowItem::EMPTY;
/// assert_eq!(item.flags & ClientWindowItem::SHOWN, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Zeroable)]
pub struct ClientWindowItem {
    /// Size and version of this struct.
    pub header: Header,
    /// [`ClientWindowItem::BUTTON`], [`ClientWindowItem::SWITCH`] or
    /// [`ClientWindowItem::FIELD`].
    pub kind: u32,
    /// A field's argument: [`ClientWindowItem::PLAYER`] to
    /// [`ClientWindowItem::TEMPLATE`]; 0 for other kinds.
    pub field: u32,
    /// [`ClientWindowItem::SHOWN`], [`ClientWindowItem::ON`] and
    /// [`ClientWindowItem::OFF`].
    pub flags: u32,
    /// Zero.
    pub reserved: u32,
    /// The window, as `mod:window`.
    pub window: Str,
    /// The item's id in the window.
    pub item: Str,
    /// What the item shows: a button's or a field's label, a switch's
    /// player.
    pub label: Str,
    /// The line the item stands under: a row's text, a whole action's
    /// label; empty for none.
    pub line: Str,
}

impl_headed!(ClientWindowItem);

impl ClientWindowItem {
    /// A button: a row's, an action's Run, a form's Cancel, the window's
    /// Close or its map's.
    pub const BUTTON: u32 = 1;
    /// A list's switch for one player.
    pub const SWITCH: u32 = 2;
    /// A field of an action's argument.
    pub const FIELD: u32 = 3;
    /// A field naming one of the server's players: a list to pick from and
    /// a name to type.
    pub const PLAYER: u32 = 1;
    /// A field of a few options to pick from.
    pub const CHOICE: u32 = 2;
    /// A field of a number in a range.
    pub const NUMBER: u32 = 3;
    /// An on-or-off field, which a press flips.
    pub const TOGGLE: u32 = 4;
    /// A field of typed text.
    pub const TEXT: u32 = 5;
    /// A field of one of the game's items: a search to type and a list of
    /// what it finds to pick from.
    pub const ITEM: u32 = 6;
    /// A field of one of the game's templates, as [`ClientWindowItem::ITEM`].
    pub const TEMPLATE: u32 = 7;
    /// The window shows and takes commands: the newest the client holds.
    pub const SHOWN: u32 = 1;
    /// A switch is on.
    pub const ON: u32 = 2;
    /// The window draws the item off, as an action the player may not run
    /// now.
    pub const OFF: u32 = 4;
    /// No item, its header set.
    pub const EMPTY: Self = Self {
        header: Header::of::<Self>(),
        kind: 0,
        field: 0,
        flags: 0,
        reserved: 0,
        window: Str::EMPTY,
        item: Str::EMPTY,
        label: Str::EMPTY,
        line: Str::EMPTY,
    };
}
