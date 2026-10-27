use core::ops::RangeInclusive;

use crate::PlayerKey;
use crate::i18n::{Language, Texts};
use crate::world::Player;

use super::sheet::ArgView;

/// A value the player picked for an argument, in the order of
/// [`Action::arg`](crate::actions::Action::arg); a [list](crate::actions::Action::per_player) hands the player and the
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
}

/// An argument of an action: a typed choice, never free text.
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

    /// One of the server's players online now, picked from a list; named in
    /// a console line.
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

/// What an action acts on for a player, from [`Action::at`](crate::actions::Action::at): an id the
/// handler gets back and a label the player reads.
///
/// # Examples
///
/// ```
/// use enshwick::actions::Subject;
///
/// let locker = Subject::new(42, "Your locker at (2, 10, -4)");
/// assert_eq!(locker.id, 42);
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
    pub(super) texts: &'a Texts,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Via {
    /// The game's interface: the reply shows as a notice.
    Interface,
    /// The server's [console](crate::console), such as a dedicated server's
    /// command file: the reply is the console's answer, and the player an
    /// administrator with no character.
    Console,
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
    /// What the action acts on, from [`Action::at`](crate::actions::Action::at); found again for this
    /// run.
    pub subject: Option<&'a Subject>,
    /// The values picked, in the order of the arguments.
    pub args: &'a [Value],
    /// The language the player reads.
    pub language: Language,
    /// Where it came from.
    pub via: Via,
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
