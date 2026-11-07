//! Player actions as a script declares them: the namespace `actions`, a
//! mod's [`Actions`] as an object, and the [`Ask`] and [`Request`] its
//! functions get as objects whose methods read the Rust fields.
//!
//! An action is a table of the builder's fields, and `at` answers
//! `{ Ok = subject }` or `{ Err = reason }`, as the Rust `Result` does.

use std::sync::Arc;

use super::registry::Registry;
use super::{
    Action, ActionTarget, Actions, Arg, Ask, FindSubjectError, Outcome, Request, Subject, Value,
    Via,
};
use crate::i18n::{Language, Texts};
use crate::script::{
    self, Answer, Bound, Callback, Class, Error, FromScript, Function, IntoScript, Object, Param,
    Signature, Ty, Typed,
};
use crate::world::Player;
use crate::{Load, Off, PlayerKey};

/// An action as a script declares it: the fields of [`Action`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "PlayerAction")]
pub(crate) struct ScriptAction {
    /// The action's id, 1 to 32 of `[a-z0-9-]`, as [`Action::new`] takes it.
    pub id: String,
    /// The button's label, a text key.
    pub label: String,
    /// A line on what the action does, a text key, as [`Action::help`] sets it.
    #[script(default)]
    pub help: String,
    /// The arguments in order, as [`Action::arg`] adds them; a list takes none.
    #[script(default)]
    pub args: Vec<Arg>,
    /// Whether only the server's administrators see and run it, as
    /// [`Action::admins`] sets it.
    #[script(default)]
    pub admins: bool,
    /// Who sees and runs it, as [`Action::allow`] decides; only `true`
    /// allows, and with `admins` both must hold.
    pub allow: Option<AllowFn>,
    /// What it acts on for a player, as [`Action::at`] finds it.
    pub at: Option<AtFn>,
    /// The players of a list, as [`Action::per_player`] names them; such an
    /// action is a list of players with a switch each.
    pub per_player: Option<PerPlayerFn>,
}

impl ScriptAction {
    /// The Rust action, its functions calling the script's.
    fn into_action(self, registry: &Arc<Registry>) -> Action {
        let mut action = match self.per_player {
            Some(list) => {
                let registry = Arc::clone(registry);
                Action::per_player(&self.id, &self.label, move |ask, subject| {
                    list.players(ScriptAsk::of(ask, &registry), subject.cloned())
                })
            }
            None => Action::new(&self.id, &self.label),
        };
        if !self.help.is_empty() {
            action = action.help(&self.help);
        }
        for arg in self.args {
            action = action.arg(arg);
        }
        let admins = self.admins;
        match self.allow {
            Some(allow) => {
                action = action
                    .allow(move |player| (!admins || player.is_admin()) && allow.allows(player));
            }
            None if admins => action = action.admins(),
            None => {}
        }
        if let Some(at) = self.at {
            let registry = Arc::clone(registry);
            action = action.at(move |ask| at.find(ScriptAsk::of(ask, &registry)));
        }
        action
    }
}

/// A script's `allow`: whether a player sees and runs the action.
#[derive(Clone, Debug)]
pub(crate) struct AllowFn(Callback);

impl AllowFn {
    /// Only `true` allows: nil, any other value or an error refuses, so a gate
    /// fails closed.
    fn allows(&self, player: &Player) -> bool {
        matches!(
            self.0.call(&mut [player.clone().into_value()]),
            Ok(script::Value::Bool(true))
        )
    }
}

impl Typed for AllowFn {
    const TY: &'static Ty = &Ty::Function(&Signature::new(
        &[Param::new("player", <Player as Typed>::TY)],
        <bool as Answer>::TY,
    ));
}

/// A script's `at`: what the action acts on for a player.
#[derive(Clone, Debug)]
pub(crate) struct AtFn(Callback);

impl AtFn {
    fn find(&self, ask: ScriptAsk) -> Result<Subject, FindSubjectError> {
        match Option::<Found>::from_answer(self.0.call(&mut [ask.into_value()])) {
            Some(Found::Ok(subject)) => Ok(subject),
            Some(Found::Err(reason)) => Err(reason.into()),
            None => Err(FindSubjectError::from("")),
        }
    }
}

impl Typed for AtFn {
    const TY: &'static Ty = &Ty::Function(&Signature::new(
        &[Param::new("ask", <ScriptAsk as Typed>::TY)],
        <Option<Found> as Answer>::TY,
    ));
}

/// A script's `per_player`: the players of a list and their switches.
#[derive(Clone, Debug)]
pub(crate) struct PerPlayerFn(Callback);

impl PerPlayerFn {
    fn players(&self, ask: ScriptAsk, subject: Option<Subject>) -> Vec<(PlayerKey, String)> {
        let answer = self.0.call(&mut [ask.into_value(), subject.into_value()]);
        Option::<Vec<Listed>>::from_answer(answer)
            .unwrap_or_default()
            .into_iter()
            .map(|listed| (listed.key, listed.name))
            .collect()
    }
}

impl Typed for PerPlayerFn {
    const TY: &'static Ty = &Ty::Function(&Signature::new(
        &[
            Param::new("ask", <ScriptAsk as Typed>::TY),
            Param::new("subject", <Option<Subject> as Typed>::TY),
        ],
        <Option<Vec<Listed>> as Answer>::TY,
    ));
}

macro_rules! function_fields {
    ($($t:ty),*) => {$(
        impl IntoScript for $t {
            fn into_value(self) -> script::Value {
                script::Value::Function(self.0)
            }
        }

        impl FromScript for $t {
            fn from_value(value: script::Value) -> Result<Self, Error> {
                Callback::from_value(value).map(Self)
            }
        }
    )*};
}

function_fields!(AllowFn, AtFn, PerPlayerFn);

/// What a script's `at` found, as the `Result` of [`Action::at`]: the
/// subject, or the reason the player reads beside the dimmed action.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "FoundSubject")]
enum Found {
    /// The action acts on this.
    Ok(Subject),
    /// Nothing to act on, and why, in the player's language.
    Err(String),
}

/// A player a list shows, with the switch on.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "ListedPlayer")]
struct Listed {
    /// The account key, as [`Player::key`].
    pub key: PlayerKey,
    /// The name the list shows.
    pub name: String,
}

/// Who asks, as a script's `at` and `per_player` get it: [`Ask`]'s fields
/// as methods, and its `say`.
#[derive(Clone)]
pub(crate) struct ScriptAsk {
    player: Player,
    language: Language,
    target: Option<ActionTarget>,
    registry: Arc<Registry>,
}

impl ScriptAsk {
    fn of(ask: &Ask<'_>, registry: &Arc<Registry>) -> Self {
        Self {
            player: ask.player.clone(),
            language: ask.language,
            target: ask.target,
            registry: Arc::clone(registry),
        }
    }
}

impl Object for ScriptAsk {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Ask",
        "Who asks about an action, for its `at` and a list's players.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_ASK];
}

#[crate::script(ask)]
impl ScriptAsk {
    /// The player, as [`Ask::player`] holds it.
    #[script(name = "player")]
    pub(crate) fn player_script(&self) -> Player {
        self.player.clone()
    }

    /// The language they read, as [`Ask::language`] holds it.
    #[script(name = "language")]
    pub(crate) fn language_script(&self) -> Language {
        self.language
    }

    /// The object the client named for this run, as [`Ask::target`] holds
    /// it.
    #[script(name = "target")]
    pub(crate) fn target_script(&self) -> Option<ActionTarget> {
        self.target
    }

    /// The mod's text `key` in the player's language, filled with `values`,
    /// a list of `{ name, value }`, as [`Ask::say`] fills it.
    #[script(name = "say")]
    pub(crate) fn say_script(&self, key: &str, values: &[(String, String)]) -> String {
        say(&self.registry.texts, key, self.language, values)
    }
}

/// A player's request, as a script's handler gets it: [`Request`]'s fields
/// as methods, and its `say`.
#[derive(Clone)]
pub(crate) struct ScriptRequest {
    player: Player,
    subject: Option<Subject>,
    args: Vec<Value>,
    language: Language,
    via: Via,
    target: Option<ActionTarget>,
    registry: Arc<Registry>,
}

impl Object for ScriptRequest {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Request",
        "A player's request to run an action, handed to its handler on the server.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_REQUEST];
}

#[crate::script(request)]
impl ScriptRequest {
    /// Who asks, as [`Request::player`] holds it.
    #[script(name = "player")]
    pub(crate) fn player_script(&self) -> Player {
        self.player.clone()
    }

    /// What the action acts on, as [`Request::subject`] holds it.
    #[script(name = "subject")]
    pub(crate) fn subject_script(&self) -> Option<Subject> {
        self.subject.clone()
    }

    /// The values picked, in the order of the arguments, as
    /// [`Request::args`] holds them; a choice is its index from 0.
    #[script(name = "args")]
    pub(crate) fn args_script(&self) -> Vec<Value> {
        self.args.clone()
    }

    /// The language the player reads, as [`Request::language`] holds it.
    #[script(name = "language")]
    pub(crate) fn language_script(&self) -> Language {
        self.language
    }

    /// Where it came from, as [`Request::via`] holds it.
    #[script(name = "via")]
    pub(crate) fn via_script(&self) -> Via {
        self.via
    }

    /// The object the client named, as [`Request::target`] holds it.
    #[script(name = "target")]
    pub(crate) fn target_script(&self) -> Option<ActionTarget> {
        self.target
    }

    /// The mod's text `key` in the player's language, filled with `values`,
    /// a list of `{ name, value }`, as [`Request::say`] fills it.
    #[script(name = "say")]
    pub(crate) fn say_script(&self, key: &str, values: &[(String, String)]) -> String {
        say(&self.registry.texts, key, self.language, values)
    }
}

fn say(texts: &Texts, key: &str, language: Language, values: &[(String, String)]) -> String {
    let values: Vec<(&str, &str)> = values
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    texts.fill(key, language, &values)
}

/// How a request went, as a script's handler answers it: [`Outcome`]'s
/// parts as a table.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "ActionOutcome")]
pub(crate) struct ScriptOutcome {
    /// Whether the action was done, as [`Outcome::is_done`] says.
    pub done: bool,
    /// The text the player reads, as [`Outcome::text`] gives it.
    pub text: String,
}

/// A handler's answer, from `enshwick.actions.done` or `refused`: nil is
/// done without a text, and an error refused, as the runtime turns the mod
/// off.
impl Answer for Outcome {
    const TY: &'static Ty = &Ty::Optional(<ScriptOutcome as Typed>::TY);

    fn from_answer(answer: Result<script::Value, Error>) -> Self {
        match answer.and_then(Option::<ScriptOutcome>::from_value) {
            Ok(None) => Self::done(""),
            Ok(Some(outcome)) => Self {
                done: outcome.done,
                text: outcome.text,
            },
            Err(_) => Self::refused(""),
        }
    }
}

impl Object for Arg {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Arg",
        "An argument of an action: a typed choice, never free text.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[];
}

impl Object for Actions {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Actions",
        "A mod's player actions, declared once and run from its controls, panels or the console.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_ACTIONS];
}

#[crate::script(actions)]
impl Actions {
    /// Declares `action` with its `handler`, which runs on the server's game
    /// thread, as [`Actions::add`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::add`].
    #[script(name = "add")]
    pub(crate) fn add_script(
        &self,
        action: ScriptAction,
        handler: impl Fn(ScriptRequest) -> Outcome + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let registry = Arc::clone(&self.registry);
        let action = action.into_action(&self.registry);
        self.add(action, move |request: &Request<'_>| {
            handler(ScriptRequest {
                player: request.player.clone(),
                subject: request.subject.cloned(),
                args: request.args.to_vec(),
                language: request.language,
                via: request.via,
                target: request.target,
                registry: Arc::clone(&registry),
            })
        })
    }

    /// On a client, asks the server to run this mod's action `action` with
    /// `args`, as [`Actions::run`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::run`].
    #[script(name = "run")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn run_script(&self, action: &str, args: Vec<Value>) -> Result<(), Off> {
        self.run(action, &args)
    }

    /// On a client, asks the server to run this mod's action `action` with
    /// `args` on the object `target`, as [`Actions::run_at`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::run_at`].
    #[script(name = "run_at")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn run_at_script(
        &self,
        action: &str,
        args: Vec<Value>,
        target: ActionTarget,
    ) -> Result<(), Off> {
        self.run_at(action, &args, target)
    }

    /// Puts the declared action `action` on `key`, as [`Actions::control`]
    /// does.
    ///
    /// # Errors
    ///
    /// As [`Actions::control`].
    #[script(name = "control")]
    pub(crate) fn control_script(
        &self,
        action: &str,
        key: crate::controls::Key,
    ) -> Result<(), Off> {
        self.control(action, key)
    }
}

/// The free functions of [`actions`](super) as a script's namespace
/// `actions`: the arguments, subjects and outcomes an action takes.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct ActionFns;

#[crate::script(action_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl ActionFns {
    /// One of the server's players online now, as [`Arg::player`] makes it.
    #[script]
    #[must_use]
    pub fn player(self, label: &str) -> Arg {
        Arg::player(label)
    }

    /// One of `choices`, each `{ word, label }`, as [`Arg::choice`] makes it.
    #[script]
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub fn choice(self, label: &str, choices: Vec<(String, String)>) -> Arg {
        let choices: Vec<(&str, &str)> = choices
            .iter()
            .map(|(word, label)| (word.as_str(), label.as_str()))
            .collect();
        Arg::choice(label, &choices)
    }

    /// A number from `min` to `max` in steps of `step`, 0 for any value, as
    /// [`Arg::number`] makes it from the range `min..=max`.
    #[script]
    #[must_use]
    pub fn number(self, label: &str, min: f64, max: f64, step: f64) -> Arg {
        Arg::number(label, min..=max, step)
    }

    /// On or off, as [`Arg::toggle`] makes it.
    #[script]
    #[must_use]
    pub fn toggle(self, label: &str) -> Arg {
        Arg::toggle(label)
    }

    /// What an action acts on, as [`Subject::new`] makes it.
    #[script]
    #[must_use]
    pub fn subject(self, id: u64, label: String) -> Subject {
        Subject::new(id, label)
    }

    /// An action done, with the text the player reads, as
    /// [`Outcome::done`] makes it.
    #[script]
    #[must_use]
    pub(crate) fn done(self, text: String) -> ScriptOutcome {
        ScriptOutcome { done: true, text }
    }

    /// An action refused or failed, with the text the player reads, as
    /// [`Outcome::refused`] makes it.
    #[script]
    #[must_use]
    pub(crate) fn refused(self, text: String) -> ScriptOutcome {
        ScriptOutcome { done: false, text }
    }
}

impl Bound for ActionFns {
    const NAME: &'static str = "actions";
    const DOC: &'static str = "Player actions: what a player asks a mod to do, declared once and run from its controls, panels or the console.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_ACTION_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

#[crate::script(actions)]
impl Load<'_> {
    /// The mod's player actions, with the labels' texts `texts`, as
    /// [`Load::actions`] gives them.
    ///
    /// # Errors
    ///
    /// As [`Load::actions`].
    #[script(name = "actions")]
    pub(crate) fn actions_script(&mut self, texts: Texts) -> Result<Actions, Off> {
        self.actions(texts)
    }
}
