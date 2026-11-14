use std::sync::{Arc, RwLock};

mod fit;
mod help;
mod read;

#[cfg(test)]
pub(super) use fit::fits;
pub(super) use fit::{button_fault, optional_fault};
#[cfg(test)]
pub(super) use read::toggle_word;
use read::{ReadArgsError, words};

use crate::Host;
use crate::PlayerKey;
use crate::i18n::{Language, Texts};
use crate::locks::RelockRw;
use crate::world::Player;

use super::request::{ActionTarget, Ask, Outcome, Refused, RefusedReason, Request, Value, Via};
use super::sheet::{ArgView, Entry};
use super::{Action, Handler, MAX_KNOWN, MAX_PLAYERS, sheet};

pub(super) struct Declared {
    pub(super) action: Action,
    pub(super) handler: Handler,
}

/// A line typed on the console, in chat or by a remote caller, as the
/// loader hands it over.
pub(super) struct Typed<'a> {
    /// The line after the word that named the mod.
    pub(super) line: &'a str,
    /// The word that named the mod, its command name, alias or id, in
    /// lower case; empty from a loader that does not tell it.
    pub(super) word: &'a str,
    pub(super) via: Via,
}

impl Typed<'_> {
    /// The command as the caller types it: the word, after a `/` in chat.
    fn command(&self) -> String {
        if self.via == Via::Chat && !self.word.is_empty() {
            format!("/{}", self.word)
        } else {
            self.word.to_owned()
        }
    }
}

pub(super) struct Registry {
    pub(super) texts: Texts,
    pub(super) host: Host,
    pub(super) declared: RwLock<Vec<Arc<Declared>>>,
    /// The windows the mod showed, each as one player holds it.
    pub(super) windows: RwLock<super::window::Kept>,
    /// The mod's own function for a window a client closed or a player who
    /// left.
    pub(super) closed: RwLock<Option<super::Closed>>,
    /// The mod's own function for a run the platform refused before its
    /// handler.
    pub(super) refused: RwLock<Option<super::RefusedFn>>,
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

    /// Tells the mod's own function, out of the lock, of `refused`, a run
    /// refused before its handler with `outcome`; the outcome.
    pub(super) fn refused(&self, refused: &Refused<'_>, outcome: Outcome) -> Outcome {
        let heard = self.refused.relock_read().clone();
        if let Some(heard) = heard
            && let Err(panicked) = crate::host::caught(|| heard(refused))
        {
            *self.refused.relock_write() = None;
            crate::warn!(
                "refused runs unheard: their function panicked: {}",
                panicked.text()
            );
        }
        outcome
    }

    fn online(&self) -> Vec<Player> {
        enshwick_world::__internal::online(self.host)
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

    /// Runs the text form `typed.line`, the part after the word that named
    /// the mod, for `player` reading `language`: `help`, `help <id>`, or
    /// an action's id and its values as words.
    pub(super) fn run_from_console(
        &self,
        player: &Player,
        language: Language,
        typed: &Typed<'_>,
    ) -> Outcome {
        let line = typed.line;
        let words = words(line);
        let Some((first, rest)) = words.split_first() else {
            return Outcome::done(self.help(player, language, typed));
        };
        let word = first.text.to_lowercase();
        let declared = self.declared();
        let named = |id: &str| declared.iter().find(|d| d.action.id == id);
        if word == "help" {
            let Some(about) = rest.first().map(|w| w.text.to_lowercase()) else {
                return Outcome::done(self.help(player, language, typed));
            };
            return match named(&about) {
                Some(d) if Self::allowed(d, player) => Outcome::done(self.help_of(d, language)),
                _ => self.unknown(&about, player, language, typed),
            };
        }
        let refused = |action: &str, reason, outcome| {
            let refused = Refused {
                player,
                via: typed.via,
                action,
                text: line,
                values: &[],
                reason,
            };
            self.refused(&refused, outcome)
        };
        let Some(d) = named(&word) else {
            let outcome = self.unknown(&word, player, language, typed);
            return refused(&word, RefusedReason::UnknownAction, outcome);
        };
        if !Self::allowed(d, player) {
            let outcome = Outcome::refused(self.say("actions.denied", language, &[]));
            return refused(&d.action.id, RefusedReason::NoRight, outcome);
        }
        match self.console_args(d, player, line, rest, language) {
            Ok(values) => self.run(d, player, &values, language, typed.via, None),
            Err(why) => {
                let outcome = Outcome::refused(self.refusal(&why, &d.action, language));
                refused(&d.action.id, why.reason(), outcome)
            }
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
            .map(|(key, name)| (key, sheet::clean(enshwick_base::peer::cut_name(&name))))
            .collect()
    }

    fn allowed(d: &Declared, player: &Player) -> bool {
        player.permissions & d.action.needs == d.action.needs
            && d.action.allow.as_ref().is_none_or(|allow| allow(player))
    }

    /// Runs `d` for `player` with `values`, read at the boundary by a caller
    /// that checked [`allowed`](Self::allowed) once already.
    fn run(
        &self,
        d: &Declared,
        player: &Player,
        values: &[Value],
        language: Language,
        via: Via,
        target: Option<ActionTarget>,
    ) -> Outcome {
        let ask = self.ask(player, language, target);
        let subject = match d.action.at.as_ref().map(|at| at(&ask)) {
            Some(Ok(subject)) => Some(subject),
            Some(Err(why)) => return Outcome::refused(why.reason()),
            None => None,
        };
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
            group: if d.action.group.is_empty() {
                String::new()
            } else {
                self.say(&d.action.group, language, &[])
            },
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
            if d.action.with_self {
                let on = switched.iter().any(|(k, _)| *k == player.key);
                players.insert(0, (player.key, player.name.clone(), on));
            }
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
                    let viewer = (player.key, player.name.clone());
                    d.action
                        .with_self
                        .then_some(&viewer)
                        .into_iter()
                        .chain(others)
                        .chain(&known)
                        .take(MAX_PLAYERS)
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                },
                default: arg.default.clone(),
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

    /// Forgets window `id` as `player` holds it, their client having closed
    /// it, or with an empty id every window of theirs, since they left;
    /// then tells the mod's own function, out of the lock.
    pub(super) fn window_closed(&self, player: PlayerKey, id: &str) {
        {
            let mut kept = self.windows.relock_write();
            if id.is_empty() {
                kept.forget_player(player);
            } else {
                kept.forget(id, &[player]);
            }
        }
        let closed = self.closed.relock_read().clone();
        if let Some(closed) = closed {
            closed(player, Some(id).filter(|id| !id.is_empty()));
        }
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
        let refused = |reason, values: &[Value], text: String| {
            let refused = Refused {
                player,
                via: Via::Interface,
                action: id,
                text: args,
                values,
                reason,
            };
            self.refused(&refused, Outcome::refused(text))
        };
        let declared = self.declared();
        let Some(d) = declared.iter().find(|d| d.action.id == id) else {
            let text = self.say(
                "actions.unknown",
                language,
                &[("word", id), ("help", "help")],
            );
            return refused(RefusedReason::UnknownAction, &[], text);
        };
        if !Self::allowed(d, player) {
            let text = self.say("actions.denied", language, &[]);
            return refused(RefusedReason::NoRight, &[], text);
        }
        let mut values = match Self::long_text(&d.action, args) {
            Some(why) => Err((why, Vec::new())),
            None => sheet::decode_args(args).ok_or((ReadArgsError::Usage, Vec::new())),
        };
        if let Ok(values) = &mut values {
            let left_out = d.action.args.iter().skip(values.len());
            values.extend(left_out.map_while(|arg| arg.default.clone()));
        }
        let read = values.and_then(|mut values| {
            let read = self
                .name_players(d, player, &mut values, language, target)
                .and_then(|()| self.values_fit(d, &values));
            match read {
                Ok(()) => Ok(values),
                Err(why) => Err((why, values)),
            }
        });
        match read {
            Ok(values) => self.run(d, player, &values, language, Via::Interface, target),
            Err((why, values)) => {
                let text = self.refusal(&why, &d.action, language);
                refused(why.reason(), &values, text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(action: Action) -> Declared {
        Declared {
            action,
            handler: Box::new(|_| Outcome::done("")),
        }
    }

    fn with_permissions(permissions: u8) -> Player {
        Player {
            entity: crate::Entity(2),
            slot: 0,
            key: PlayerKey::new(7),
            permissions,
            name: "Ann".to_owned(),
        }
    }

    #[test]
    fn administrators_only_holds_beside_an_allow_set_before_or_after_it() {
        let (guest, admin) = (
            with_permissions(0),
            with_permissions(crate::abi::PlayerInfo::CAN_KICK_BAN),
        );
        for action in [
            Action::new("audit", "Audit").admins().allow(|_| true),
            Action::new("audit", "Audit").allow(|_| true).admins(),
        ] {
            let d = declared(action);
            assert!(!Registry::allowed(&d, &guest));
            assert!(Registry::allowed(&d, &admin));
        }
        let closed = declared(Action::new("audit", "Audit").admins().allow(|_| false));
        assert!(!Registry::allowed(&closed, &admin), "both must hold");
    }
}
