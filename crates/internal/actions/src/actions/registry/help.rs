//! What a typed line reads back about a mod's actions: the usage of one,
//! the list of all a caller may run, and the refusal of an unknown one.

use std::fmt::Write as _;

use crate::i18n::Language;
use crate::world::Player;

use super::super::Action;
use super::super::request::Outcome;
use super::super::sheet::ArgView;
use super::{Declared, Registry, Typed};

/// The ids a refusal of an unknown one names as close to it, at most.
const NEAR: usize = 3;

impl Registry {
    /// The usage line of `action` on the console: its id, its arguments, an
    /// optional one in square brackets, and its label.
    pub(super) fn usage(&self, action: &Action, language: Language) -> String {
        let player = self.say("actions.player", language, &[]);
        let on_off = format!(
            "{}|{}",
            self.say("actions.on", language, &[]),
            self.say("actions.off", language, &[])
        );
        let label = self.say(&action.label, language, &[]);
        if action.on.is_some() {
            return format!("{} <{player}> <{on_off}>: {label}", action.id);
        }
        let mut line = action.id.clone();
        for arg in &action.args {
            let shown = match arg.kind {
                ArgView::PLAYER => player.clone(),
                ArgView::CHOICE => arg
                    .choices
                    .iter()
                    .map(|(word, _)| word.as_str())
                    .collect::<Vec<_>>()
                    .join("|"),
                ArgView::NUMBER => format!("{}-{}", arg.range.0, arg.range.1),
                ArgView::TEXT | ArgView::ITEM => self.say(&arg.label, language, &[]),
                _ => on_off.clone(),
            };
            let (open, close) = if arg.default.is_some() {
                ('[', ']')
            } else {
                ('<', '>')
            };
            let _ = write!(line, " {open}{shown}{close}");
        }
        format!("{line}: {label}")
    }

    /// The plain name of the mod that `word` names in a typed line, its
    /// command name, alias or id, as the Mods screen shows it; the word
    /// where no mod listed has it.
    fn mod_name(word: &str) -> String {
        let id = crate::commands::list()
            .into_iter()
            .find(|set| set.name == word || set.alias.as_deref() == Some(word))
            .map_or_else(|| word.to_owned(), |set| set.mod_id);
        crate::mods::all()
            .into_iter()
            .find(|entry| entry.id == id)
            .map_or(id, |entry| entry.label())
    }

    /// The help of a typed line for `player`: a line naming the mod and how
    /// to run and ask about an action, then one line for each action the
    /// player may run, the ones without a group first and the rest under
    /// their groups, in the order declared.
    pub(super) fn help(&self, player: &Player, language: Language, typed: &Typed<'_>) -> String {
        let header = if typed.word.is_empty() {
            self.say("actions.usage", language, &[])
        } else {
            self.say(
                "actions.help",
                language,
                &[
                    ("mod", &Self::mod_name(typed.word)),
                    ("command", &typed.command()),
                ],
            )
        };
        let declared = self.declared();
        let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
        for d in declared.iter().filter(|d| Self::allowed(d, player)) {
            let line = self.usage(&d.action, language);
            let group = d.action.group.as_str();
            match groups.iter_mut().find(|(name, _)| *name == group) {
                Some((_, lines)) => lines.push(line),
                None => groups.push((group, vec![line])),
            }
        }
        groups.sort_by_key(|(name, _)| !name.is_empty());
        let mut lines = vec![header];
        for (group, actions) in groups {
            if !group.is_empty() {
                lines.push(format!("{}:", self.say(group, language, &[])));
            }
            lines.extend(actions);
        }
        lines.join("\n")
    }

    /// What `help <id>` answers for `d`: its usage line and, when the mod
    /// gave one, what the action does.
    pub(super) fn help_of(&self, d: &Declared, language: Language) -> String {
        let usage = self.usage(&d.action, language);
        if d.action.help.is_empty() {
            usage
        } else {
            format!("{usage}\n{}", self.say(&d.action.help, language, &[]))
        }
    }

    /// The refusal of `word`, which names no action `player` may run: the
    /// closest ids among those they may, and how to list them all.
    pub(super) fn unknown(
        &self,
        word: &str,
        player: &Player,
        language: Language,
        typed: &Typed<'_>,
    ) -> Outcome {
        let declared = self.declared();
        let ids = declared
            .iter()
            .filter(|d| Self::allowed(d, player))
            .map(|d| d.action.id.as_str());
        let near = crate::near::near(word, ids, NEAR).join(", ");
        let help = format!("{} help", typed.command());
        let values = [("word", word), ("help", help.trim()), ("near", &near)];
        let key = if near.is_empty() {
            "actions.unknown"
        } else {
            "actions.unknown_near"
        };
        Outcome::refused(self.say(key, language, &values))
    }
}
