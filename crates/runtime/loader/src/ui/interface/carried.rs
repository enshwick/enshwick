//! Carried windows on the client: the windows the server's mods show this
//! player, drawn with the game's widgets though the mods have no part here.
//!
//! The newest window shows; Close takes it away on this client and tells
//! the server, which stops updating it, and the one under it shows. A lost
//! link takes them all. A button or a list's switch runs the mod's
//! action on the server, which checks it again, and a button whose action
//! takes more values opens a form for the rest. An action drawn whole shows
//! its fields above a button that runs it. Long windows scroll.

use core::cell::RefCell;
use std::sync::Arc;

use arc_swap::ArcSwapOption;
use enshwick_core::abi::UiWidget;
use enshwick_core::actions::Value;
use enshwick_core::actions::sheet::{ArgView, Entry, encode_args};
use enshwick_core::actions::window::{ButtonView, ItemView, View, decode};

use super::engine::{Engine, Scroll, Window};
use super::{Painter, Widgets, copy};
use crate::base::remote::Message;
use crate::engine::texts;
use crate::ui::windows::MAX_HELD;

mod fields;

use fields::{Field, field, value};

/// Rows of a window this far apart.
const GAP: f32 = 14.0;

/// A window a server's mod showed this client.
#[derive(Clone, Debug, PartialEq)]
struct Held {
    mod_id: String,
    window: String,
    view: View,
}

/// The windows the client holds, the newest last, and `None` while it
/// holds none: swapped whole, so a frame reads them without a lock.
static CARRIED: ArcSwapOption<Vec<Held>> = ArcSwapOption::const_empty();

/// Changes the windows the client holds with `change`, which may run more
/// than once.
fn change(change: impl Fn(&mut Vec<Held>)) {
    CARRIED.rcu(|held| {
        let mut held = held.as_deref().cloned().unwrap_or_default();
        change(&mut held);
        (!held.is_empty()).then(|| Arc::new(held))
    });
}

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
    change(|held| hold(held, id, window, open, view.clone()));
}

/// The server closed mod `id`'s window `window`, or the player did.
pub(crate) fn closed(id: &str, window: &str) {
    change(|held| held.retain(|h| !(h.mod_id == id && h.window == window)));
}

/// Forgets every window, as when the client leaves its server or loses
/// its link to it.
pub(crate) fn forget() {
    CARRIED.store(None);
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

    /// Takes in what the player did: a press opens its form, and a form, a
    /// run or a cancel starts the action's fields afresh, so a value typed
    /// for one run is not sent with the next.
    fn after(&mut self, click: &Click) {
        let (form, action) = match click {
            Click::Form(form) => (Some(form.clone()), form.action.clone()),
            Click::Run(action, _) => (None, action.clone()),
            Click::Cancel => {
                let left = self.form.take().map(|form| form.action);
                (None, left.unwrap_or_default())
            }
            Click::Nothing | Click::Close => return,
        };
        self.form = form;
        self.fields.retain(|((of, _), _)| *of != action);
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
    let held = CARRIED.load();
    let Some(top) = held.as_deref().and_then(|held| held.last()) else {
        DRAWN.set(None);
        return;
    };
    let click = DRAWN.with_borrow_mut(|slot| {
        let shows = |drawn: &Drawn| drawn.key.0 == top.mod_id && drawn.key.1 == top.window;
        if !slot.as_ref().is_some_and(shows) {
            *slot = Some(Drawn::new((top.mod_id.clone(), top.window.clone())));
        }
        let Some(drawn) = slot.as_mut() else {
            return Click::Nothing;
        };
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window; `drawn` stays in the
        // thread-local `DRAWN`, borrowed and unmoved for the call.
        let click = unsafe { window(engine, context, &top.view, drawn) };
        drawn.after(&click);
        click
    });
    match click {
        Click::Close => {
            closed(&top.mod_id, &top.window);
            // The server stops updating a window its player closed; one that
            // takes no messages forgets it when the player leaves.
            let _sent = crate::base::net::to_server(&Message::WindowClose {
                id: top.mod_id.clone(),
                window: top.window.clone(),
            });
        }
        Click::Run(action, values) => {
            if !crate::actions::player_actions::run_on_server(
                &top.mod_id,
                &action,
                &encode_args(&values),
                None,
            ) {
                crate::log!(
                    DEBUG,
                    "windows: the server takes no runs for {}:{action}",
                    top.mod_id
                );
            }
        }
        Click::Nothing | Click::Form(_) | Click::Cancel => {}
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
    let height = crate::ui::listing::scroll_height(false);
    // SAFETY: the window started above in this frame, on its thread, with
    // its `context`; `drawn.scroll` stays in place until `scroll_end` below.
    let scrolled = unsafe { engine.scroll_begin(context, &mut drawn.scroll, height, GAP) };
    paint.set_scrolled(scrolled);
    let form = drawn.form.take();
    let mut click = match &form {
        Some(form) => form_widgets(&paint, view, form, drawn),
        None => items(&paint, view, drawn),
    };
    drawn.form = form;
    // SAFETY: `scroll_begin` started `drawn.scroll` in this frame inside this
    // window; it did not move, and the painter's widgets close every frame
    // they open.
    unsafe { engine.scroll_end(&mut drawn.scroll) };
    paint.set_scrolled(false);
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
fn items(paint: &impl Widgets, view: &View, drawn: &mut Drawn) -> Click {
    let mut click = Click::Nothing;
    let mut group = "";
    for item in &view.items {
        let entry = match item {
            ItemView::Action(id) => view.entries.iter().find(|e| e.id == *id),
            _ => None,
        };
        if let Some(entry) = entry
            && entry.group != group
        {
            group = &entry.group;
            if !group.is_empty() {
                paint.label(group.as_bytes(), UiWidget::STYLE_H2, true, false);
            }
        }
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
            ItemView::Action(_) => {
                entry.map_or(Click::Nothing, |entry| action(paint, entry, drawn))
            }
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
fn action(paint: &impl Widgets, entry: &Entry, drawn: &mut Drawn) -> Click {
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
    let set = fields(paint, &entry.id, &entry.args, 0, drawn);
    // An action without fields is one button under its own label.
    let label = if entry.args.is_empty() {
        entry.label.as_str()
    } else {
        texts::text("windows.run")
    };
    // Drawn while a field is not set too, off, so the rows under it stay.
    if !paint.button(label.as_bytes(), entry.offered && set) {
        return Click::Nothing;
    }
    values(&entry.id, &entry.args, 0, drawn).map_or(Click::Nothing, |values| {
        Click::Run(entry.id.clone(), values)
    })
}

/// Draws a button's form: the fields of its action past the values the
/// button gave, with Run and Cancel; what the player did.
fn form_widgets(paint: &impl Widgets, view: &View, form: &Form, drawn: &mut Drawn) -> Click {
    let Some(entry) = view.entries.iter().find(|e| e.id == form.action) else {
        return Click::Cancel;
    };
    paint.label(form.label.as_bytes(), UiWidget::STYLE_H2, true, false);
    let from = form.given.len();
    let set = fields(paint, &entry.id, &entry.args, from, drawn);
    let run = paint.button(texts::text("windows.run").as_bytes(), entry.offered && set);
    if paint.button(texts::text("windows.cancel").as_bytes(), true) {
        return Click::Cancel;
    }
    if !run {
        return Click::Nothing;
    }
    match values(&entry.id, &entry.args, from, drawn) {
        Some(rest) => {
            let mut values = form.given.clone();
            values.extend(rest);
            Click::Run(form.action.clone(), values)
        }
        None => Click::Nothing,
    }
}

/// Draws the fields of `action`'s arguments `args` from `from` on; whether
/// each holds a value or its argument has a default.
fn fields(
    paint: &impl Widgets,
    action: &str,
    args: &[ArgView],
    from: usize,
    drawn: &mut Drawn,
) -> bool {
    let mut set = true;
    for (index, arg) in args.iter().enumerate().skip(from) {
        set &= field(paint, arg, drawn.field(action, index, arg)) || arg.default.is_some();
    }
    set
}

/// The values the fields of `action`'s arguments `args` from `from` on
/// hold, an argument's default where its field holds none; `None` while
/// one is not set. Built only for a run, never per frame.
fn values(action: &str, args: &[ArgView], from: usize, drawn: &mut Drawn) -> Option<Vec<Value>> {
    args.iter()
        .enumerate()
        .skip(from)
        .map(|(index, arg)| {
            value(arg, drawn.field(action, index, arg)).or_else(|| arg.default.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use enshwick_core::PlayerKey;

    use super::super::recorder::{Answer, Drew, Recorder};
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

    fn say() -> Entry {
        Entry {
            id: "say".into(),
            label: "Announce".into(),
            offered: true,
            args: vec![ArgView {
                kind: ArgView::TEXT,
                label: "Message".into(),
                max: 64.0,
                ..ArgView::default()
            }],
            ..Entry::default()
        }
    }

    fn drawn() -> Drawn {
        Drawn::new(("you.admin".into(), "panel".into()))
    }

    fn run_button(drew: &[Drew]) -> Option<bool> {
        drew.iter().find_map(|widget| match widget {
            Drew::Button { text, enabled } if text == texts::text("windows.run") => Some(*enabled),
            _ => None,
        })
    }

    #[test]
    fn an_action_s_run_stays_drawn_but_off_until_its_fields_are_set() {
        let paint = Recorder::default().answer(texts::text("windows.run"), Answer::Press);
        assert_eq!(action(&paint, &say(), &mut drawn()), Click::Nothing);
        let drew = paint.drew();
        assert_eq!(run_button(&drew), Some(false), "drawn, off: {drew:?}");
        assert_eq!(
            drew.last(),
            Some(&Drew::Button {
                text: texts::text("windows.run").into(),
                enabled: false
            })
        );
    }

    #[test]
    fn an_action_runs_with_the_values_its_fields_hold() {
        let paint = Recorder::default()
            .answer(
                texts::text("windows.text"),
                Answer::Type("Restart soon".into()),
            )
            .answer(texts::text("windows.run"), Answer::Press);
        assert_eq!(
            action(&paint, &say(), &mut drawn()),
            Click::Run("say".into(), vec![Value::Text("Restart soon".into())])
        );
        let refused = Entry {
            offered: false,
            ..say()
        };
        assert_eq!(
            action(&paint, &refused, &mut drawn()),
            Click::Nothing,
            "an action the player may not run now"
        );
    }

    #[test]
    fn an_action_without_fields_is_one_button_under_its_label() {
        let wave = Entry {
            id: "wave".into(),
            label: "Wave".into(),
            offered: true,
            ..Entry::default()
        };
        let paint = Recorder::default().answer("Wave", Answer::Press);
        assert_eq!(
            action(&paint, &wave, &mut drawn()),
            Click::Run("wave".into(), Vec::new())
        );
        assert_eq!(
            paint.drew(),
            [Drew::Button {
                text: "Wave".into(),
                enabled: true
            }]
        );
    }

    #[test]
    fn a_list_s_switch_runs_its_action_on_that_player_and_reads_only_when_not_offered() {
        let list = Entry {
            id: "allowed".into(),
            label: "Allowed".into(),
            per_player: true,
            offered: true,
            players: vec![(PlayerKey::new(7), "Ann".into(), false)],
            ..Entry::default()
        };
        let paint = Recorder::default().answer("Ann", Answer::Flip(true));
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        assert_eq!(
            action(&paint, &list, &mut drawn()),
            Click::Run("allowed".into(), vec![ann, Value::Toggle(true)])
        );
        let shut = Entry {
            offered: false,
            ..list
        };
        assert_eq!(action(&paint, &shut, &mut drawn()), Click::Nothing);
    }

    #[test]
    fn a_form_runs_with_the_button_s_values_and_the_rest_or_cancels() {
        let ann = Value::Player {
            key: PlayerKey::new(7),
            name: String::new(),
        };
        let ban = Entry {
            id: "ban".into(),
            offered: true,
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
        let form = Form {
            label: "Ban".into(),
            action: "ban".into(),
            given: vec![ann.clone()],
        };
        let empty = Recorder::default().answer(texts::text("windows.run"), Answer::Press);
        assert_eq!(
            form_widgets(&empty, &view, &form, &mut drawn()),
            Click::Nothing
        );
        assert_eq!(run_button(&empty.drew()), Some(false));

        let typed = Recorder::default()
            .answer(texts::text("windows.text"), Answer::Type("spam".into()))
            .answer(texts::text("windows.run"), Answer::Press);
        assert_eq!(
            form_widgets(&typed, &view, &form, &mut drawn()),
            Click::Run("ban".into(), vec![ann, Value::Text("spam".into())])
        );
        let cancels = Recorder::default().answer(texts::text("windows.cancel"), Answer::Press);
        assert_eq!(
            form_widgets(&cancels, &view, &form, &mut drawn()),
            Click::Cancel
        );
        let gone = Form {
            action: "kick".into(),
            ..form
        };
        assert_eq!(
            form_widgets(&typed, &view, &gone, &mut drawn()),
            Click::Cancel,
            "the action left the window"
        );
    }

    #[test]
    fn a_form_a_run_or_a_cancel_starts_the_action_s_fields_afresh() {
        let arg = ArgView {
            kind: ArgView::TEXT,
            max: 64.0,
            ..ArgView::default()
        };
        let form = Form {
            label: "Ban".into(),
            action: "ban".into(),
            given: Vec::new(),
        };
        let typed =
            |drawn: &mut Drawn, action: &str| drawn.field(action, 1, &arg).text.text().to_owned();
        let mut drawn = Drawn::new(("you.admin".into(), "panel".into()));
        drawn.field("ban", 1, &arg).text.set("griefing");
        drawn.field("say", 1, &arg).text.set("hello");

        drawn.after(&Click::Nothing);
        assert_eq!(typed(&mut drawn, "ban"), "griefing");
        drawn.after(&Click::Run("ban".into(), Vec::new()));
        assert_eq!(
            typed(&mut drawn, "ban"),
            "",
            "the reason of the last ban is gone"
        );
        assert_eq!(
            typed(&mut drawn, "say"),
            "hello",
            "another action's field stays"
        );

        drawn.field("ban", 1, &arg).text.set("spam");
        drawn.after(&Click::Form(form.clone()));
        assert_eq!(drawn.form, Some(form));
        assert_eq!(typed(&mut drawn, "ban"), "", "a form opens empty");
        drawn
            .field("ban", 1, &arg)
            .text
            .set("typed, then cancelled");
        drawn.after(&Click::Cancel);
        assert_eq!(drawn.form, None);
        assert_eq!(typed(&mut drawn, "ban"), "");
        assert_eq!(typed(&mut drawn, "say"), "hello");
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

    fn held() -> Vec<(String, String)> {
        let held = CARRIED.load();
        let held = held.as_deref().map(Vec::as_slice).unwrap_or_default();
        held.iter()
            .map(|h| (h.mod_id.clone(), h.window.clone()))
            .collect()
    }

    #[test]
    fn the_server_s_close_takes_only_its_own_window() {
        heard("you.admin", "panel", true, "W\tPanel\n");
        heard("you.admin", "bans", true, "W\tBans\n");
        heard("you.other", "panel", true, "W\tOther\n");
        heard("you.admin", "broken", true, "no title");
        closed("you.admin", "panel");
        assert_eq!(
            held(),
            [
                ("you.admin".to_owned(), "bans".to_owned()),
                ("you.other".to_owned(), "panel".to_owned())
            ]
        );
        closed("you.admin", "bans");
        closed("you.other", "panel");
        assert!(CARRIED.load().is_none(), "nothing held");
    }

    #[test]
    fn a_lost_link_forgets_the_windows_so_none_draws_over_the_menu() {
        heard("you.admin", "panel", true, "W\tPanel\n");
        assert_eq!(held().len(), 1);
        crate::base::net::link_lost();
        assert!(CARRIED.load().is_none());
    }
}
