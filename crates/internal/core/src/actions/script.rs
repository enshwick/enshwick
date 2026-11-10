//! Player actions as a script declares them: the namespace `actions`, a
//! mod's [`Actions`] as an object, and the [`Ask`] and [`Request`] its
//! functions get as objects whose methods read the Rust fields.
//!
//! An action is a table of the builder's fields, and `at` answers
//! `{ Ok = subject }` or `{ Err = reason }`, as the Rust `Result` does.

use std::sync::Arc;

use super::registry::Registry;
use super::{
    Action, ActionTarget, Actions, Arg, Ask, FindSubjectError, Outcome, Request, Row, Subject,
    Value, Via, Window,
};
use crate::i18n::{Language, Texts};
use crate::script::{
    self, Answer, Bound, Callback, Class, Error, FromScript, Function, IntoScript, Object, Param,
    Signature, Ty, Typed,
};
use crate::world::{Player, PlayerPermissions};
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
    /// The group the help lists it under, a text key, as [`Action::group`]
    /// sets it.
    #[script(default)]
    pub group: String,
    /// Whether its players include the one who looks, as
    /// [`Action::with_self`] sets it.
    #[script(default)]
    pub with_self: bool,
    /// The arguments in order, as [`Action::arg`] adds them; a list takes none.
    #[script(default)]
    pub args: Vec<Arg>,
    /// Whether only the server's administrators see and run it, as
    /// [`Action::admins`] sets it.
    #[script(default)]
    pub admins: bool,
    /// The permissions a player needs, all of them, as [`Action::needs`]
    /// adds each.
    #[script(default)]
    pub needs: Vec<PlayerPermissions>,
    /// Who sees and runs it, as [`Action::allow`] decides; only `true`
    /// allows, and with `admins` both must hold.
    pub allow: Option<AllowFn>,
    /// What it acts on for a player, as [`Action::at`] finds it.
    pub at: Option<AtFn>,
    /// The players of a list, as [`Action::per_player`] names them; such an
    /// action is a list of players with a switch each.
    pub per_player: Option<PerPlayerFn>,
    /// Accounts the mod knows beyond the players online, as
    /// [`Action::known`] names them.
    pub known: Option<KnownFn>,
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
        action = action.help(&self.help).group(&self.group);
        if self.with_self {
            action = action.with_self();
        }
        for arg in self.args {
            action = action.arg(arg);
        }
        action = self.needs.into_iter().fold(action, Action::needs);
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
        if let Some(known) = self.known {
            let registry = Arc::clone(registry);
            action = action.known(move |ask| known.accounts(ScriptAsk::of(ask, &registry)));
        }
        action
    }
}

/// A window as a script shows it: the fields of [`Window`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "ActionWindow")]
pub(crate) struct ScriptWindow {
    /// The window's id, 1 to 32 of `[a-z0-9-]`, as [`Window::new`] takes it.
    pub id: String,
    /// The title, a text key.
    pub title: String,
    /// The items in order, as [`Window::text`], [`Window::row`] and
    /// [`Window::action`] add them.
    #[script(default)]
    pub items: Vec<WindowItem>,
}

/// An item of a script's window.
#[derive(Clone, Debug, crate::Script)]
pub(crate) enum WindowItem {
    /// A line, as [`Window::text`] adds it.
    Text(WindowText),
    /// A line with buttons, as [`Window::row`] adds it.
    Row(WindowRow),
    /// An action drawn whole, by id, as [`Window::action`] adds it.
    Action(String),
}

/// A text key and the values it is filled with.
#[derive(Clone, Debug, crate::Script)]
pub(crate) struct WindowText {
    /// The text key.
    pub key: String,
    /// The values, a list of `{ name, value }`.
    #[script(default)]
    pub values: Vec<(String, String)>,
}

/// A row of a script's window: the fields of [`Row`]'s builder.
#[derive(Clone, Debug, crate::Script)]
pub(crate) struct WindowRow {
    /// The line.
    pub text: WindowText,
    /// Its buttons, as [`Row::button`] adds them.
    #[script(default)]
    pub buttons: Vec<WindowButton>,
}

/// A button of a row, as [`Row::button`] takes it.
#[derive(Clone, Debug, crate::Script)]
pub(crate) struct WindowButton {
    /// The label, a text key.
    pub label: String,
    /// The action it runs.
    pub action: String,
    /// The values it gives first.
    #[script(default)]
    pub args: Vec<Value>,
}

impl WindowText {
    fn values(&self) -> Vec<(&str, &str)> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect()
    }
}

impl ScriptWindow {
    /// The Rust window.
    fn into_window(self) -> Window {
        self.items.into_iter().fold(
            Window::new(&self.id, &self.title),
            |window, item| match item {
                WindowItem::Text(text) => window.text(&text.key, &text.values()),
                WindowItem::Row(row) => window.row(
                    row.buttons
                        .into_iter()
                        .fold(Row::new(&row.text.key, &row.text.values()), |r, b| {
                            r.button(&b.label, &b.action, &b.args)
                        }),
                ),
                WindowItem::Action(action) => window.action(&action),
            },
        )
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

/// A script's `known`: the accounts the mod knows beyond those online.
#[derive(Clone, Debug)]
pub(crate) struct KnownFn(Callback);

impl KnownFn {
    fn accounts(&self, ask: ScriptAsk) -> Vec<(PlayerKey, String)> {
        Option::<Vec<Listed>>::from_answer(self.0.call(&mut [ask.into_value()]))
            .unwrap_or_default()
            .into_iter()
            .map(|listed| (listed.key, listed.name))
            .collect()
    }
}

impl Typed for KnownFn {
    const TY: &'static Ty = &Ty::Function(&Signature::new(
        &[Param::new("ask", <ScriptAsk as Typed>::TY)],
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

function_fields!(AllowFn, AtFn, PerPlayerFn, KnownFn);

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

/// A player a list shows, with the switch on, or an account the mod knows.
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
        "An argument of an action: a typed choice, or a line of text of bounded bytes.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_ARG];
}

#[crate::script(arg)]
impl Arg {
    /// The argument as one a typed line may leave out, taking `default`
    /// then, as [`Arg::optional`] makes it.
    #[script(name = "optional")]
    #[must_use]
    pub(crate) fn optional_script(&self, default: Value) -> Self {
        self.clone().optional(default)
    }
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

    /// Names the mod's actions as commands, `/<alias>` beside the name in
    /// the mod's id, as
    /// [`Actions::commands`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::commands`].
    #[script(name = "commands")]
    pub(crate) fn commands_script(&self, alias: &str) -> Result<(), Off> {
        self.commands(alias)
    }

    /// Tells the platform that who may run the mod's actions changed, as
    /// [`Actions::rules_changed`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::rules_changed`].
    #[script(name = "rules_changed")]
    pub(crate) fn rules_changed_script(&self) -> Result<(), Off> {
        self.rules_changed()
    }

    /// Shows `window` to the players with the account keys `players`, or
    /// updates it where it shows, as [`Actions::show`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::show`].
    #[script(name = "show")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn show_script(
        &self,
        window: ScriptWindow,
        players: Vec<PlayerKey>,
    ) -> Result<(), Off> {
        self.show(window.into_window(), &players)
    }

    /// Closes the window `id` on the clients of the players with the
    /// account keys `players`, as [`Actions::close`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::close`].
    #[script(name = "close")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn close_script(&self, id: &str, players: Vec<PlayerKey>) -> Result<(), Off> {
        self.close(id, &players)
    }

    /// Puts `window` in place of the one of its id on the clients of
    /// `players` that still show it, opening it for no one, as
    /// [`Actions::update`] does.
    ///
    /// # Errors
    ///
    /// As [`Actions::update`].
    #[script(name = "update")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn update_script(
        &self,
        window: ScriptWindow,
        players: Vec<PlayerKey>,
    ) -> Result<(), Off> {
        self.update(window.into_window(), &players)
    }

    /// Calls `closed` with a player's account key and a window's id when
    /// their client closed that window, and with the key alone when the
    /// player left, as [`Actions::on_closed`] does.
    #[script(name = "on_closed")]
    pub(crate) fn on_closed_script(
        &self,
        closed: impl Fn(PlayerKey, Option<String>) + Send + Sync + 'static,
    ) {
        self.on_closed(move |player, window| closed(player, window.map(str::to_owned)));
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

    /// A line of text of 1 to `max_bytes` bytes, as [`Arg::text`] makes it.
    #[script]
    #[must_use]
    pub fn text(self, label: &str, max_bytes: usize) -> Arg {
        Arg::text(label, max_bytes)
    }

    /// One of the game's items, which the client lists by the names its
    /// game shows, as [`Arg::item`] makes it.
    #[script]
    #[must_use]
    pub fn item(self, label: &str) -> Arg {
        Arg::item(label)
    }

    /// Whether `id` can name an action or a window, as
    /// [`is_valid_id`](super::is_valid_id) says.
    #[script]
    #[must_use]
    pub fn is_valid_id(self, id: &str) -> bool {
        super::is_valid_id(id)
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
