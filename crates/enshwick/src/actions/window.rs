//! Windows a mod shows on players' clients from the server, and their text form.
//!
//! The client's loader draws them with the game's widgets, though the mod
//! has no part there.
//!
//! A mod builds a [`Window`] of lines, rows with buttons and whole actions,
//! and shows it with [`Actions::show`](super::Actions::show). The server
//! renders it for each player in their language, with only what they may
//! run, and carries the text to their client. Mods never read that text;
//! the loader and [`Actions`](super::Actions) share it.
//!
//! The text is a line per item and tab-separated fields: `W` the title,
//! `T` a line, `R` a row, `B` a button of the row before it and `E` an
//! action drawn whole. The sheet's own lines follow, with the entries of
//! the actions the window names. Texts never hold a tab or a line break.

use core::fmt::Write as _;

use super::Value;
use super::sheet::{self, Entry, clean};

/// The most items a window holds: lines, rows and actions.
pub const MAX_ITEMS: usize = 64;

/// The most buttons a row holds.
pub const MAX_BUTTONS: usize = 4;

/// The most windows a mod keeps at once, each under its own id.
pub const MAX_WINDOWS: usize = 16;

/// The most bytes of a title, line or label the client shows; the server
/// cuts a longer one at a character's edge.
pub const MAX_LINE: usize = 160;

/// The most bytes of a window's text as the server carries it, room left
/// in the loader's message for the mod's id and name; items past it are
/// left out.
pub const MAX_BYTES: usize = 7_168;

/// Fills a text key with values in one reader's language.
type Say<'a> = &'a dyn Fn(&str, &[(&str, &str)]) -> String;

/// A text key and the values it is filled with, in each player's language.
#[derive(Clone, Debug, Default, PartialEq)]
struct Text {
    key: String,
    values: Vec<(String, String)>,
}

impl Text {
    fn new(key: &str, values: &[(&str, &str)]) -> Self {
        Self {
            key: key.to_owned(),
            values: values
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        }
    }

    /// The text as one line in a language, `say` filling its key.
    fn line(&self, say: Say<'_>) -> String {
        let values: Vec<(&str, &str)> = self
            .values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        let text = say(&self.key, &values);
        clean(&text[..text.floor_char_boundary(MAX_LINE)])
    }
}

/// A button of a [`Row`]: its label, and the action it runs with the values
/// it gives first.
#[derive(Clone, Debug, PartialEq)]
struct Button {
    label: Text,
    action: String,
    args: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
enum Item {
    Text(Text),
    Row(Row),
    Action(String),
}

/// A window of a mod of side server, drawn on players' clients with the
/// game's widgets: a title, then lines, rows with buttons and whole
/// actions, in order. Show it with [`Actions::show`](super::Actions::show).
///
/// Texts are keys of the mod's texts, filled in each player's language; a
/// key without a text shows as it is. A player gets the window only when
/// they may run every action it names, and each run is checked again on
/// the server.
///
/// # Examples
///
/// ```
/// use enshwick::actions::{Row, Value, Window};
///
/// let ann = Value::Player { key: enshwick::PlayerKey::new(7), name: "Ann".into() };
/// let players = Window::new("players", "Players online")
///     .text("{count} online", &[("count", "1")])
///     .row(Row::new("Ann", &[]).button("Kick", "kick", &[ann]))
///     .action("say");
/// assert_eq!(players.id(), "players");
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub(super) id: String,
    title: Text,
    items: Vec<Item>,
}

impl Window {
    /// An empty window titled `title`; `id`, 1 to 32 of `[a-z0-9-]`, names
    /// it to show it again, which updates it in place, or to close it.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn new(id: &str, title: &str) -> Self {
        Self {
            id: id.to_owned(),
            title: Text::new(title, &[]),
            items: Vec::new(),
        }
    }

    /// The window's id.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Adds a line: the text `key` filled with `values`.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn text(mut self, key: &str, values: &[(&str, &str)]) -> Self {
        self.items.push(Item::Text(Text::new(key, values)));
        self
    }

    /// Adds a row: a line with buttons beside it.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn row(mut self, row: Row) -> Self {
        self.items.push(Item::Row(row));
        self
    }

    /// Adds the mod's action `action` whole, as the platform draws it: its
    /// argument fields above a button that runs it, or, for a
    /// [list](super::Action::per_player), its players with a switch each.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn action(mut self, action: &str) -> Self {
        self.items.push(Item::Action(action.to_owned()));
        self
    }

    /// The ids of the actions the window names, rows' buttons included,
    /// in order and maybe more than once.
    pub(super) fn actions(&self) -> impl Iterator<Item = &str> {
        self.items.iter().flat_map(|item| {
            let ids: Vec<&str> = match item {
                Item::Text(_) => Vec::new(),
                Item::Row(row) => row.buttons.iter().map(|b| b.action.as_str()).collect(),
                Item::Action(action) => vec![action.as_str()],
            };
            ids
        })
    }

    /// Why the window cannot be shown: too many items or buttons, or a
    /// button giving more values than its action takes, `takes` saying how
    /// many each declared action takes; `None` when it can.
    pub(super) fn fault(&self, takes: &dyn Fn(&str) -> Option<usize>) -> Option<String> {
        if self.items.len() > MAX_ITEMS {
            return Some(format!("{} items, at most {MAX_ITEMS}", self.items.len()));
        }
        for item in &self.items {
            match item {
                Item::Row(row) if row.buttons.len() > MAX_BUTTONS => {
                    return Some(format!(
                        "a row of {} buttons, at most {MAX_BUTTONS}",
                        row.buttons.len()
                    ));
                }
                Item::Row(row) => {
                    for button in &row.buttons {
                        match takes(&button.action) {
                            None => return Some(format!("no action {}", button.action)),
                            Some(n) if button.args.len() > n => {
                                return Some(format!(
                                    "a button gives {} values to {}, which takes {n}",
                                    button.args.len(),
                                    button.action
                                ));
                            }
                            Some(_) => {}
                        }
                    }
                }
                Item::Action(action) if takes(action).is_none() => {
                    return Some(format!("no action {action}"));
                }
                _ => {}
            }
        }
        None
    }

    /// The window's text for one reader, `say` filling a text key in their
    /// language, with `entries`, the sheet entries of the actions it names.
    pub(super) fn render(&self, say: Say<'_>, entries: &[Entry]) -> String {
        let tail = sheet::encode(entries);
        let mut out = format!("W\t{}\n", self.title.line(say));
        for item in &self.items {
            let mut lines = String::new();
            match item {
                Item::Text(text) => {
                    let _ = writeln!(lines, "T\t{}", text.line(say));
                }
                Item::Row(row) => {
                    let _ = writeln!(lines, "R\t{}", row.text.line(say));
                    for button in &row.buttons {
                        let args = sheet::encode_args(&button.args).replace('\t', ARG);
                        let _ = writeln!(
                            lines,
                            "B\t{}\t{}\t{args}",
                            button.label.line(say),
                            clean(&button.action)
                        );
                    }
                }
                Item::Action(action) => {
                    let _ = writeln!(lines, "E\t{}", clean(action));
                }
            }
            if out.len() + lines.len() + tail.len() > MAX_BYTES {
                break;
            }
            out.push_str(&lines);
        }
        out.push_str(&tail);
        out
    }
}

/// A row of a [`Window`]: a line with buttons beside it, each running one
/// of the mod's actions.
///
/// # Examples
///
/// See [`Window`].
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    text: Text,
    buttons: Vec<Button>,
}

impl Row {
    /// A row reading the text `key` filled with `values`, without buttons.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn new(key: &str, values: &[(&str, &str)]) -> Self {
        Self {
            text: Text::new(key, values),
            buttons: Vec::new(),
        }
    }

    /// Adds a button labelled `label` that runs the mod's action `action`
    /// with `args` as its first values. When the action takes more, a press
    /// opens a dialog with fields for the rest, such as a ban's reason after
    /// the row's player.
    ///
    /// # Examples
    ///
    /// See [`Window`].
    #[must_use]
    pub fn button(mut self, label: &str, action: &str, args: &[Value]) -> Self {
        self.buttons.push(Button {
            label: Text::new(label, &[]),
            action: action.to_owned(),
            args: args.to_vec(),
        });
        self
    }
}

/// The separator of a button's values inside its field.
const ARG: &str = "\u{1f}";

/// A window as its text reads on the client.
///
/// # Examples
///
/// ```
/// use enshwick::actions::window::{ItemView, decode};
///
/// let window = decode("W\tPlayers\nT\t2 online\nE\tsay\n").unwrap();
/// assert_eq!(window.title, "Players");
/// assert_eq!(window.items, [ItemView::Text("2 online".into()), ItemView::Action("say".into())]);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// The title, in the player's language.
    pub title: String,
    /// The items, in order.
    pub items: Vec<ItemView>,
    /// The sheet entries of the actions the items name.
    pub entries: Vec<Entry>,
}

/// An item of a [`View`].
///
/// # Examples
///
/// See [`View`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ItemView {
    /// A line.
    Text(String),
    /// A line with buttons.
    Row {
        /// The line.
        text: String,
        /// Its buttons.
        buttons: Vec<ButtonView>,
    },
    /// An action drawn whole, by id; its entry is in [`View::entries`].
    Action(String),
}

/// A button of a row, as the client reads it.
///
/// # Examples
///
/// See [`View`].
#[derive(Clone, Debug, PartialEq)]
pub struct ButtonView {
    /// Its label.
    pub label: String,
    /// The action it runs.
    pub action: String,
    /// The values it gives first.
    pub args: Vec<Value>,
}

/// The window of `text`, in bounds: `None` past [`MAX_BYTES`] or without
/// its title first. Items past [`MAX_ITEMS`], buttons past
/// [`MAX_BUTTONS`] and lines it cannot read are left out, and texts are cut
/// to [`MAX_LINE`].
///
/// # Examples
///
/// See [`View`].
#[must_use]
pub fn decode(text: &str) -> Option<View> {
    if text.len() > MAX_BYTES {
        return None;
    }
    let mut lines = text.lines();
    let title = lines.next()?.strip_prefix("W\t")?;
    let cut = |text: &str| text[..text.floor_char_boundary(MAX_LINE)].to_owned();
    let mut view = View {
        title: cut(title),
        ..View::default()
    };
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let item = match fields.as_slice() {
            ["T", text] => ItemView::Text(cut(text)),
            ["R", text] => ItemView::Row {
                text: cut(text),
                buttons: Vec::new(),
            },
            ["E", action] if super::is_valid_id(action) => ItemView::Action((*action).to_owned()),
            ["B", label, action, args] => {
                if let (Some(ItemView::Row { buttons, .. }), true, Some(args)) = (
                    view.items.last_mut(),
                    super::is_valid_id(action),
                    sheet::decode_args(&args.replace(ARG, "\t")),
                ) && buttons.len() < MAX_BUTTONS
                {
                    buttons.push(ButtonView {
                        label: cut(label),
                        action: (*action).to_owned(),
                        args,
                    });
                }
                continue;
            }
            _ => continue,
        };
        if view.items.len() < MAX_ITEMS {
            view.items.push(item);
        }
    }
    view.entries = sheet::decode(text);
    view.entries.truncate(MAX_ITEMS);
    Some(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlayerKey;

    fn say(key: &str, values: &[(&str, &str)]) -> String {
        values.iter().fold(key.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
    }

    #[test]
    fn a_window_round_trips_with_its_rows_buttons_and_actions() {
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        let window = Window::new("players", "Players")
            .text("{n}\tonline", &[("n", "2")])
            .row(
                Row::new("Ann", &[])
                    .button("Kick", "kick", core::slice::from_ref(&ann))
                    .button("Say", "say", &[Value::Text("hi there".into())]),
            )
            .action("allowed");
        let entry = Entry {
            id: "allowed".into(),
            label: "Allowed".into(),
            per_player: true,
            offered: true,
            ..Entry::default()
        };
        let view = decode(&window.render(&say, core::slice::from_ref(&entry))).unwrap();
        assert_eq!(view.title, "Players");
        assert_eq!(
            view.items,
            [
                ItemView::Text("2 online".into()),
                ItemView::Row {
                    text: "Ann".into(),
                    buttons: vec![
                        ButtonView {
                            label: "Kick".into(),
                            action: "kick".into(),
                            args: vec![ann],
                        },
                        ButtonView {
                            label: "Say".into(),
                            action: "say".into(),
                            args: vec![Value::Text("hi there".into())],
                        },
                    ],
                },
                ItemView::Action("allowed".into()),
            ]
        );
        assert_eq!(view.entries, [entry]);
        assert_eq!(
            window.actions().collect::<Vec<_>>(),
            ["kick", "say", "allowed"]
        );
    }

    #[test]
    fn a_window_stays_within_its_bounds_both_ways() {
        let long = "x".repeat(MAX_LINE * 2);
        let mut window = Window::new("big", &long);
        for _ in 0..MAX_ITEMS {
            window = window.text(&long, &[]);
        }
        let text = window.render(&say, &[]);
        assert!(text.len() <= MAX_BYTES, "{}", text.len());
        let view = decode(&text).unwrap();
        assert_eq!(view.title.len(), MAX_LINE);
        assert!(
            view.items.len() < MAX_ITEMS,
            "items past the bytes are left out"
        );

        assert_eq!(decode("T\tno title\n"), None);
        assert_eq!(
            decode(&format!("W\tt\n{}", "T\tx\n".repeat(MAX_BYTES))),
            None
        );
        let many = format!("W\tt\n{}", "T\tx\n".repeat(MAX_ITEMS + 5));
        assert_eq!(decode(&many).unwrap().items.len(), MAX_ITEMS);
        let buttons = format!("W\tt\nR\tr\n{}", "B\tgo\tgo\t\n".repeat(MAX_BUTTONS + 2));
        let Some(ItemView::Row { buttons, .. }) = decode(&buttons).unwrap().items.pop() else {
            panic!("a row");
        };
        assert_eq!(buttons.len(), MAX_BUTTONS);
        let bad = decode("W\tt\nB\tlost\tgo\t\nE\tBAD ID\nR\tr\nB\tx\tgo\tq9\n").unwrap();
        assert_eq!(
            bad.items,
            [ItemView::Row {
                text: "r".into(),
                buttons: Vec::new()
            }],
            "a button without its row, a bad id and bad values are left out"
        );
    }

    #[test]
    fn a_window_names_its_faults() {
        let takes = |action: &str| (action == "kick").then_some(1);
        let fine = Window::new("w", "W").row(Row::new("r", &[]).button("Kick", "kick", &[]));
        assert_eq!(fine.fault(&takes), None);
        let unknown = Window::new("w", "W").action("fly");
        assert_eq!(unknown.fault(&takes).as_deref(), Some("no action fly"));
        let crowded =
            (0..=MAX_BUTTONS).fold(Row::new("r", &[]), |row, _| row.button("k", "kick", &[]));
        assert!(Window::new("w", "W").row(crowded).fault(&takes).is_some());
        let over =
            Row::new("r", &[]).button("k", "kick", &[Value::Toggle(true), Value::Toggle(false)]);
        assert!(Window::new("w", "W").row(over).fault(&takes).is_some());
        let long = (0..=MAX_ITEMS).fold(Window::new("w", "W"), |w, _| w.text("t", &[]));
        assert!(long.fault(&takes).is_some());
    }
}
