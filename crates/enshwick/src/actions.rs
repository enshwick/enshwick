//! Player actions: what a player asks a mod to do, declared once and run
//! from the mod's own controls and panels or the console.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! A mod declares each action with its id, its label, who may use it, what
//! it acts on and its arguments as typed choices, and handles it on the
//! server. The platform does the rest:
//!
//! - it runs the action when the mod's own control or panel asks with
//!   [`Actions::run`];
//! - it sends the player's pick to the server over the loader's own
//!   messages, checks there who may use the action, runs the handler and
//!   shows the reply as one of the game's notices;
//! - it runs the same action from the server's [console](crate::console),
//!   as the mod's id, the action's id and its values as words, such as
//!   `weather set storm 10` in a dedicated server's command file.
//!
//! The server decides everything: the client only sends what the player
//! picked, and the server checks again on every run. Texts are keys of the
//! mod's [`Texts`], shown in each player's language; a key without a text
//! shows as it is, so plain English works too.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Arg, Outcome, Value};
//! use enshwick::i18n::Texts;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let actions = load.actions(Texts::default())?;
//!     let kinds = [("clear", "Clear"), ("rain", "Rain"), ("storm", "Storm")];
//!     actions.add(
//!         Action::new("set", "Change the weather")
//!             .admins()
//!             .arg(Arg::choice("Weather", &kinds))
//!             .arg(Arg::number("Minutes", 1.0..=60.0, 1.0)),
//!         move |request| match request.args {
//!             [Value::Choice(kind), Value::Number(minutes)] => {
//!                 Outcome::done(format!("{} for {minutes} min", kinds[*kind].1))
//!             }
//!             _ => Outcome::refused("pick a weather"),
//!         },
//!     )?;
//!     Ok(())
//! }
//! ```

use registry::Declared;

use crate::locks::RelockRw;

pub mod sheet;
pub mod window;

pub use window::{Row, Window};

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use crate::PlayerKey;
use crate::abi::{ActionCall, ActionReply, Str, read_headed};
use crate::controls::{Control, Key};
use crate::i18n::{Language, Texts};
use crate::world::{Player, PlayerPermissions};
use crate::{Load, Off};

/// The platform's own texts for the console's answers; a mod's texts
/// override them by key.
const PLATFORM_TEXTS: &str = r#"
[en]
"actions.usage" = "Actions, each after the mod's id:"
"actions.unknown" = "No action {word}; help lists them."
"actions.denied" = "You may not do that."
"actions.no_player" = "No player named {name}."
"actions.several" = "Several players are named {name}."
"actions.gone" = "That player is no longer here."
"actions.bad" = "Write it so: {usage}"
"actions.too_long" = "{label} takes at most {max} bytes."
"actions.player" = "player"
"actions.on" = "on"
"actions.off" = "off"
"actions.commands_off" = "This mod's commands are off after an error."

[uk]
"actions.usage" = "Дії, кожна після id мода:"
"actions.unknown" = "Немає дії {word}; help перелічує їх."
"actions.denied" = "Тобі цього не можна."
"actions.no_player" = "Немає гравця на ім'я {name}."
"actions.several" = "Кілька гравців мають ім'я {name}."
"actions.gone" = "Цього гравця вже немає."
"actions.bad" = "Пиши так: {usage}"
"actions.too_long" = "{label}: щонайбільше {max} байтів."
"actions.player" = "гравець"
"actions.on" = "on"
"actions.off" = "off"
"actions.commands_off" = "Команди цього мода вимкнено після помилки."
"#;

/// The longest action id.
pub const MAX_ID: usize = 32;

/// Players an action's list or a player argument shows at most.
pub const MAX_PLAYERS: usize = 32;

/// The most bytes an [`Arg::text`] takes.
pub const MAX_TEXT: usize = 256;

type Allow = Box<dyn Fn(&Player) -> bool + Send + Sync>;
type At = Box<dyn Fn(&Ask<'_>) -> Result<Subject, FindSubjectError> + Send + Sync>;
type On = Box<dyn Fn(&Ask<'_>, Option<&Subject>) -> Vec<(PlayerKey, String)> + Send + Sync>;
type Known = Box<dyn Fn(&Ask<'_>) -> Vec<(PlayerKey, String)> + Send + Sync>;

/// The most accounts the platform takes from an action's
/// [`known`](Action::known) function, in the order it gives them.
pub const MAX_KNOWN: usize = 1_024;
type Handler = Box<dyn Fn(&Request<'_>) -> Outcome + Send + Sync>;

/// An action as a mod declares it: hand it to [`Actions::add`] with its
/// handler.
///
/// An action is a button with its arguments as rows above it, or, made with
/// [`Action::per_player`], a list of players with a switch each.
///
/// # Examples
///
/// See the [module](self).
pub struct Action {
    id: String,
    label: String,
    help: String,
    args: Vec<Arg>,
    /// The permission bits a player needs, all of them.
    needs: u8,
    allow: Option<Allow>,
    at: Option<At>,
    on: Option<On>,
    known: Option<Known>,
}

impl core::fmt::Debug for Action {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Action")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("args", &self.args)
            .field("needs", &self.needs)
            .field("per_player", &self.on.is_some())
            .field("knows_accounts", &self.known.is_some())
            .finish_non_exhaustive()
    }
}

impl Action {
    /// A button labelled `label`; `id`, 1 to 32 of `[a-z0-9-]`, names it in
    /// console lines.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn new(id: &str, label: &str) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            help: String::new(),
            args: Vec::new(),
            needs: 0,
            allow: None,
            at: None,
            on: None,
            known: None,
        }
    }

    /// A list of players with a switch each: the players online and those
    /// `on` names, each switched on when `on` names them. A flip runs the
    /// handler with [`Value::Player`] and [`Value::Toggle`]; on the console
    /// it is `<id> <player> on|off`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let friends = Action::per_player("friends", "Friends", |_ask, _subject| vec![(enshwick::PlayerKey::new(7), "Ann".into())]);
    /// assert_eq!(friends.id(), "friends");
    /// ```
    #[must_use]
    pub fn per_player<F>(id: &str, label: &str, on: F) -> Self
    where
        F: Fn(&Ask<'_>, Option<&Subject>) -> Vec<(PlayerKey, String)> + Send + Sync + 'static,
    {
        let mut action = Self::new(id, label);
        action.on = Some(Box::new(on));
        action
    }

    /// The action's id.
    ///
    /// # Examples
    ///
    /// See [`Arg::player`].
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// A line on what the action does, a text key, shown under its label and
    /// in the console's help.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let action = Action::new("home", "Go home").help("Takes you to your bed");
    /// assert_eq!(action.id(), "home");
    /// ```
    #[must_use]
    pub fn help(mut self, help: &str) -> Self {
        help.clone_into(&mut self.help);
        self
    }

    /// Adds an argument, after the ones before. A list takes none.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn arg(mut self, arg: Arg) -> Self {
        self.args.push(arg);
        self
    }

    /// Only players for whom `allow` is true see the action and may run it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let action = Action::new("wave", "Wave").allow(|player| !player.name.is_empty());
    /// assert_eq!(action.id(), "wave");
    /// ```
    #[must_use]
    pub fn allow<F>(mut self, allow: F) -> Self
    where
        F: Fn(&Player) -> bool + Send + Sync + 'static,
    {
        self.allow = Some(Box::new(allow));
        self
    }

    /// Only the server's administrators see the action and may run it.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn admins(self) -> Self {
        self.allow(Player::is_admin)
    }

    /// Only players the server gives `permission`, as the game's user
    /// groups grant it, see the action and may run it. Each call adds one
    /// the player needs; with [`allow`](Self::allow) both must hold. The
    /// server checks it on every run, and carries a player only the
    /// controls of actions they may run.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    /// use enshwick::world::PlayerPermissions;
    ///
    /// let action = Action::new("repair", "Repair the base").needs(PlayerPermissions::CanEditBase);
    /// assert_eq!(action.id(), "repair");
    /// ```
    #[must_use]
    pub const fn needs(mut self, permission: PlayerPermissions) -> Self {
        self.needs |= permission.bit();
        self
    }

    /// What the action acts on for a player, found on the server each time
    /// the player looks or runs it: a [`Subject`], or a [`FindSubjectError`]
    /// with why there is none, which the player reads beside the dimmed
    /// action. A mod's own
    /// permission rules, extension points included, belong here.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Subject};
    ///
    /// let action = Action::new("open", "Open the gate").at(|ask| {
    ///     if ask.player.name.is_empty() {
    ///         Err(ask.say("No gate near you", &[]).into())
    ///     } else {
    ///         Ok(Subject::new(1, ask.say("The north gate", &[])))
    ///     }
    /// });
    /// assert_eq!(action.id(), "open");
    /// ```
    #[must_use]
    pub fn at<F>(mut self, at: F) -> Self
    where
        F: Fn(&Ask<'_>) -> Result<Subject, FindSubjectError> + Send + Sync + 'static,
    {
        self.at = Some(Box::new(at));
        self
    }

    /// Accounts the mod knows beyond the players online, such as banned
    /// players or those refused at the door, newest first: account key and
    /// last name. The action's [player](Arg::player) arguments and its
    /// [list](Action::per_player) offer them after the players online, up
    /// to [`MAX_PLAYERS`] rows, and a typed name finds any of the first
    /// [`MAX_KNOWN`], so an administrator acts on a player who is offline.
    /// A name that several accounts share is refused. Called on the server
    /// each time a player looks or runs the action.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::PlayerKey;
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let ban = Action::new("ban", "Ban")
    ///     .admins()
    ///     .arg(Arg::player("Player"))
    ///     .known(|_ask| vec![(PlayerKey::new(42), "Bob".into())]);
    /// assert_eq!(ban.id(), "ban");
    /// ```
    #[must_use]
    pub fn known<F>(mut self, known: F) -> Self
    where
        F: Fn(&Ask<'_>) -> Vec<(PlayerKey, String)> + Send + Sync + 'static,
    {
        self.known = Some(Box::new(known));
        self
    }
}

/// A mod's player actions, from [`Load::actions`]; `Clone`, so handlers and
/// settings callbacks may keep it.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone)]
pub struct Actions {
    registry: Arc<Registry>,
}

impl core::fmt::Debug for Actions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Actions").finish_non_exhaustive()
    }
}

impl Actions {
    /// Declares `action` with its handler, which runs on the server's game
    /// thread when a player runs it from the interface or the console names
    /// it.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id, one declared before, or arguments on a
    /// list.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn add<F>(&self, action: Action, handler: F) -> Result<(), Off>
    where
        F: Fn(&Request<'_>) -> Outcome + Send + Sync + 'static,
    {
        let id = action.id.clone();
        if !is_valid_id(&id) || id == "help" {
            return Err(Off::new(format!(
                "action {id}: ids are 1 to 32 of [a-z0-9-], not help"
            )));
        }
        if action.on.is_some() && !action.args.is_empty() {
            return Err(Off::new(format!("action {id}: a list takes no arguments")));
        }
        let mut declared = self.registry.declared.relock_write();
        if declared.iter().any(|d| d.action.id == id) {
            return Err(Off::new(format!("action {id}: the id is taken")));
        }
        declared.push(Arc::new(Declared {
            action,
            handler: Box::new(handler),
        }));
        Ok(())
    }

    /// On a client, asks the server to run this mod's action `action` with
    /// `args`: the server checks who may run it, runs the handler and shows
    /// the reply as a notice. A [control](crate::controls)'s handler uses it
    /// for what changes the world.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, or while the server takes no messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// use enshwick::controls::{Control, Key};
    /// use enshwick::i18n::Texts;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(Texts::default())?;
    ///     actions.add(Action::new("honk", "Honk"), |_| Outcome::done("Honk!"))?;
    ///     load.control(Control::new("horn", "Horn").key(Key::parse("H")?), move |_| {
    ///         let _sent = actions.run("honk", &[]);
    ///     })?;
    ///     Ok(())
    /// }
    /// ```
    pub fn run(&self, action: &str, args: &[Value]) -> Result<(), Off> {
        let host = self.registry.host;
        let args = sheet::encode_args(args);
        let status = (host.api().run_action)(host.handle(), Str::new(action), Str::new(&args));
        crate::load::ok(status, (), || format!("action {action}"))
    }

    /// Runs the action as [`run`](Self::run) does, on the object `target`,
    /// such as the chest whose screen holds the panel the player pressed:
    /// the server hands it to the action's [`at`](Action::at) as
    /// [`Ask::target`] and to its handler once it checked the object exists
    /// and stands within reach of the player, as [`Ask::target`] says.
    ///
    /// # Errors
    ///
    /// [`Off`] on a server, while the server takes no messages, or for a
    /// position that is not finite.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, ActionTarget, Outcome};
    /// use enshwick::i18n::Texts;
    /// use enshwick::ui::{Edge, HudScreenType};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(Texts::default())?;
    ///     actions.add(
    ///         Action::new("sort", "Sort").at(|ask| match ask.target {
    ///             Some(chest) => Ok(enshwick::actions::Subject::new(1, format!("{chest:?}"))),
    ///             None => Err("Open a chest first".into()),
    ///         }),
    ///         |_| Outcome::done("Sorted"),
    ///     )?;
    ///     let chest = ActionTarget::at([10.0, 2.0, -4.5]);
    ///     load.extend_screen(HudScreenType::StorageChest, Edge::Right, move |panel| {
    ///         if panel.button("Sort") {
    ///             let _sent = actions.run_at("sort", &[], chest);
    ///         }
    ///     })
    /// }
    /// ```
    pub fn run_at(&self, action: &str, args: &[Value], target: ActionTarget) -> Result<(), Off> {
        let host = self.registry.host;
        let args = sheet::encode_args(args);
        let raw = target.to_raw();
        let status = (host.api().run_action_at)(
            host.handle(),
            Str::new(action),
            Str::new(&args),
            &raw const raw,
        );
        crate::load::ok(status, (), || format!("action {action}"))
    }

    /// Puts the declared action `action` on `key` during init, so a press runs it.
    ///
    /// It declares a [control](crate::controls) of the same name that runs
    /// the action on the server, as [`Control::runs`](crate::controls::Control::runs)
    /// does. Its label is the action's label in each player's language. A mod
    /// of side server offers it to the clients that join.
    ///
    /// # Errors
    ///
    /// [`Off`] for an action not declared yet, one with arguments or a list
    /// of players, or when the loader refuses the control, such as when the
    /// mod already declared a control named after the action.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// use enshwick::controls::Key;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts_from("[en]\n\"day\" = \"Make it day\"\n\n[uk]\n\"day\" = \"Зробити день\"\n")?;
    ///     let actions = load.actions(texts)?;
    ///     actions.add(Action::new("day", "day").admins(), |_| Outcome::done("Morning"))?;
    ///     actions.control("day", Key::parse("F8")?)
    /// }
    /// ```
    pub fn control(&self, action: &str, key: Key) -> Result<(), Off> {
        let label = {
            let declared = self.registry.declared.relock_read();
            let found = declared
                .iter()
                .find(|d| d.action.id == action)
                .ok_or_else(|| Off::new(format!("action {action}: declare it before its key")))?;
            if found.action.on.is_some() || !found.action.args.is_empty() {
                return Err(Off::new(format!(
                    "action {action}: a key runs only an action without arguments"
                )));
            }
            found.action.label.clone()
        };
        let control = Control::new(action, &label).key(key).runs(action);
        crate::controls::declare(self.registry.host, &control, |_| {})
    }

    /// Names the mod's actions as [commands](crate::commands) during init:
    /// `/<name> <action> <values>`, or the same with the shorter `alias`,
    /// then runs an action from a player's chat, the server's console or a
    /// remote console, with the caller's rights, and `/<alias> help` lists
    /// the actions the caller may run. Each is 1 to 32 of `[a-z0-9-]`, and
    /// neither is `enshwick` or `help`.
    ///
    /// When another mod named one of the words first, the loader turns off
    /// only this mod's commands, says so in the log and on the Mods screen
    /// naming both mods, and the call returns `Ok`: the actions keep their
    /// other paths.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed word, an alias not shorter than the name, a
    /// second call, or a call after init.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Outcome};
    /// use enshwick::i18n::Texts;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(Texts::default())?;
    ///     actions.add(Action::new("online", "Players online"), |_| Outcome::done("2 online"))?;
    ///     // `/admin-tools online` and `/adl online` run it.
    ///     actions.commands("admin-tools", "adl")
    /// }
    /// ```
    pub fn commands(&self, name: &str, alias: &str) -> Result<(), Off> {
        if !crate::commands::names_fit(name, alias) {
            return Err(Off::new(format!(
                "commands /{name} /{alias}: each is 1 to 32 of [a-z0-9-], the alias shorter, neither enshwick nor help"
            )));
        }
        if self
            .registry
            .commands
            .swap(true, core::sync::atomic::Ordering::AcqRel)
        {
            return Err(Off::new("commands: named once already"));
        }
        let host = self.registry.host;
        let status = (host.api().commands)(host.handle(), Str::new(name), Str::new(alias));
        if status == crate::abi::Status::REFUSED {
            crate::warn!(
                "commands /{name} /{alias} are off: another mod's commands or id hold one of them"
            );
            return Ok(());
        }
        crate::load::ok(status, (), || format!("commands /{name} /{alias}"))
    }

    /// Tells the platform that who may run the mod's actions changed, such
    /// as when an [`allow`](Action::allow) rule reads settings that just
    /// changed: the server carries each player the controls they may run
    /// now, within two seconds. Every run checks the rules again anyway.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses, as for a mod turned off.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicBool, Ordering};
    ///
    /// use enshwick::actions::Actions;
    ///
    /// fn open_to_all(actions: &Actions, open: &Arc<AtomicBool>) -> Result<(), enshwick::Off> {
    ///     open.store(true, Ordering::Relaxed);
    ///     actions.rules_changed()
    /// }
    /// ```
    pub fn rules_changed(&self) -> Result<(), Off> {
        let host = self.registry.host;
        let status = (host.api().actions_changed)(host.handle());
        crate::load::ok(status, (), || "actions: rules changed".to_owned())
    }

    /// Shows `window` to the players with the account keys `players`, or
    /// updates it where it shows already, from the server: the client's
    /// loader draws it with the game's widgets, so a mod of side server
    /// opens windows on clients that run only Enshwick. After the server's
    /// step each player gets the window in their language, only when they
    /// may run every action it names; a press of its buttons runs the
    /// action as any run, checked again on the server. A player not online
    /// is skipped. Works the same on a dedicated server and in a hosted or
    /// single-player game, where the host gets its windows in process.
    ///
    /// The mod keeps the window for each player it names, apart from the
    /// window other players hold under the same id, and the server renders
    /// a player's own again after their run. It keeps the newest
    /// [`window::MAX_KEPT`], counted per player; a player whose window
    /// went past that sees it close at their next update.
    ///
    /// # Errors
    ///
    /// [`Off`] on a client; for a malformed id, a window past its bounds
    /// ([`window::MAX_ITEMS`], [`window::MAX_BUTTONS`]), a button or item
    /// naming an action not declared yet, or a button giving more values
    /// than its action takes; or when the loader refuses, as while too
    /// many shows wait.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Arg, Outcome, Window};
    /// use enshwick::i18n::Texts;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(Texts::default())?;
    ///     actions.add(
    ///         Action::new("say", "Announce").admins().arg(Arg::text("Message", 200)),
    ///         |_| Outcome::done("Sent"),
    ///     )?;
    ///     let shown = actions.clone();
    ///     actions.add(Action::new("panel", "Admin panel").admins(), move |request| {
    ///         let window = Window::new("panel", "Admin panel").action("say");
    ///         match shown.show(window, &[request.player.key]) {
    ///             Ok(()) => Outcome::done(""),
    ///             Err(why) => Outcome::refused(why.to_string()),
    ///         }
    ///     })?;
    ///     actions.control("panel", enshwick::controls::Key::parse("F7")?)
    /// }
    /// ```
    pub fn show(&self, window: Window, players: &[PlayerKey]) -> Result<(), Off> {
        let id = window.id.clone();
        if !is_valid_id(&id) {
            return Err(Off::new(format!(
                "window {id}: ids are 1 to 32 of [a-z0-9-]"
            )));
        }
        let fault = {
            let declared = self.registry.declared.relock_read();
            window.fault(&|action| {
                declared.iter().find(|d| d.action.id == action).map(|d| {
                    if d.action.on.is_some() {
                        2
                    } else {
                        d.action.args.len()
                    }
                })
            })
        };
        if let Some(fault) = fault {
            return Err(Off::new(format!("window {id}: {fault}")));
        }
        self.registry.windows.relock_write().keep(window, players);
        self.carry_window(&id, players, false)
    }

    /// Closes the window `id` on the clients of the players with the
    /// account keys `players`, from the server, and forgets it for them;
    /// [`show`](Self::show) opens it again.
    ///
    /// # Errors
    ///
    /// [`Off`] on a client, or when the loader refuses, as while too many
    /// shows and closes wait.
    ///
    /// # Examples
    ///
    /// ```
    /// fn close_panel(actions: &enshwick::actions::Actions, admin: enshwick::PlayerKey) {
    ///     let _closed = actions.close("panel", &[admin]);
    /// }
    /// ```
    pub fn close(&self, id: &str, players: &[PlayerKey]) -> Result<(), Off> {
        self.registry.windows.relock_write().forget(id, players);
        self.carry_window(id, players, true)
    }

    /// Hands the loader a show or a close of window `id` for `players`, in
    /// parts of the most players one call takes.
    fn carry_window(&self, id: &str, players: &[PlayerKey], close: bool) -> Result<(), Off> {
        let host = self.registry.host;
        for part in players.chunks(64) {
            let count = u32::try_from(part.len()).unwrap_or(u32::MAX);
            let status = (host.api().action_window)(
                host.handle(),
                Str::new(id),
                part.as_ptr(),
                count,
                u32::from(close),
            );
            crate::load::ok(status, (), || format!("window {id}"))?;
        }
        Ok(())
    }
}

/// What the mod answers the loader: a text, such as its sheet, or how a run
/// went.
enum Answered {
    Text(String),
    Run(Outcome),
}

thread_local! {
    /// The last reply, alive until the next question on this thread.
    static REPLY: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Runs a typed line for `player`, from chat, the console or a remote
/// caller: a panic in a handler turns off only the mod's typed lines, with a
/// warning, and every later one is refused saying so.
fn run_command(
    registry: &Registry,
    player: &Player,
    line: &str,
    language: Language,
    via: Via,
) -> Outcome {
    use core::sync::atomic::Ordering;

    if registry.commands_off.load(Ordering::Acquire) {
        return Outcome::refused(registry.say("actions.commands_off", language, &[]));
    }
    let run = || registry.run_from_console(player, line, language, via);
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).unwrap_or_else(|payload| {
        registry.commands_off.store(true, Ordering::Release);
        crate::warn!(
            "commands off: a handler panicked: {}",
            crate::host::panic_text(payload.as_ref())
        );
        Outcome::refused("")
    })
}

/// Answers one of the loader's questions about this mod's actions.
///
/// # Safety
///
/// - `call` is null or points at an [`ActionCall`] readable for the size its
///   header states, with no other writer during the call.
/// - `call`'s strings are null or point at their `len` bytes, alive and
///   unchanged for the call.
/// - `call`'s `reply`, where its header covers it, is null or writable for
///   one [`ActionReply`], which the loader reads before it asks again on
///   this thread.
/// - `user` is the `Box<Arc<Registry>>` that [`Load::actions`] handed over,
///   which the loader keeps for the process.
/// - `out` is null or writable for one [`Str`], which the loader reads
///   before it asks again on this thread.
unsafe extern "system" fn answer(call: *const ActionCall, user: *mut c_void, out: *mut Str) {
    // SAFETY: the caller guarantees `call` is null or an `ActionCall`
    // readable for the size its header states with no concurrent writer,
    // which is the contract of `read_headed`.
    let Some(call) = (unsafe { read_headed(call, size_of::<ActionCall>()) }) else {
        return;
    };
    // SAFETY: the caller guarantees `user` is the boxed `Arc<Registry>` that
    // `hand_over` leaked for the process, so it is aligned, alive and only
    // read through shared references.
    let registry = unsafe { &*user.cast::<Arc<Registry>>() };
    let player = Player::from_info(&call.player);
    let language = Language(call.language);
    // SAFETY: the caller keeps the action's bytes alive and unchanged for
    // the call, and `action` is not used after it returns.
    let action = unsafe { call.action.as_str() }.unwrap_or("");
    // SAFETY: the caller keeps the arguments' bytes alive and unchanged for
    // the call, and `args` is not used after it returns.
    let args = unsafe { call.args.as_str() }.unwrap_or("");
    let answered = crate::host::guard(
        "panic in an action",
        || Answered::Text(String::new()),
        || match call.kind {
            ActionCall::SHEET => Answered::Text(registry.sheet(&player, language)),
            ActionCall::WINDOW => Answered::Text(registry.window(&player, language, action)),
            ActionCall::RUN => {
                let covered = call.header.covers(
                    core::mem::offset_of!(ActionCall, target)
                        + size_of::<crate::abi::ActionTarget>(),
                );
                let target = covered
                    .then(|| ActionTarget::from_raw(&call.target))
                    .flatten();
                Answered::Run(registry.run_from_interface(&player, action, args, language, target))
            }
            ActionCall::CONSOLE => Answered::Run(run_command(
                registry,
                &player,
                action,
                Language::FALLBACK,
                Via::Console,
            )),
            ActionCall::CHAT => {
                Answered::Run(run_command(registry, &player, action, language, Via::Chat))
            }
            ActionCall::REMOTE => Answered::Run(run_command(
                registry,
                &player,
                action,
                Language::FALLBACK,
                Via::Remote,
            )),
            _ => Answered::Text(String::new()),
        },
    );
    let slot = if call
        .header
        .covers(core::mem::offset_of!(ActionCall, reply) + size_of::<*mut ActionReply>())
    {
        call.reply
    } else {
        core::ptr::null_mut()
    };
    REPLY.with_borrow_mut(|kept| match answered {
        Answered::Run(outcome) if !slot.is_null() => {
            *kept = outcome.text;
            let reply = ActionReply {
                header: crate::abi::Header::of::<ActionReply>(),
                done: u8::from(outcome.done),
                text: Str::new(kept),
            };
            // SAFETY: `slot` is not null, as the guard checks, and the caller
            // guarantees it writable for one `ActionReply`; its text lives in
            // this thread's buffer until the next question, after the loader
            // read it.
            unsafe { slot.write(reply) };
        }
        answered if !out.is_null() => {
            *kept = match answered {
                Answered::Text(text) => text,
                // A call without a reply slot reads a run as `1` or `0` and the text.
                Answered::Run(outcome) => format!("{}{}", u8::from(outcome.done), outcome.text),
            };
            // SAFETY: `out` is not null, as the guard checks, and the caller
            // guarantees it is writable for one `Str`; the text lives in this
            // thread's buffer until the next question, after the loader read
            // it.
            unsafe { out.write(Str::new(kept)) };
        }
        _ => {}
    });
}

impl Load<'_> {
    /// The mod's player actions: declare each with [`Actions::add`]. `texts`
    /// holds the labels' texts, as [`Load::texts`] reads them; the
    /// platform's own replies are in it under `actions.*` too, so a mod may
    /// change them. The loader keeps these texts beneath the ones the mod
    /// hands itself (see [`Load::texts`]), before or after: a key of both
    /// reads as the mod handed it, and an action's control
    /// ([`Actions::control`]) reads its label from them in each player's
    /// language, a missing variety falling back as any text does.
    ///
    /// # Errors
    ///
    /// [`Off`] for a second call, or when the loader refuses the actions.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn actions(&mut self, texts: Texts) -> Result<Actions, Off> {
        let mut all = Texts::parse(PLATFORM_TEXTS)?;
        all.merge(texts);
        let host = self.host();
        let registry = Arc::new(Registry {
            texts: all,
            host,
            declared: RwLock::new(Vec::new()),
            windows: RwLock::default(),
            commands: core::sync::atomic::AtomicBool::new(false),
            commands_off: core::sync::atomic::AtomicBool::new(false),
        });
        crate::load::hand_over(
            Arc::clone(&registry),
            |user| (host.api().actions)(host.handle(), answer, user),
            "actions",
        )?;
        crate::i18n::hand_under(host, &registry.texts);
        Ok(Actions { registry })
    }
}

/// Whether `id` can name an action or a [`Window`]: 1 to 32 characters of
/// `[a-z0-9-]`.
///
/// # Examples
///
/// ```
/// assert!(enshwick::actions::is_valid_id("ban-list"));
/// assert!(!enshwick::actions::is_valid_id("Ban List"));
/// ```
#[must_use]
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::registry::{fits, toggle_word};
    use super::*;

    #[test]
    fn toggle_words_read_both_ways() {
        let texts = Texts::parse(PLATFORM_TEXTS).unwrap();
        assert_eq!(toggle_word("ON", &texts, Language::FALLBACK), Some(true));
        assert_eq!(toggle_word("deny", &texts, Language::UK_UA), Some(false));
        assert_eq!(toggle_word("maybe", &texts, Language::FALLBACK), None);
    }

    #[test]
    fn values_fit_their_arguments() {
        let number = Arg::number("n", 5.0..=1.0, -1.0);
        assert_eq!(number.range, (1.0, 5.0));
        assert!((number.step - 1.0).abs() < f64::EPSILON);
        assert!(fits(&Value::Number(3.0), &number));
        assert!(!fits(&Value::Number(6.0), &number));
        assert!(!fits(&Value::Toggle(true), &number));
        let choice = Arg::choice("c", &[("a", "A")]);
        assert!(fits(&Value::Choice(0), &choice));
        assert!(!fits(&Value::Choice(1), &choice));
    }
}

mod registry;
mod request;
mod script;

use registry::Registry;
pub use request::*;
#[doc(hidden)]
pub use script::ActionFns;
pub(crate) use script::ScriptOutcome;
