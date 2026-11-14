//! The one argument pipe of an action: the words of a typed line and the
//! values the interface sends, read once against the action's arguments
//! at the boundary, and the reply when they name none.

use crate::PlayerKey;
use crate::content::Guid;
use crate::i18n::{Language, Texts};
use crate::world::Player;

use super::super::Action;
use super::super::request::{ActionTarget, Arg, RefusedReason, Value};
use super::super::sheet::ArgView;
use super::fit::{fits, takes_line};
use super::{Declared, Registry};

impl ReadArgsError {
    /// The reason the mod's [`Refused`](super::super::request::Refused) names.
    pub(super) const fn reason(&self) -> RefusedReason {
        match self {
            Self::Usage => RefusedReason::Usage,
            Self::NoPlayer { .. } => RefusedReason::UnknownPlayer,
            Self::SeveralPlayers { .. } => RefusedReason::SeveralPlayers,
            Self::Gone => RefusedReason::PlayerGone,
            Self::TooLong { .. } => RefusedReason::TooLong,
            Self::NoItem { .. } => RefusedReason::UnknownItem,
            Self::SeveralItems { .. } => RefusedReason::SeveralItems,
        }
    }
}

/// Why the words of a console line or the values from the interface name no
/// arguments of an action.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum ReadArgsError {
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
    /// No item the game knows has the name; `near` lists names that look
    /// like it.
    #[error("no item {name}")]
    NoItem { name: String, near: Vec<String> },
    /// Several items have the name.
    #[error("several items are named {name}")]
    SeveralItems { name: String },
}

/// A word of a console line: a run without white space, or a run between
/// quotes without them, and the bytes it spans in the line, its quotes
/// included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Word<'a> {
    pub(super) text: &'a str,
    pub(super) from: usize,
    pub(super) to: usize,
}

/// The words of `line`; an unclosed quote runs to the end of the line.
pub(super) fn words(line: &str) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some(&(at, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        let quoted = c == '"';
        let (start, stop): (usize, fn(char) -> bool) = if quoted {
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
            to: if quoted {
                (end + 1).min(line.len())
            } else {
                end
            },
        });
    }
    words
}

/// The text a text argument takes from `words`, a run of the words of
/// `line`: as typed from the first to the last, or without its quotes when
/// it is one quoted run.
pub(super) fn run_of_line<'a>(line: &'a str, words: &[Word<'a>]) -> Option<&'a str> {
    match words {
        [] => None,
        [only] => Some(only.text),
        [first, .., last] => Some(&line[first.from..last.to]),
    }
}

impl Registry {
    /// The reply in `language` to a run of `action` refused for `error`.
    pub(super) fn refusal(
        &self,
        error: &ReadArgsError,
        action: &Action,
        language: Language,
    ) -> String {
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
            ReadArgsError::NoItem { name, near } if near.is_empty() => {
                self.say("actions.no_item", language, &[("name", name)])
            }
            ReadArgsError::NoItem { name, near } => self.say(
                "actions.no_item_near",
                language,
                &[("name", name), ("near", &near.join(", "))],
            ),
            ReadArgsError::SeveralItems { name } => {
                self.say("actions.several_items", language, &[("name", name)])
            }
        }
    }

    /// The value the one word `word` gives `arg`, which is no player.
    pub(super) fn word_value(&self, arg: &Arg, word: &str, language: Language) -> Option<Value> {
        match arg.kind {
            ArgView::CHOICE => arg
                .choices
                .iter()
                .position(|(w, _)| w.eq_ignore_ascii_case(word))
                .or_else(|| {
                    word.parse::<usize>()
                        .ok()
                        .and_then(|n| n.checked_sub(1))
                        .filter(|&n| n < arg.choices.len())
                })
                .map(Value::Choice),
            ArgView::NUMBER => word
                .replace(',', ".")
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= arg.range.0 && *n <= arg.range.1)
                .map(Value::Number),
            ArgView::TEXT => arg.takes_text(word).then(|| Value::Text(word.to_owned())),
            ArgView::TOGGLE => toggle_word(word, &self.texts, language).map(Value::Toggle),
            _ => None,
        }
    }

    /// The values the `words` of `line` give the optional arguments
    /// `optional`, which fill in order: each word reads as its argument,
    /// and the arguments left without a word take their defaults. An
    /// optional text or item, always the last, takes what is left of the
    /// line, starting at the first word that does not read as the argument
    /// before it. Without one a word that does not read, or one too many,
    /// is an error.
    pub(super) fn optional_values(
        &self,
        optional: &[Arg],
        line: &str,
        words: &[Word<'_>],
        language: Language,
    ) -> Result<Vec<Value>, ReadArgsError> {
        let (text, plain) = match optional.split_last() {
            Some((last, plain)) if takes_line(last) => (Some(last), plain),
            _ => (None, optional),
        };
        let default = |arg: &Arg| match arg.default.clone() {
            Some(Value::Item(item)) if !self.knows_item(item) => Err(ReadArgsError::NoItem {
                name: item.to_string(),
                near: Vec::new(),
            }),
            Some(value) => Ok(value),
            None => Err(ReadArgsError::Usage),
        };
        let mut values = Vec::with_capacity(optional.len());
        let mut rest = words;
        let mut text_began = false;
        for arg in plain {
            let read = rest
                .split_first()
                .filter(|_| !text_began)
                .and_then(|(word, after)| {
                    Some((self.word_value(arg, word.text, language)?, after))
                });
            match read {
                Some((value, after)) => {
                    values.push(value);
                    rest = after;
                }
                None if text.is_some() || rest.is_empty() => {
                    text_began = true;
                    values.push(default(arg)?);
                }
                None => return Err(ReadArgsError::Usage),
            }
        }
        match (text, run_of_line(line, rest)) {
            (Some(arg), Some(typed)) if arg.kind == ArgView::ITEM => {
                values.push(self.item_named(typed)?);
            }
            (Some(arg), Some(typed)) => values.push(Self::text_value(arg, typed)?),
            (Some(arg), None) => values.push(default(arg)?),
            (None, Some(_)) => return Err(ReadArgsError::Usage),
            (None, None) => {}
        }
        Ok(values)
    }

    /// The values of the `words` of `line` after an action's id on the
    /// console. A player or a text that only optional arguments follow
    /// takes the rest of the line up to them: the last words go to the
    /// optional arguments as far as each reads as its own.
    pub(super) fn console_args(
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
        let args = &d.action.args;
        let required = args.iter().take_while(|arg| arg.default.is_none()).count();
        let (required, optional) = args.split_at(required);
        // A last optional text or item takes the rest of the line itself,
        // so the arguments before it take one word each.
        let text_ends = optional.last().is_some_and(takes_line);
        let mut values = Vec::with_capacity(args.len());
        let mut rest = words;
        let mut tail = None;
        for (i, arg) in required.iter().enumerate() {
            let takes_rest = i + 1 == required.len() && !text_ends;
            let value = match arg.kind {
                ArgView::PLAYER | ArgView::TEXT | ArgView::ITEM => {
                    let taken = if takes_rest {
                        let (taken, given) = (1..=rest.len())
                            .find_map(|kept| {
                                self.optional_values(optional, line, &rest[kept..], language)
                                    .ok()
                                    .map(|given| (kept, given))
                            })
                            .ok_or_else(bad)?;
                        tail = Some(given);
                        taken
                    } else {
                        1
                    };
                    let (mine, after) = rest.split_at_checked(taken).ok_or_else(bad)?;
                    rest = if takes_rest { &[] } else { after };
                    if arg.kind == ArgView::PLAYER {
                        self.player_named(&joined(mine), &known)?
                    } else if arg.kind == ArgView::ITEM {
                        self.item_named(run_of_line(line, mine).ok_or_else(bad)?)?
                    } else {
                        Self::text_value(arg, run_of_line(line, mine).ok_or_else(bad)?)?
                    }
                }
                _ => {
                    let (word, after) = rest.split_first().ok_or_else(bad)?;
                    rest = after;
                    self.word_value(arg, word.text, language).ok_or_else(bad)?
                }
            };
            values.push(value);
        }
        let tail = match tail {
            Some(given) => given,
            None => self.optional_values(optional, line, rest, language)?,
        };
        values.extend(tail);
        Ok(values)
    }

    /// The item `text` names on the console: its GUID, a content id in
    /// full, a vanilla debug name, or a name the game shows where the
    /// server's game has its texts. A GUID or content id of an item the
    /// server's game does not know is refused under the text typed.
    pub(super) fn item_named(&self, text: &str) -> Result<Value, ReadArgsError> {
        use crate::content::vanilla::{self, VanillaError};

        if let Some(item) = Guid::try_parse(text).or_else(|| Guid::try_named(text)) {
            return if self.knows_item(item) {
                Ok(Value::Item(item))
            } else {
                Err(ReadArgsError::NoItem {
                    name: text.to_owned(),
                    near: Vec::new(),
                })
            };
        }
        let several = || ReadArgsError::SeveralItems {
            name: text.to_owned(),
        };
        let debug_near = match vanilla::item(text) {
            Ok(item) => return Ok(Value::Item(item)),
            Err(VanillaError::Ambiguous { .. }) => return Err(several()),
            Err(VanillaError::Unknown { near, .. }) => near,
            Err(_) => Vec::new(),
        };
        match vanilla::item_by_display_name(text) {
            Ok(item) => Ok(Value::Item(item)),
            Err(VanillaError::Ambiguous { .. }) => Err(several()),
            Err(error) => Err(ReadArgsError::NoItem {
                name: text.to_owned(),
                near: match error {
                    VanillaError::Unknown { near, .. } if !near.is_empty() => near,
                    _ => debug_near,
                },
            }),
        }
    }

    /// Whether the server's game knows `item` by this very GUID. The game
    /// keys items by [`Guid::fnv1a`], so a GUID that shares an item's key
    /// but is not that item is refused: a vanilla item's GUID is the one its
    /// registry lists, and a mod's the one its declared content id gives.
    /// Without the item registry the key is all the server knows.
    fn knows_item(&self, item: Guid) -> bool {
        let id = crate::ItemId::from_guid(item);
        if !enshwick_content::__internal::knows_item(&self.host, id) {
            return false;
        }
        let Some(vanilla) = crate::content::vanilla::Vanilla::game() else {
            return true;
        };
        match enshwick_content::__internal::item_with_id(vanilla, id.get()) {
            Some(vanilla_item) => vanilla_item == item,
            None => enshwick_content::__internal::content_id(&self.host, item).is_some(),
        }
    }

    /// The first item of `values` the server's game does not know.
    fn unknown_item(&self, values: &[Value]) -> Option<Guid> {
        values.iter().find_map(|value| match value {
            Value::Item(item) if !self.knows_item(*item) => Some(*item),
            _ => None,
        })
    }

    /// `text` as the value of the text argument `arg`, or why it takes
    /// none: too long, or empty or with a control character.
    fn text_value(arg: &Arg, text: &str) -> Result<Value, ReadArgsError> {
        if arg.takes_text(text) {
            return Ok(Value::Text(text.to_owned()));
        }
        Err(Self::too_long(arg, text).unwrap_or(ReadArgsError::Usage))
    }

    /// The error of `text` given to the text argument `arg` when it is
    /// longer than `arg` takes.
    fn too_long(arg: &Arg, text: &str) -> Option<ReadArgsError> {
        let longer = u16::try_from(text.len()).map_or(true, |len| f64::from(len) > arg.range.1);
        longer.then(|| ReadArgsError::TooLong {
            label: arg.label.clone(),
            max: arg.range.1.to_string(),
        })
    }

    /// The error of the interface's `args` for `action` when one of their
    /// texts is longer than its argument takes, as a typed line answers;
    /// the interface keeps a text within it, so only a changed client sends
    /// one.
    pub(super) fn long_text(action: &Action, args: &str) -> Option<ReadArgsError> {
        args.split('\t')
            .zip(&action.args)
            .filter(|(_, arg)| arg.kind == ArgView::TEXT)
            .find_map(|(field, arg)| Self::too_long(arg, field.strip_prefix('t')?))
    }

    /// The one player named `name` among those online and `listed`.
    pub(super) fn player_named(
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

    /// Whether the interface's `values` fit `d`'s arguments, the one check
    /// of a value a client sent: their count and shape, each within its
    /// argument, and every item one the server's game knows. A typed line's
    /// values are read word by word against their arguments instead.
    pub(super) fn values_fit(&self, d: &Declared, values: &[Value]) -> Result<(), ReadArgsError> {
        let shape_fits = if d.action.on.is_some() {
            matches!(values, [Value::Player { .. }, Value::Toggle(_)])
        } else {
            values.len() == d.action.args.len()
                && values.iter().zip(&d.action.args).all(|(v, a)| fits(v, a))
        };
        if !shape_fits {
            return Err(ReadArgsError::Usage);
        }
        match self.unknown_item(values) {
            Some(item) => Err(ReadArgsError::NoItem {
                name: item.to_string(),
                near: Vec::new(),
            }),
            None => Ok(()),
        }
    }

    /// Fills in the players in `values`, which the interface sends by key,
    /// or as a typed name in a player's place: online, on the list of the
    /// object `target` names, or among the accounts the action knows.
    pub(super) fn name_players(
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

/// On or off, from a console word.
pub(in crate::actions) fn toggle_word(
    word: &str,
    texts: &Texts,
    language: Language,
) -> Option<bool> {
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
    fn a_run_of_a_line_keeps_its_spaces_and_drops_the_quotes_of_one_run() {
        let line = "say  two  spaces ";
        let all = words(line);
        assert_eq!(run_of_line(line, &all[1..]), Some("two  spaces"));
        let quoted = r#"say "Hi all""#;
        assert_eq!(run_of_line(quoted, &words(quoted)[1..]), Some("Hi all"));
        let mixed = r#"say "Hi" all"#;
        assert_eq!(run_of_line(mixed, &words(mixed)[1..]), Some(r#""Hi" all"#));
        assert_eq!(run_of_line("say", &[]), None);
        let counted = r#"give Stone  "Axe" 5"#;
        let all = words(counted);
        assert_eq!(
            run_of_line(counted, &all[1..3]),
            Some(r#"Stone  "Axe""#),
            "a run that stops before the last word keeps its closing quote"
        );
        let unclosed = r#"say "no end"#;
        assert_eq!(words(unclosed)[1].to, unclosed.len());
    }
}
