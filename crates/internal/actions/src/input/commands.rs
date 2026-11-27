//! Commands for the client's own interface and input: open or close a
//! screen, choose the in-game menu's tab, each with an id and an outcome
//! the client's own state confirms.

use core::num::NonZeroU16;

use super::Input;
use crate::abi::{ClientCommand as RawCommand, ClientCommandStatus as Raw, Status, Str};
use crate::controls::GameAction;
use crate::{Entity, RecipeId, WorldError};
use enshwick_base::ui::Response;
use enshwick_inventory::inventory::SlotRef;
use enshwick_world::__internal::world_status;

mod luau;
mod status;

pub use status::{CancelReason, CommandState, CommandStatus, JoinFailure, JoinPhase};

/// What a [`ClientCommand`] does.
///
/// # Examples
///
/// ```
/// use enshwick::input::CommandAction;
///
/// let open = CommandAction::OpenScreen("Backpack".to_owned());
/// assert_ne!(open, CommandAction::CloseScreen);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CommandAction {
    /// Opens a screen the game opens from a key in the world, by its
    /// `HudScreenType` name: `SystemMenu`, `InGameMenu`, `Map`, `Journal`,
    /// `Character`, `PlayerCrafting`, `Backpack`, `SkillTree`, `TextChat` or
    /// `Social`.
    OpenScreen(String),
    /// Closes the open screen, back to the world.
    CloseScreen,
    /// Shows the in-game menu's tab, named as the screen it stands for, as
    /// [`ClientSnapshot::menu_tab`](super::ClientSnapshot::menu_tab) names
    /// it.
    ChooseTab(String),
    /// Turns the camera once by `yaw` and `pitch` radians, before the
    /// player's camera sensitivity; each at most a quarter turn. A positive
    /// yaw turns right.
    Look {
        /// Radians of yaw.
        yaw: f32,
        /// Radians of pitch.
        pitch: f32,
    },
    /// Holds `direction` (x right, y forward, length at most 1) for
    /// `hold_ms` milliseconds, at most 250.
    Move {
        /// The direction.
        direction: [f32; 2],
        /// The hold in milliseconds.
        hold_ms: u32,
    },
    /// Highlights `recipe` in the open crafting menu, crafting nothing.
    HighlightRecipe(RecipeId),
    /// Joins the dedicated server `server`, `host:query_port`, from the main
    /// menu with a character selected, giving `password` when it asks for
    /// one. Its [`CommandStatus::phase`] follows the join at the game's own
    /// pace. Its command's session is 0.
    Join {
        /// The server, as `host:query_port`.
        server: String,
        /// The server's password, `None` for none.
        password: Option<Password>,
    },
    /// Leaves the world back to the main menu.
    Leave,
    /// Holds `action` in the local player's input for `hold_ms`
    /// milliseconds, 1 to 2000, as its key would; one step is a tap.
    ///
    /// The game acts on the form its key sets: an attack is
    /// [`GameAction::MainhandActionTap`] with
    /// [`GameAction::MainhandActionRelease`], an interaction
    /// [`GameAction::ContextualActionTap`]; a plain action such as
    /// [`GameAction::Jump`] or [`GameAction::Sprint`] acts on its own bit.
    /// Several presses hold at once, beside a look or a move.
    ///
    /// A screen the held action opens, as an interaction opens a chest, is
    /// its effect: the press is observed then and stops holding. A screen
    /// that a key of the player's or another command opens cancels it.
    Press {
        /// The action.
        action: GameAction,
        /// The hold in milliseconds.
        hold_ms: u32,
    },
    /// Sets the local player's waypoint at a world position, x, y and z in
    /// metres, or moves it there, as the map's own waypoint does.
    SetWaypoint([f64; 3]),
    /// Clears the local player's waypoint.
    ClearWaypoint,
    /// Pings a world position, x, y and z in metres, for the players of the
    /// world, as the map's own ping does; the waypoint stays.
    Ping([f64; 3]),
    /// Hosts a world from the main menu, as its own Play does: the world
    /// and the character by their places in the main menu's lists, from 0,
    /// each below 16, offline or open to other players `online`. Its
    /// [`CommandStatus::phase`] follows play as a join's does. Its
    /// command's session is 0.
    Host {
        /// The world's place in the main menu's list.
        world: u16,
        /// The character's place in the main menu's list.
        character: u16,
        /// Whether other players may join it online.
        online: bool,
    },
    /// Opens the loader's Mods screen at its list of mods, over the main
    /// menu or the pause menu, whichever shows;
    /// [`ClientSnapshot::loader_screen`](super::ClientSnapshot::loader_screen)
    /// tells when it shows.
    OpenMods,
    /// Opens the page of the mod with this id on the Mods screen, opening
    /// the screen first.
    OpenMod(String),
    /// Presses the settings gear of the mod with this id on the Mods
    /// screen, opening the screen first: its page, or the joined server's
    /// settings of a mod with nothing to set here.
    OpenModSettings(String),
    /// Presses `item` of the window `window` that a server's mod shows, as
    /// a click would: a button, a list's switch, or an on-or-off field,
    /// which it flips. Both ids are as [`Input::client_windows`] lists
    /// them; only the newest window takes it. Observed in the frame the
    /// press clicks; [`CancelReason::ItemOff`] when the window draws the
    /// item off, such as a Run whose fields are not all set.
    PressWindowItem {
        /// The window, as `mod:window`.
        window: String,
        /// The item's id in it.
        item: String,
    },
    /// Picks `option`, by the name the window shows, in the list of `item`
    /// of the window `window`: a choice, a player, or one of the game's
    /// items or templates whatever its search shows.
    PickWindowItem {
        /// The window, as `mod:window`.
        window: String,
        /// The item's id in it.
        item: String,
        /// The option's name, as the window shows it.
        option: String,
    },
    /// Sets the text of `item`'s text field in the window `window` to
    /// `text`, at most 255 bytes, as typing would: a text's field, a
    /// player's name or an item's search. Observed once the field reads
    /// `text`.
    SetWindowText {
        /// The window, as `mod:window`.
        window: String,
        /// The item's id in it.
        item: String,
        /// The text.
        text: String,
    },
    /// Answers the loader's dialog that shows, a mod's
    /// (`Ui::ask`) or the loader's own, with a
    /// response one of its buttons gives, as a click would, on any screen.
    /// Observed once the dialog closes with that answer;
    /// [`WorldError::Unavailable`] while the dialog `dialog` names does not
    /// show, which
    /// [`ClientSnapshot::dialog_buttons`](super::ClientSnapshot::dialog_buttons)
    /// tells, and [`WorldError::BadArgs`] for a response the dialog has no
    /// button for.
    AnswerDialog {
        /// The dialog, as [`ClientSnapshot::dialog`](super::ClientSnapshot::dialog)
        /// numbers it.
        dialog: u32,
        /// The answer.
        response: Response,
    },
    /// Crafts `recipe` `count` times through the game's own crafting
    /// request, as the craft button does, at `station` (the chest-like
    /// workbench whose screen shows, as
    /// [`ClientSnapshot::host`](super::ClientSnapshot::host) names it) or by
    /// hand for `None`. The server checks every craft: resources, the
    /// recipe and the station. Applied once the request is written;
    /// observed once the client's copy of the player's inventory changes
    /// after it; a craft the server refuses changes nothing, so it ends
    /// timed out. Any change counts, so a craft sent before the last one's
    /// items arrived may take those for its own.
    Craft {
        /// The recipe.
        recipe: RecipeId,
        /// How many times, 1 to 65,535.
        count: u16,
        /// The station, `None` to craft by hand.
        station: Option<Entity>,
    },
    /// Moves `count` items, or the whole stack for `None`, from the slot
    /// `from` to `to`, through the game's own item transfer, as a drag
    /// does: a count into an empty slot splits a stack, a move into an
    /// action bar slot fills it, a move out of an equipment slot unequips.
    /// The slots are those [`Input::client_slots`] lists, or a chest's.
    /// Observed once the client's copy of either slot's inventory changes;
    /// a move the server refuses changes neither, so it ends timed out.
    MoveItem {
        /// The slot the items leave.
        from: SlotRef,
        /// The slot they go to.
        to: SlotRef,
        /// How many, `None` for the whole stack.
        count: Option<NonZeroU16>,
    },
    /// Equips the item in `slot` into the equipment slot `equipment`, a
    /// member of the game's `EquipmentSlot` such as `RangedWeapon`, through
    /// the game's own equipment request. Observed once the client's copy of
    /// the slot's inventory changes; the server refuses an item of another
    /// kind than the equipment slot, so that one ends timed out. An item a
    /// [`Self::MoveItem`] took out of the equipment, which the game's
    /// equipment slot still names, moves back into the equipment slot it
    /// left while that slot is empty; a server mod hears that as an item
    /// transfer, not an equip.
    Equip {
        /// The slot that holds the item.
        slot: SlotRef,
        /// The equipment slot.
        equipment: String,
    },
    /// Takes every stack of the inventory `from`, such as the chest
    /// [`ClientSnapshot::host`](super::ClientSnapshot::host) names, into the
    /// player's, as a storage window's take all does. Observed once the
    /// client's copy of either inventory changes; one that finds nothing to
    /// move changes neither, so it ends timed out, as a refused one does.
    TakeAll(Entity),
    /// Moves every stack of the player's into the inventory `to`, as a
    /// storage window's deposit all does. Observed as [`Self::TakeAll`].
    DepositAll(Entity),
    /// Destroys the stack in `slot`, as the backpack's Delete does (client
    /// 26430625, read statically); the game has no drop of an item on the
    /// ground. It is an item transfer of type
    /// [`Destroy`](crate::interactions::InventoryActionType::Destroy), so a
    /// server mod refuses it through its interaction filter's move or an
    /// item filter on trash. Observed once the client's copy of the slot's
    /// inventory changes.
    Trash(SlotRef),
    /// Sends `text` to the world's chat as the local player's own line, as
    /// the chat window's Enter does, on any server the client plays on: 1
    /// to [`CommandAction::MAX_CHAT`] bytes, refused at the call outside
    /// them. Observed once the game's interface queue takes the line; the
    /// server's chat filter and mute see it as any player's line, and a
    /// server whose chat is off drops it.
    Chat(String),
    /// Binds `key`, or no key for `None`, to the control `mod_id` declared
    /// as `control`, in its first key or, `second`, its second, on that
    /// mod's page of the Mods screen, as the page's key capture or its Clear
    /// does: the key works at once and the loader keeps it for this player
    /// in `mod-controls.toml`. A mod of the joined server that this client
    /// does not have counts too, while its key bindings show. Observed once
    /// the file's write is queued; [`CommandState::Unavailable`] while the
    /// Mods screen does not show that mod's page, and
    /// [`CancelReason::ChangeRefused`] when the file cannot be written.
    Rebind {
        /// The mod's id.
        mod_id: String,
        /// The control's name, as the mod declared it.
        control: String,
        /// Whether it sets the control's second key, not its first.
        second: bool,
        /// The key, `None` to clear it.
        key: Option<crate::controls::Key>,
    },
    /// Sets `mod_id`'s setting `key` to `value`, one TOML value such as
    /// `true`, `3`, `0.5` or `"text"`, at most
    /// [`CommandAction::MAX_SETTING`] bytes, on that mod's page of the Mods
    /// screen, as the page's own field does: checked against the mod's
    /// schema and by the mod, written to the world's settings file of the
    /// mod with its comments kept, and the mod told. Only in a world this
    /// process runs, whose settings the page changes. Observed once the page
    /// made the change and the file's write is queued, not once it is on
    /// disk; [`CommandState::Unavailable`] while the page does
    /// not show the setting, and [`CancelReason::ChangeRefused`] for a value
    /// the schema or the mod refuses, or a setting the page shows read-only.
    SetSetting {
        /// The mod's id.
        mod_id: String,
        /// The setting's key, as the mod's settings name it.
        key: String,
        /// The value, as TOML.
        value: String,
    },
    /// Quits the game from the main menu through the game's own quit
    /// command, so the game ends on its own shutdown path. Its command's
    /// session is 0; refused outside the main menu and while a join or a
    /// host runs. Observed once the interface's queue takes it; the process
    /// then ends, so its status reads at most once more.
    Quit,
}

impl CommandAction {
    /// The longest line, in bytes, [`CommandAction::Chat`] sends: the
    /// game's own limit.
    pub const MAX_CHAT: usize = RawCommand::MAX_CHAT;

    /// The longest value, in bytes, [`CommandAction::SetSetting`] sets.
    pub const MAX_SETTING: usize = RawCommand::MAX_SETTING;
}

/// A server's password for [`CommandAction::Join`]: the loader hands it to
/// the game's own join and keeps, logs and returns none of it, and its
/// `Debug` shows none of it either.
///
/// # Examples
///
/// ```
/// use enshwick::input::Password;
///
/// let password = Password::new("hunter2");
/// assert!(!format!("{password:?}").contains("hunter2"));
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    /// The password `text`; the game takes at most 1024 bytes.
    ///
    /// # Examples
    ///
    /// See [`Password`].
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl core::fmt::Debug for Password {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Password(..)")
    }
}

/// A command for the client, from [`Input::client_command`]: what it does,
/// the session it belongs to and the screen the client must show.
///
/// # Examples
///
/// ```
/// use enshwick::input::{ClientCommand, CommandAction};
///
/// fn open_backpack(input: enshwick::input::Input) -> Option<enshwick::input::CommandId> {
///     let snapshot = input.client_snapshot().ok()?;
///     let command = ClientCommand::new(CommandAction::OpenScreen("Backpack".to_owned()), snapshot.session())
///         .on_screen("InGame");
///     input.client_command(&command).ok()
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct ClientCommand {
    action: CommandAction,
    session: u64,
    expected_screen: Option<String>,
}

impl ClientCommand {
    /// `action` for the client's session `session`, as
    /// [`ClientSnapshot::session`](super::ClientSnapshot::session) names it,
    /// on any screen; 0 takes the session of the first frame that runs it.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[must_use]
    pub const fn new(action: CommandAction, session: u64) -> Self {
        Self {
            action,
            session,
            expected_screen: None,
        }
    }

    /// The same command, run only while the HUD screen `screen` shows: one
    /// that finds another is cancelled.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[must_use]
    pub fn on_screen(mut self, screen: &str) -> Self {
        self.expected_screen = Some(screen.to_owned());
        self
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm per action, exhaustive so a new action cannot build without its raw form"
    )]
    fn raw(&self) -> RawCommand {
        let mut raw = enshwick_abi::headed!(RawCommand {
            session: self.session,
            expected_screen: self.expected_screen.as_deref().map_or(Str::EMPTY, Str::new),
            ..RawCommand::EMPTY
        });
        match &self.action {
            CommandAction::OpenScreen(screen) => {
                raw.kind = RawCommand::OPEN_SCREEN;
                raw.target = Str::new(screen);
            }
            CommandAction::CloseScreen => raw.kind = RawCommand::CLOSE_SCREEN,
            CommandAction::ChooseTab(tab) => {
                raw.kind = RawCommand::CHOOSE_TAB;
                raw.target = Str::new(tab);
            }
            CommandAction::Look { yaw, pitch } => {
                raw.kind = RawCommand::LOOK;
                raw.look = [*yaw, *pitch];
            }
            CommandAction::Move { direction, hold_ms } => {
                raw.kind = RawCommand::MOVE;
                raw.direction = *direction;
                raw.hold_ms = *hold_ms;
            }
            CommandAction::HighlightRecipe(recipe) => {
                raw.kind = RawCommand::HIGHLIGHT_RECIPE;
                raw.recipe = *recipe;
            }
            CommandAction::Join { server, password } => {
                raw.kind = RawCommand::JOIN;
                raw.target = Str::new(server);
                raw.password = password.as_ref().map_or(Str::EMPTY, |p| Str::new(&p.0));
            }
            CommandAction::Leave => raw.kind = RawCommand::LEAVE,
            CommandAction::Press { action, hold_ms } => {
                raw.kind = RawCommand::PRESS;
                raw.target = Str::new(action.name());
                raw.hold_ms = *hold_ms;
            }
            CommandAction::SetWaypoint(at) => {
                raw.kind = RawCommand::SET_WAYPOINT;
                raw.position = *at;
            }
            CommandAction::ClearWaypoint => raw.kind = RawCommand::CLEAR_WAYPOINT,
            CommandAction::Ping(at) => {
                raw.kind = RawCommand::PING;
                raw.position = *at;
            }
            CommandAction::Host {
                world,
                character,
                online,
            } => {
                raw.kind = RawCommand::HOST;
                raw.world = *world;
                raw.character = *character;
                raw.online = u32::from(*online);
            }
            CommandAction::OpenMods => raw.kind = RawCommand::OPEN_MODS,
            CommandAction::OpenMod(id) => {
                raw.kind = RawCommand::OPEN_MOD;
                raw.target = Str::new(id);
            }
            CommandAction::OpenModSettings(id) => {
                raw.kind = RawCommand::OPEN_MOD_SETTINGS;
                raw.target = Str::new(id);
            }
            CommandAction::PressWindowItem { window, item } => {
                raw.kind = RawCommand::WINDOW_PRESS;
                raw.target = Str::new(window);
                raw.item = Str::new(item);
            }
            CommandAction::PickWindowItem {
                window,
                item,
                option,
            } => {
                raw.kind = RawCommand::WINDOW_PICK;
                raw.target = Str::new(window);
                raw.item = Str::new(item);
                raw.text = Str::new(option);
            }
            CommandAction::SetWindowText { window, item, text } => {
                raw.kind = RawCommand::WINDOW_TEXT;
                raw.target = Str::new(window);
                raw.item = Str::new(item);
                raw.text = Str::new(text);
            }
            CommandAction::AnswerDialog { dialog, response } => {
                raw.kind = RawCommand::ANSWER_DIALOG;
                raw.dialog = *dialog;
                raw.answer = response.to_raw();
            }
            CommandAction::Chat(text) => {
                raw.kind = RawCommand::CHAT;
                raw.text = Str::new(text);
            }
            CommandAction::Rebind {
                mod_id,
                control,
                second,
                key,
            } => {
                raw.kind = RawCommand::REBIND;
                raw.target = Str::new(mod_id);
                raw.item = Str::new(control);
                raw.key_slot = u32::from(*second);
                raw.key = key.map_or(0, crate::controls::Key::bits);
            }
            CommandAction::SetSetting { mod_id, key, value } => {
                raw.kind = RawCommand::SET_SETTING;
                raw.target = Str::new(mod_id);
                raw.item = Str::new(key);
                raw.text = Str::new(value);
            }
            CommandAction::Quit => raw.kind = RawCommand::QUIT,
            CommandAction::Craft { .. }
            | CommandAction::MoveItem { .. }
            | CommandAction::Equip { .. }
            | CommandAction::TakeAll(_)
            | CommandAction::DepositAll(_)
            | CommandAction::Trash(_) => item_raw(&self.action, &mut raw),
        }
        raw
    }
}

/// Fills `raw` with the item command `action`; the others leave it.
fn item_raw(action: &CommandAction, raw: &mut RawCommand) {
    match action {
        CommandAction::Craft {
            recipe,
            count,
            station,
        } => {
            raw.kind = RawCommand::CRAFT;
            raw.recipe = *recipe;
            raw.amount = u32::from(*count);
            raw.entity = station.unwrap_or(Entity::from_engine(0));
        }
        CommandAction::MoveItem { from, to, count } => {
            raw.kind = RawCommand::MOVE_ITEM;
            raw.slot = from.0;
            raw.target_slot = to.0;
            raw.amount = count.map_or(0, |count| u32::from(count.get()));
        }
        CommandAction::Equip { slot, equipment } => {
            raw.kind = RawCommand::EQUIP;
            raw.slot = slot.0;
            raw.target = Str::new(equipment);
        }
        CommandAction::TakeAll(from) => {
            raw.kind = RawCommand::TAKE_ALL;
            raw.entity = *from;
        }
        CommandAction::DepositAll(to) => {
            raw.kind = RawCommand::DEPOSIT_ALL;
            raw.entity = *to;
        }
        CommandAction::Trash(slot) => {
            raw.kind = RawCommand::TRASH;
            raw.slot = slot.0;
        }
        _ => {}
    }
}

/// A queued command's id, for [`Input::command_status`].
///
/// # Examples
///
/// ```
/// use enshwick::input::CommandId;
///
/// assert_ne!(CommandId::new(1), CommandId::new(2));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct CommandId(u64);

impl CommandId {
    /// The id `id`, as the loader numbered it.
    ///
    /// # Examples
    ///
    /// See [`CommandId`].
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The loader's number for the command, to log, store or hand on; the
    /// Luau API gives the same number.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::input::CommandId;
    ///
    /// assert_eq!(CommandId::new(7).as_u64(), 7);
    /// ```
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[crate::script(client_commands)]
impl Input {
    /// Queues `command` for the client's interface or input; its id. The
    /// call answers only that it is queued: [`Input::command_status`]
    /// tells what came of it. It runs later on the client's game thread,
    /// only while the client is still in the command's session on its
    /// screen. One command of each kind waits per mod, and one press of each
    /// game action, up to four presses.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] while one of its kind waits or the mod holds
    /// four presses, and for a join, a host or a quit where it cannot start;
    /// [`WorldError::BadArgs`] for a value past its bound, a name the build
    /// does not know, such as an equipment slot, tab or game action, a mod
    /// id no loaded mod has, a control its mod did not declare, a response
    /// the dialog has no button for, or a command for another screen or
    /// session than the client's; [`WorldError::Unavailable`] on a server and on a build
    /// without what it needs, a screen to open among them.
    ///
    /// # Examples
    ///
    /// See [`ClientCommand`].
    #[script]
    pub fn client_command(self, command: &ClientCommand) -> Result<CommandId, WorldError> {
        let raw = command.raw();
        let mut id = 0u64;
        let host = self.0.host;
        world_status((host.api().client_command)(
            host.handle(),
            &raw const raw,
            &raw mut id,
        ))?;
        Ok(CommandId(id))
    }

    /// Cancels the mod's command `id` if it still waits.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server.
    ///
    /// # Examples
    ///
    /// ```
    /// fn give_up(input: enshwick::input::Input, id: enshwick::input::CommandId) {
    ///     let _cancelled = input.cancel_command(id);
    /// }
    /// ```
    #[script]
    pub fn cancel_command(self, id: CommandId) -> Result<(), WorldError> {
        let raw = enshwick_abi::headed!(RawCommand {
            kind: RawCommand::CANCEL,
            cancels: id.0,
            ..RawCommand::EMPTY
        });
        let mut out = 0u64;
        let host = self.0.host;
        world_status((host.api().client_command)(
            host.handle(),
            &raw const raw,
            &raw mut out,
        ))
    }

    /// Where the mod's command `id` stands.
    ///
    /// # Errors
    ///
    /// [`WorldError::Unavailable`] on a server and for an id the loader no
    /// longer keeps: it keeps the last 64 commands.
    ///
    /// # Examples
    ///
    /// See [`CommandStatus`].
    #[script]
    pub fn command_status(self, id: CommandId) -> Result<CommandStatus, WorldError> {
        let mut raw = Raw::EMPTY;
        let host = self.0.host;
        let status = (host.api().client_command_status)(host.handle(), id.0, &raw mut raw);
        if status != Status::OK {
            world_status(status)?;
        }
        Ok(CommandStatus {
            state: CommandState::from_raw(&raw),
            age_ms: raw.age_ms,
            phase: JoinPhase::from_raw(raw.phase),
        })
    }
}

#[cfg(test)]
mod tests;
