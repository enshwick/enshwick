//! Player actions: what a player asks a mod to do, declared once and offered
//! in the game's interface.
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
//! - it runs the same action from the server's [console](crate::console),
//!   as the mod's id, the action's id and its values as words, such as
//!   `weather set storm 10` in a dedicated server's command file.
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

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::{Arc, RwLock};

use crate::PlayerKey;
use crate::abi::{ActionCall, Str, read_headed};
use crate::i18n::{Language, Texts};
use crate::world::Player;
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
"actions.bad" = "Write it so: {usage}"
"actions.player" = "player"
"actions.on" = "on"
"actions.off" = "off"

[uk]
"actions.usage" = "Дії, кожна після id мода:"
"actions.unknown" = "Немає дії {word}; help перелічує їх."
"actions.denied" = "Тобі цього не можна."
"actions.no_player" = "Немає гравця на ім'я {name}."
"actions.several" = "Кілька гравців мають ім'я {name}."
"actions.bad" = "Пиши так: {usage}"
"actions.player" = "гравець"
"actions.on" = "on"
"actions.off" = "off"
"#;

/// The longest action id.
pub const MAX_ID: usize = 32;

/// Players an action's list or a player argument shows at most.
pub const MAX_PLAYERS: usize = 32;

type Allow = Box<dyn Fn(&Player) -> bool + Send + Sync>;
type At = Box<dyn Fn(&Ask<'_>) -> Result<Subject, String> + Send + Sync>;
type On = Box<dyn Fn(&Ask<'_>, Option<&Subject>) -> Vec<(PlayerKey, String)> + Send + Sync>;
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
            hint: false,
            allow: None,
            at: None,
            on: None,
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
        if !is_valid_id(&id) || id == "help" {
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
        let mut declared = self.registry.declared.relock_write();
        if declared.iter().any(|d| d.action.id == id) {
            return Err(Off::new(format!("action {id}: the id is taken")));
        }
        declared.push(Declared {
            action,
            handler: Box::new(handler),
        });
        Ok(())
    }
}

thread_local! {
    /// The last reply, alive until the next question on this thread.
    static REPLY: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Answers one of the loader's questions about this mod's actions.
///
/// # Safety
///
/// - `call` is null or points at an [`ActionCall`] readable for the size its
///   header states, with no other writer during the call.
/// - `call`'s strings are null or point at their `len` bytes, alive and
///   unchanged for the call.
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
    let reply = crate::host::guard("panic in an action", String::new, || match call.kind {
        ActionCall::SHEET => registry.sheet(&player, language),
        ActionCall::RUN => registry.run_from_interface(&player, action, args, language),
        ActionCall::HINTS => registry.hints(&player, language),
        ActionCall::CONSOLE => {
            let outcome = registry.run_from_console(&player, action);
            format!("{}{}", u8::from(outcome.done), outcome.text)
        }
        _ => String::new(),
    });
    if out.is_null() {
        return;
    }
    REPLY.with_borrow_mut(|kept| {
        *kept = reply;
        // SAFETY: `out` is not null, as the early return ensures, and the
        // caller guarantees it is writable for one `Str`; the text lives in
        // this thread's buffer until the next question, after the loader read
        // it.
        unsafe { out.write(Str::new(kept)) };
    });
}

impl Load<'_> {
    /// The mod's player actions: declare each with [`Actions::add`]. `texts`
    /// holds the labels' texts, as [`Load::texts`] reads them; the
    /// platform's own replies are in it under `actions.*` too, so a mod may
    /// change them.
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
        });
        crate::load::hand_over(
            Arc::clone(&registry),
            |user| (host.api().actions)(host.handle(), answer, user),
            "actions",
        )?;
        Ok(Actions { registry })
    }
}

/// Whether `id` can name an action: 1 to 32 characters of `[a-z0-9-]`.
fn is_valid_id(id: &str) -> bool {
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

use registry::Registry;
pub use request::*;
