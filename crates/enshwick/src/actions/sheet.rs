//! The sheet: the actions a player may use, as the server's loader hands
//! them to the client's, which draws them. Mods never read it; the loader
//! and [`Actions`](super::Actions) share it.
//!
//! A sheet is text, a line per item and tab-separated fields: `A` an
//! action, `G` an argument of the action before it, `O` a player of an
//! action's list. Texts never hold a tab or a line break.

use core::fmt::Write as _;

use super::Value;
use crate::PlayerKey;

/// An action a player may use, as the interface shows it.
///
/// # Examples
///
/// ```
/// use enshwick::actions::sheet::{Entry, decode, encode};
///
/// let entry = Entry { id: "list".into(), label: "Who may use it".into(), ..Entry::default() };
/// assert_eq!(decode(&encode(&[entry.clone()])), [entry]);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// The action's id.
    pub id: String,
    /// Its label, in the player's language.
    pub label: String,
    /// A line on what it does; may be empty.
    pub help: String,
    /// Whether it is a list of players with a switch each.
    pub per_player: bool,
    /// Whether it shows as a hint on the HUD when at hand.
    pub hint: bool,
    /// Whether the player may run it now.
    pub offered: bool,
    /// What it acts on, such as "your locker at (2, 10, -4)", when offered;
    /// why not, when not.
    pub subject: String,
    /// Its arguments, for an action that is not a list.
    pub args: Vec<ArgView>,
    /// The players of a list: account key, name and whether switched on.
    pub players: Vec<(PlayerKey, String, bool)>,
}

/// An argument as the interface shows it.
///
/// # Examples
///
/// See [`Entry`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArgView {
    /// What it is: [`ArgView::PLAYER`], [`ArgView::CHOICE`],
    /// [`ArgView::NUMBER`] or [`ArgView::TOGGLE`].
    pub kind: u8,
    /// Its label, in the player's language.
    pub label: String,
    /// A number's least value.
    pub min: f64,
    /// A number's greatest value.
    pub max: f64,
    /// A number's step; 0 for any value.
    pub step: f64,
    /// A choice's options, in the player's language.
    pub choices: Vec<String>,
    /// A player argument's options: account key and name.
    pub players: Vec<(PlayerKey, String)>,
}

impl ArgView {
    /// One of the server's players.
    pub const PLAYER: u8 = 0;
    /// One of a few options.
    pub const CHOICE: u8 = 1;
    /// A number in a range.
    pub const NUMBER: u8 = 2;
    /// On or off.
    pub const TOGGLE: u8 = 3;
}

/// The separator of a list inside a field.
const ITEM: char = '\u{1f}';
/// The separator of a player's key and name inside a list item.
const PAIR: char = '\u{1e}';

/// `text` with no tab, line break or separator left in it.
///
/// # Examples
///
/// ```
/// use enshwick::actions::sheet::clean;
///
/// assert_eq!(clean("two\tparts\n"), "two parts ");
/// ```
#[must_use]
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' | ITEM | PAIR => ' ',
            c => c,
        })
        .collect()
}

/// The sheet's text for `entries`.
///
/// # Examples
///
/// See [`Entry`].
#[must_use]
pub fn encode(entries: &[Entry]) -> String {
    let mut out = String::new();
    for entry in entries {
        let flags =
            u8::from(entry.per_player) | u8::from(entry.hint) << 1 | u8::from(entry.offered) << 2;
        let _ = writeln!(
            out,
            "A\t{}\t{flags}\t{}\t{}\t{}",
            clean(&entry.id),
            clean(&entry.label),
            clean(&entry.help),
            clean(&entry.subject)
        );
        for arg in &entry.args {
            let choices = if arg.kind == ArgView::PLAYER {
                arg.players
                    .iter()
                    .map(|(key, name)| format!("{key:016x}{PAIR}{}", clean(name)))
                    .collect::<Vec<_>>()
            } else {
                arg.choices.iter().map(|c| clean(c)).collect()
            };
            let _ = writeln!(
                out,
                "G\t{}\t{}\t{}\t{}\t{}\t{}",
                arg.kind,
                clean(&arg.label),
                arg.min,
                arg.max,
                arg.step,
                choices.join(&ITEM.to_string())
            );
        }
        for (key, name, on) in &entry.players {
            let _ = writeln!(out, "O\t{key:016x}\t{}\t{}", u8::from(*on), clean(name));
        }
    }
    out
}

/// The entries of a sheet's text; lines it cannot read are skipped.
///
/// # Examples
///
/// See [`Entry`].
#[must_use]
pub fn decode(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        match fields.as_slice() {
            ["A", id, flags, label, help, subject] => {
                let flags = flags.parse::<u8>().unwrap_or(0);
                entries.push(Entry {
                    id: (*id).to_owned(),
                    label: (*label).to_owned(),
                    help: (*help).to_owned(),
                    per_player: flags & 1 != 0,
                    hint: flags & 2 != 0,
                    offered: flags & 4 != 0,
                    subject: (*subject).to_owned(),
                    args: Vec::new(),
                    players: Vec::new(),
                });
            }
            ["G", kind, label, min, max, step, choices] => {
                let Some(entry) = entries.last_mut() else {
                    continue;
                };
                let kind = kind.parse::<u8>().unwrap_or(u8::MAX);
                let items = choices.split(ITEM).filter(|c| !c.is_empty());
                let (choices, players) = if kind == ArgView::PLAYER {
                    let players = items
                        .filter_map(|item| {
                            let (key, name) = item.split_once(PAIR)?;
                            Some((
                                u64::from_str_radix(key, 16).ok().map(PlayerKey::new)?,
                                name.to_owned(),
                            ))
                        })
                        .collect();
                    (Vec::new(), players)
                } else {
                    (items.map(str::to_owned).collect(), Vec::new())
                };
                entry.args.push(ArgView {
                    kind,
                    label: (*label).to_owned(),
                    min: min.parse().unwrap_or(0.0),
                    max: max.parse().unwrap_or(0.0),
                    step: step.parse().unwrap_or(0.0),
                    choices,
                    players,
                });
            }
            ["O", key, on, name] => {
                let (Some(entry), Ok(key)) = (
                    entries.last_mut(),
                    u64::from_str_radix(key, 16).map(PlayerKey::new),
                ) else {
                    continue;
                };
                entry.players.push((key, (*name).to_owned(), *on == "1"));
            }
            _ => {}
        }
    }
    entries
}

/// The text of an action's arguments, as the interface sends them to the
/// server; a player argument travels by account key alone.
///
/// # Examples
///
/// ```
/// use enshwick::actions::Value;
/// use enshwick::actions::sheet::{decode_args, encode_args};
///
/// let args = [Value::Choice(2), Value::Number(1.5), Value::Toggle(true)];
/// assert_eq!(decode_args(&encode_args(&args)), Some(args.to_vec()));
/// ```
#[must_use]
pub fn encode_args(args: &[Value]) -> String {
    let mut out = String::new();
    for (i, value) in args.iter().enumerate() {
        if i > 0 {
            out.push('\t');
        }
        let _ = match value {
            Value::Player { key, .. } => write!(out, "p{key:016x}"),
            Value::Choice(index) => write!(out, "c{index}"),
            Value::Number(number) => write!(out, "n{number}"),
            Value::Toggle(on) => write!(out, "b{}", u8::from(*on)),
        };
    }
    out
}

/// The arguments of `text`, players without their names; `None` when a
/// field cannot be read.
///
/// # Examples
///
/// See [`encode_args`].
#[must_use]
pub fn decode_args(text: &str) -> Option<Vec<Value>> {
    if text.is_empty() {
        return Some(Vec::new());
    }
    text.split('\t')
        .map(|field| {
            let (tag, rest) = field.split_at_checked(1)?;
            Some(match tag {
                "p" => Value::Player {
                    key: u64::from_str_radix(rest, 16).ok().map(PlayerKey::new)?,
                    name: String::new(),
                },
                "c" => Value::Choice(rest.parse().ok()?),
                "n" => Value::Number(rest.parse::<f64>().ok().filter(|n| n.is_finite())?),
                "b" => Value::Toggle(rest == "1"),
                _ => return None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sheet_round_trips_with_arguments_and_players() {
        let entries = vec![
            Entry {
                id: "access".into(),
                label: "Who may use it".into(),
                help: "Let players use the locker".into(),
                per_player: true,
                hint: true,
                offered: true,
                subject: "Your locker at (2, 10, -4)".into(),
                args: Vec::new(),
                players: vec![
                    (PlayerKey::new(7), "Ann".into(), true),
                    (PlayerKey::new(u64::MAX), "Bob\tLee".into(), false),
                ],
            },
            Entry {
                id: "give".into(),
                label: "Give".into(),
                offered: false,
                subject: "No locker in reach".into(),
                args: vec![
                    ArgView {
                        kind: ArgView::PLAYER,
                        label: "To".into(),
                        players: vec![(PlayerKey::new(9), "Cy".into())],
                        ..ArgView::default()
                    },
                    ArgView {
                        kind: ArgView::CHOICE,
                        label: "What".into(),
                        choices: vec!["Wood".into(), "Stone".into()],
                        ..ArgView::default()
                    },
                    ArgView {
                        kind: ArgView::NUMBER,
                        label: "How many".into(),
                        min: 1.0,
                        max: 99.0,
                        step: 1.0,
                        ..ArgView::default()
                    },
                ],
                ..Entry::default()
            },
        ];
        let mut expected = entries.clone();
        expected[0].players[1].1 = "Bob Lee".into();
        assert_eq!(decode(&encode(&entries)), expected);
        assert!(decode("G\t0\tx\t0\t0\t0\t\nnonsense\n").is_empty());
    }

    #[test]
    fn arguments_round_trip_and_bad_ones_are_refused() {
        let args = vec![
            Value::Player {
                key: PlayerKey::new(0xdead_beef),
                name: String::new(),
            },
            Value::Toggle(false),
        ];
        assert_eq!(decode_args(&encode_args(&args)), Some(args));
        assert_eq!(decode_args(""), Some(Vec::new()));
        assert_eq!(decode_args("x1"), None);
        assert_eq!(decode_args("nNaN"), None);
        assert_eq!(decode_args("pzz"), None);
    }
}
