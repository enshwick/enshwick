//! Carried windows on the client: the windows the server's mods show this
//! player, drawn with the game's widgets though the mods have no part here.
//!
//! The newest window shows; Close takes it away on this client and tells
//! the server, which stops updating it, and the one under it shows. A lost
//! link takes them all. A button or a list's switch runs the mod's
//! action on the server, which checks it again, and a button whose action
//! takes more values opens a form for the rest. An action drawn whole shows
//! its fields above a button that runs it. Long windows scroll.
//!
//! The markers of the held windows' maps show on the game's own map as the
//! loader's, and the windows step aside while the Map screen is open, so
//! the player sees the map and sets the waypoint the server reads.

use core::cell::RefCell;
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use enshwick_core::abi::UiWidget;
use enshwick_core::actions::Value;
use enshwick_core::actions::sheet::{ArgView, Entry, encode_args};
use enshwick_core::actions::window::{ButtonView, ItemView, MapView, View, decode};

use super::engine::{Engine, Scroll, Window};
use super::{Painter, Widgets, copy};
use crate::base::remote::Message;
use crate::engine::texts;
use crate::ui::windows::MAX_HELD;

/// The game's name of the `HudScreenType` member of the Map screen.
const MAP_SCREEN: &str = "Map";

mod fields;
mod markers;

use fields::{Field, field, value};
use markers::mark;

/// Rows of a window this far apart.
const GAP: f32 = 14.0;

/// A window a server's mod showed this client.
#[derive(Clone, Debug, PartialEq)]
struct Held {
    mod_id: String,
    window: String,
    view: View,
}

/// The carried windows' state on the client.
struct Carried {
    /// The windows the client holds, the newest last, and `None` while it
    /// holds none: swapped whole, so a frame reads them without a lock.
    held: ArcSwapOption<Vec<Held>>,
    /// The map whose markers are on the game's map, `None` for none; locked
    /// while they are set, so the last change's markers stay.
    marked: Mutex<Option<MapView>>,
}

static STATE: Carried = Carried {
    held: ArcSwapOption::const_empty(),
    marked: Mutex::new(None),
};

/// Changes the windows the client holds with `change`, which may run more
/// than once, then their markers on the map.
fn change(change: impl Fn(&mut Vec<Held>)) {
    STATE.held.rcu(|held| {
        let mut held = held.as_deref().cloned().unwrap_or_default();
        change(&mut held);
        (!held.is_empty()).then(|| Arc::new(held))
    });
    mark();
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
    STATE.held.store(None);
    mark();
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
    /// The HUD screen byte of the Map screen, which the window steps
    /// aside for.
    map_screen: Option<u8>,
    /// The Map screen's byte while the build can also open it from the
    /// window, set each frame: the modal window takes the game's map key,
    /// so its Map items open the map with a button.
    map_opens: Option<u8>,
    /// The tries left for a map open the interface refused, such as on a
    /// full queue; `None` while none waits.
    map_tries: Option<u8>,
}

/// How many frames a refused map open is tried again, about a second.
const MAP_OPEN_TRIES: u8 = 60;

impl Drawn {
    fn new(key: (String, String)) -> Self {
        Self {
            key,
            window: Window::new(),
            scroll: Scroll::new(),
            fields: Vec::new(),
            form: None,
            map_screen: crate::engine::screens::byte(MAP_SCREEN),
            map_opens: None,
            map_tries: None,
        }
    }

    /// Takes in whether the build can open a screen this frame, since the
    /// toggle's feature can turn off while the window shows.
    fn refresh(&mut self, toggles: bool) {
        self.map_opens = self.map_screen.filter(|_| toggles);
        if self.map_opens.is_none() {
            self.map_tries = None;
        }
    }

    /// Takes in the interface's answer to a map open: a refusal is tried
    /// again on the next frames; whether it gave up.
    fn opened(&mut self, taken: bool) -> bool {
        let left = if taken {
            None
        } else {
            self.map_tries.unwrap_or(MAP_OPEN_TRIES).checked_sub(1)
        };
        let gave_up = !taken && left.is_none();
        self.map_tries = left;
        gave_up
    }

    /// `click`, or the map open that waits for another try when the player
    /// did nothing else.
    fn retry(&self, click: Click) -> Click {
        match (click, self.map_opens.filter(|_| self.map_tries.is_some())) {
            (Click::Nothing, Some(map)) => Click::OpenMap(map),
            (click, _) => click,
        }
    }

    /// Leaves the window undrawn while the Map screen is open, keeping its
    /// form and fields; its panel starts afresh, so the Back that closes the
    /// map is not read as Back over the window when it draws again.
    fn step_aside(&mut self) {
        self.window = Window::new();
        self.map_tries = None;
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
                let left = self.form.as_ref().map(|form| form.action.clone());
                (None, left.unwrap_or_default())
            }
            Click::Nothing | Click::Close | Click::OpenMap(_) => return,
        };
        if self.form != form {
            self.scroll.start_over();
        }
        self.form = form;
        self.fields.retain(|((of, _), _)| *of != action);
    }

    /// What Back does, one step a press as on the Mods screen: it ends the
    /// typing of a field that has the caret, else leaves an open form, else
    /// closes the window.
    fn back(&mut self) -> Click {
        let mut ended = false;
        for (_, field) in &mut self.fields {
            if field.text.typing() {
                field.text.end_typing();
                ended = true;
            }
        }
        if ended {
            Click::Nothing
        } else if self.form.is_some() {
            Click::Cancel
        } else {
            Click::Close
        }
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
    /// Opens the HUD screen of this byte, the Map.
    OpenMap(u8),
}

/// Draws the newest window the server's mods showed this client, stepping
/// aside while `screen`, the screen the player sees, is the Map.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
pub(super) unsafe fn draw(engine: &Engine, context: usize, screen: Option<u8>) {
    let held = STATE.held.load();
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
        if screen.is_some() && screen == drawn.map_screen {
            drawn.step_aside();
            return Click::Nothing;
        }
        drawn.refresh(
            crate::actions::client_control::GROUPS
                .toggle
                .get()
                .is_some(),
        );
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window; `drawn` stays in the
        // thread-local `DRAWN`, borrowed and unmoved for the call.
        let click = unsafe { window(engine, context, &top.view, drawn) };
        drawn.after(&click);
        drawn.retry(click)
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
        Click::OpenMap(map) => {
            // SAFETY: this function's `# Safety`: inside the interface frame
            // on its thread, with the frame's own live `context`.
            let taken = crate::actions::client_control::GROUPS
                .toggle
                .get()
                .is_some_and(|toggle| unsafe { toggle.toggle(context, map) });
            let gave_up = DRAWN
                .with_borrow_mut(|slot| slot.as_mut().is_some_and(|drawn| drawn.opened(taken)));
            if gave_up {
                crate::log!(
                    WARN,
                    "windows: the interface took no map open in {MAP_OPEN_TRIES} frames"
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
    // Back (Escape, B), bound to the window's panel from the last frame so
    // the press does not also open the pause menu, steps back.
    // SAFETY: in a frame of the interface update on its thread, by this
    // function's `# Safety`.
    let back = unsafe { super::dialog::back_over(engine, &drawn.window) };
    let stepped = if back { drawn.back() } else { Click::Nothing };
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
    } else if stepped != Click::Nothing {
        click = stepped;
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
            ItemView::Map(_) => {
                let line = texts::text("windows.map");
                paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
                let open = texts::text("windows.map.open");
                let pressed = paint.button(open.as_bytes(), drawn.map_opens.is_some());
                drawn
                    .map_opens
                    .filter(|_| pressed)
                    .map_or(Click::Nothing, Click::OpenMap)
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
mod tests;
