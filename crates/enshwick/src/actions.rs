//! Player actions: what a player asks a mod to do, declared once and offered
//! in the game's interface and as a chat command.
//!
//! A mod declares each action with its id, its label, who may use it, what
//! it acts on and its arguments as typed choices, and handles it on the
//! server. The platform does the rest:
//!
//! - it draws the action in the game's style, on the mod's page of the Mods
//!   screen, and, for an action with a [hint](Action::hint), as a line on
//!   the HUD while its subject is at hand, with a key that opens a window of
//!   the mod's actions;
//! - it sends the player's pick to the server over the loader's own
//!   messages, checks there who may use the action, runs the handler and
//!   shows the reply as one of the game's notices;
//! - it answers the same action as a chat command, `/<command> <id> ...`,
//!   with help made from the declarations, for players whose client lacks
//!   Enshwick and for those who prefer typing.
//!
//! The server decides everything: the client only shows what the server
//! offered and sends what the player picked, and the server checks again on
//! every run. Texts are keys of the mod's [`Texts`], shown in each player's
//! language; a key without a text shows as it is, so plain English works
//! too.
//!
//! # Examples
//!
//! ```
//! use enshwick::actions::{Action, Arg, Outcome, Value};
//! use enshwick::i18n::Texts;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let actions = load.actions("weather", Texts::default())?;
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

pub mod sheet;

use core::ffi::c_void;
use core::ops::RangeInclusive;
use core::sync::atomic::{AtomicU32, Ordering};
use std::cell::RefCell;
use std::sync::{Arc, PoisonError, RwLock};

use crate::abi::{ActionCall, Str, read_headed};
use crate::commands::{CommandContext, Reply, is_valid_name};
use crate::i18n::{Language, Texts};
use crate::world::Player;
use crate::{Host, Load, Off};

use sheet::{ArgView, Entry};

/// The platform's own texts for replies to the chat form; a mod's texts
/// override them by key.
const PLATFORM_TEXTS: &str = r#"
[en]
"actions.usage" = "/{command}: what you can do"
"actions.unknown" = "No action {word}. /{command} help lists them."
"actions.denied" = "You may not do that."
"actions.no_player" = "No player named {name}."
"actions.several" = "Several players are named {name}."
"actions.bad" = "Use it so: {usage}"
"actions.console" = "The console has no character."
"actions.player" = "player"
"actions.on" = "on"
"actions.off" = "off"

[uk]
"actions.usage" = "/{command}: що можна зробити"
"actions.unknown" = "Немає дії {word}. /{command} help перелічує їх."
"actions.denied" = "Тобі цього не можна."
"actions.no_player" = "Немає гравця на ім'я {name}."
"actions.several" = "Кілька гравців мають ім'я {name}."
"actions.bad" = "Пиши так: {usage}"
"actions.console" = "У консолі немає персонажа."
"actions.player" = "гравець"
"actions.on" = "on"
"actions.off" = "off"
"#;

/// Players an action's list or a player argument shows at most.
pub const MAX_PLAYERS: usize = 32;

/// A value the player picked for an argument, in the order of
/// [`Action::arg`]; a [list](Action::per_player) hands the player and the
/// switch.
///
/// # Examples
///
/// ```
/// use enshwick::actions::Value;
///
/// fn minutes(args: &[Value]) -> Option<f64> {
///     args.iter().find_map(|v| if let Value::Number(n) = v { Some(*n) } else { None })
/// }
/// assert_eq!(minutes(&[Value::Toggle(true), Value::Number(5.0)]), Some(5.0));
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Value {
    /// One of the server's players: the account key, and the name as the
    /// server knows it.
    Player {
        /// The account key, as [`Player::key`].
        key: u64,
        /// The name, online or as the mod's list keeps it.
        name: String,
    },
    /// The index of the picked option of [`Arg::choice`].
    Choice(usize),
    /// A number in the range of [`Arg::number`].
    Number(f64),
    /// On or off.
    Toggle(bool),
}

/// An argument of an action: a typed choice, never free text.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    kind: u8,
    label: String,
    choices: Vec<(String, String)>,
    range: (f64, f64),
    step: f64,
}

impl Arg {
    fn new(kind: u8, label: &str) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            choices: Vec::new(),
            range: (0.0, 0.0),
            step: 0.0,
        }
    }

    /// One of the server's players online now, picked from a list; typed
    /// by name in chat.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let action = Action::new("invite", "Invite").arg(Arg::player("Who"));
    /// assert_eq!(action.id(), "invite");
    /// ```
    #[must_use]
    pub fn player(label: &str) -> Self {
        Self::new(ArgView::PLAYER, label)
    }

    /// One of `choices`, each a word typed in chat and a label key.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn choice(label: &str, choices: &[(&str, &str)]) -> Self {
        let mut arg = Self::new(ArgView::CHOICE, label);
        arg.choices = choices
            .iter()
            .map(|(word, label)| ((*word).to_owned(), (*label).to_owned()))
            .collect();
        arg
    }

    /// A number in `range`, in steps of `step`; 0 for any value.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn number(label: &str, range: RangeInclusive<f64>, step: f64) -> Self {
        let (start, end) = range.into_inner();
        let mut arg = Self::new(ArgView::NUMBER, label);
        arg.range = (start.min(end), start.max(end));
        arg.step = if step.is_finite() { step.abs() } else { 0.0 };
        arg
    }

    /// On or off.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let action = Action::new("pvp", "Fights between players").arg(Arg::toggle("On"));
    /// assert_eq!(action.id(), "pvp");
    /// ```
    #[must_use]
    pub fn toggle(label: &str) -> Self {
        Self::new(ArgView::TOGGLE, label)
    }
}

/// What an action acts on for a player, from [`Action::at`]: an id the
/// handler gets back and a label the player reads.
///
/// # Examples
///
/// ```
/// use enshwick::actions::Subject;
///
/// let safe = Subject::new(42, "Your Safe at (2, 10, -4)");
/// assert_eq!(safe.id, 42);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subject {
    /// The mod's own id for it, such as a packed position.
    pub id: u64,
    /// What the player reads, in their language.
    pub label: String,
}

impl Subject {
    /// A subject.
    ///
    /// # Examples
    ///
    /// See [`Subject`].
    #[must_use]
    pub fn new(id: u64, label: impl Into<String>) -> Self {
        Self {
            id,
            label: label.into(),
        }
    }
}

/// Who asks, for [`Action::at`] and a list's players.
///
/// # Examples
///
/// See [`Action::at`].
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    /// The player.
    pub player: &'a Player,
    /// The language they read: the game's on their client for the
    /// interface, the one [`Actions::set_language`] set for chat.
    pub language: Language,
    texts: &'a Texts,
}

impl Ask<'_> {
    /// The mod's text `key` in the player's language, filled as
    /// [`Texts::fill`] does.
    ///
    /// # Examples
    ///
    /// See [`Action::at`].
    #[must_use]
    pub fn say(&self, key: &str, values: &[(&str, &str)]) -> String {
        self.texts.fill(key, self.language, values)
    }
}

/// Where a request came from.
///
/// # Examples
///
/// See [`Request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Via {
    /// The game's interface: the reply shows as a notice.
    Interface,
    /// A chat command: the reply goes to the player's chat.
    Chat,
}

/// A player's request to run an action, handed to its handler on the
/// server's game thread once the platform checked who may use it.
///
/// # Examples
///
/// ```
/// use enshwick::actions::{Outcome, Request};
///
/// fn greet(request: &Request<'_>) -> Outcome {
///     Outcome::done(request.say("hello", &[("name", &request.player.name)]))
/// }
/// ```
#[derive(Debug)]
pub struct Request<'a> {
    /// Who asks.
    pub player: &'a Player,
    /// What the action acts on, from [`Action::at`]; found again for this
    /// run.
    pub subject: Option<&'a Subject>,
    /// The values picked, in the order of the arguments.
    pub args: &'a [Value],
    /// The language the player reads.
    pub language: Language,
    /// Where it came from.
    pub via: Via,
    texts: &'a Texts,
}

impl Request<'_> {
    /// The mod's text `key` in the player's language, filled as
    /// [`Texts::fill`] does.
    ///
    /// # Examples
    ///
    /// See [`Request`].
    #[must_use]
    pub fn say(&self, key: &str, values: &[(&str, &str)]) -> String {
        self.texts.fill(key, self.language, values)
    }
}

/// How a request went: the text the player reads, as a notice or in chat.
///
/// # Examples
///
/// ```
/// use enshwick::actions::Outcome;
///
/// let done: Outcome = "Done".into();
/// assert_eq!(done, Outcome::done("Done"));
/// assert!(!Outcome::refused("Not here").is_done());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    done: bool,
    text: String,
}

impl Outcome {
    /// The action was done.
    ///
    /// # Examples
    ///
    /// See [`Outcome`].
    #[must_use]
    pub fn done(text: impl Into<String>) -> Self {
        Self {
            done: true,
            text: text.into(),
        }
    }

    /// The action was refused or failed.
    ///
    /// # Examples
    ///
    /// See [`Outcome`].
    #[must_use]
    pub fn refused(text: impl Into<String>) -> Self {
        Self {
            done: false,
            text: text.into(),
        }
    }

    /// Whether it was done.
    ///
    /// # Examples
    ///
    /// See [`Outcome`].
    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.done
    }

    /// The text the player reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Outcome;
    ///
    /// assert_eq!(Outcome::done("Ann may use it").text(), "Ann may use it");
    /// ```
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl From<String> for Outcome {
    fn from(text: String) -> Self {
        Self::done(text)
    }
}

impl From<&str> for Outcome {
    fn from(text: &str) -> Self {
        Self::done(text)
    }
}

type Allow = Box<dyn Fn(&Player) -> bool + Send + Sync>;
type At = Box<dyn Fn(&Ask<'_>) -> Result<Subject, String> + Send + Sync>;
type On = Box<dyn Fn(&Ask<'_>, Option<&Subject>) -> Vec<(u64, String)> + Send + Sync>;
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
    hint: bool,
    verbs: Option<(String, String)>,
    allow: Option<Allow>,
    at: Option<At>,
    on: Option<On>,
}

impl core::fmt::Debug for Action {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Action")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("args", &self.args)
            .field("hint", &self.hint)
            .field("per_player", &self.on.is_some())
            .finish_non_exhaustive()
    }
}

impl Action {
    /// A button labelled `label`, run in chat as `/<command> <id>`; `id` is
    /// 1 to 32 of `[a-z0-9-]`.
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
            hint: false,
            verbs: None,
            allow: None,
            at: None,
            on: None,
        }
    }

    /// A list of players with a switch each: the players online and those
    /// `on` names, each switched on when `on` names them. A flip runs the
    /// handler with [`Value::Player`] and [`Value::Toggle`]; in chat it is
    /// `/<command> <id> <player> on|off`, or the two [verbs](Action::verbs).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let friends = Action::per_player("friends", "Friends", |_ask, _subject| vec![(7, "Ann".into())]);
    /// assert_eq!(friends.id(), "friends");
    /// ```
    #[must_use]
    pub fn per_player<F>(id: &str, label: &str, on: F) -> Self
    where
        F: Fn(&Ask<'_>, Option<&Subject>) -> Vec<(u64, String)> + Send + Sync + 'static,
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
    /// in the chat help.
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

    /// What the action acts on for a player, found on the server each time
    /// the player looks or runs it: a [`Subject`], or why there is none,
    /// which the player reads beside the dimmed action. A mod's own
    /// permission rules, extension points included, belong here.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Subject};
    ///
    /// let action = Action::new("open", "Open the gate").at(|ask| {
    ///     if ask.player.name.is_empty() {
    ///         Err(ask.say("No gate near you", &[]))
    ///     } else {
    ///         Ok(Subject::new(1, ask.say("The north gate", &[])))
    ///     }
    /// });
    /// assert_eq!(action.id(), "open");
    /// ```
    #[must_use]
    pub fn at<F>(mut self, at: F) -> Self
    where
        F: Fn(&Ask<'_>) -> Result<Subject, String> + Send + Sync + 'static,
    {
        self.at = Some(Box::new(at));
        self
    }

    /// Shows a line on the HUD while the action's subject is at hand, with
    /// the key that opens a window of the mod's actions. Needs
    /// [`Action::at`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Subject};
    ///
    /// let action = Action::new("rest", "Rest here").at(|_| Ok(Subject::new(0, "Bed"))).hint();
    /// assert_eq!(action.id(), "rest");
    /// ```
    #[must_use]
    pub fn hint(mut self) -> Self {
        self.hint = true;
        self
    }

    /// The two chat words that switch a list's player on and off, such as
    /// `allow` and `deny`: `/<command> allow <player>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::Action;
    ///
    /// let access = Action::per_player("access", "Who may use it", |_, _| Vec::new()).verbs("allow", "deny");
    /// assert_eq!(access.id(), "access");
    /// ```
    #[must_use]
    pub fn verbs(mut self, on: &str, off: &str) -> Self {
        self.verbs = Some((on.to_owned(), off.to_owned()));
        self
    }
}

struct Declared {
    action: Action,
    handler: Handler,
}

struct Registry {
    command: String,
    texts: Texts,
    language: AtomicU32,
    host: Host,
    declared: RwLock<Vec<Declared>>,
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
        f.debug_struct("Actions")
            .field("command", &self.registry.command)
            .finish_non_exhaustive()
    }
}

impl Actions {
    /// Declares `action` with its handler, which runs on the server's game
    /// thread when a player runs it from the interface or the chat.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id, one declared before, arguments on a list,
    /// or a hint without [`Action::at`].
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn add<F>(&self, action: Action, handler: F) -> Result<(), Off>
    where
        F: Fn(&Request<'_>) -> Outcome + Send + Sync + 'static,
    {
        let id = action.id.clone();
        if !is_valid_name(&id) || id == "help" {
            return Err(Off::new(format!(
                "action {id}: ids are 1 to 32 of [a-z0-9-], not help"
            )));
        }
        if action.on.is_some() && !action.args.is_empty() {
            return Err(Off::new(format!("action {id}: a list takes no arguments")));
        }
        if action.hint && action.at.is_none() {
            return Err(Off::new(format!("action {id}: a hint needs a subject")));
        }
        let mut declared = self
            .registry
            .declared
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let taken = |word: &str| {
            declared.iter().any(|d| {
                d.action.id == word
                    || d.action
                        .verbs
                        .as_ref()
                        .is_some_and(|(on, off)| on == word || off == word)
            })
        };
        let words = core::iter::once(id.as_str()).chain(
            action
                .verbs
                .iter()
                .flat_map(|(on, off)| [on.as_str(), off.as_str()]),
        );
        for word in words {
            if taken(word) {
                return Err(Off::new(format!("action {id}: {word} is taken")));
            }
        }
        declared.push(Declared {
            action,
            handler: Box::new(handler),
        });
        Ok(())
    }

    /// The language of the chat form's labels and of [`Request::language`]
    /// for chat; the interface uses each player's own.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let actions = load.actions("hello", Texts::default())?;
    ///     actions.set_language(Language::UK_UA);
    ///     Ok(())
    /// }
    /// ```
    pub fn set_language(&self, language: Language) {
        self.registry.language.store(language.0, Ordering::Relaxed);
    }

    /// Runs the chat form `args` for `ctx`'s player, as the command does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::actions::{Action, Actions};
    /// use enshwick::commands::CommandContext;
    ///
    /// fn console_help(actions: &Actions) -> String {
    ///     let ctx = CommandContext { player: None, name: "hello".into() };
    ///     actions.chat(&ctx, &["help"]).text().to_owned()
    /// }
    /// ```
    #[must_use]
    pub fn chat(&self, ctx: &CommandContext, args: &[&str]) -> Reply {
        self.registry.chat(ctx, args)
    }
}

impl Registry {
    fn chat_language(&self) -> Language {
        Language(self.language.load(Ordering::Relaxed))
    }

    fn say(&self, key: &str, language: Language, values: &[(&str, &str)]) -> String {
        self.texts.fill(key, language, values)
    }

    fn online(&self) -> Vec<Player> {
        crate::world::online(self.host)
    }

    fn ask<'a>(&'a self, player: &'a Player, language: Language) -> Ask<'a> {
        Ask {
            player,
            language,
            texts: &self.texts,
        }
    }

    /// The usage line of `action` in chat.
    fn usage(&self, action: &Action, language: Language) -> Vec<String> {
        let command = &self.command;
        let player = format!("<{}>", self.say("actions.player", language, &[]));
        let on_off = format!(
            "<{}|{}>",
            self.say("actions.on", language, &[]),
            self.say("actions.off", language, &[])
        );
        let label = self.say(&action.label, language, &[]);
        if action.on.is_some() {
            let mut lines = vec![format!(
                "/{command} {} {player} {on_off}: {label}",
                action.id
            )];
            if let Some((on, off)) = &action.verbs {
                lines.push(format!(
                    "/{command} {on} {player}, /{command} {off} {player}"
                ));
            }
            return lines;
        }
        let mut line = format!("/{command} {}", action.id);
        for arg in &action.args {
            let shown = match arg.kind {
                ArgView::PLAYER => player.clone(),
                ArgView::CHOICE => format!(
                    "<{}>",
                    arg.choices
                        .iter()
                        .map(|(word, _)| word.as_str())
                        .collect::<Vec<_>>()
                        .join("|")
                ),
                ArgView::NUMBER => format!("<{}-{}>", arg.range.0, arg.range.1),
                _ => on_off.clone(),
            };
            line.push(' ');
            line.push_str(&shown);
        }
        vec![format!("{line}: {label}")]
    }

    fn help(&self, player: Option<&Player>, language: Language) -> String {
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        let mut lines = vec![self.say("actions.usage", language, &[("command", &self.command)])];
        for d in declared.iter() {
            if player.is_some_and(|p| d.action.allow.as_ref().is_some_and(|allow| !allow(p))) {
                continue;
            }
            lines.extend(self.usage(&d.action, language));
            if !d.action.help.is_empty() {
                lines.push(format!("  {}", self.say(&d.action.help, language, &[])));
            }
        }
        lines.join("\n")
    }

    /// The chat form: `/<command> help`, `/<command> <id> ...` or a list's
    /// verb.
    fn chat(&self, ctx: &CommandContext, args: &[&str]) -> Reply {
        let language = self.chat_language();
        let Some((&word, rest)) = args.split_first() else {
            return self.help(ctx.player.as_ref(), language).into();
        };
        let word = word.to_lowercase();
        if word == "help" {
            return self.help(ctx.player.as_ref(), language).into();
        }
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        let found = declared.iter().find_map(|d| {
            if d.action.id == word {
                return Some((d, None));
            }
            let (on, off) = d.action.verbs.as_ref()?;
            (*on == word)
                .then_some((d, Some(true)))
                .or_else(|| (*off == word).then_some((d, Some(false))))
        });
        let Some((d, verb)) = found else {
            return self
                .say(
                    "actions.unknown",
                    language,
                    &[("word", &word), ("command", &self.command)],
                )
                .into();
        };
        let Some(player) = &ctx.player else {
            return self.say("actions.console", language, &[]).into();
        };
        let outcome = match self.chat_args(d, player, rest, verb, language) {
            Ok(values) => self.run(d, player, &values, language, Via::Chat),
            Err(why) => Outcome::refused(why),
        };
        outcome.text.into()
    }

    /// The values of the words after an action's id in chat.
    fn chat_args(
        &self,
        d: &Declared,
        player: &Player,
        words: &[&str],
        verb: Option<bool>,
        language: Language,
    ) -> Result<Vec<Value>, String> {
        let bad = || {
            self.say(
                "actions.bad",
                language,
                &[("usage", &self.usage(&d.action, language).join(", "))],
            )
        };
        if d.action.on.is_some() {
            let (name, on) = if let Some(on) = verb {
                (words.join(" "), on)
            } else {
                let (last, name) = words.split_last().ok_or_else(bad)?;
                let on = toggle_word(last, &self.texts, language).ok_or_else(bad)?;
                (name.join(" "), on)
            };
            if name.is_empty() {
                return Err(bad());
            }
            let listed = self.listed(d, player, language);
            let target = self.player_named(&name, &listed, language)?;
            return Ok(vec![target, Value::Toggle(on)]);
        }
        let mut values = Vec::with_capacity(d.action.args.len());
        let mut rest = words;
        for (i, arg) in d.action.args.iter().enumerate() {
            let last = i + 1 == d.action.args.len();
            let value = if arg.kind == ArgView::PLAYER {
                let taken = if last { rest.len() } else { 1 };
                if rest.len() < taken || taken == 0 {
                    return Err(bad());
                }
                let name = rest[..taken].join(" ");
                rest = &rest[taken..];
                self.player_named(&name, &[], language)?
            } else {
                let (&word, tail) = rest.split_first().ok_or_else(bad)?;
                rest = tail;
                match arg.kind {
                    ArgView::CHOICE => Value::Choice(
                        arg.choices
                            .iter()
                            .position(|(w, _)| w.eq_ignore_ascii_case(word))
                            .or_else(|| {
                                word.parse::<usize>()
                                    .ok()
                                    .and_then(|n| n.checked_sub(1))
                                    .filter(|&n| n < arg.choices.len())
                            })
                            .ok_or_else(bad)?,
                    ),
                    ArgView::NUMBER => Value::Number(
                        word.replace(',', ".")
                            .parse::<f64>()
                            .ok()
                            .filter(|n| n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1)
                            .ok_or_else(bad)?,
                    ),
                    _ => Value::Toggle(toggle_word(word, &self.texts, language).ok_or_else(bad)?),
                }
            };
            values.push(value);
        }
        if !rest.is_empty() {
            return Err(bad());
        }
        Ok(values)
    }

    /// The one player named `name` among those online and `listed`.
    fn player_named(
        &self,
        name: &str,
        listed: &[(u64, String)],
        language: Language,
    ) -> Result<Value, String> {
        let lower = name.to_lowercase();
        let mut found: Vec<(u64, String)> = self
            .online()
            .into_iter()
            .filter(|p| p.is_named(name))
            .map(|p| (p.key, p.name))
            .collect();
        for (key, listed_name) in listed {
            if listed_name.to_lowercase() == lower && !found.iter().any(|(k, _)| k == key) {
                found.push((*key, listed_name.clone()));
            }
        }
        match found.as_slice() {
            [(key, name)] => Ok(Value::Player {
                key: *key,
                name: name.clone(),
            }),
            [] => Err(self.say("actions.no_player", language, &[("name", name)])),
            _ => Err(self.say("actions.several", language, &[("name", name)])),
        }
    }

    /// The players a list switches on for `player` now; none while the
    /// action has nothing at hand.
    fn listed(&self, d: &Declared, player: &Player, language: Language) -> Vec<(u64, String)> {
        let ask = self.ask(player, language);
        let subject = match d.action.at.as_ref().map(|at| at(&ask)) {
            Some(Ok(subject)) => Some(subject),
            Some(Err(_)) => return Vec::new(),
            None => None,
        };
        d.action
            .on
            .as_ref()
            .map(|on| on(&ask, subject.as_ref()))
            .unwrap_or_default()
    }

    fn allowed(d: &Declared, player: &Player) -> bool {
        d.action.allow.as_ref().is_none_or(|allow| allow(player))
    }

    /// Runs `d` for `player` with `values` after the checks.
    fn run(
        &self,
        d: &Declared,
        player: &Player,
        values: &[Value],
        language: Language,
        via: Via,
    ) -> Outcome {
        if !Self::allowed(d, player) {
            return Outcome::refused(self.say("actions.denied", language, &[]));
        }
        let ask = self.ask(player, language);
        let subject = match d.action.at.as_ref().map(|at| at(&ask)) {
            Some(Ok(subject)) => Some(subject),
            Some(Err(why)) => return Outcome::refused(why),
            None => None,
        };
        let shape_fits = if d.action.on.is_some() {
            matches!(values, [Value::Player { .. }, Value::Toggle(_)])
        } else {
            values.len() == d.action.args.len()
                && values.iter().zip(&d.action.args).all(|(v, a)| fits(v, a))
        };
        if !shape_fits {
            return Outcome::refused(self.say(
                "actions.bad",
                language,
                &[("usage", &self.usage(&d.action, language).join(", "))],
            ));
        }
        (d.handler)(&Request {
            player,
            subject: subject.as_ref(),
            args: values,
            language,
            via,
            texts: &self.texts,
        })
    }

    /// The sheet of `player`: every action they may use, in `language`.
    fn sheet(&self, player: &Player, language: Language) -> String {
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        let online = self.online();
        let others: Vec<(u64, String)> = online
            .iter()
            .filter(|p| p.key != player.key)
            .map(|p| (p.key, p.name.clone()))
            .take(MAX_PLAYERS)
            .collect();
        let mut entries = Vec::new();
        for d in declared.iter().filter(|d| Self::allowed(d, player)) {
            let ask = self.ask(player, language);
            let subject = d.action.at.as_ref().map(|at| at(&ask));
            let (offered, subject_text) = match &subject {
                Some(Ok(subject)) => (true, subject.label.clone()),
                Some(Err(why)) => (false, why.clone()),
                None => (true, String::new()),
            };
            let mut entry = Entry {
                id: d.action.id.clone(),
                label: self.say(&d.action.label, language, &[]),
                help: if d.action.help.is_empty() {
                    String::new()
                } else {
                    self.say(&d.action.help, language, &[])
                },
                per_player: d.action.on.is_some(),
                hint: d.action.hint,
                offered,
                subject: subject_text,
                args: Vec::new(),
                players: Vec::new(),
            };
            if let Some(on) = &d.action.on {
                let switched = if offered {
                    on(&ask, subject.as_ref().and_then(|s| s.as_ref().ok()))
                } else {
                    Vec::new()
                };
                let mut players: Vec<(u64, String, bool)> = others
                    .iter()
                    .map(|(key, name)| (*key, name.clone(), switched.iter().any(|(k, _)| k == key)))
                    .collect();
                for (key, name) in &switched {
                    if *key != player.key && !players.iter().any(|(k, _, _)| k == key) {
                        players.push((*key, name.clone(), true));
                    }
                }
                players.sort_by_key(|(_, name, _)| name.to_lowercase());
                players.truncate(MAX_PLAYERS);
                entry.players = players;
            }
            for arg in &d.action.args {
                entry.args.push(ArgView {
                    kind: arg.kind,
                    label: self.say(&arg.label, language, &[]),
                    min: arg.range.0,
                    max: arg.range.1,
                    step: arg.step,
                    choices: arg
                        .choices
                        .iter()
                        .map(|(_, label)| self.say(label, language, &[]))
                        .collect(),
                    players: if arg.kind == ArgView::PLAYER {
                        others.clone()
                    } else {
                        Vec::new()
                    },
                });
            }
            entries.push(entry);
        }
        sheet::encode(&entries)
    }

    /// Runs action `id` for `player` from the interface; `1` or `0` and the
    /// text.
    fn run_from_interface(
        &self,
        player: &Player,
        id: &str,
        args: &str,
        language: Language,
    ) -> String {
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        let outcome = match declared.iter().find(|d| d.action.id == id) {
            None => Outcome::refused(self.say(
                "actions.unknown",
                language,
                &[("word", id), ("command", &self.command)],
            )),
            Some(d) => match sheet::decode_args(args) {
                None => Outcome::refused(self.say("actions.denied", language, &[])),
                Some(mut values) => match self.name_players(d, player, &mut values, language) {
                    Ok(()) => self.run(d, player, &values, language, Via::Interface),
                    Err(why) => Outcome::refused(why),
                },
            },
        };
        format!("{}{}", u8::from(outcome.done), outcome.text)
    }

    /// Fills in the names of the players in `values`, which the interface
    /// sends by key: online, or on the list.
    fn name_players(
        &self,
        d: &Declared,
        player: &Player,
        values: &mut [Value],
        language: Language,
    ) -> Result<(), String> {
        if !values.iter().any(|v| matches!(v, Value::Player { .. })) {
            return Ok(());
        }
        let online = self.online();
        let listed = if d.action.on.is_some() {
            self.listed(d, player, language)
        } else {
            Vec::new()
        };
        for value in values.iter_mut() {
            if let Value::Player { key, name } = value {
                let known = online
                    .iter()
                    .find(|p| p.key == *key)
                    .map(|p| p.name.clone())
                    .or_else(|| {
                        listed
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, n)| n.clone())
                    });
                *name = known.ok_or_else(|| {
                    self.say(
                        "actions.no_player",
                        language,
                        &[("name", &format!("{key:016x}"))],
                    )
                })?;
            }
        }
        Ok(())
    }

    /// The actions with a hint that `player` has at hand.
    fn hints(&self, player: &Player, language: Language) -> String {
        let declared = self.declared.read().unwrap_or_else(PoisonError::into_inner);
        let mut out = String::new();
        for d in declared
            .iter()
            .filter(|d| d.action.hint && Self::allowed(d, player))
        {
            let ask = self.ask(player, language);
            if let Some(Ok(subject)) = d.action.at.as_ref().map(|at| at(&ask)) {
                out.push_str(&sheet::clean(&d.action.id));
                out.push('\t');
                out.push_str(&sheet::clean(&self.say(&d.action.label, language, &[])));
                out.push('\t');
                out.push_str(&sheet::clean(&subject.label));
                out.push('\n');
            }
        }
        out
    }
}

/// Whether `value` fits `arg`'s type and range.
fn fits(value: &Value, arg: &Arg) -> bool {
    match (value, arg.kind) {
        (Value::Player { .. }, ArgView::PLAYER) | (Value::Toggle(_), ArgView::TOGGLE) => true,
        (Value::Choice(index), ArgView::CHOICE) => *index < arg.choices.len(),
        (Value::Number(n), ArgView::NUMBER) => {
            n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1
        }
        _ => false,
    }
}

/// On or off, from a word typed in chat.
fn toggle_word(word: &str, texts: &Texts, language: Language) -> Option<bool> {
    let word = word.to_lowercase();
    let on = texts.fill("actions.on", language, &[]).to_lowercase();
    let off = texts.fill("actions.off", language, &[]).to_lowercase();
    if word == on || ["on", "yes", "true", "1", "allow"].contains(&word.as_str()) {
        Some(true)
    } else if word == off || ["off", "no", "false", "0", "deny"].contains(&word.as_str()) {
        Some(false)
    } else {
        None
    }
}

thread_local! {
    /// The last reply, alive until the next question on this thread.
    static REPLY: RefCell<String> = const { RefCell::new(String::new()) };
}

unsafe extern "system" fn answer(call: *const ActionCall, user: *mut c_void, out: *mut Str) {
    // SAFETY: the loader passes a valid call for the duration of this one;
    // every field is there since the minor that added actions.
    let Some(call) = (unsafe { read_headed(call, size_of::<ActionCall>()) }) else {
        return;
    };
    // SAFETY: `user` is the leaked `Arc<Registry>` this function was
    // registered with.
    let registry = unsafe { &*user.cast::<Arc<Registry>>() };
    let player = Player::from_info(&call.player);
    let language = Language(call.language);
    // SAFETY: the loader's text, valid for the call.
    let action = unsafe { call.action.as_str() }.unwrap_or("");
    // SAFETY: as above.
    let args = unsafe { call.args.as_str() }.unwrap_or("");
    let reply = crate::host::guard("panic in an action", String::new, || match call.kind {
        ActionCall::SHEET => registry.sheet(&player, language),
        ActionCall::RUN => registry.run_from_interface(&player, action, args, language),
        ActionCall::HINTS => registry.hints(&player, language),
        _ => String::new(),
    });
    if out.is_null() {
        return;
    }
    REPLY.with_borrow_mut(|kept| {
        *kept = reply;
        // SAFETY: the loader's out pointer; the text lives in this thread's
        // buffer until the next question, after the loader copied it.
        unsafe { out.write(Str::new(kept)) };
    });
}

impl Load<'_> {
    /// The mod's player actions, answered in chat as `/<command>`: declare
    /// each with [`Actions::add`]. `texts` holds the labels' texts, as
    /// [`Load::texts`] reads them; the platform's own replies are in it
    /// under `actions.*` too, so a mod may change them.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed or taken command name, or a second call.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    pub fn actions(&mut self, command: &str, texts: Texts) -> Result<Actions, Off> {
        let mut all = Texts::parse(PLATFORM_TEXTS)?;
        all.merge(texts);
        let host = self.host();
        let registry = Arc::new(Registry {
            command: command.to_owned(),
            texts: all,
            language: AtomicU32::new(Language::FALLBACK.0),
            host,
            declared: RwLock::new(Vec::new()),
        });
        let actions = Actions {
            registry: Arc::clone(&registry),
        };
        crate::load::hand_over(
            Arc::clone(&registry),
            |user| (host.api().actions)(host.handle(), answer, user),
            "actions",
        )?;
        let chat = actions.clone();
        let help = format!("actions; /{command} help lists them");
        self.add_command(command, &help, move |ctx, args| chat.chat(ctx, args))?;
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
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
