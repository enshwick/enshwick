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

use std::sync::{Arc, RwLock};

use crate::Off;
use crate::PlayerKey;
use crate::abi::Str;
use crate::controls::{Control, Key};
use crate::i18n::Texts;
use crate::world::{Player, PlayerPermissions};

/// The platform's own texts for the console's answers; a mod's texts
/// override them by key.
const PLATFORM_TEXTS: &str = r#"
[en]
"actions.usage" = "Actions, each after the mod's id:"
"actions.help" = "{mod}: {command} <action> runs one, {command} help <action> explains one."
"actions.unknown" = "No action {word}; {help} lists them."
"actions.unknown_near" = "No action {word}. Closest: {near}. {help} lists them."
"actions.denied" = "You may not do that."
"actions.no_player" = "No player named {name}."
"actions.several" = "Several players are named {name}."
"actions.gone" = "That player is no longer here."
"actions.no_item" = "No item {name}."
"actions.no_item_near" = "No item {name}. Closest: {near}."
"actions.several_items" = "Several items are named {name}; name one by its GUID."
"actions.bad" = "Write it so: {usage}"
"actions.too_long" = "{label} takes at most {max} bytes."
"actions.player" = "player"
"actions.on" = "on"
"actions.off" = "off"
"actions.commands_off" = "This mod's commands are off after an error."

[uk]
"actions.usage" = "Дії, кожна після id мода:"
"actions.help" = "{mod}: {command} <дія> виконує дію, {command} help <дія> пояснює її."
"actions.unknown" = "Немає дії {word}; {help} перелічує їх."
"actions.unknown_near" = "Немає дії {word}. Найближчі: {near}. {help} перелічує їх."
"actions.denied" = "Тобі цього не можна."
"actions.no_player" = "Немає гравця на ім'я {name}."
"actions.several" = "Кілька гравців мають ім'я {name}."
"actions.gone" = "Цього гравця вже немає."
"actions.no_item" = "Немає предмета {name}."
"actions.no_item_near" = "Немає предмета {name}. Найближчі: {near}."
"actions.several_items" = "Кілька предметів мають назву {name}; вкажи один за його GUID."
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
type Closed = Arc<dyn Fn(PlayerKey, Option<&str>) + Send + Sync>;

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
    /// The text key of the group the help lists the action under; empty
    /// for none.
    group: String,
    /// Whether the players the action offers include the one who looks.
    with_self: bool,
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
            group: String::new(),
            with_self: false,
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

    /// A line on what the action does, a text key, shown under its label
    /// and by `help <id>` on a typed line.
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

    /// The group the action belongs to, a text key such as `Players` or
    /// `World`: the help of the mod's typed lines lists the actions under
    /// their groups, the groups in the order their first actions were
    /// declared, after the actions without one.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let kick = Action::new("kick", "Kick").group("Players").arg(Arg::player("Player"));
    /// assert_eq!(kick.id(), "kick");
    /// ```
    #[must_use]
    pub fn group(mut self, group: &str) -> Self {
        group.clone_into(&mut self.group);
        self
    }

    /// Offers the player who looks at the action among its players: first
    /// in a [list](Action::per_player) and in the picker of each
    /// [player](Arg::player) argument, which otherwise hold only the
    /// others. For a switch or an action a player may use on themselves,
    /// such as a heal.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let shielded = Action::per_player("shield", "Shield", |_ask, _subject| Vec::new()).with_self();
    /// assert_eq!(shielded.id(), "shield");
    /// ```
    #[must_use]
    pub const fn with_self(mut self) -> Self {
        self.with_self = true;
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

/// A mod's player actions, from [`Load::actions`](../struct.Load.html#method.actions); `Clone`, so handlers and
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
    /// [`Off`] for a malformed id, one declared before, arguments on a
    /// list, an [optional](Arg::optional) argument before one that is not,
    /// or one whose default its argument does not take.
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
        if let Some(fault) = registry::optional_fault(&action.args) {
            return Err(Off::new(format!("action {id}: {fault}")));
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
    /// `/<name> <action> <values>` then runs an action from a player's
    /// chat, the server's console or a remote console, with the caller's
    /// rights, and `/<name> help` lists the actions the caller may run. The
    /// name is the part of the mod's id after the author, `admin-tools` for
    /// `you.admin-tools`; `alias` is a shorter word for it, 1 to 32 of
    /// `[a-z0-9-]`, neither `enshwick` nor `help`.
    ///
    /// An alias goes to the first mod that asks for it. When another mod's
    /// commands, name or id hold the word, this mod's commands run by their
    /// full name alone, the loader says so once in the log and on the Mods
    /// screen naming both mods, and the call returns `Ok`;
    /// [`commands::list`](crate::commands::list) then shows the mod without
    /// an alias. When another mod's commands have the same name, the loader
    /// turns off only this mod's commands, says so the same way, and the
    /// call returns `Ok`: the actions keep their other paths.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed alias, an alias not shorter than the name, a
    /// mod whose name is `enshwick` or `help`, a second call, or a call
    /// after init.
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
    ///     // In the mod `you.admin-tools`, `/admin-tools online` and
    ///     // `/adm online` run it.
    ///     actions.commands("adm")
    /// }
    /// ```
    pub fn commands(&self, alias: &str) -> Result<(), Off> {
        if !crate::commands::is_alias(alias) {
            return Err(Off::new(format!(
                "commands, alias /{alias}: an alias is 1 to 32 of [a-z0-9-], neither enshwick nor help"
            )));
        }
        let named = &self.registry.commands;
        if named.load(core::sync::atomic::Ordering::Acquire) {
            return Err(Off::new("commands: named once already"));
        }
        let host = self.registry.host;
        let status = (host.api().commands)(host.handle(), Str::EMPTY, Str::new(alias));
        if matches!(status, crate::abi::Status::OK | crate::abi::Status::REFUSED) {
            named.store(true, core::sync::atomic::Ordering::Release);
        }
        if status == crate::abi::Status::REFUSED {
            crate::warn!("commands are off: another mod's commands have this mod's name");
            return Ok(());
        }
        crate::load::ok(status, (), || {
            let rule = if status == crate::abi::Status::BAD_ARGS {
                ", which must be shorter than the mod's name, itself neither enshwick nor help"
            } else {
                ""
            };
            format!("commands, alias /{alias}{rule}")
        })
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
    /// naming an action not declared yet, or a button giving values its
    /// action does not take; or when the loader refuses, as while too many
    /// shows wait. The mod keeps the window only for the players the
    /// loader took.
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
        let window = self.checked(window)?;
        let mut kept = self.registry.windows.relock_write();
        self.carry_window(&window.id, players, crate::abi::WINDOW_SHOW, |part| {
            kept.keep(&window, part);
        })
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
        let mut kept = self.registry.windows.relock_write();
        self.carry_window(id, players, crate::abi::WINDOW_CLOSE, |part| {
            kept.forget(id, part);
        })
    }

    /// Hands the loader `change`, a show, a close or an update of window
    /// `id`, for `players`, in parts of the most players one call takes;
    /// `taken` gets each part the loader took.
    fn carry_window(
        &self,
        id: &str,
        players: &[PlayerKey],
        change: u32,
        mut taken: impl FnMut(&[PlayerKey]),
    ) -> Result<(), Off> {
        let host = self.registry.host;
        for part in players.chunks(window::MAX_CALL_PLAYERS) {
            let count = u32::try_from(part.len()).unwrap_or(u32::MAX);
            let status = (host.api().action_window)(
                host.handle(),
                Str::new(id),
                part.as_ptr(),
                count,
                change,
            );
            crate::load::ok(status, (), || format!("window {id}"))?;
            taken(part);
        }
        Ok(())
    }

    /// `window` behind an [`Arc`], once its id and what its buttons and
    /// items name are checked against the actions declared.
    fn checked(&self, window: Window) -> Result<Arc<Window>, Off> {
        let id = &window.id;
        if !is_valid_id(id) {
            return Err(Off::new(format!(
                "window {id}: ids are 1 to 32 of [a-z0-9-]"
            )));
        }
        let fault = {
            let declared = self.registry.declared.relock_read();
            window.fault(
                &|action, values| match declared.iter().find(|d| d.action.id == action) {
                    Some(d) => values.and_then(|v| registry::button_fault(&d.action, v)),
                    None => Some(format!("no action {action}")),
                },
            )
        };
        match fault {
            Some(fault) => Err(Off::new(format!("window {id}: {fault}"))),
            None => Ok(Arc::new(window)),
        }
    }

    /// Puts `window` in place of the one of its id on the clients of
    /// `players` that still show it, from the server, and opens it for no
    /// one: a player who closed the window, or was never shown it, gets
    /// nothing. For a list that changes while players hold it, such as a
    /// roster when someone joins; [`show`](Self::show) opens the window.
    ///
    /// # Errors
    ///
    /// As [`show`](Self::show); also [`Off`] under a loader before ABI
    /// 11.23, which knows no update.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Actions, Window};
    ///
    /// fn refresh(actions: &Actions, admins: &[enshwick::PlayerKey], online: usize) {
    ///     let roster = Window::new("roster", "Players").text("{n} online", &[("n", &online.to_string())]);
    ///     let _queued = actions.update(roster, admins);
    /// }
    /// ```
    pub fn update(&self, window: Window, players: &[PlayerKey]) -> Result<(), Off> {
        let window = self.checked(window)?;
        let mut kept = self.registry.windows.relock_write();
        self.carry_window(&window.id, players, crate::abi::WINDOW_UPDATE, |part| {
            kept.replace(&window, part);
        })
    }

    /// Calls `closed` on the server's game thread when a player's client
    /// closed one of the mod's windows, with the window's id, or when a
    /// player left, with `None` for every window they held. A client keeps
    /// eight windows of all mods: a ninth lets the oldest go, which its mod
    /// hears as closed, and so is a window that renders nothing or too much
    /// for a player, which closes on their client. The mod has
    /// forgotten those windows by then; a mod that keeps state of its own
    /// for a window frees it here. One function at a time: a second call
    /// replaces the first.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions(enshwick::i18n::Texts::default())?;
    ///     actions.on_closed(|player, window| match window {
    ///         Some(id) => enshwick::info!("{player:?} closed {id}"),
    ///         None => enshwick::info!("{player:?} left"),
    ///     });
    ///     Ok(())
    /// }
    /// ```
    pub fn on_closed<F>(&self, closed: F)
    where
        F: Fn(PlayerKey, Option<&str>) + Send + Sync + 'static,
    {
        *self.registry.closed.relock_write() = Some(Arc::new(closed));
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::actions`.
    #[doc(hidden)]
    fn actions(&mut self, texts: Texts) -> Result<Actions, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn actions(&mut self, texts: Texts) -> Result<Actions, Off> {
        let mut all = Texts::parse(PLATFORM_TEXTS)?;
        all.merge(texts);
        let host = self.host();
        let registry = Arc::new(Registry {
            texts: all,
            host,
            declared: RwLock::new(Vec::new()),
            windows: RwLock::default(),
            closed: RwLock::new(None),
            commands: core::sync::atomic::AtomicBool::new(false),
            commands_off: core::sync::atomic::AtomicBool::new(false),
        });
        crate::load::hand_over(
            Arc::clone(&registry),
            |user| (host.api().actions)(host.handle(), answer::answer, user),
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
    use crate::i18n::Language;

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

mod answer;
mod registry;
mod request;
pub(crate) mod script;

use registry::Registry;
pub use request::*;
#[doc(hidden)]
pub use script::ActionFns;
pub(crate) use script::ScriptOutcome;
