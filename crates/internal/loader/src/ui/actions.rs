//! Player actions in the interface: the actions the server offers, drawn
//! with the game's settings rows and buttons on the Mods screen.

use enshwick::abi::UiWidget;
use enshwick::actions::Value;
use enshwick::actions::sheet::{ArgView, Entry, encode_args};

use crate::actions::ModSheet;
use crate::texts;

use super::Painter;

/// Players of a list a page shows.
const PLAYERS_PER_PAGE: usize = 5;

/// The value a row holds for an argument.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Held {
    Index(u32),
    Number(f32),
    On(bool),
}

/// What a page of actions shows and what the player picked on it.
#[derive(Debug)]
pub(super) struct View {
    /// The mod whose actions show.
    id: String,
    /// The action showing, by index.
    action: usize,
    /// The first player of a list's page.
    first: usize,
    /// The rows' values, for the action they were made for.
    held: Vec<Held>,
    held_for: Option<String>,
    /// The last outcome, and whether it went through.
    message: Option<(String, bool)>,
    /// What was heard last.
    news: u64,
}

impl View {
    /// A view of mod `id`'s actions, asking the server for them.
    pub(super) fn new(id: String) -> Self {
        let asked = crate::actions::ask_server();
        Self {
            id,
            action: 0,
            first: 0,
            held: Vec::new(),
            held_for: None,
            message: (!asked).then(|| (texts::text("actions.closed").to_owned(), false)),
            news: crate::actions::heard(|h| h.news),
        }
    }

    /// The mod showing, for the screen's title: its name.
    pub(super) fn title(&self) -> String {
        let name = crate::actions::heard(|h| {
            h.sheets
                .as_ref()?
                .iter()
                .find(|m| m.id == self.id)
                .map(|m| m.label.clone())
        });
        match name {
            Some(name) => texts::fill("actions.title", &[("name", &name)]),
            None => texts::text("actions").to_owned(),
        }
    }
}

/// Whether the server offers mod `id` actions, as last heard.
pub(super) fn offers(id: &str) -> bool {
    crate::actions::heard(|h| {
        h.sheets
            .as_ref()
            .is_some_and(|s| s.iter().any(|m| m.id == id && !m.entries.is_empty()))
    })
}

/// Draws `view`'s page.
pub(super) fn body(paint: &Painter<'_>, view: &mut View) {
    let (news, result) = crate::actions::heard(|h| (h.news, h.result.clone()));
    if news != view.news {
        view.news = news;
        if let Some((ok, text)) = result {
            view.message = Some((text, ok));
        }
    }
    if let Some((text, ok)) = &view.message {
        let style = if *ok {
            UiWidget::STYLE_POSITIVE
        } else {
            UiWidget::STYLE_NEGATIVE
        };
        paint.label(text.as_bytes(), style, true, true);
    }
    let Some(sheets) = crate::actions::heard(|h| h.sheets.clone()) else {
        paint.label(
            texts::text("actions.asking").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return;
    };
    match sheets.iter().find(|m| m.id == view.id) {
        Some(sheet) if !sheet.entries.is_empty() => actions(paint, view, sheet),
        _ => paint.label(
            texts::text("actions.none").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            true,
        ),
    }
}

/// A mod's actions, one at a time.
fn actions(paint: &Painter<'_>, view: &mut View, sheet: &ModSheet) {
    view.action = view.action.min(sheet.entries.len() - 1);
    let entry = &sheet.entries[view.action];
    paint.label(entry.label.as_bytes(), UiWidget::STYLE_H3, false, false);
    if !entry.help.is_empty() {
        paint.label(
            entry.help.as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    if !entry.subject.is_empty() {
        let style = if entry.offered {
            UiWidget::STYLE_BODY
        } else {
            UiWidget::STYLE_WARNING
        };
        paint.label(entry.subject.as_bytes(), style, true, false);
    }
    let run = if entry.per_player {
        players(paint, view, entry)
    } else {
        button(paint, view, entry)
    };
    if let Some(args) = run {
        let sent = crate::actions::run_on_server(&sheet.id, &entry.id, &args);
        view.message = Some(if sent {
            (texts::text("actions.sent").to_owned(), true)
        } else {
            (texts::text("actions.closed").to_owned(), false)
        });
    }
    if view.action + 1 < sheet.entries.len()
        && paint.button(texts::text("actions.next").as_bytes(), true)
    {
        view.action += 1;
        view.first = 0;
    }
    if view.action > 0 && paint.button(texts::text("actions.previous").as_bytes(), true) {
        view.action -= 1;
        view.first = 0;
    }
}

/// A list of players with a switch each; the arguments of a flip.
fn players(paint: &Painter<'_>, view: &mut View, entry: &Entry) -> Option<String> {
    if entry.players.is_empty() {
        paint.label(
            texts::text("actions.no-players").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
        return None;
    }
    let count = entry.players.len();
    view.first = view.first.min(count - 1) / PLAYERS_PER_PAGE * PLAYERS_PER_PAGE;
    let mut flipped = None;
    for (key, name, on) in entry.players.iter().skip(view.first).take(PLAYERS_PER_PAGE) {
        if let Some(now) = paint.toggle(name.as_bytes(), *on, !entry.offered) {
            flipped = Some(encode_args(&[
                Value::Player {
                    key: *key,
                    name: String::new(),
                },
                Value::Toggle(now),
            ]));
        }
    }
    if view.first + PLAYERS_PER_PAGE < count
        && paint.button(texts::text("actions.players.next").as_bytes(), true)
    {
        view.first += PLAYERS_PER_PAGE;
    }
    if view.first > 0 && paint.button(texts::text("actions.players.previous").as_bytes(), true) {
        view.first = view.first.saturating_sub(PLAYERS_PER_PAGE);
    }
    flipped
}

/// The value a fresh row of `arg` shows.
fn fresh(arg: &ArgView) -> Held {
    match arg.kind {
        ArgView::NUMBER => {
            #[expect(clippy::cast_possible_truncation, reason = "a row shows an f32")]
            let min = arg.min as f32;
            Held::Number(min)
        }
        ArgView::TOGGLE => Held::On(false),
        _ => Held::Index(0),
    }
}

/// The steps of a number row: its stops, both ends included; 0 for any
/// value.
fn steps(arg: &ArgView) -> u32 {
    if arg.step <= 0.0 || arg.max <= arg.min {
        return 0;
    }
    let stops = ((arg.max - arg.min) / arg.step).round() + 1.0;
    if (2.0..=1000.0).contains(&stops) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked to lie in 2..=1000"
        )]
        let stops = stops as u32;
        stops
    } else {
        0
    }
}

/// An action's argument rows and its button; the arguments of a press.
fn button(paint: &Painter<'_>, view: &mut View, entry: &Entry) -> Option<String> {
    if view.held_for.as_deref() != Some(entry.id.as_str()) || view.held.len() != entry.args.len() {
        view.held = entry.args.iter().map(fresh).collect();
        view.held_for = Some(entry.id.clone());
    }
    let read_only = !entry.offered;
    for (arg, held) in entry.args.iter().zip(view.held.iter_mut()) {
        match (arg.kind, *held) {
            (ArgView::PLAYER, Held::Index(index)) => {
                let names: Vec<&[u8]> = arg.players.iter().map(|(_, n)| n.as_bytes()).collect();
                if let Some(picked) = paint.dropdown(arg.label.as_bytes(), &names, index, read_only)
                {
                    *held = Held::Index(picked);
                }
            }
            (ArgView::CHOICE, Held::Index(index)) => {
                let names: Vec<&[u8]> = arg.choices.iter().map(String::as_bytes).collect();
                if let Some(picked) = paint.dropdown(arg.label.as_bytes(), &names, index, read_only)
                {
                    *held = Held::Index(picked);
                }
            }
            (ArgView::NUMBER, Held::Number(value)) => {
                #[expect(clippy::cast_possible_truncation, reason = "a row shows an f32")]
                let range = (arg.min as f32, arg.max as f32);
                if let Some(now) =
                    paint.slider(arg.label.as_bytes(), value, range, steps(arg), read_only)
                {
                    *held = Held::Number(now);
                }
            }
            (ArgView::TOGGLE, Held::On(on)) => {
                if let Some(now) = paint.toggle(arg.label.as_bytes(), on, read_only) {
                    *held = Held::On(now);
                }
            }
            _ => {}
        }
    }
    if !paint.button(entry.label.as_bytes(), entry.offered) {
        return None;
    }
    let mut values = Vec::with_capacity(entry.args.len());
    for (arg, held) in entry.args.iter().zip(&view.held) {
        values.push(match (arg.kind, *held) {
            (ArgView::PLAYER, Held::Index(index)) => {
                let (key, _) = arg.players.get(index as usize)?;
                Value::Player {
                    key: *key,
                    name: String::new(),
                }
            }
            (ArgView::CHOICE, Held::Index(index)) => Value::Choice(index as usize),
            (ArgView::NUMBER, Held::Number(value)) => Value::Number(snap(arg, f64::from(value))),
            (ArgView::TOGGLE, Held::On(on)) => Value::Toggle(on),
            _ => return None,
        });
    }
    Some(encode_args(&values))
}

/// `value` on the nearest step of `arg`, inside its range.
fn snap(arg: &ArgView, value: f64) -> f64 {
    let stepped = if arg.step > 0.0 {
        arg.min + ((value - arg.min) / arg.step).round() * arg.step
    } else {
        value
    };
    stepped.clamp(arg.min, arg.max.max(arg.min))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_snap_to_their_steps_and_count_their_stops() {
        let arg = ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            max: 60.0,
            step: 1.0,
            ..ArgView::default()
        };
        assert_eq!(steps(&arg), 60);
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(near(snap(&arg, 4.6), 5.0));
        assert!(near(snap(&arg, 99.0), 60.0));
        let any = ArgView { step: 0.0, ..arg };
        assert_eq!(steps(&any), 0);
        assert!(near(snap(&any, 4.6), 4.6));
    }
}
