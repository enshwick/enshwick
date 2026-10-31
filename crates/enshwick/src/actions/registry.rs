use std::sync::RwLock;

use crate::Host;
use crate::PlayerKey;
use crate::i18n::{Language, Texts};
use crate::locks::RelockRw;
use crate::world::Player;

use super::request::{Arg, Ask, Outcome, Request, Value, Via};
use super::sheet::{ArgView, Entry};
use super::{Action, Handler, MAX_PLAYERS, sheet};

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
}

pub(super) struct Registry {
    pub(super) texts: Texts,
    pub(super) host: Host,
    pub(super) declared: RwLock<Vec<Declared>>,
}

impl Registry {
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
        }
    }

    fn help(&self, player: &Player, language: Language) -> String {
        let declared = self.declared.relock_read();
        let mut lines = vec![self.say("actions.usage", language, &[])];
        for d in declared.iter().filter(|d| Self::allowed(d, player)) {
            lines.push(self.usage(&d.action, language));
            if !d.action.help.is_empty() {
                lines.push(format!("  {}", self.say(&d.action.help, language, &[])));
            }
        }
        lines.join("\n")
    }

    /// Runs the console line `line`, the part after the mod's id: `help`,
    /// or an action's id and its values as words.
    pub(super) fn run_from_console(&self, player: &Player, line: &str) -> Outcome {
        let language = Language::FALLBACK;
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&word, rest)) = words.split_first() else {
            return Outcome::done(self.help(player, language));
        };
        let word = word.to_lowercase();
        if word == "help" {
            return Outcome::done(self.help(player, language));
        }
        let declared = self.declared.relock_read();
        let Some(d) = declared.iter().find(|d| d.action.id == word) else {
            return Outcome::refused(self.say("actions.unknown", language, &[("word", &word)]));
        };
        match self.console_args(d, player, rest, language) {
            Ok(values) => self.run(d, player, &values, language, Via::Console),
            Err(why) => Outcome::refused(self.refusal(&why, &d.action, language)),
        }
    }

    /// The values of the words after an action's id on the console.
    fn console_args(
        &self,
        d: &Declared,
        player: &Player,
        words: &[&str],
        language: Language,
    ) -> Result<Vec<Value>, ReadArgsError> {
        let bad = || ReadArgsError::Usage;
        if d.action.on.is_some() {
            let (last, name) = words.split_last().ok_or_else(bad)?;
            let on = toggle_word(last, &self.texts, language).ok_or_else(bad)?;
            let name = name.join(" ");
            if name.is_empty() {
                return Err(bad());
            }
            let listed = self.listed(d, player, language);
            let target = self.player_named(&name, &listed)?;
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
                self.player_named(&name, &[])?
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

    /// The players a list switches on for `player` now; none while the
    /// action has nothing at hand.
    fn listed(
        &self,
        d: &Declared,
        player: &Player,
        language: Language,
    ) -> Vec<(PlayerKey, String)> {
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
            texts: &self.texts,
        })
    }

    /// The sheet of `player`: every action they may use, in `language`.
    pub(super) fn sheet(&self, player: &Player, language: Language) -> String {
        let declared = self.declared.relock_read();
        let online = self.online();
        let others: Vec<(PlayerKey, String)> = online
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
    pub(super) fn run_from_interface(
        &self,
        player: &Player,
        id: &str,
        args: &str,
        language: Language,
    ) -> String {
        let declared = self.declared.relock_read();
        let outcome = match declared.iter().find(|d| d.action.id == id) {
            None => Outcome::refused(self.say("actions.unknown", language, &[("word", id)])),
            Some(d) => match sheet::decode_args(args) {
                None => Outcome::refused(self.say("actions.denied", language, &[])),
                Some(mut values) => match self.name_players(d, player, &mut values, language) {
                    Ok(()) => self.run(d, player, &values, language, Via::Interface),
                    Err(why) => Outcome::refused(self.refusal(&why, &d.action, language)),
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
    ) -> Result<(), ReadArgsError> {
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
                *name = known.ok_or_else(|| ReadArgsError::NoPlayer {
                    name: format!("{key:016x}"),
                })?;
            }
        }
        Ok(())
    }

    /// The actions with a hint that `player` has at hand.
    pub(super) fn hints(&self, player: &Player, language: Language) -> String {
        let declared = self.declared.relock_read();
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
pub(super) fn fits(value: &Value, arg: &Arg) -> bool {
    match (value, arg.kind) {
        (Value::Player { .. }, ArgView::PLAYER) | (Value::Toggle(_), ArgView::TOGGLE) => true,
        (Value::Choice(index), ArgView::CHOICE) => *index < arg.choices.len(),
        (Value::Number(n), ArgView::NUMBER) => {
            n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1
        }
        _ => false,
    }
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
