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
//! the player sees the map and sets the waypoint the server reads. The
//! game's map key opens it under a window with a map, as does its button.
//!
//! A mod's client command presses an item of the newest window, picks an
//! option or sets a text, by the ids [`items`] lists: a press makes that
//! one widget answer as a click in the frame, and a field takes the pick
//! or the text as the player's input would, so the window acts as it does
//! for the player.

use core::cell::{Cell, RefCell};
use std::sync::{Arc, Mutex};

use arc_swap::ArcSwapOption;
use enshwick_base::Relock;
use enshwick_core::PlayerKey;
use enshwick_core::abi::{ClientWindowItem, Str, UiWidget};
use enshwick_core::actions::Value;
use enshwick_core::actions::sheet::{ArgView, Entry, encode_args};
use enshwick_core::actions::window::{ButtonView, ItemView, MapView, View, decode};

use super::engine::{Engine, Scroll, Window};
use super::{Painter, Widgets, copy};
use crate::actions::client_commands::windows::{self, Act, Answer, Ask, Target};
use crate::base::remote::Message;
use crate::engine::texts;
use crate::ui::windows::MAX_HELD;

/// The game's name of the `HudScreenType` member of the Map screen.
const MAP_SCREEN: &str = "Map";

mod fields;
mod items;
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
    /// The form open in the window that shows, by its mod and window id,
    /// for the listing of its items on any thread; set only when it opens
    /// or closes.
    form: Mutex<Option<(String, String, Form)>>,
}

static STATE: Carried = Carried {
    held: ArcSwapOption::const_empty(),
    marked: Mutex::new(None),
    form: Mutex::new(None),
};

/// The items of every window the client holds, the newest window last and
/// alone [`ClientWindowItem::SHOWN`], handed to `put` with their strings
/// kept until the next call on this thread.
pub(crate) fn listed<R>(put: impl FnOnce(&[ClientWindowItem]) -> R) -> R {
    let held = STATE.held.load();
    let held = held.as_deref().map_or(&[][..], Vec::as_slice);
    let form = STATE.form.relock().clone();
    let map_opens = map_opens();
    let mut listing = Vec::new();
    for (at, window) in held.iter().enumerate() {
        let shown = at + 1 == held.len();
        let open = form
            .as_ref()
            .filter(|(id, name, _)| shown && *id == window.mod_id && *name == window.window)
            .map(|(.., form)| form);
        let name = format!("{}:{}", window.mod_id, window.window);
        for mut item in items::list(&window.view, open, map_opens) {
            if shown {
                item.flags |= ClientWindowItem::SHOWN;
            }
            listing.push((name.clone(), item.target.id(), item));
        }
    }
    LENT.with_borrow_mut(|lent| {
        // The caller copies the strings after its call returns, so the last
        // listing's go only now.
        *lent = listing;
        let raw: Vec<ClientWindowItem> = lent
            .iter()
            .map(|(window, id, item)| ClientWindowItem {
                kind: item.kind,
                field: item.field,
                flags: item.flags,
                window: Str::new(window),
                item: Str::new(id),
                label: Str::new(&item.label),
                line: Str::new(&item.line),
                ..ClientWindowItem::EMPTY
            })
            .collect();
        put(&raw)
    })
}

/// Whether the build opens the Map screen from a window, as
/// [`Drawn::refresh`] reads it in each frame.
fn map_opens() -> bool {
    crate::engine::screens::byte(MAP_SCREEN).is_some()
        && crate::actions::client_control::GROUPS
            .toggle
            .get()
            .is_some()
}

/// Whether the carried windows draw on this build: the interface linked,
/// and neither its frame nor their draw is fused off, so a command no draw
/// can answer is refused when queued.
pub(super) fn draws() -> bool {
    !super::INTERFACE_FUSE.is_off() && !super::CARRIED_FUSE.is_off()
}

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
    // A reopened window starts without the form, so none lists before it draws.
    let _form = STATE
        .form
        .relock()
        .take_if(|(of, name, _)| *of == id && *name == window);
}

/// Forgets every window, as when the client leaves its server or loses
/// its link to it.
pub(crate) fn forget() {
    STATE.held.store(None);
    *STATE.form.relock() = None;
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
    /// window, set each frame: its Map items open the map with a button too,
    /// on a build whose windows do not let the game's map key through.
    map_opens: Option<u8>,
    /// The tries left for a map open the interface refused, such as on a
    /// full queue; `None` while none waits.
    map_tries: Option<u8>,
    /// The item a client command presses in this frame's draw.
    pressing: Option<Target>,
    /// The field, by its action and argument, and the text a client command
    /// put in it, answered once this frame's draw shows whether it holds it.
    filling: Option<(String, usize, String)>,
}

/// How many frames a refused map open is tried again, about a second.
const MAP_OPEN_TRIES: u8 = 60;

impl Drawn {
    fn new(key: (String, String)) -> Self {
        *STATE.form.relock() = None;
        Self {
            key,
            window: Window::new(),
            scroll: Scroll::new(),
            fields: Vec::new(),
            form: None,
            map_screen: crate::engine::screens::byte(MAP_SCREEN),
            map_opens: None,
            map_tries: None,
            pressing: None,
            filling: None,
        }
    }

    /// Whether a command presses `target`, one of the window's own buttons.
    fn presses(&self, target: &Target) -> bool {
        self.pressing.as_ref() == Some(target)
    }

    /// Whether a command presses the button that runs `action`.
    fn presses_run(&self, action: &str) -> bool {
        matches!(&self.pressing, Some(Target::Run(a)) if a == action)
    }

    /// Whether a command presses the switch of `key` in `action`'s list.
    fn presses_switch(&self, action: &str, key: PlayerKey) -> bool {
        matches!(&self.pressing, Some(Target::Switch(a, k)) if a == action && *k == key)
    }

    /// Takes in a client command `ask` on the window that shows, `view`, as
    /// the player's input would: a field takes its pick or flip at once and
    /// its text for the draw to show, and a press of a button or a switch is
    /// left for the draw. The answer, `None` for one the draw gives.
    fn take(&mut self, view: &View, ask: &Ask) -> Option<Answer> {
        let listed = items::list(view, self.form.as_ref(), self.map_opens.is_some());
        let Some(item) = listed.iter().find(|item| item.target == ask.target) else {
            return Some(Answer::Missing);
        };
        if let Target::Field(action, index) = &ask.target {
            let arg = view
                .entries
                .iter()
                .find(|e| e.id == *action)
                .and_then(|entry| entry.args.get(*index));
            let taken = arg.is_some_and(|arg| {
                fields::take(arg, self.field(action, *index, arg), ask.act, &ask.text)
            });
            if taken && ask.act == Act::Text {
                self.filling = Some((action.clone(), *index, ask.text.clone()));
                return None;
            }
            return Some(if taken { Answer::Shown } else { Answer::Off });
        }
        if ask.act != Act::Press
            || item.flags & ClientWindowItem::OFF != 0
            || !self.fields_set(view, ask)
        {
            return Some(Answer::Off);
        }
        self.pressing = Some(ask.target.clone());
        None
    }

    /// The answer to the text a command put in a field, once the window
    /// drew: shown while the field holds it, off when the game's own text
    /// won the draw.
    fn filled(&mut self) -> Answer {
        let Some((action, index, text)) = self.filling.take() else {
            return Answer::Off;
        };
        let holds = self
            .fields
            .iter()
            .find(|((a, i), _)| *a == action && *i == index)
            .is_some_and(|(_, field)| field.text.text() == text);
        if holds { Answer::Shown } else { Answer::Off }
    }

    /// Whether the press `ask` names a Run whose fields all hold a value, or
    /// no Run, as the draw turns a Run off while one is not set.
    fn fields_set(&mut self, view: &View, ask: &Ask) -> bool {
        let Target::Run(action) = &ask.target else {
            return true;
        };
        let Some(entry) = view.entries.iter().find(|e| e.id == *action) else {
            return true;
        };
        let from = self
            .form
            .as_ref()
            .filter(|form| form.action == *action)
            .map_or(0, |form| form.given.len());
        values(action, &entry.args, from, self).is_some()
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

    /// Whether the window steps aside on `screen`, the screen the player
    /// sees: the Map screen, opened with the map key or as the in-game
    /// menu's Map tab, which [`Tab::seen`] names by the Map screen too.
    ///
    /// [`Tab::seen`]: crate::actions::client_control::Tab::seen
    fn aside(&self, screen: Option<u8>) -> bool {
        screen.is_some() && screen == self.map_screen
    }

    /// Whether the game's map key reaches the game under the window: it
    /// holds a Map item, and no field of it has the caret, so a letter typed
    /// there stays in the field.
    fn passes_map(&self, view: &View) -> bool {
        view.items
            .iter()
            .any(|item| matches!(item, ItemView::Map(_)))
            && !self.fields.iter().any(|(_, field)| field.text.typing())
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
            let (id, window) = self.key.clone();
            *STATE.form.relock() = form.clone().map(|form| (id, window, form));
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
    /// The listing [`listed`] last lent on this thread, kept for the strings
    /// it lent until its next call here.
    static LENT: RefCell<Vec<(String, String, items::Listed)>> = const { RefCell::new(Vec::new()) };
    static DRAWN: RefCell<Option<Drawn>> = const { RefCell::new(None) };
    /// Whether this frame's draw wants the game's map key let through,
    /// taken by [`pass_map`] once every window of the frame drew.
    static MAP_WANTED: Cell<bool> = const { Cell::new(false) };
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
    // Claimed before the draw acts on it, so a command a mod cancels on its
    // own thread never acts; each claim ends in an answer or a retry.
    let asked = windows::asked().filter(|(id, _)| windows::claim(*id));
    let Some(top) = held.as_deref().and_then(|held| held.last()) else {
        DRAWN.set(None);
        if let Some((id, _)) = asked {
            windows::answer(id, Answer::Missing);
        }
        return;
    };
    let click = DRAWN.with_borrow_mut(|slot| {
        let shows = |drawn: &Drawn| drawn.key.0 == top.mod_id && drawn.key.1 == top.window;
        if !slot.as_ref().is_some_and(shows) {
            *slot = None;
        }
        let drawn =
            slot.get_or_insert_with(|| Drawn::new((top.mod_id.clone(), top.window.clone())));
        let aside = drawn.aside(screen);
        MAP_WANTED.set(!aside && drawn.passes_map(&top.view));
        if aside {
            drawn.step_aside();
            if let Some((id, _)) = asked {
                windows::retry(id);
            }
            return Click::Nothing;
        }
        drawn.refresh(
            crate::actions::client_control::GROUPS
                .toggle
                .get()
                .is_some(),
        );
        // Only the window that shows takes a command, as only it takes clicks.
        let pressed = asked.and_then(|(id, ask)| {
            let answer = if ask.mod_id == top.mod_id && ask.window == top.window {
                drawn.take(&top.view, &ask)
            } else {
                Some(Answer::Missing)
            };
            match answer {
                Some(answer) => {
                    windows::answer(id, answer);
                    None
                }
                None => Some(id),
            }
        });
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window; `drawn` stays in the
        // thread-local `DRAWN`, borrowed and unmoved for the call.
        let click = unsafe { window(engine, context, &top.view, drawn) };
        if let Some(id) = pressed {
            let answer = match drawn.pressing.take() {
                // Another click, the player's or Back, won the frame: the
                // press waits for the next one.
                Some(target) => made(&top.view, &target, &click).then_some(Answer::Shown),
                None => Some(drawn.filled()),
            };
            match answer {
                Some(answer) => windows::answer(id, answer),
                None => windows::retry(id),
            }
        }
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

/// Ends the frame's map key: lets it through the game's modal windows only
/// while this frame drew a carried window that wants it, `drawn` is whether
/// the loader's windows drew at all (not under the game's own popup), and
/// no field of any window has the caret. Called on every frame with a
/// context, so a skipped or fused-off draw gives the game its keys back.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread,
///   after every window of the frame drew.
/// - `context` is the non-zero context `Engine::context` read in this frame.
pub(super) unsafe fn pass_map(engine: &Engine, context: usize, drawn: bool) {
    let wanted = map_wanted(MAP_WANTED.take(), drawn, super::engine::caret_shown());
    if let Some(keys) = &engine.map_keys {
        // SAFETY: the caller's contract: inside the interface frame, on its
        // thread, with its live `context`.
        unsafe { keys.pass(context, wanted) };
    }
}

/// Whether the map key passes: a draw asked for it, the loader's windows
/// drew this frame, and no field types.
const fn map_wanted(asked: bool, drawn: bool, typing: bool) -> bool {
    asked && drawn && !typing
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
    if paint.button(texts::text("actions.close").as_bytes(), true) || drawn.presses(&Target::Close)
    {
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
    // The row button a command presses, counted down to among the buttons
    // that run its action.
    let mut row_press = match &drawn.pressing {
        Some(Target::RowButton(action, n)) => Some((action.clone(), *n)),
        _ => None,
    };
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
                    let commanded = match &mut row_press {
                        Some((action, n)) if *action == button.action => {
                            let now = *n == 0;
                            *n = n.wrapping_sub(1);
                            now
                        }
                        _ => false,
                    };
                    if paint.button(button.label.as_bytes(), true) || commanded {
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
                let on = drawn.map_opens.is_some();
                let pressed =
                    paint.button(open.as_bytes(), on) || (on && drawn.presses(&Target::Map));
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

/// Whether `click`, what the frame did in `view`, is the press of `target`
/// a client command made, so no other click won the frame.
fn made(view: &View, target: &Target, click: &Click) -> bool {
    match (target, click) {
        (Target::Close, Click::Close)
        | (Target::Cancel, Click::Cancel)
        | (Target::Map, Click::OpenMap(_)) => true,
        (Target::Run(action), Click::Run(ran, _)) => action == ran,
        (Target::Switch(action, key), Click::Run(ran, values)) => {
            action == ran
                && matches!(values.first(), Some(Value::Player { key: k, .. }) if k == key)
        }
        (Target::RowButton(action, n), _) => view
            .items
            .iter()
            .filter_map(|item| match item {
                ItemView::Row { buttons, .. } => Some(buttons),
                _ => None,
            })
            .flatten()
            .filter(|button| button.action == *action)
            .nth(*n)
            .is_some_and(|button| press(view, button) == *click),
        _ => false,
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
            let drawn_now = paint.toggle(name.as_bytes(), *on, !entry.offered);
            let commanded =
                (entry.offered && drawn.presses_switch(&entry.id, *key)).then_some(!*on);
            if let Some(now) = drawn_now.or(commanded) {
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
    let on = entry.offered && set;
    if !(paint.button(label.as_bytes(), on) || (on && drawn.presses_run(&entry.id))) {
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
    let on = entry.offered && set;
    let run = paint.button(texts::text("windows.run").as_bytes(), on)
        || (on && drawn.presses_run(&entry.id));
    if paint.button(texts::text("windows.cancel").as_bytes(), true)
        || drawn.presses(&Target::Cancel)
    {
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
