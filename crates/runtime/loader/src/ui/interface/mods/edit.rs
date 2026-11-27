//! A client command's change on a mod's page of the screen: a key bound to
//! one of its controls, or one of its settings set, through the page's own
//! paths, as its key capture and its field make them.

use crate::actions::client_commands::edits::{self, Answer, Ask};
use crate::actions::controls::Row as Control;
use crate::base::configure::rows::{Lock, Row};
use crate::engine::loader::Loader;

use super::{Page, Screen, controls, page_rows, settings};

/// What the page does with a change.
#[derive(Debug, PartialEq, Eq)]
enum Step<'a> {
    /// Binds `bits` in `slot` of control `index`, among every control.
    Rebind {
        index: usize,
        slot: usize,
        bits: u32,
    },
    /// Sets setting `key` to the TOML `value`.
    Setting { key: &'a str, value: &'a str },
    /// The command ends: the page holds no such change, or refuses it.
    End(Answer),
}

/// The step for `ask` on the page of mod `page_mod`, whose settings rows are
/// `rows` (none on a page of key bindings alone), with every mod's
/// `controls`: a change for another mod's page, or for a control or setting
/// it does not show, is missing, and one for a setting it shows read-only
/// is refused.
fn plan<'a>(ask: &'a Ask, page_mod: &str, rows: Option<&[Row]>, controls: &[Control]) -> Step<'a> {
    if ask.mod_id() != page_mod {
        return Step::End(Answer::Missing);
    }
    match ask {
        Ask::Rebind {
            mod_id,
            control,
            slot,
            bits,
        } => {
            let id = crate::base::remote::control_id(mod_id, control);
            controls
                .iter()
                .find(|row| row.mod_id == *mod_id && row.id == id)
                .map_or(Step::End(Answer::Missing), |row| Step::Rebind {
                    index: row.index,
                    slot: *slot,
                    bits: *bits,
                })
        }
        Ask::Setting { key, value, .. } => {
            match rows.into_iter().flatten().find(|row| row.key == *key) {
                None => Step::End(Answer::Missing),
                Some(row) if row.lock != Lock::Open => Step::End(Answer::Refused),
                Some(_) => Step::Setting { key, value },
            }
        }
    }
}

/// Makes the change a client command handed the screen, on the page it
/// shows, and answers the command; the page shows the outcome as it shows a
/// click's.
pub(super) fn run(loader: &Loader, screen: &mut Screen) {
    let Some((id, ask)) = edits::asked() else {
        return;
    };
    if !edits::claim(id) {
        return;
    }
    let all = crate::actions::controls::rows();
    let answer = match &mut screen.page {
        Page::Settings {
            index,
            rows,
            place,
            controls: view,
            ..
        } => {
            let page_mod = loader.mods.get(*index).map_or("", |m| m.id.as_str());
            match plan(&ask, page_mod, Some(rows), &all) {
                Step::Rebind { index, slot, bits } => {
                    done(controls::rebind(loader, view, index, slot, bits))
                }
                Step::Setting { key, value } => {
                    let (line, ok) = settings::change(loader, *index, key, value);
                    *rows = page_rows(loader, *index, *place);
                    screen.message = Some((line, ok));
                    done(ok)
                }
                Step::End(answer) => answer,
            }
        }
        Page::Bindings {
            id: page_mod,
            controls: view,
            ..
        } => match plan(&ask, page_mod, None, &all) {
            Step::Rebind { index, slot, bits } => {
                done(controls::rebind(loader, view, index, slot, bits))
            }
            Step::Setting { .. } => Answer::Missing,
            Step::End(answer) => answer,
        },
        _ => Answer::Missing,
    };
    edits::answer(id, answer);
}

/// The answer to a change that went through, `ok`, or not.
const fn done(ok: bool) -> Answer {
    if ok { Answer::Done } else { Answer::Refused }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::configure::rows::Widget;

    fn control(index: usize, mod_id: &str, name: &str) -> Control {
        Control {
            index,
            id: crate::base::remote::control_id(mod_id, name),
            mod_id: mod_id.to_owned(),
            mod_name: mod_id.to_owned(),
            label: name.to_owned(),
            keys: [0; 2],
        }
    }

    fn setting(key: &str, lock: Lock) -> Row {
        Row {
            key: key.to_owned(),
            label: key.to_owned(),
            group: None,
            doc: String::new(),
            widget: Widget::Toggle(true),
            lock,
        }
    }

    #[test]
    fn a_rebind_finds_its_control_among_every_mod_s_only_on_its_mod_s_page() {
        let controls = [
            control(0, "you.other", "toggle"),
            control(1, "you.lights", "toggle"),
        ];
        let ask = Ask::Rebind {
            mod_id: "you.lights".to_owned(),
            control: "toggle".to_owned(),
            slot: 1,
            bits: 0x22,
        };
        assert_eq!(
            plan(&ask, "you.lights", None, &controls),
            Step::Rebind {
                index: 1,
                slot: 1,
                bits: 0x22
            },
            "the control of the page's mod, not another's of the same name"
        );
        assert_eq!(
            plan(&ask, "you.other", None, &controls),
            Step::End(Answer::Missing),
            "another mod's page"
        );
        let unknown = Ask::Rebind {
            mod_id: "you.lights".to_owned(),
            control: "dim".to_owned(),
            slot: 0,
            bits: 0,
        };
        assert_eq!(
            plan(&unknown, "you.lights", None, &controls),
            Step::End(Answer::Missing)
        );
    }

    #[test]
    fn a_setting_goes_through_only_where_its_page_shows_it_open() {
        let ask = |key: &str| Ask::Setting {
            mod_id: "you.lights".to_owned(),
            key: key.to_owned(),
            value: "12".to_owned(),
        };
        let rows = [
            setting("range", Lock::Open),
            setting("secret", Lock::Server),
        ];
        let range = ask("range");
        assert_eq!(
            plan(&range, "you.lights", Some(&rows), &[]),
            Step::Setting {
                key: "range",
                value: "12"
            }
        );
        assert_eq!(
            plan(&ask("secret"), "you.lights", Some(&rows), &[]),
            Step::End(Answer::Refused),
            "shown read-only"
        );
        assert_eq!(
            plan(&ask("dim"), "you.lights", Some(&rows), &[]),
            Step::End(Answer::Missing)
        );
        assert_eq!(
            plan(&range, "you.lights", None, &[]),
            Step::End(Answer::Missing),
            "a page of key bindings alone"
        );
        assert_eq!(
            plan(&range, "you.lights", Some(&[]), &[]),
            Step::End(Answer::Missing),
            "the main menu shows no settings"
        );
    }

    #[test]
    fn a_change_that_went_through_is_done_and_one_that_did_not_refused() {
        assert_eq!(done(true), Answer::Done);
        assert_eq!(done(false), Answer::Refused);
    }
}
