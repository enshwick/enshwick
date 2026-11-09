//! Carried windows on the client: the windows the server's mods show this
//! player, drawn with the game's widgets though the mods have no part here.
//!
//! The newest window shows; Close takes it away on this client only, and
//! the one under it shows. A button or a list's switch runs the mod's
//! action on the server, which checks it again, and a button whose action
//! takes more values opens a form for the rest. An action drawn whole shows
//! its fields above a button that runs it. Long windows scroll.

use core::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::PlayerKey;
use enshwick::abi::UiWidget;
use enshwick::actions::Value;
use enshwick::actions::sheet::{ArgView, Entry, encode_args};
use enshwick::actions::window::{ButtonView, ItemView, View, decode};

use super::engine::{Engine, Scroll, TextBuffer, Window};
use super::{Painter, copy};
use crate::locks::Relock;
use crate::texts;

/// The windows the client keeps at most; a new one past it drops the
/// oldest, as the server notes it.
const MAX_HELD: usize = 8;
/// Rows of a window this far apart.
const GAP: f32 = 14.0;
/// The most steps a number's slider takes.
const MAX_STEPS: f64 = 1_000.0;

/// A window a server's mod showed this client.
#[derive(Clone, Debug, PartialEq)]
struct Held {
    mod_id: String,
    window: String,
    view: View,
}

/// The windows the client holds, the newest last.
struct Carried {
    held: Mutex<Vec<Held>>,
    /// Set while `held` may hold anything, so a frame without windows takes
    /// no lock.
    any: AtomicBool,
}

static CARRIED: Carried = Carried {
    held: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
};

/// `held` once the server sent mod `id`'s window `window` as `view`: in
/// place where the client holds it, or last when `open`, the oldest going
/// past [`MAX_HELD`]; an update of a window the client closed changes
/// nothing.
fn hold(held: &mut Vec<Held>, id: &str, window: &str, open: bool, view: View) {
    match held
        .iter_mut()
        .find(|h| h.mod_id == id && h.window == window)
    {
        Some(kept) => kept.view = view,
        None if open => {
            held.push(Held {
                mod_id: id.to_owned(),
                window: window.to_owned(),
                view,
            });
            if held.len() > MAX_HELD {
                held.remove(0);
            }
        }
        None => {}
    }
}

/// The server sent mod `id`'s window `window` as `text`, opening it when
/// `open`; a text that does not read is dropped.
pub(crate) fn heard(id: &str, window: &str, open: bool, text: &str) {
    let Some(view) = decode(text) else {
        crate::log!(DEBUG, "windows: {id}:{window} does not read");
        return;
    };
    let mut held = CARRIED.held.relock();
    hold(&mut held, id, window, open, view);
    CARRIED.any.store(!held.is_empty(), Ordering::Release);
}

/// The server closed mod `id`'s window `window`.
pub(crate) fn closed(id: &str, window: &str) {
    let mut held = CARRIED.held.relock();
    held.retain(|h| !(h.mod_id == id && h.window == window));
    CARRIED.any.store(!held.is_empty(), Ordering::Release);
}

/// Forgets every window, as when the client leaves its server.
pub(crate) fn forget() {
    CARRIED.held.relock().clear();
    CARRIED.any.store(false, Ordering::Release);
}

/// What the player set in one field.
struct Field {
    pick: u32,
    /// The account `pick` named when it was last drawn.
    picked: Option<PlayerKey>,
    number: f32,
    on: bool,
    text: Box<TextBuffer>,
}

impl Field {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a slider takes an f32; the server checks the value it gets"
    )]
    fn new(arg: &ArgView) -> Self {
        Self {
            pick: 0,
            picked: None,
            number: arg.min as f32,
            on: false,
            text: TextBuffer::new(),
        }
    }
}

/// A button's form: the values its button gave, and fields for the rest.
#[derive(Clone, Debug, PartialEq)]
struct Form {
    label: String,
    action: String,
    given: Vec<Value>,
}

/// The drawing state of the window that shows, on the interface thread.
struct Drawn {
    key: (String, String),
    window: Window,
    scroll: Scroll,
    fields: Vec<((String, usize), Field)>,
    form: Option<Form>,
}

impl Drawn {
    fn new(key: (String, String)) -> Self {
        Self {
            key,
            window: Window::new(),
            scroll: Scroll::new(),
            fields: Vec::new(),
            form: None,
        }
    }

    /// The field of action `action`'s argument `index`, made for `arg`.
    fn field(&mut self, action: &str, index: usize, arg: &ArgView) -> &mut Field {
        let at = self
            .fields
            .iter()
            .position(|((a, i), _)| a == action && *i == index)
            .unwrap_or_else(|| {
                self.fields
                    .push(((action.to_owned(), index), Field::new(arg)));
                self.fields.len() - 1
            });
        &mut self.fields[at].1
    }
}

thread_local! {
    static DRAWN: RefCell<Option<Drawn>> = const { RefCell::new(None) };
}

/// What the player did in a window this frame.
#[derive(Debug, PartialEq)]
enum Click {
    Nothing,
    Close,
    Run(String, Vec<Value>),
    Form(Form),
    Cancel,
}

/// Draws the newest window the server's mods showed this client.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
pub(super) unsafe fn draw(engine: &Engine, context: usize) {
    if !CARRIED.any.load(Ordering::Acquire) {
        DRAWN.set(None);
        return;
    }
    let mut held = CARRIED.held.relock();
    let Some(top) = held.last() else {
        DRAWN.set(None);
        return;
    };
    let mod_id = top.mod_id.clone();
    let click = DRAWN.with_borrow_mut(|slot| {
        let key = (top.mod_id.clone(), top.window.clone());
        if slot.as_ref().is_none_or(|drawn| drawn.key != key) {
            *slot = Some(Drawn::new(key));
        }
        let Some(drawn) = slot.as_mut() else {
            return Click::Nothing;
        };
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window; `drawn` stays in the
        // thread-local `DRAWN`, borrowed and unmoved for the call.
        unsafe { window(engine, context, &top.view, drawn) }
    });
    match click {
        Click::Nothing => return,
        Click::Close => {
            held.pop();
            CARRIED.any.store(!held.is_empty(), Ordering::Release);
            DRAWN.set(None);
            return;
        }
        Click::Form(form) => {
            DRAWN.with_borrow_mut(|d| d.as_mut().map(|d| d.form = Some(form)));
            return;
        }
        Click::Cancel => {
            DRAWN.with_borrow_mut(|d| d.as_mut().map(|d| d.form = None));
            return;
        }
        Click::Run(..) => {}
    }
    drop(held);
    if let Click::Run(action, values) = click {
        DRAWN.with_borrow_mut(|d| d.as_mut().map(|d| d.form = None));
        if !crate::actions::run_on_server(&mod_id, &action, &encode_args(&values), None) {
            crate::log!(
                DEBUG,
                "windows: the server takes no runs for {mod_id}:{action}"
            );
        }
    }
}

/// Draws `view` as one window; what the player did.
///
/// # Safety
///
/// As [`draw`]; `drawn` stays in place until it returns.
unsafe fn window(engine: &Engine, context: usize, view: &View, drawn: &mut Drawn) -> Click {
    let (mod_id, id) = &drawn.key;
    let name = copy(&[b"enshwick/carried/", mod_id.as_bytes(), b"/", id.as_bytes()]);
    let title = copy(&[view.title.as_bytes()]);
    // SAFETY: this function's `# Safety` gives the frame, its thread and its
    // `context`; `drawn.window` stays in place until `window_end` below, and
    // `name` and `title` live in the frame's text arena.
    unsafe { engine.window_begin(context, &mut drawn.window, &name, &title) };
    // SAFETY: the window started one call earlier in this frame, on its
    // thread, with the frame's `context`, and `paint` is not used after
    // `window_end` below.
    let paint = unsafe { Painter::new(engine, context) };
    let height = crate::listing::scroll_height(false);
    // SAFETY: the window started above in this frame, on its thread, with
    // its `context`; `drawn.scroll` stays in place until `scroll_end` below.
    let scrolled = unsafe { engine.scroll_begin(context, &mut drawn.scroll, height, GAP) };
    paint.set_scrolled(scrolled);
    let mut click = match drawn.form.clone() {
        Some(form) => form_widgets(&paint, view, &form, drawn),
        None => items(&paint, view, drawn),
    };
    // SAFETY: `scroll_begin` started `drawn.scroll` in this frame inside this
    // window; it did not move, and the painter's widgets close every frame
    // they open.
    unsafe { engine.scroll_end(&mut drawn.scroll) };
    if paint.button(texts::text("actions.close").as_bytes(), true) {
        click = Click::Close;
    }
    // SAFETY: `window_begin` started `drawn.window` in this frame, on this
    // thread, with this `context`; it did not move, and the scroll area and
    // the painter's widgets closed every frame they opened.
    unsafe { engine.window_end(context, &mut drawn.window) };
    click
}

/// Draws the window's items; what the player did.
fn items(paint: &Painter<'_>, view: &View, drawn: &mut Drawn) -> Click {
    let mut click = Click::Nothing;
    for item in &view.items {
        let done = match item {
            ItemView::Text(text) => {
                paint.label(text.as_bytes(), UiWidget::STYLE_BODY, true, false);
                Click::Nothing
            }
            ItemView::Row { text, buttons } => {
                paint.label(text.as_bytes(), UiWidget::STYLE_BODY, true, false);
                let mut pressed = Click::Nothing;
                for button in buttons {
                    if paint.button(button.label.as_bytes(), true) {
                        pressed = press(view, button);
                    }
                }
                pressed
            }
            ItemView::Action(id) => view
                .entries
                .iter()
                .find(|e| e.id == *id)
                .map_or(Click::Nothing, |entry| action(paint, entry, drawn)),
            _ => Click::Nothing,
        };
        if done != Click::Nothing {
            click = done;
        }
    }
    click
}

/// What a press of `button` does: run its action, or open a form for the
/// values its action takes beyond the ones it gives.
fn press(view: &View, button: &ButtonView) -> Click {
    let takes = view
        .entries
        .iter()
        .find(|e| e.id == button.action)
        .map_or(0, |e| if e.per_player { 0 } else { e.args.len() });
    if button.args.len() < takes {
        Click::Form(Form {
            label: button.label.clone(),
            action: button.action.clone(),
            given: button.args.clone(),
        })
    } else {
        Click::Run(button.action.clone(), button.args.clone())
    }
}

/// Draws `entry` whole: a list's players with a switch each, or its fields
/// above a button that runs it; what the player did.
fn action(paint: &Painter<'_>, entry: &Entry, drawn: &mut Drawn) -> Click {
    if entry.per_player || !entry.args.is_empty() {
        paint.label(entry.label.as_bytes(), UiWidget::STYLE_H3, true, false);
    }
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
            UiWidget::STYLE_SECONDARY
        } else {
            UiWidget::STYLE_WARNING
        };
        paint.label(entry.subject.as_bytes(), style, true, false);
    }
    if entry.per_player {
        let mut click = Click::Nothing;
        for (key, name, on) in &entry.players {
            if let Some(now) = paint.toggle(name.as_bytes(), *on, !entry.offered) {
                let who = Value::Player {
                    key: *key,
                    name: String::new(),
                };
                click = Click::Run(entry.id.clone(), vec![who, Value::Toggle(now)]);
            }
        }
        return click;
    }
    let values = fields(paint, &entry.id, &entry.args, 0, drawn);
    // An action without fields is one button under its own label.
    let label = if entry.args.is_empty() {
        entry.label.as_str()
    } else {
        texts::text("windows.run")
    };
    let ready = entry.offered && values.is_some();
    match values {
        Some(values) if paint.button(label.as_bytes(), ready) => {
            Click::Run(entry.id.clone(), values)
        }
        _ => Click::Nothing,
    }
}

/// Draws a button's form: the fields of its action past the values the
/// button gave, with Run and Cancel; what the player did.
fn form_widgets(paint: &Painter<'_>, view: &View, form: &Form, drawn: &mut Drawn) -> Click {
    let Some(entry) = view.entries.iter().find(|e| e.id == form.action) else {
        return Click::Cancel;
    };
    paint.label(form.label.as_bytes(), UiWidget::STYLE_H2, true, false);
    let rest = fields(paint, &entry.id, &entry.args, form.given.len(), drawn);
    let ready = entry.offered && rest.is_some();
    let run = paint.button(texts::text("windows.run").as_bytes(), ready);
    if paint.button(texts::text("windows.cancel").as_bytes(), true) {
        return Click::Cancel;
    }
    match rest {
        Some(rest) if run => {
            let mut values = form.given.clone();
            values.extend(rest);
            Click::Run(form.action.clone(), values)
        }
        _ => Click::Nothing,
    }
}

/// Draws the fields of `action`'s arguments `args` from `from` on; their
/// values, `None` while one is not set.
fn fields(
    paint: &Painter<'_>,
    action: &str,
    args: &[ArgView],
    from: usize,
    drawn: &mut Drawn,
) -> Option<Vec<Value>> {
    let mut values = Some(Vec::new());
    for (index, arg) in args.iter().enumerate().skip(from) {
        let value = field(paint, arg, drawn.field(action, index, arg));
        values = values.zip(value).map(|(mut all, value)| {
            all.push(value);
            all
        });
    }
    values
}

/// Draws one argument's field with `state`; its value, `None` while not
/// set.
fn field(paint: &Painter<'_>, arg: &ArgView, state: &mut Field) -> Option<Value> {
    let label = arg.label.as_bytes();
    match arg.kind {
        ArgView::PLAYER => {
            let names: Vec<&[u8]> = arg.players.iter().map(|(_, n)| n.as_bytes()).collect();
            state.pick = followed(&arg.players, state.picked, state.pick);
            if names.is_empty() {
                paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
            } else if let Some(pick) = paint.dropdown(label, &names, state.pick, false) {
                state.pick = pick;
            }
            state.picked = usize::try_from(state.pick)
                .ok()
                .and_then(|at| arg.players.get(at))
                .map(|(key, _)| *key);
            let _typed = paint.text_field(
                &mut state.text,
                texts::text("windows.name").as_bytes(),
                false,
            );
            player_value(arg, state.pick, state.text.text())
        }
        ArgView::CHOICE => {
            let options: Vec<&[u8]> = arg.choices.iter().map(String::as_bytes).collect();
            if let Some(pick) = paint.dropdown(label, &options, state.pick, false) {
                state.pick = pick;
            }
            let pick = usize::try_from(state.pick).ok()?;
            (pick < arg.choices.len()).then_some(Value::Choice(pick))
        }
        ArgView::NUMBER => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a slider takes an f32; the server checks the value it gets"
            )]
            let range = (arg.min as f32, arg.max as f32);
            if let Some(number) = paint.slider(label, state.number, range, steps(arg), false) {
                state.number = number;
            }
            Some(Value::Number(snapped(arg, f64::from(state.number))))
        }
        ArgView::TOGGLE => {
            if let Some(on) = paint.toggle(label, state.on, false) {
                state.on = on;
            }
            Some(Value::Toggle(state.on))
        }
        ArgView::TEXT => {
            paint.label(label, UiWidget::STYLE_SECONDARY, false, false);
            let _typed = paint.text_field(
                &mut state.text,
                texts::text("windows.text").as_bytes(),
                false,
            );
            text_value(arg, state.text.text())
        }
        _ => None,
    }
}

/// The row of the account `picked` in `players` as an update left them,
/// else `pick`: a join or a leave moves the rows under the player's pick.
fn followed(players: &[(PlayerKey, String)], picked: Option<PlayerKey>, pick: u32) -> u32 {
    picked
        .and_then(|key| players.iter().position(|(k, _)| *k == key))
        .and_then(|at| u32::try_from(at).ok())
        .unwrap_or(pick)
}

/// A player argument's value: the typed name when there is one, which the
/// server looks up, else the player picked from the list.
fn player_value(arg: &ArgView, pick: u32, typed: &str) -> Option<Value> {
    let typed = typed.trim();
    if !typed.is_empty() {
        return Some(Value::Text(typed.to_owned()));
    }
    let (key, _) = arg.players.get(usize::try_from(pick).ok()?)?;
    Some(Value::Player {
        key: *key,
        name: String::new(),
    })
}

/// A text argument's value: the typed text within its bytes.
fn text_value(arg: &ArgView, typed: &str) -> Option<Value> {
    let typed = typed.trim();
    let len = u16::try_from(typed.len()).ok()?;
    (len > 0 && f64::from(len) <= arg.max).then(|| Value::Text(typed.to_owned()))
}

/// The steps of a number's slider: one per step of its range, 0 for any
/// value or more than [`MAX_STEPS`].
fn steps(arg: &ArgView) -> u32 {
    let count = ((arg.max - arg.min) / arg.step).round() + 1.0;
    if arg.step > 0.0 && count.is_finite() && (1.0..=MAX_STEPS).contains(&count) {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked above: a whole number from 1 to MAX_STEPS"
        )]
        return count as u32;
    }
    0
}

/// `value` on its argument's step and within its range.
fn snapped(arg: &ArgView, value: f64) -> f64 {
    let stepped = if arg.step > 0.0 {
        arg.min + ((value - arg.min) / arg.step).round() * arg.step
    } else {
        value
    };
    // Not `clamp`: the range comes from the server's text, and `clamp` panics
    // on a reversed or NaN one.
    stepped.max(arg.min).min(arg.max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(title: &str) -> View {
        View {
            title: title.into(),
            ..View::default()
        }
    }

    #[test]
    fn a_window_opens_updates_in_place_and_an_update_of_a_closed_one_changes_nothing() {
        let mut held = Vec::new();
        hold(&mut held, "you.admin", "panel", true, view("one"));
        hold(&mut held, "you.admin", "bans", true, view("bans"));
        hold(&mut held, "you.admin", "panel", false, view("two"));
        hold(&mut held, "you.admin", "gone", false, view("gone"));
        let titles: Vec<&str> = held.iter().map(|h| h.view.title.as_str()).collect();
        assert_eq!(titles, ["two", "bans"]);
        for n in 0..MAX_HELD {
            hold(&mut held, "you.admin", &format!("w{n}"), true, view("w"));
        }
        assert_eq!(held.len(), MAX_HELD);
        assert!(held.iter().all(|h| h.window != "panel"), "the oldest goes");
    }

    #[test]
    fn a_press_runs_or_opens_a_form_for_the_values_its_action_still_takes() {
        let ban = Entry {
            id: "ban".into(),
            args: vec![
                ArgView {
                    kind: ArgView::PLAYER,
                    ..ArgView::default()
                },
                ArgView {
                    kind: ArgView::TEXT,
                    max: 64.0,
                    ..ArgView::default()
                },
            ],
            ..Entry::default()
        };
        let view = View {
            entries: vec![ban],
            ..View::default()
        };
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        let button = |args: Vec<Value>| ButtonView {
            label: "Ban".into(),
            action: "ban".into(),
            args,
        };
        assert_eq!(
            press(&view, &button(vec![ann.clone()])),
            Click::Form(Form {
                label: "Ban".into(),
                action: "ban".into(),
                given: vec![ann.clone()],
            })
        );
        let full = vec![ann, Value::Text("spam".into())];
        assert_eq!(
            press(&view, &button(full.clone())),
            Click::Run("ban".into(), full)
        );
    }

    #[test]
    fn fields_give_values_within_their_arguments() {
        let players = ArgView {
            kind: ArgView::PLAYER,
            players: vec![(PlayerKey::new(7), "Ann".into())],
            ..ArgView::default()
        };
        assert_eq!(
            player_value(&players, 0, ""),
            Some(Value::Player {
                key: PlayerKey::new(7),
                name: String::new()
            })
        );
        assert_eq!(
            player_value(&players, 0, " Bob "),
            Some(Value::Text("Bob".into()))
        );
        assert_eq!(player_value(&players, 3, ""), None);

        let text = ArgView {
            kind: ArgView::TEXT,
            min: 1.0,
            max: 4.0,
            ..ArgView::default()
        };
        assert_eq!(text_value(&text, " hi "), Some(Value::Text("hi".into())));
        assert_eq!(text_value(&text, "toolong"), None);
        assert_eq!(text_value(&text, "  "), None);

        let hours = ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            max: 48.0,
            step: 1.0,
            ..ArgView::default()
        };
        assert_eq!(steps(&hours), 48);
        assert!((snapped(&hours, 3.4) - 3.0).abs() < f64::EPSILON);
        assert!((snapped(&hours, 99.0) - 48.0).abs() < f64::EPSILON);
        let any = ArgView { step: 0.0, ..hours };
        assert_eq!(steps(&any), 0);
        let fine = ArgView {
            step: 0.001,
            ..any.clone()
        };
        assert_eq!(steps(&fine), 0, "past MAX_STEPS the slider takes any value");
    }

    #[test]
    fn a_number_snaps_to_its_step_from_the_least_value_and_any_value_stays() {
        let even = ArgView {
            kind: ArgView::NUMBER,
            min: 1.0,
            max: 9.0,
            step: 2.0,
            ..ArgView::default()
        };
        assert!((snapped(&even, 4.2) - 5.0).abs() < f64::EPSILON);
        let any = ArgView { step: 0.0, ..even };
        assert!((snapped(&any, 4.2) - 4.2).abs() < f64::EPSILON);
    }

    #[test]
    fn a_reversed_or_unreadable_range_from_the_server_does_not_panic() {
        let reversed = ArgView {
            kind: ArgView::NUMBER,
            min: 9.0,
            max: 1.0,
            ..ArgView::default()
        };
        assert!((snapped(&reversed, 4.0) - 1.0).abs() < f64::EPSILON);
        let unreadable = ArgView {
            min: f64::NAN,
            max: f64::NAN,
            ..reversed
        };
        assert!((snapped(&unreadable, 4.0) - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_player_pick_follows_its_account_when_an_update_moves_the_rows() {
        let (ann, bob) = (PlayerKey::new(7), PlayerKey::new(8));
        let both = [(ann, "Ann".to_owned()), (bob, "Bob".to_owned())];
        assert_eq!(followed(&both, None, 1), 1, "nothing drawn yet");
        assert_eq!(followed(&both, Some(bob), 1), 1);
        assert_eq!(followed(&both[1..], Some(bob), 1), 0, "Ann left");
        assert_eq!(followed(&both[..1], Some(bob), 1), 1, "Bob left");
    }

    #[test]
    fn each_action_s_argument_keeps_a_field_of_its_own() {
        let arg = ArgView {
            kind: ArgView::CHOICE,
            ..ArgView::default()
        };
        let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
        drawn.field("ban", 1, &arg).pick = 3;
        assert_eq!(drawn.field("say", 1, &arg).pick, 0, "another action");
        assert_eq!(drawn.field("ban", 0, &arg).pick, 0, "another argument");
        assert_eq!(drawn.field("ban", 1, &arg).pick, 3, "the same field");
        assert_eq!(drawn.fields.len(), 3);
    }

    #[test]
    fn the_server_s_close_takes_only_its_own_window() {
        heard("you.admin", "panel", true, "W\tPanel\n");
        heard("you.admin", "bans", true, "W\tBans\n");
        heard("you.other", "panel", true, "W\tOther\n");
        heard("you.admin", "broken", true, "no title");
        assert!(CARRIED.any.load(Ordering::Acquire));
        closed("you.admin", "panel");
        let left = |held: &[Held]| -> Vec<(String, String)> {
            held.iter()
                .map(|h| (h.mod_id.clone(), h.window.clone()))
                .collect()
        };
        assert_eq!(
            left(&CARRIED.held.relock()),
            [
                ("you.admin".to_owned(), "bans".to_owned()),
                ("you.other".to_owned(), "panel".to_owned())
            ]
        );
        closed("you.admin", "bans");
        closed("you.other", "panel");
        assert!(!CARRIED.any.load(Ordering::Acquire), "nothing held");
    }
}
