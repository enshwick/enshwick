use core::ops::RangeInclusive;

use crate::content::Guid;
use crate::i18n::{Language, Texts};
use crate::world::Player;
use crate::{Entity, PlayerKey};

use super::sheet::ArgView;

/// A value the player picked for an argument, in the order of
/// [`Action::arg`](crate::actions::Action::arg); a [list](crate::actions::Action::per_player) hands the player and the
/// switch.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::actions::Value;
///
/// fn minutes(args: &[Value]) -> Option<f64> {
///     args.iter().find_map(|v| if let Value::Number(n) = v { Some(*n) } else { None })
/// }
/// assert_eq!(minutes(&[Value::Toggle(true), Value::Number(5.0)]), Some(5.0));
/// ```
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[script(name = "ActionValue")]
#[non_exhaustive]
pub enum Value {
    /// One of the server's players: the account key, and the name as the
    /// server knows it.
    Player {
        /// The account key, as [`Player::key`].
        key: PlayerKey,
        /// The name, online or as the mod's list keeps it.
        name: String,
    },
    /// The index of the picked option of [`Arg::choice`].
    Choice(usize),
    /// A number in the range of [`Arg::number`].
    Number(f64),
    /// On or off.
    Toggle(bool),
    /// The text typed for an [`Arg::text`], within its bytes and without a
    /// control character.
    Text(String),
    /// The item picked for an [`Arg::item`]: one the server's game knows,
    /// checked before the handler runs.
    Item(Guid),
}

/// An argument of an action: a typed choice, or a line of text of at most
/// [`MAX_TEXT`](crate::actions::MAX_TEXT) bytes.
///
/// # Examples
///
/// See the [module](crate::actions).
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub(super) kind: u8,
    pub(super) label: String,
    pub(super) choices: Vec<(String, String)>,
    pub(super) range: (f64, f64),
    pub(super) step: f64,
    /// The value a typed line that leaves the argument out gives it.
    pub(super) default: Option<Value>,
}

impl Arg {
    fn new(kind: u8, label: &str) -> Self {
        Self {
            kind,
            label: label.to_owned(),
            choices: Vec::new(),
            range: (0.0, 0.0),
            step: 0.0,
            default: None,
        }
    }

    /// Lets a typed line leave the argument out: it then takes `default`.
    /// Only the last arguments of an action may be optional, and never a
    /// [player](Self::player); `default` is a value the argument takes, or
    /// an empty text. A text followed only by optional arguments takes the
    /// rest of the line up to them: the last words go to the optional
    /// arguments, in order, as far as each reads as its argument, so with
    /// an item and an optional count `Stone Axe 5` is the item `Stone Axe`
    /// five times and `Stone Axe` is one. A text in quotes keeps every word
    /// in it. The help shows the argument in square brackets.
    ///
    /// An optional text is always the last argument and takes what is left
    /// of the line itself: the arguments before it then take one word
    /// each, so with a player, optional minutes and an optional reason
    /// `Ann 5 spamming a lot` is five minutes for spamming a lot, and
    /// `Ann spamming` is the default minutes.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Arg, Value};
    ///
    /// let give = Action::new("give", "Give an item")
    ///     .arg(Arg::player("Player"))
    ///     .arg(Arg::text("Item", 64))
    ///     .arg(Arg::number("Count", 1.0..=999.0, 1.0).optional(Value::Number(1.0)));
    /// assert_eq!(give.id(), "give");
    /// ```
    #[must_use]
    pub fn optional(mut self, default: Value) -> Self {
        self.default = Some(default);
        self
    }

    /// One of the server's players online now, picked from a list; named in
    /// a console line.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let action = Action::new("invite", "Invite").arg(Arg::player("Who"));
    /// assert_eq!(action.id(), "invite");
    /// ```
    #[must_use]
    pub fn player(label: &str) -> Self {
        Self::new(ArgView::PLAYER, label)
    }

    /// One of `choices`, each a word for console lines and a label key.
    ///
    /// # Examples
    ///
    /// See the [module](crate::actions).
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
    /// See the [module](crate::actions).
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
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let action = Action::new("pvp", "Fights between players").arg(Arg::toggle("On"));
    /// assert_eq!(action.id(), "pvp");
    /// ```
    #[must_use]
    pub fn toggle(label: &str) -> Self {
        Self::new(ArgView::TOGGLE, label)
    }

    /// A line of text of 1 to `max_bytes` bytes of UTF-8, at most
    /// [`MAX_TEXT`](crate::actions::MAX_TEXT), such as a ban reason or an
    /// announcement. On the console the last text argument takes the rest
    /// of the line; one before it takes one word or a `"quoted run"`. The
    /// server refuses a longer text and any control character.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Arg};
    ///
    /// let say = Action::new("say", "Announce").admins().arg(Arg::text("Message", 200));
    /// assert_eq!(say.id(), "say");
    /// ```
    #[must_use]
    pub fn text(label: &str, max_bytes: usize) -> Self {
        let max = u16::try_from(max_bytes.clamp(1, super::MAX_TEXT)).unwrap_or(u16::MAX);
        let mut arg = Self::new(ArgView::TEXT, label);
        arg.range = (1.0, f64::from(max));
        arg
    }

    /// One of the game's items, vanilla or a mod's. A window lists the
    /// vanilla items by the names the game shows in the player's language,
    /// searched by typing, and sends the server the picked item; the names
    /// come from the client's own game, so the server needs none. A console
    /// line names any item by its GUID or content id, and a vanilla item
    /// also by its debug name or, where the server's game has its texts as
    /// in a game a player hosts, by a name the game shows; it takes words
    /// as a [text](Self::text) does, and an optional item, like an optional
    /// text, is the last argument. The server refuses an item its game does
    /// not know before the handler runs.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::{Action, Arg, Outcome, Value};
    ///
    /// let spawn = Action::new("spawn", "Spawn an item").admins().arg(Arg::item("Item"));
    /// assert_eq!(spawn.id(), "spawn");
    /// let picked = |args: &[Value]| match args {
    ///     [Value::Item(item)] => Outcome::done(format!("spawned {item}")),
    ///     _ => Outcome::refused("no item"),
    /// };
    /// assert!(!picked(&[]).is_done());
    /// ```
    #[must_use]
    pub fn item(label: &str) -> Self {
        Self {
            kind: ArgView::ITEM,
            ..Self::text(label, super::MAX_TEXT)
        }
    }

    /// Whether `text` fits this text argument: its bytes within the range
    /// and no control character.
    pub(super) fn takes_text(&self, text: &str) -> bool {
        u16::try_from(text.len())
            .is_ok_and(|len| f64::from(len) >= self.range.0 && f64::from(len) <= self.range.1)
            && !text.chars().any(char::is_control)
    }
}

/// What an action acts on for a player, from [`Action::at`](crate::actions::Action::at): an id the
/// handler gets back and a label the player reads.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::actions::Subject;
///
/// let locker = Subject::new(42, "Your locker at (2, 10, -4)");
/// assert_eq!(locker.id, 42);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
pub struct Subject {
    /// The mod's own id for it, such as a packed position.
    pub id: u64,
    /// What the player reads, in their language.
    pub label: String,
}

/// Why an action has nothing to act on for a player, from
/// [`Action::at`](crate::actions::Action::at): the reason the player reads
/// beside the dimmed action, in their language.
///
/// A `&str` or `String` turns into one with `into`.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::actions::FindSubjectError;
///
/// let none: FindSubjectError = "No gate near you".into();
/// assert_eq!(none.reason(), "No gate near you");
/// assert_eq!(none.to_string(), "nothing to act on: No gate near you");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("nothing to act on: {reason}")]
pub struct FindSubjectError {
    reason: String,
}

impl FindSubjectError {
    /// What the player reads.
    ///
    /// # Examples
    ///
    /// See [`FindSubjectError`].
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl From<String> for FindSubjectError {
    fn from(reason: String) -> Self {
        Self { reason }
    }
}

impl From<&str> for FindSubjectError {
    fn from(reason: &str) -> Self {
        Self {
            reason: reason.to_owned(),
        }
    }
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

/// Who asks, for [`Action::at`](crate::actions::Action::at) and a list's players.
///
/// # Examples
///
/// See [`Action::at`](crate::actions::Action::at).
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    /// The player.
    pub player: &'a Player,
    /// The language they read: the game's on their client; the fallback
    /// for the console.
    pub language: Language,
    /// The object the player's client named for this run, from
    /// [`Actions::run_at`](crate::actions::Actions::run_at); `None` for a
    /// run without one, the console and the actions' list. The server
    /// checked it first: a named entity exists in its world and gives its
    /// own position, and the object stands within 12 metres of the player.
    /// Whether it is the kind of object the action takes and the player's
    /// to use is for `at` to check.
    pub target: Option<ActionTarget>,
    pub(super) texts: &'a Texts,
}

/// The object a run of an action names, such as the chest whose screen
/// holds the panel that ran it: its entity in the server's world, where it
/// stands, or both. The name is enshwick's.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::actions::ActionTarget;
///
/// let chest = ActionTarget::at([10.0, 2.0, -4.5]);
/// assert_eq!(chest.position, Some([10.0, 2.0, -4.5]));
/// assert_eq!(chest.entity, None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct ActionTarget {
    /// Its entity in the server's world, as the server sent it to the
    /// client; entity ids differ between the worlds of a server and its
    /// clients.
    #[script(default)]
    pub entity: Option<Entity>,
    /// Where it stands, in metres.
    #[script(default)]
    pub position: Option<[f64; 3]>,
}

impl ActionTarget {
    /// The object with the server's entity `entity`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// let chest = enshwick::actions::ActionTarget::entity(enshwick::Entity(4096));
    /// assert_eq!(chest.entity, Some(enshwick::Entity(4096)));
    /// ```
    #[must_use]
    pub const fn entity(entity: Entity) -> Self {
        Self {
            entity: Some(entity),
            position: None,
        }
    }

    /// The object standing at `position`, in metres.
    ///
    /// # Examples
    ///
    /// See [`ActionTarget`].
    #[must_use]
    pub const fn at(position: [f64; 3]) -> Self {
        Self {
            entity: None,
            position: Some(position),
        }
    }

    /// The same object, standing at `position` too, so the server can
    /// check either.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::actions::ActionTarget;
    ///
    /// let chest = ActionTarget::entity(enshwick::Entity(4096)).with_position([1.0, 2.0, 3.0]);
    /// assert_eq!(chest.position, Some([1.0, 2.0, 3.0]));
    /// ```
    #[must_use]
    pub const fn with_position(mut self, position: [f64; 3]) -> Self {
        self.position = Some(position);
        self
    }

    /// The object `raw` names; `None` for none.
    pub(crate) fn from_raw(raw: &crate::abi::ActionTarget) -> Option<Self> {
        use crate::abi::ActionTarget as Raw;
        let target = Self {
            entity: (raw.flags & Raw::ENTITY != 0).then_some(raw.entity),
            position: (raw.flags & Raw::POSITION != 0).then_some(raw.position),
        };
        (target != Self::default()).then_some(target)
    }

    /// The object as the loader takes it.
    pub(crate) fn to_raw(self) -> crate::abi::ActionTarget {
        use crate::abi::ActionTarget as Raw;
        Raw {
            flags: self.entity.map_or(0, |_| Raw::ENTITY)
                | self.position.map_or(0, |_| Raw::POSITION),
            entity: self.entity.unwrap_or(Entity(0)),
            position: self.position.unwrap_or_default(),
            ..Raw::NONE
        }
    }
}

impl Ask<'_> {
    /// The mod's text `key` in the player's language, filled as
    /// [`Texts::fill`] does.
    ///
    /// # Examples
    ///
    /// See [`Action::at`](crate::actions::Action::at).
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub enum Via {
    /// The game's interface: the reply shows as a notice.
    Interface,
    /// The server's [console](crate::console), such as a dedicated server's
    /// command file: the reply is the console's answer, and the player the
    /// console, with every permission and no character.
    Console,
    /// A [command](crate::commands) the player typed in chat: the reply
    /// goes to that player alone.
    Chat,
    /// A command another mod ran for a remote caller with
    /// [`commands::run`](crate::commands::run), such as a remote console's
    /// client: the player is that caller, with the permissions the mod gave
    /// it, its label as the name and no character.
    Remote,
}

/// A player's request to run an action, handed to its handler on the
/// server's game thread once the platform checked who may use it.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
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
    /// What the action acts on, from [`Action::at`](crate::actions::Action::at); found again for this
    /// run.
    pub subject: Option<&'a Subject>,
    /// The values picked, in the order of the arguments.
    pub args: &'a [Value],
    /// The language the player reads.
    pub language: Language,
    /// Where it came from.
    pub via: Via,
    /// The object the client named, as [`Ask::target`] says.
    pub target: Option<ActionTarget>,
    pub(super) texts: &'a Texts,
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

/// How a request went: the text the player reads as a notice, or the
/// console's answer.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::actions::Outcome;
///
/// let done: Outcome = "Done".into();
/// assert_eq!(done, Outcome::done("Done"));
/// assert!(!Outcome::refused("Not here").is_done());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub(super) done: bool,
    pub(super) text: String,
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
    /// # extern crate enshwick_core as enshwick;
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
