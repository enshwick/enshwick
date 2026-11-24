//! The items of a carried window as a client command names them: what the
//! window draws now, in the order it draws them, each with its id.
//!
//! The ids come from what the window's mod declared, as
//! [`Target`] reads them, so a caller finds an item by its action and the
//! command reaches the same widget the player would click.

use enshwick_core::abi::ClientWindowItem as Raw;
use enshwick_core::actions::sheet::{ArgView, Entry};
use enshwick_core::actions::window::{ItemView, View};

use super::Form;
use crate::actions::client_commands::windows::Target;
use crate::engine::texts;

/// One item of a carried window.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Listed {
    pub(super) target: Target,
    /// [`Raw::BUTTON`], [`Raw::SWITCH`] or [`Raw::FIELD`].
    pub(super) kind: u32,
    /// A field's argument kind, [`Raw::PLAYER`] to [`Raw::TEMPLATE`]; 0
    /// for other kinds.
    pub(super) field: u32,
    pub(super) label: String,
    /// The row's text or the action's label the item stands under.
    pub(super) line: String,
    /// [`Raw::ON`] and [`Raw::OFF`].
    pub(super) flags: u32,
}

impl Listed {
    fn button(target: Target, label: &str, line: &str, off: bool) -> Self {
        Self {
            target,
            kind: Raw::BUTTON,
            field: 0,
            label: label.to_owned(),
            line: line.to_owned(),
            flags: if off { Raw::OFF } else { 0 },
        }
    }
}

/// The field kind of `arg` as the ABI names it; `None` for one this loader
/// draws no field for.
pub(super) const fn field_kind(arg: &ArgView) -> Option<u32> {
    Some(match arg.kind {
        ArgView::PLAYER => Raw::PLAYER,
        ArgView::CHOICE => Raw::CHOICE,
        ArgView::NUMBER => Raw::NUMBER,
        ArgView::TOGGLE => Raw::TOGGLE,
        ArgView::TEXT => Raw::TEXT,
        ArgView::ITEM => Raw::ITEM,
        ArgView::TEMPLATE => Raw::TEMPLATE,
        _ => return None,
    })
}

/// The items `view` draws: its own, or, with `form` open, the form's, and
/// the window's Close last.
pub(super) fn list(view: &View, form: Option<&Form>) -> Vec<Listed> {
    let mut listed = Vec::new();
    match form {
        Some(form) => form_items(view, form, &mut listed),
        None => view_items(view, &mut listed),
    }
    let close = texts::text("actions.close");
    listed.push(Listed::button(Target::Close, close, "", false));
    listed
}

/// The items of `view` with no form open.
fn view_items(view: &View, listed: &mut Vec<Listed>) {
    // The row buttons running each action so far, for their `#n`.
    let mut counted: Vec<(&str, usize)> = Vec::new();
    for item in &view.items {
        match item {
            ItemView::Row { text, buttons } => {
                for button in buttons {
                    let action = button.action.as_str();
                    let n = if let Some((_, n)) = counted.iter_mut().find(|(a, _)| *a == action) {
                        *n += 1;
                        *n
                    } else {
                        counted.push((action, 0));
                        0
                    };
                    let target = Target::RowButton(action.to_owned(), n);
                    listed.push(Listed::button(target, &button.label, text, false));
                }
            }
            ItemView::Action(id) => {
                if let Some(entry) = view.entries.iter().find(|e| e.id == *id) {
                    action_items(entry, 0, listed);
                }
            }
            ItemView::Map(_) => {
                let open = texts::text("windows.map.open");
                listed.push(Listed::button(Target::Map, open, "", false));
            }
            _ => {}
        }
    }
}

/// The items of `form`, a button's form over `view`.
fn form_items(view: &View, form: &Form, listed: &mut Vec<Listed>) {
    // The draw cancels a form whose action left the window; only Close
    // stays.
    let Some(entry) = view.entries.iter().find(|e| e.id == form.action) else {
        return;
    };
    fields(entry, form.given.len(), &form.label, listed);
    let run = texts::text("windows.run");
    let target = Target::Run(entry.id.clone());
    listed.push(Listed::button(target, run, &form.label, !entry.offered));
    let cancel = texts::text("windows.cancel");
    listed.push(Listed::button(Target::Cancel, cancel, &form.label, false));
}

/// The items of `entry` drawn whole: a list's switches, or its fields from
/// `from` on and the button that runs it.
fn action_items(entry: &Entry, from: usize, listed: &mut Vec<Listed>) {
    if entry.per_player {
        for (key, name, on) in &entry.players {
            let mut flags = if *on { Raw::ON } else { 0 };
            if !entry.offered {
                flags |= Raw::OFF;
            }
            listed.push(Listed {
                target: Target::Switch(entry.id.clone(), *key),
                kind: Raw::SWITCH,
                field: 0,
                label: name.clone(),
                line: entry.label.clone(),
                flags,
            });
        }
        return;
    }
    fields(entry, from, &entry.label, listed);
    // An action without fields is one button under its own label.
    let label = if entry.args.is_empty() {
        entry.label.as_str()
    } else {
        texts::text("windows.run")
    };
    let target = Target::Run(entry.id.clone());
    listed.push(Listed::button(target, label, &entry.label, !entry.offered));
}

/// The fields of `entry`'s arguments from `from` on, under `line`.
fn fields(entry: &Entry, from: usize, line: &str, listed: &mut Vec<Listed>) {
    for (index, arg) in entry.args.iter().enumerate().skip(from) {
        let Some(field) = field_kind(arg) else {
            continue;
        };
        listed.push(Listed {
            target: Target::Field(entry.id.clone(), index),
            kind: Raw::FIELD,
            field,
            label: arg.label.clone(),
            line: line.to_owned(),
            flags: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use enshwick_core::PlayerKey;
    use enshwick_core::actions::Value;
    use enshwick_core::actions::window::ButtonView;

    use super::*;

    fn entry(id: &str, args: Vec<ArgView>) -> Entry {
        Entry {
            id: id.into(),
            label: id.to_uppercase(),
            offered: true,
            args,
            ..Entry::default()
        }
    }

    fn reason() -> ArgView {
        ArgView {
            kind: ArgView::TEXT,
            label: "Reason".into(),
            max: 64.0,
            ..ArgView::default()
        }
    }

    /// The window of the windows check: a row per player with a Ban
    /// button, an Announce action and a Friends list.
    fn check() -> View {
        let ban = |key: u64, name: &str| ItemView::Row {
            text: name.into(),
            buttons: vec![ButtonView {
                label: "Ban".into(),
                action: "ban".into(),
                args: vec![Value::Player {
                    key: PlayerKey::new(key),
                    name: String::new(),
                }],
            }],
        };
        let player = ArgView {
            kind: ArgView::PLAYER,
            label: "Player".into(),
            ..ArgView::default()
        };
        View {
            title: "Windows check".into(),
            items: vec![
                ItemView::Text("1 online".into()),
                ban(7, "Ann"),
                ban(8, "Bob"),
                ItemView::Action("say".into()),
                ItemView::Action("friends".into()),
            ],
            entries: vec![
                entry("ban", vec![player, reason()]),
                entry("say", vec![reason()]),
                Entry {
                    per_player: true,
                    players: vec![(PlayerKey::new(8), "Bob".into(), false)],
                    ..entry("friends", Vec::new())
                },
            ],
        }
    }

    fn ids(listed: &[Listed]) -> Vec<String> {
        listed.iter().map(|item| item.target.id()).collect()
    }

    #[test]
    fn a_window_lists_its_row_buttons_fields_switches_and_close_by_id() {
        let listed = list(&check(), None);
        assert_eq!(
            ids(&listed),
            ["ban#0", "ban#1", "say/0", "say", "friends@8", "@close"]
        );
        assert_eq!(listed[1].line, "Bob", "a row button stands under its row");
        assert_eq!((listed[2].kind, listed[2].field), (Raw::FIELD, Raw::TEXT));
        assert_eq!(listed[4].flags & Raw::ON, 0, "Bob's switch is off");
    }

    #[test]
    fn an_open_form_lists_only_the_fields_past_what_its_button_gave() {
        let form = Form {
            label: "Ban".into(),
            action: "ban".into(),
            given: vec![Value::Toggle(true)],
        };
        let listed = list(&check(), Some(&form));
        assert_eq!(ids(&listed), ["ban/1", "ban", "@cancel", "@close"]);
        let gone = Form {
            action: "kick".into(),
            ..form
        };
        assert_eq!(ids(&list(&check(), Some(&gone))), ["@close"]);
    }

    #[test]
    fn an_action_the_player_may_not_run_lists_its_button_and_switches_off() {
        let mut view = check();
        for entry in &mut view.entries {
            entry.offered = false;
        }
        let listed = list(&view, None);
        let off = |id: &str| {
            listed
                .iter()
                .find(|item| item.target.id() == id)
                .is_some_and(|item| item.flags & Raw::OFF != 0)
        };
        assert!(off("say") && off("friends@8"));
        assert!(!off("ban#0"), "a row's button always draws on");
    }
}
