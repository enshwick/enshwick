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
    /// tells, and refused for a response the dialog has no button for.
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
    /// the slot's inventory changes.
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
    /// Destroys the stack in `slot`, as the game's trash does; the game has
    /// no drop of an item on the ground. Observed as [`Self::Equip`].
    Trash(SlotRef),
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
        let mut raw = RawCommand {
            session: self.session,
            expected_screen: self.expected_screen.as_deref().map_or(Str::EMPTY, Str::new),
            ..RawCommand::EMPTY
        };
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
            raw.entity = station.unwrap_or(Entity(0));
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
    /// four presses, a bad argument
    /// for a wrong screen, tab or bound, [`WorldError::Unavailable`] on a
    /// server and on a build without what it needs.
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
        let raw = RawCommand {
            kind: RawCommand::CANCEL,
            cancels: id.0,
            ..RawCommand::EMPTY
        };
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
mod tests {
    use super::luau::LuauCommand;
    use super::*;

    #[test]
    fn each_action_fills_its_own_fields_of_the_raw_command() {
        let open =
            ClientCommand::new(CommandAction::OpenScreen("Map".to_owned()), 2).on_screen("InGame");
        let raw = open.raw();
        assert_eq!((raw.kind, raw.session), (RawCommand::OPEN_SCREEN, 2));
        // SAFETY: the view points at `open`'s target, alive here.
        let target = unsafe { raw.target.as_str() };
        // SAFETY: the view points at `open`'s screen, alive here.
        let screen = unsafe { raw.expected_screen.as_str() };
        assert_eq!((target, screen), (Some("Map"), Some("InGame")));
        let walk = ClientCommand::new(
            CommandAction::Move {
                direction: [0.0, 1.0],
                hold_ms: 250,
            },
            2,
        )
        .raw();
        assert_eq!(
            (walk.kind, walk.direction, walk.hold_ms),
            (RawCommand::MOVE, [0.0, 1.0], 250)
        );
    }

    #[test]
    fn a_luau_command_needs_what_its_kind_takes() {
        let luau = |kind: &str, target: Option<&str>| LuauCommand {
            kind: kind.to_owned(),
            session: 1,
            expected_screen: None,
            target: target.map(str::to_owned),
            look: None,
            direction: None,
            hold_ms: None,
            recipe: None,
            password: None,
            action: None,
            position: None,
            world: None,
            character: None,
            online: None,
            window: None,
            item: None,
            text: None,
            answer: None,
            dialog: None,
            count: None,
            entity: None,
            slot: None,
            target_slot: None,
            equipment: None,
        };
        assert!(ClientCommand::try_from(luau("open_screen", Some("Map"))).is_ok());
        assert!(ClientCommand::try_from(luau("open_screen", None)).is_err());
        assert!(ClientCommand::try_from(luau("dance", None)).is_err());
        assert!(
            ClientCommand::try_from(luau("answer_dialog", None)).is_err(),
            "an answer needs its response"
        );
        let answer = |dialog| LuauCommand {
            answer: Some(Response::Ok),
            dialog,
            count: None,
            entity: None,
            slot: None,
            target_slot: None,
            equipment: None,
            ..luau("answer_dialog", None)
        };
        assert!(
            ClientCommand::try_from(answer(None)).is_err(),
            "an answer needs its dialog"
        );
        let command = ClientCommand::try_from(answer(Some(4))).expect("a whole answer");
        let raw = command.raw();
        assert_eq!(
            (raw.kind, raw.dialog, raw.answer),
            (RawCommand::ANSWER_DIALOG, 4, Response::Ok.to_raw())
        );
        assert!(
            ClientCommand::try_from(luau("close_screen", None))
                .is_ok_and(|c| c.action == CommandAction::CloseScreen)
        );
    }

    #[test]
    fn a_press_carries_its_action_s_game_name_and_its_hold() {
        let press = ClientCommand::new(
            CommandAction::Press {
                action: GameAction::ContextualActionTap,
                hold_ms: 60,
            },
            3,
        )
        .raw();
        // SAFETY: the view points at the action's static name.
        let target = unsafe { press.target.as_str() };
        assert_eq!(
            (press.kind, target, press.hold_ms, press.session),
            (RawCommand::PRESS, Some("ContextualAction_Tap"), 60, 3)
        );
    }

    #[test]
    fn item_commands_carry_their_slots_counts_and_entities_within_bounds() {
        let craft = ClientCommand::new(
            CommandAction::Craft {
                recipe: RecipeId(0x7cd8_0a04),
                count: 2,
                station: None,
            },
            1,
        )
        .raw();
        assert_eq!(
            (craft.kind, craft.amount, craft.entity),
            (RawCommand::CRAFT, 2, Entity(0))
        );
        assert!(craft.in_bounds());
        assert!(!RawCommand { amount: 0, ..craft }.in_bounds(), "no craft");
        let (from, to) = (SlotRef::new(Entity(0x41), 2), SlotRef::new(Entity(0x42), 0));
        let split = ClientCommand::new(
            CommandAction::MoveItem {
                from,
                to,
                count: NonZeroU16::new(5),
            },
            1,
        )
        .raw();
        assert_eq!(
            (split.slot, split.target_slot, split.amount),
            (from.0, to.0, 5)
        );
        assert!(split.in_bounds());
        assert!(
            !RawCommand {
                target_slot: from.0,
                ..split
            }
            .in_bounds(),
            "a move onto its own slot"
        );
        let whole = ClientCommand::new(
            CommandAction::MoveItem {
                from,
                to,
                count: None,
            },
            1,
        )
        .raw();
        assert_eq!(whole.amount, 0, "the whole stack");
        let take = ClientCommand::new(CommandAction::TakeAll(Entity(500)), 1).raw();
        assert_eq!(
            (take.kind, take.entity),
            (RawCommand::TAKE_ALL, Entity(500))
        );
        assert!(
            !RawCommand {
                entity: Entity(0),
                ..take
            }
            .in_bounds()
        );
        let trash = ClientCommand::new(CommandAction::Trash(from), 1).raw();
        assert_eq!((trash.kind, trash.slot), (RawCommand::TRASH, from.0));
    }

    #[test]
    fn a_luau_move_item_takes_a_count_above_zero_or_none_for_the_whole_stack() {
        let (from, to) = (SlotRef::new(Entity(9), 0), SlotRef::new(Entity(9), 1));
        let luau = |count: Option<u16>| LuauCommand {
            kind: "move_item".to_owned(),
            session: 1,
            expected_screen: None,
            target: None,
            look: None,
            direction: None,
            hold_ms: None,
            recipe: None,
            password: None,
            action: None,
            position: None,
            world: None,
            character: None,
            online: None,
            window: None,
            item: None,
            text: None,
            answer: None,
            dialog: None,
            count,
            entity: None,
            slot: Some(from),
            target_slot: Some(to),
            equipment: None,
        };
        let moved = |count| ClientCommand::try_from(luau(count)).map(|c| c.action).ok();
        assert_eq!(
            moved(Some(4)),
            Some(CommandAction::MoveItem {
                from,
                to,
                count: NonZeroU16::new(4),
            })
        );
        assert_eq!(
            moved(None),
            Some(CommandAction::MoveItem {
                from,
                to,
                count: None,
            })
        );
        assert_eq!(moved(Some(0)), None, "a count of 0 moves nothing");
    }

    #[test]
    fn a_luau_press_needs_a_game_action_by_its_game_name_and_a_hold() {
        let luau = |action: Option<&str>, hold_ms: Option<u32>| LuauCommand {
            kind: "press".to_owned(),
            session: 1,
            expected_screen: None,
            target: None,
            look: None,
            direction: None,
            hold_ms,
            recipe: None,
            password: None,
            action: action.map(str::to_owned),
            position: None,
            world: None,
            character: None,
            online: None,
            window: None,
            item: None,
            text: None,
            answer: None,
            dialog: None,
            count: None,
            entity: None,
            slot: None,
            target_slot: None,
            equipment: None,
        };
        assert!(
            ClientCommand::try_from(luau(Some("MainhandAction_Tap"), Some(16))).is_ok_and(|c| {
                c.action
                    == CommandAction::Press {
                        action: GameAction::MainhandActionTap,
                        hold_ms: 16,
                    }
            })
        );
        assert!(ClientCommand::try_from(luau(Some("Dance"), Some(16))).is_err());
        assert!(ClientCommand::try_from(luau(None, Some(16))).is_err());
        assert!(ClientCommand::try_from(luau(Some("Jump"), None)).is_err());
    }

    #[test]
    fn a_cancelled_status_names_its_reason() {
        let raw = Raw {
            state: Raw::CANCELLED,
            reason: Raw::PLAYER_INPUT,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&raw),
            CommandState::Cancelled(CancelReason::PlayerInput)
        );
        assert_eq!(CancelReason::PlayerInput.name(), "player_input");
    }

    #[test]
    fn a_luau_join_never_shows_its_password_in_a_debug_print() {
        let luau = LuauCommand {
            kind: "join".to_owned(),
            session: 0,
            expected_screen: None,
            target: Some("127.0.0.1:15637".to_owned()),
            look: None,
            direction: None,
            hold_ms: None,
            recipe: None,
            password: Some("hunter2".to_owned()),
            action: None,
            position: None,
            world: None,
            character: None,
            online: None,
            window: None,
            item: None,
            text: None,
            answer: None,
            dialog: None,
            count: None,
            entity: None,
            slot: None,
            target_slot: None,
            equipment: None,
        };
        let shown = format!("{luau:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("127.0.0.1:15637"));
    }

    #[test]
    fn a_join_carries_its_server_and_password_and_a_failed_one_names_why() {
        let join = ClientCommand::new(
            CommandAction::Join {
                server: "127.0.0.1:15637".to_owned(),
                password: Some(Password::new("hunter2")),
            },
            0,
        );
        let raw = join.raw();
        assert_eq!(raw.kind, RawCommand::JOIN);
        // SAFETY: the view points at `join`'s server, alive here.
        let target = unsafe { raw.target.as_str() };
        // SAFETY: the view points at `join`'s password, alive here.
        let password = unsafe { raw.password.as_str() };
        assert_eq!(
            (target, password),
            (Some("127.0.0.1:15637"), Some("hunter2"))
        );
        assert!(
            !format!("{join:?}").contains("hunter2"),
            "the password never shows"
        );
        let failed = Raw {
            state: Raw::FAILED,
            failure: Raw::FULL,
            phase: Raw::PHASE_MENU,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&failed),
            CommandState::Failed(JoinFailure::Full)
        );
        assert_eq!(JoinPhase::from_raw(failed.phase), Some(JoinPhase::Menu));
        assert_eq!(JoinPhase::from_raw(0), None, "no phase for other kinds");
        assert_eq!(JoinFailure::from_raw(99), JoinFailure::Other(99));
    }

    #[test]
    fn a_reason_a_newer_loader_adds_reads_as_other_not_as_a_known_one() {
        let raw = Raw {
            state: Raw::CANCELLED,
            reason: 99,
            ..Raw::EMPTY
        };
        assert_eq!(
            CommandState::from_raw(&raw),
            CommandState::Cancelled(CancelReason::Other(99))
        );
        assert_eq!(CancelReason::Other(99).name(), "other");
    }
}
