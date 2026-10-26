//! Player actions in the interface: the actions the server offers, drawn
//! with the game's settings rows and buttons, on the Mods screen and in a
//! window of their own; a line on the HUD while an action with a hint is at
//! hand, and a key that opens the window there.
//!
//! The key is read once a frame while the HUD draws, so never in a menu,
//! and only while the game's window has the focus. It is one the game's
//! text chat cannot type into, an F key by default.

use core::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, PoisonError};

use enshwick::abi::UiWidget;
use enshwick::actions::Value;
use enshwick::actions::sheet::{ArgView, Entry, encode_args};

use super::engine::{Engine, Window};
use super::{HudItem, Painter, copy};
use crate::actions::{Hint, ModSheet};
use crate::texts;

/// Players of a list a page shows.
const PLAYERS_PER_PAGE: usize = 5;
/// Mods the list of mods with actions shows.
const MODS_PER_PAGE: usize = 5;
/// `VK_F7`, the key that opens the window by default.
const DEFAULT_KEY: u32 = 0x76;

/// The virtual key that opens the window at a hint; 0 for none.
static KEY: AtomicU32 = AtomicU32::new(DEFAULT_KEY);
/// The key's name, for the hint.
static KEY_NAME: Mutex<String> = Mutex::new(String::new());
/// Whether the key was down last frame.
static WAS_DOWN: AtomicBool = AtomicBool::new(false);
/// Asked to open the window, for a hint's mod and action.
static OPEN: Mutex<Option<(String, String)>> = Mutex::new(None);

/// The virtual key and its name for `name` in the loader's settings: F1 to
/// F12, a letter or a digit; "" or "off" for none.
fn parse_key(name: &str) -> Option<(u32, String)> {
    let upper = name.trim().to_ascii_uppercase();
    if upper.is_empty() || upper == "OFF" {
        return Some((0, String::new()));
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok())
        && (1..=12).contains(&n)
    {
        return Some((0x6f + n, upper));
    }
    match upper.as_bytes() {
        [c] if c.is_ascii_uppercase() || c.is_ascii_digit() => Some((u32::from(*c), upper)),
        _ => None,
    }
}

/// Takes the key from the loader's settings.
pub(crate) fn configure(name: &str) {
    let (key, shown) = parse_key(name).unwrap_or_else(|| {
        crate::log!(WARN, "actions_key {name:?} is no key: F7 opens actions");
        (DEFAULT_KEY, "F7".to_owned())
    });
    KEY.store(key, Ordering::Relaxed);
    *KEY_NAME.lock().unwrap_or_else(PoisonError::into_inner) = shown;
}

fn key_name() -> String {
    let name = KEY_NAME.lock().unwrap_or_else(PoisonError::into_inner);
    if name.is_empty() {
        "F7".to_owned()
    } else {
        name.clone()
    }
}

/// Whether the game's window has the focus.
fn focused() -> bool {
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    // SAFETY: `GetForegroundWindow` takes no arguments and may return null,
    // checked below.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return false;
    }
    let mut process = 0u32;
    // SAFETY: `window` is a non-null handle, checked by the guard before it,
    // and `process` is a live `u32` the call writes; a handle that closed
    // meanwhile only fails the call, leaving `process` 0.
    unsafe { GetWindowThreadProcessId(window, &raw mut process) };
    // SAFETY: `GetCurrentProcessId` takes no arguments and cannot fail.
    process == unsafe { GetCurrentProcessId() }
}

/// Whether the key went down since the last frame.
fn pressed(key: u32) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    let Ok(key) = i32::try_from(key) else {
        return false;
    };
    // SAFETY: `GetAsyncKeyState` takes a key code by value and keeps no
    // pointer; an unknown code returns 0.
    let down = unsafe { GetAsyncKeyState(key) } < 0;
    let was = WAS_DOWN.swap(down, Ordering::Relaxed);
    down && !was && focused()
}

/// Adds the hint line to the HUD and opens the window when the player
/// presses the key; runs while the HUD draws.
pub(super) fn hud(items: &mut Vec<HudItem>) {
    let hints = crate::actions::heard(|h| h.hints.clone());
    let key = KEY.load(Ordering::Relaxed);
    let Some(first) = hints.first() else {
        WAS_DOWN.store(false, Ordering::Relaxed);
        return;
    };
    let name = key_name();
    for hint in &hints {
        let line = if key == 0 {
            format!("{}: {}", hint.label, hint.subject)
        } else {
            texts::fill(
                "actions.hint",
                &[
                    ("key", &name),
                    ("label", &hint.label),
                    ("subject", &hint.subject),
                ],
            )
        };
        items.push(HudItem {
            anchor: UiWidget::BOTTOM_CENTER,
            kind: UiWidget::LABEL,
            style: UiWidget::STYLE_BODY,
            text: copy(&[line.as_bytes()]),
            value: 0.0,
        });
    }
    if key != 0 && pressed(key) {
        open(first);
    }
}

/// Opens the window at `hint`'s action.
fn open(hint: &Hint) {
    *OPEN.lock().unwrap_or_else(PoisonError::into_inner) =
        Some((hint.id.clone(), hint.action.clone()));
}

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
    /// The mod whose actions show; `None` for the list of mods.
    id: Option<String>,
    /// Whether the list of mods came first, so Back returns to it.
    from_list: bool,
    /// The action showing, by index, and the one asked for by id.
    action: usize,
    wanted: Option<String>,
    /// The first player of a list's page, or the first mod of the list.
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
    /// A view of mod `id`'s actions, or of the mods with actions, asking the
    /// server for them.
    pub(super) fn new(id: Option<String>, wanted: Option<String>) -> Self {
        let asked = crate::actions::ask_server();
        Self {
            from_list: id.is_none(),
            id,
            action: 0,
            wanted,
            first: 0,
            held: Vec::new(),
            held_for: None,
            message: (!asked).then(|| (texts::text("actions.closed").to_owned(), false)),
            news: crate::actions::heard(|h| h.news),
        }
    }

    /// One step back; true when the view is done.
    pub(super) fn back(&mut self) -> bool {
        if self.id.is_some() && self.from_list {
            self.id = None;
            self.first = 0;
            self.message = None;
            return false;
        }
        true
    }

    /// The mod showing, for the screen's title: its name.
    pub(super) fn title(&self) -> String {
        let name = self.id.as_ref().and_then(|id| {
            crate::actions::heard(|h| {
                h.sheets
                    .as_ref()?
                    .iter()
                    .find(|m| m.id == *id)
                    .map(|m| m.label.clone())
            })
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

/// Whether the server offers any actions, as last heard.
pub(super) fn any_offered() -> bool {
    crate::actions::heard(|h| {
        h.sheets
            .as_ref()
            .is_some_and(|s| s.iter().any(|m| !m.entries.is_empty()))
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
    match view.id.clone() {
        None => mods(paint, view, &sheets),
        Some(id) => match sheets.iter().find(|m| m.id == id) {
            Some(sheet) if !sheet.entries.is_empty() => actions(paint, view, sheet),
            _ => paint.label(
                texts::text("actions.none").as_bytes(),
                UiWidget::STYLE_SECONDARY,
                true,
                true,
            ),
        },
    }
}

/// The list of mods with actions.
fn mods(paint: &Painter<'_>, view: &mut View, sheets: &[ModSheet]) {
    let offering: Vec<&ModSheet> = sheets.iter().filter(|m| !m.entries.is_empty()).collect();
    if offering.is_empty() {
        paint.label(
            texts::text("actions.none").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            true,
        );
        return;
    }
    view.first = view.first.min(offering.len().saturating_sub(1)) / MODS_PER_PAGE * MODS_PER_PAGE;
    for sheet in offering.iter().skip(view.first).take(MODS_PER_PAGE) {
        if paint.button(sheet.label.as_bytes(), true) {
            view.id = Some(sheet.id.clone());
            view.action = 0;
            view.first = 0;
            view.message = None;
            return;
        }
    }
    if view.first + MODS_PER_PAGE < offering.len()
        && paint.button(texts::text("mods.page.next").as_bytes(), true)
    {
        view.first += MODS_PER_PAGE;
    }
    if view.first > 0 && paint.button(texts::text("mods.page.previous").as_bytes(), true) {
        view.first = view.first.saturating_sub(MODS_PER_PAGE);
    }
}

/// A mod's actions, one at a time.
fn actions(paint: &Painter<'_>, view: &mut View, sheet: &ModSheet) {
    if let Some(wanted) = view.wanted.take()
        && let Some(index) = sheet.entries.iter().position(|e| e.id == wanted)
    {
        view.action = index;
        view.first = 0;
    }
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

/// The actions window in game.
struct Open {
    view: View,
    window: Window,
}

thread_local! {
    static WINDOW: RefCell<Option<Open>> = const { RefCell::new(None) };
}

/// Draws the actions window while it is open.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
pub(super) unsafe fn draw(engine: &Engine, context: usize) {
    if let Some((id, action)) = OPEN.lock().unwrap_or_else(PoisonError::into_inner).take() {
        WINDOW.set(Some(Open {
            view: View::new(Some(id), Some(action)),
            window: Window::new(),
        }));
    }
    WINDOW.with_borrow_mut(|slot| {
        let Some(open) = slot.as_mut() else {
            return;
        };
        let title = copy(&[open.view.title().as_bytes()]);
        let name = copy(&[b"enshwick/loader/actions"]);
        // SAFETY: this function's `# Safety` gives the frame, its thread and
        // its `context`; `open.window` stays in the thread-local `WINDOW`,
        // borrowed and unmoved until `window_end` below, and `name` and
        // `title` live in the frame's text arena.
        unsafe { engine.window_begin(context, &mut open.window, &name, &title) };
        // SAFETY: the window started one call earlier in this frame, on its
        // thread, with the frame's `context`, and `paint` is not used after
        // `window_end` below.
        let paint = unsafe { Painter::new(engine, context) };
        body(&paint, &mut open.view);
        let close = paint.button(texts::text("actions.close").as_bytes(), true);
        // SAFETY: `window_begin` started `open.window` in this frame, on this
        // thread, with this `context`; it did not move, and the painter's
        // widgets close every frame they open.
        unsafe { engine.window_end(context, &mut open.window) };
        if close {
            *slot = None;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_from_their_names() {
        assert_eq!(parse_key("f7"), Some((0x76, "F7".to_owned())));
        assert_eq!(parse_key("F12"), Some((0x7b, "F12".to_owned())));
        assert_eq!(parse_key("k"), Some((u32::from(b'K'), "K".to_owned())));
        assert_eq!(parse_key("off"), Some((0, String::new())));
        assert_eq!(parse_key("F13"), None);
        assert_eq!(parse_key("Esc"), None);
    }

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
