use std::sync::{Arc, RwLock};

use crate::Host;
use crate::PlayerKey;
use crate::i18n::{Language, Texts};
use crate::locks::RelockRw;
use crate::world::Player;

use super::request::{ActionTarget, Arg, Ask, Outcome, Request, Value, Via};
use super::sheet::{ArgView, Entry};
use super::{Action, Handler, MAX_KNOWN, MAX_PLAYERS, sheet};

pub(super) struct Declared {
    pub(super) action: Action,
    pub(super) handler: Handler,
}

/// Why the words of a console line or the values from the interface name no
/// arguments of an action.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
enum ReadArgsError {
    /// The words do not follow the action's usage.
    #[error("the words do not follow the action's usage")]
    Usage,
    /// No player online or listed has the name.
    #[error("no player named {name}")]
    NoPlayer { name: String },
    /// Several players have the name.
    #[error("several players are named {name}")]
    SeveralPlayers { name: String },
    /// The interface named a player by an account no one online or listed
    /// has, as when they just left.
    #[error("the player named is gone")]
    Gone,
    /// A text is longer than its argument takes.
    #[error("{label} takes at most {max} bytes")]
    TooLong { label: String, max: String },
}

/// A word of a console line: a run without white space, or a run between
/// quotes without them, and the byte where it starts in the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Word<'a> {
    text: &'a str,
    from: usize,
}

/// The words of `line`; an unclosed quote runs to the end of the line.
fn words(line: &str) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let (start, stop): (usize, fn(char) -> bool) = if c == '"' {
            chars.next();
            (at + c.len_utf8(), |c| c == '"')
        } else {
            (at, char::is_whitespace)
        };
        let end = chars
            .by_ref()
            .find(|&(_, c)| stop(c))
            .map_or(line.len(), |(i, _)| i);
        words.push(Word {
            text: &line[start..end],
            from: at,
        });
    }
    words
}

/// The text a last text argument takes from `words`, the rest of `line`:
/// as typed, or without its quotes when it is one quoted run.
fn rest_of_line<'a>(line: &'a str, words: &[Word<'a>]) -> Option<&'a str> {
    match words {
        [] => None,
        [only] => Some(only.text),
        [first, ..] => Some(line[first.from..].trim_end()),
    }
}

pub(super) struct Registry {
    pub(super) texts: Texts,
    pub(super) host: Host,
    pub(super) declared: RwLock<Vec<Arc<Declared>>>,
    /// The windows the mod showed, each as one player holds it.
    pub(super) windows: RwLock<super::window::Kept>,
    /// Set once the mod named its actions as commands.
    pub(super) commands: core::sync::atomic::AtomicBool,
    /// Set once a handler panicked during a command: the mod's commands
    /// stay off, its actions keep their other paths.
    pub(super) commands_off: core::sync::atomic::AtomicBool,
}

impl Registry {
    /// The declared actions, out of the lock, so a handler, `allow`, `at` or
    /// `per_player` may add an action without deadlocking.
    fn declared(&self) -> Vec<Arc<Declared>> {
        self.declared.relock_read().clone()
    }

    pub(super) fn say(&self, key: &str, language: Language, values: &[(&str, &str)]) -> String {
        self.texts.fill(key, language, values)
    }

    fn online(&self) -> Vec<Player> {
        crate::world::online(self.host)
    }

    fn ask<'a>(
        &'a self,
        player: &'a Player,
        language: Language,
        target: Option<ActionTarget>,
    ) -> Ask<'a> {
        Ask {
            player,
            language,
            target,
            texts: &self.texts,
        }
    }

    /// The usage line of `action` on the console.
    fn usage(&self, action: &Action, language: Language) -> String {
        let player = format!("<{}>", self.say("actions.player", language, &[]));
        let on_off = format!(
            "<{}|{}>",
            self.say("actions.on", language, &[]),
            self.say("actions.off", language, &[])
        );
        let label = self.say(&action.label, language, &[]);
        if action.on.is_some() {
            return format!("{} {player} {on_off}: {label}", action.id);
        }
        let mut line = action.id.clone();
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
                ArgView::TEXT => format!("<{}>", self.say(&arg.label, language, &[])),
                _ => on_off.clone(),
            };
            line.push(' ');
            line.push_str(&shown);
        }
        format!("{line}: {label}")
    }

    /// The reply in `language` to a run of `action` refused for `error`.
    fn refusal(&self, error: &ReadArgsError, action: &Action, language: Language) -> String {
        match error {
            ReadArgsError::Usage => self.say(
                "actions.bad",
                language,
                &[("usage", &self.usage(action, language))],
            ),
            ReadArgsError::NoPlayer { name } => {
                self.say("actions.no_player", language, &[("name", name)])
            }
            ReadArgsError::SeveralPlayers { name } => {
                self.say("actions.several", language, &[("name", name)])
            }
            ReadArgsError::Gone => self.say("actions.gone", language, &[]),
            ReadArgsError::TooLong { label, max } => self.say(
                "actions.too_long",
                language,
                &[("label", &self.say(label, language, &[])), ("max", max)],
            ),
        }
    }

    fn help(&self, player: &Player, language: Language) -> String {
        let declared = self.declared();
        let mut lines = vec![self.say("actions.usage", language, &[])];
        for d in declared.iter().filter(|d| Self::allowed(d, player)) {
            lines.push(self.usage(&d.action, language));
            if !d.action.help.is_empty() {
                lines.push(format!("  {}", self.say(&d.action.help, language, &[])));
            }
        }
        lines.join("\n")
    }

    /// Runs the text form `line`, the part after the mod's id or command
    /// name, for `player` reading `language`, from `via`: `help`, or an
    /// action's id and its values as words.
    pub(super) fn run_from_console(
        &self,
        player: &Player,
        line: &str,
        language: Language,
        via: Via,
    ) -> Outcome {
        let words = words(line);
        let Some((first, rest)) = words.split_first() else {
            return Outcome::done(self.help(player, language));
        };
        let word = first.text.to_lowercase();
        if word == "help" {
            return Outcome::done(self.help(player, language));
        }
        let declared = self.declared();
        let Some(d) = declared.iter().find(|d| d.action.id == word) else {
            return Outcome::refused(self.say("actions.unknown", language, &[("word", &word)]));
        };
        if !Self::allowed(d, player) {
            return Outcome::refused(self.say("actions.denied", language, &[]));
        }
        match self.console_args(d, player, line, rest, language) {
            Ok(values) => self.run(d, player, &values, language, via, None),
            Err(why) => Outcome::refused(self.refusal(&why, &d.action, language)),
        }
    }

    /// The values of the `words` of `line` after an action's id on the
    /// console.
    fn console_args(
        &self,
        d: &Declared,
        player: &Player,
        line: &str,
        words: &[Word<'_>],
        language: Language,
    ) -> Result<Vec<Value>, ReadArgsError> {
        let bad = || ReadArgsError::Usage;
        let joined =
            |words: &[Word<'_>]| words.iter().map(|w| w.text).collect::<Vec<_>>().join(" ");
        if d.action.on.is_some() {
            let (last, name) = words.split_last().ok_or_else(bad)?;
            let on = toggle_word(last.text, &self.texts, language).ok_or_else(bad)?;
            let name = joined(name);
            if name.is_empty() {
                return Err(bad());
            }
            let mut listed = self.listed(d, player, language, None);
            listed.extend(Self::known_of(d, &self.ask(player, language, None)));
            let target = self.player_named(&name, &listed)?;
            return Ok(vec![target, Value::Toggle(on)]);
        }
        let known = Self::known_of(d, &self.ask(player, language, None));
        let mut values = Vec::with_capacity(d.action.args.len());
        let mut rest = words;
        for (i, arg) in d.action.args.iter().enumerate() {
            let last = i + 1 == d.action.args.len();
            let value = match arg.kind {
                ArgView::PLAYER => {
                    let taken = if last { rest.len() } else { 1 };
                    if rest.len() < taken || taken == 0 {
                        return Err(bad());
                    }
                    let name = joined(&rest[..taken]);
                    rest = &rest[taken..];
                    self.player_named(&name, &known)?
                }
                ArgView::TEXT => {
                    let text = if last {
                        let text = rest_of_line(line, rest).ok_or_else(bad)?;
                        rest = &[];
                        text
                    } else {
                        let (word, tail) = rest.split_first().ok_or_else(bad)?;
                        rest = tail;
                        word.text
                    };
                    if !arg.takes_text(text) {
                        let too_long = u16::try_from(text.len())
                            .map_or(true, |len| f64::from(len) > arg.range.1);
                        return Err(if too_long {
                            ReadArgsError::TooLong {
                                label: arg.label.clone(),
                                max: arg.range.1.to_string(),
                            }
                        } else {
                            bad()
                        });
                    }
                    Value::Text(text.to_owned())
                }
                kind => {
                    let (word, tail) = rest.split_first().ok_or_else(bad)?;
                    rest = tail;
                    let word = word.text;
                    match kind {
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
                        _ => {
                            Value::Toggle(toggle_word(word, &self.texts, language).ok_or_else(bad)?)
                        }
                    }
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
        listed: &[(PlayerKey, String)],
    ) -> Result<Value, ReadArgsError> {
        let lower = name.to_lowercase();
        let mut found: Vec<(PlayerKey, String)> = self
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
            [] => Err(ReadArgsError::NoPlayer {
                name: name.to_owned(),
            }),
            _ => Err(ReadArgsError::SeveralPlayers {
                name: name.to_owned(),
            }),
        }
    }

    /// The players a list switches on for `player` now, on the object
    /// `target` names; none while the action has nothing at hand.
    fn listed(
        &self,
        d: &Declared,
        player: &Player,
        language: Language,
        target: Option<ActionTarget>,
    ) -> Vec<(PlayerKey, String)> {
        let ask = self.ask(player, language, target);
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

    /// The accounts `d` knows for `ask`, bounded: at most [`MAX_KNOWN`],
    /// each with a name of 1 to 64 bytes without a separator.
    fn known_of(d: &Declared, ask: &Ask<'_>) -> Vec<(PlayerKey, String)> {
        d.action
            .known
            .as_ref()
            .map(|known| known(ask))
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, name)| !name.is_empty())
            .take(MAX_KNOWN)
            .map(|(key, name)| (key, sheet::clean(&name[..name.floor_char_boundary(64)])))
            .collect()
    }

    fn allowed(d: &Declared, player: &Player) -> bool {
        player.permissions & d.action.needs == d.action.needs
            && d.action.allow.as_ref().is_none_or(|allow| allow(player))
    }

    /// Runs `d` for `player` with `values` after the checks.
    fn run(
        &self,
        d: &Declared,
        player: &Player,
        values: &[Value],
        language: Language,
        via: Via,
        target: Option<ActionTarget>,
    ) -> Outcome {
        if !Self::allowed(d, player) {
            return Outcome::refused(self.say("actions.denied", language, &[]));
        }
        let ask = self.ask(player, language, target);
        let subject = match d.action.at.as_ref().map(|at| at(&ask)) {
            Some(Ok(subject)) => Some(subject),
            Some(Err(why)) => return Outcome::refused(why.reason()),
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
                &[("usage", &self.usage(&d.action, language))],
            ));
        }
        (d.handler)(&Request {
            player,
            subject: subject.as_ref(),
            args: values,
            language,
            via,
            target,
            texts: &self.texts,
        })
    }

    /// The sheet of `player`: every action they may use, in `language`.
    pub(super) fn sheet(&self, player: &Player, language: Language) -> String {
        let others = self.others(player);
        let entries: Vec<Entry> = self
            .declared()
            .iter()
            .filter(|d| Self::allowed(d, player))
            .map(|d| self.entry(d, player, language, &others))
            .collect();
        sheet::encode(&entries)
    }

    /// The players online but `player`, at most [`MAX_PLAYERS`].
    fn others(&self, player: &Player) -> Vec<(PlayerKey, String)> {
        self.online()
            .into_iter()
            .filter(|p| p.key != player.key)
            .map(|p| (p.key, p.name))
            .take(MAX_PLAYERS)
            .collect()
    }

    /// `d` as `player` sees it in `language`, `others` being the players
    /// online but them.
    fn entry(
        &self,
        d: &Declared,
        player: &Player,
        language: Language,
        others: &[(PlayerKey, String)],
    ) -> Entry {
        let ask = self.ask(player, language, None);
        let known: Vec<(PlayerKey, String)> = Self::known_of(d, &ask)
            .into_iter()
            .filter(|(key, _)| *key != player.key && !others.iter().any(|(k, _)| k == key))
            .collect();
        let subject = d.action.at.as_ref().map(|at| at(&ask));
        let (offered, subject_text) = match &subject {
            Some(Ok(subject)) => (true, subject.label.clone()),
            Some(Err(why)) => (false, why.reason().to_owned()),
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
            let mut players: Vec<(PlayerKey, String, bool)> = others
                .iter()
                .map(|(key, name)| (*key, name.clone(), switched.iter().any(|(k, _)| k == key)))
                .collect();
            for (key, name) in &switched {
                if *key != player.key && !players.iter().any(|(k, _, _)| k == key) {
                    players.push((*key, name.clone(), true));
                }
            }
            players.sort_by_key(|(_, name, _)| name.to_lowercase());
            for (key, name) in &known {
                if !players.iter().any(|(k, _, _)| k == key) {
                    players.push((*key, name.clone(), false));
                }
            }
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
                    others
                        .iter()
                        .chain(&known)
                        .take(MAX_PLAYERS)
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
            });
        }
        entry
    }

    /// The window `id` as `player` sees it in `language`: its texts filled,
    /// with the sheet entries of the actions it names; empty when the mod
    /// keeps no such window for them or the player may not run an action
    /// it names.
    pub(super) fn window(&self, player: &Player, language: Language, id: &str) -> String {
        let Some(window) = self.windows.relock_write().rendered(id, player.key) else {
            return String::new();
        };
        let declared = self.declared();
        let mut named: Vec<&Declared> = Vec::new();
        for action in window.actions() {
            if named.iter().any(|n| n.action.id == action) {
                continue;
            }
            match declared.iter().find(|d| d.action.id == action) {
                Some(d) if Self::allowed(d, player) => named.push(d),
                _ => return String::new(),
            }
        }
        let others = self.others(player);
        let entries: Vec<Entry> = named
            .iter()
            .map(|d| self.entry(d, player, language, &others))
            .collect();
        window.render(&|key, values| self.say(key, language, values), &entries)
    }

    /// Runs action `id` for `player` from the interface, on the object
    /// `target` the client named, if any.
    pub(super) fn run_from_interface(
        &self,
        player: &Player,
        id: &str,
        args: &str,
        language: Language,
        target: Option<ActionTarget>,
    ) -> Outcome {
        let declared = self.declared();
        match declared.iter().find(|d| d.action.id == id) {
            None => Outcome::refused(self.say("actions.unknown", language, &[("word", id)])),
            Some(d) if !Self::allowed(d, player) => {
                Outcome::refused(self.say("actions.denied", language, &[]))
            }
            Some(d) => match sheet::decode_args(args) {
                None => Outcome::refused(self.say("actions.denied", language, &[])),
                Some(mut values) => {
                    match self.name_players(d, player, &mut values, language, target) {
                        Ok(()) => self.run(d, player, &values, language, Via::Interface, target),
                        Err(why) => Outcome::refused(self.refusal(&why, &d.action, language)),
                    }
                }
            },
        }
    }

    /// Fills in the players in `values`, which the interface sends by key,
    /// or as a typed name in a player's place: online, on the list of the
    /// object `target` names, or among the accounts the action knows.
    fn name_players(
        &self,
        d: &Declared,
        player: &Player,
        values: &mut [Value],
        language: Language,
        target: Option<ActionTarget>,
    ) -> Result<(), ReadArgsError> {
        let player_at = |i: usize| {
            if d.action.on.is_some() {
                i == 0
            } else {
                d.action
                    .args
                    .get(i)
                    .is_some_and(|arg| arg.kind == ArgView::PLAYER)
            }
        };
        let names_one = values.iter().enumerate().any(|(i, v)| match v {
            Value::Player { .. } => true,
            Value::Text(_) => player_at(i),
            _ => false,
        });
        if !names_one {
            return Ok(());
        }
        let online = self.online();
        let mut listed = if d.action.on.is_some() {
            self.listed(d, player, language, target)
        } else {
            Vec::new()
        };
        listed.extend(Self::known_of(d, &self.ask(player, language, target)));
        for (i, value) in values.iter_mut().enumerate() {
            match value {
                Value::Player { key, name } => {
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
                    *name = known.ok_or(ReadArgsError::Gone)?;
                }
                Value::Text(typed) if player_at(i) => {
                    let found = self.player_named(typed, &listed)?;
                    *value = found;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Whether `value` fits `arg`'s type and range.
pub(super) fn fits(value: &Value, arg: &Arg) -> bool {
    match (value, arg.kind) {
        (Value::Player { .. }, ArgView::PLAYER) | (Value::Toggle(_), ArgView::TOGGLE) => true,
        (Value::Choice(index), ArgView::CHOICE) => *index < arg.choices.len(),
        (Value::Number(n), ArgView::NUMBER) => {
            n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1
        }
        (Value::Text(text), ArgView::TEXT) => arg.takes_text(text),
        _ => false,
    }
}

/// What is wrong with the `values` a window's button gives `action` first:
/// a list takes its player and on or off, any other action no more values
/// than its arguments, each fitting its own; `None` when they fit.
pub(super) fn button_fault(action: &Action, values: &[Value]) -> Option<String> {
    let id = &action.id;
    if action.on.is_some() {
        return (!matches!(values, [Value::Player { .. }, Value::Toggle(_)]))
            .then(|| format!("a button gives the list {id} other than a player and on or off"));
    }
    if values.len() > action.args.len() {
        return Some(format!(
            "a button gives {} values to {id}, which takes {}",
            values.len(),
            action.args.len()
        ));
    }
    let at = values
        .iter()
        .zip(&action.args)
        .position(|(v, a)| !fits(v, a))?;
    Some(format!(
        "a button gives {id} a value its argument {} does not take",
        at + 1
    ))
}

/// On or off, from a console word.
pub(super) fn toggle_word(word: &str, texts: &Texts, language: Language) -> Option<bool> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str) -> Vec<&str> {
        words(line).iter().map(|w| w.text).collect()
    }

    #[test]
    fn console_words_keep_quoted_runs_and_an_unclosed_quote_runs_to_the_end() {
        assert_eq!(
            texts(r#"ban "Bob Lee"  "spam""#),
            ["ban", "Bob Lee", "spam"]
        );
        assert_eq!(texts(r#"say "unclosed run"#), ["say", "unclosed run"]);
        assert_eq!(texts(r#"say """#), ["say", ""]);
        assert_eq!(texts(" \t "), Vec::<&str>::new());
    }

    #[test]
    fn the_rest_of_a_line_keeps_its_spaces_and_drops_the_quotes_of_one_run() {
        let line = "say  two  spaces ";
        let all = words(line);
        assert_eq!(rest_of_line(line, &all[1..]), Some("two  spaces"));
        let quoted = r#"say "Hi all""#;
        assert_eq!(rest_of_line(quoted, &words(quoted)[1..]), Some("Hi all"));
        let mixed = r#"say "Hi" all"#;
        assert_eq!(rest_of_line(mixed, &words(mixed)[1..]), Some(r#""Hi" all"#));
        assert_eq!(rest_of_line("say", &[]), None);
    }
}
