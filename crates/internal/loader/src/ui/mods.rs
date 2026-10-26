//! The Mods screen: a "Mods" entry in the title screen's main menu and in the
//! pause menu, beside Settings, opens a screen of the loader's own that lists
//! the mods and shows each mod's settings as the game's settings rows, built
//! from the schema the mod handed the loader.
//!
//! The entries are drawn from two hooks inside the menus' drawing: in the
//! main menu before its Credits entry, in the pause menu right after its
//! Settings entry, each calling the menu's own entry function. Escape and the
//! gamepad's B close the screen: the hooks ask for Back before the pause menu
//! does, so the press closes the screen and not the menu under it. The screen
//! closes with its menu too: a hook that did not run this frame means its
//! menu is gone.
//!
//! A change is written at once, as the game applies its own settings rows:
//! checked against the schema and by the mod, written with the file's
//! comments kept, and the mod told. While the client is joined to a server
//! that sets a mod's join values, those rows show the server's values and
//! take no changes.
//!
//! The world settings view shows the same rows under its last tab, inside
//! its scroll area, from a hook after the tab's own rows: read-only where the
//! view is, for a player who may not change the world.

use crate::mod_index::ModIndex;
use core::cell::RefCell;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use enshwick::abi::{Registers, UiWidget};
use enshwick::config::{Scope, Stage};

use crate::configure::rows::{self, Lock, Row, Widget};
use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;

use super::engine::{Engine, Window};
use super::{ENGINE, Painter, copy};

/// Mods a list page shows.
const MODS_PER_PAGE: usize = 4;
/// Settings a mod's page shows.
const ROWS_PER_PAGE: usize = 5;
/// The layers of the screen: over the main menu (layer 9) and over the
/// pause menu (0x1a), as the game's own settings and submenus.
const OVER_MAIN: u32 = 0x1a;
const OVER_PAUSE: u32 = 0x1b;
/// Rows of the screen this far apart.
const GAP: f32 = 14.0;

/// Where the screen was opened from.
const FROM_MAIN: u8 = 1;
const FROM_PAUSE: u8 = 2;

/// Asked to open, from a menu hook: where from.
static OPEN: AtomicU8 = AtomicU8::new(0);
/// The frame each menu hook last ran in.
static MAIN_SEEN: AtomicU64 = AtomicU64::new(0);
static PAUSE_SEEN: AtomicU64 = AtomicU64::new(0);
/// The frame being drawn, counted by the interface hook.
static FRAME: AtomicU64 = AtomicU64::new(0);
/// The screen's panel node, from its last frame, for the Back action.
static NODE: AtomicU64 = AtomicU64::new(0);
/// Set by a hook that saw Back while the screen showed.
static BACK: AtomicU8 = AtomicU8::new(0);
/// Set when the player asked for the world settings from the screen over
/// the pause menu.
static JUMP: AtomicU8 = AtomicU8::new(0);
/// Set while the screen shows over the pause menu, where the world settings
/// are one step away.
static JUMP_SHOWN: AtomicU8 = AtomicU8::new(0);
/// The pause menu's submenu state for the world settings.
const WORLD_SETTINGS_SUBMENU: u32 = 4;

/// What the screen shows.
enum Page {
    /// The mods, from `first`.
    List { first: usize },
    /// Mod `index`'s settings, from row `first`.
    Settings {
        index: usize,
        first: usize,
        rows: Vec<Row>,
    },
    /// The joined server's mods with settings, from `first`.
    ServerList { first: usize },
    /// The joined server's settings of mod `id`, from row `first`, as last
    /// heard (`news`).
    ServerSettings {
        id: String,
        first: usize,
        rows: Vec<Row>,
        schema: Option<enshwick::config::Schema>,
        news: u64,
    },
    /// The actions the server offers.
    Actions(super::actions::View),
}

struct Screen {
    from: u8,
    page: Page,
    /// The last change's outcome, and whether it went through.
    message: Option<(String, bool)>,
    window: Window,
}

thread_local! {
    static SCREEN: RefCell<Option<Screen>> = const { RefCell::new(None) };
}

/// Counts a frame of the interface.
pub(super) fn next_frame() -> u64 {
    FRAME.fetch_add(1, Ordering::Relaxed) + 1
}

/// Links the menu hooks; the screen draws from the interface hook.
pub(super) fn link(loader: &Loader, engine: &Engine) {
    for (site, handler, what) in [
        (
            engine.main_menu_site,
            main_menu as enshwick::abi::MidHookFn,
            "the main menu's Mods entry",
        ),
        (
            engine.pause_menu_site,
            pause_menu,
            "the pause menu's Mods entry",
        ),
        (
            engine.world_settings_site,
            world_settings,
            "the world settings' Mods rows",
        ),
    ] {
        if site == engine.world_settings_site && engine.view.is_none() {
            continue;
        }
        let link = crate::mid::MidLink {
            owner: ModIndex::LOADER,
            handler,
            user: 0,
            priority: i32::MIN,
            flags: enshwick::abi::HookFlags::NONE,
        };
        let status = loader.mids.relock().add(site, link);
        if !status.is_ok() {
            crate::log!(WARN, "{what} off: its hook was refused ({})", status.0);
        }
    }
}

/// Draws the entry and hears Back in a menu hook; `from` names the menu.
fn entry(from: u8, draw: impl FnOnce(&Engine, &super::engine::UiText) -> bool) {
    let Some(engine) = ENGINE.get() else {
        return;
    };
    let frame = FRAME.load(Ordering::Relaxed);
    let seen = if from == FROM_MAIN {
        &MAIN_SEEN
    } else {
        &PAUSE_SEEN
    };
    seen.store(frame, Ordering::Relaxed);
    let showing = SCREEN.with_borrow(|s| s.as_ref().map(|s| s.from)) == Some(from);
    // SAFETY: `entry` runs only from the main and pause menu hooks, which
    // the game reaches while drawing its menu inside a frame on the
    // interface thread.
    if showing && unsafe { engine.back_pressed(NODE.load(Ordering::Relaxed)) } {
        BACK.store(1, Ordering::Relaxed);
    }
    let text = copy(&[texts::text("mods.title").as_bytes()]);
    if draw(engine, &text) && !showing {
        OPEN.store(from, Ordering::Relaxed);
    }
}

/// The hook before the main menu's Credits entry: draws the Mods entry.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread, with null or
///   the registers it saved, valid and unaliased for the call.
unsafe extern "system" fn main_menu(registers: *mut Registers, _user: *mut core::ffi::c_void) {
    // SAFETY: the mid-hook stub passes either null or the registers it saved
    // on its stack, valid and unaliased for this call.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let menu = usize::try_from(registers.rdi).unwrap_or(0) + 8;
    let drawn = std::panic::catch_unwind(|| {
        // SAFETY: the hook at the table's `ui.main_menu_site` sits before the
        // main menu's Credits entry, on the frame's thread, where `rdi` holds
        // the menu page and +8 its entries' state; the text lives in the
        // frame's arena.
        entry(FROM_MAIN, |engine, text| unsafe {
            engine.main_menu_entry(menu, text)
        });
    });
    if drawn.is_err() {
        crate::log!(ERROR, "interface: the loader panicked in the main menu");
    }
}

/// The hook after the pause menu's Settings entry: draws the Mods entry and
/// opens the world settings when the Mods screen asked to.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread, with null or
///   the registers it saved, valid and unaliased for the call.
unsafe extern "system" fn pause_menu(registers: *mut Registers, _user: *mut core::ffi::c_void) {
    // SAFETY: the mid-hook stub passes either null or the registers it saved
    // on its stack, valid and unaliased for this call.
    let submenu = unsafe { registers.as_ref() }.map_or(0, |r| r.r12);
    if JUMP.swap(0, Ordering::Relaxed) != 0
        && let Ok(submenu) = usize::try_from(submenu)
        && submenu != 0
    {
        // SAFETY: `submenu` is non-zero by the `if` condition; at the hook at
        // the table's `ui.pause_menu_site`, `r12` points at the pause menu's
        // four-byte submenu state, which the menu reads and writes only on
        // this thread; 4 opens its world settings, as its own entry does.
        unsafe { crate::memory::write(submenu, 0, WORLD_SETTINGS_SUBMENU) };
        BACK.store(2, Ordering::Relaxed);
    }
    let drawn = std::panic::catch_unwind(|| {
        // SAFETY: the hook at the table's `ui.pause_menu_site` sits right
        // after the pause menu's Settings entry, on the frame's thread; the
        // text lives in the frame's arena.
        entry(FROM_PAUSE, |engine, text| unsafe {
            engine.pause_menu_entry(text)
        });
    });
    if drawn.is_err() {
        crate::log!(ERROR, "interface: the loader panicked in the pause menu");
    }
}

/// Draws the screen while it is open.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
/// - `frame` is the count [`next_frame`] gave this frame.
pub(super) unsafe fn draw(loader: &Loader, engine: &Engine, context: usize, frame: u64) {
    let opened = OPEN.swap(0, Ordering::Relaxed);
    SCREEN.with_borrow_mut(|slot| {
        if opened != 0 {
            let layer = if opened == FROM_MAIN {
                OVER_MAIN
            } else {
                OVER_PAUSE
            };
            *slot = Some(Screen {
                from: opened,
                page: Page::List { first: 0 },
                message: None,
                window: Window::screen(layer, GAP),
            });
            BACK.store(0, Ordering::Relaxed);
            if opened == FROM_PAUSE {
                let _asked = crate::actions::ask_server();
            }
        }
        let Some(screen) = slot.as_mut() else {
            return;
        };
        let seen = if screen.from == FROM_MAIN {
            MAIN_SEEN.load(Ordering::Relaxed)
        } else {
            PAUSE_SEEN.load(Ordering::Relaxed)
        };
        // The menu under the screen went away: the screen goes with it.
        if seen + 1 < frame {
            *slot = None;
            return;
        }
        JUMP_SHOWN.store(u8::from(screen.from == FROM_PAUSE), Ordering::Relaxed);
        match BACK.swap(0, Ordering::Relaxed) {
            // The world settings opened over the pause menu: the screen goes.
            2 => {
                *slot = None;
                JUMP_SHOWN.store(0, Ordering::Relaxed);
                return;
            }
            1 if back(screen) => {
                *slot = None;
                return;
            }
            _ => {}
        }
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window, which `show` asks for.
        let close = unsafe { show(loader, engine, context, screen) };
        NODE.store(engine.node_id(&screen.window), Ordering::Relaxed);
        if close {
            *slot = None;
        }
    });
}

/// One step back: from a mod's settings to the list; true when the screen
/// closes.
fn back(screen: &mut Screen) -> bool {
    screen.message = None;
    match screen.page {
        Page::Settings { .. } | Page::ServerList { .. } => {
            screen.page = Page::List { first: 0 };
            false
        }
        Page::ServerSettings { .. } => {
            open_server_list(screen);
            false
        }
        Page::Actions(ref mut view) => {
            if view.back() {
                screen.page = Page::List { first: 0 };
            }
            false
        }
        Page::List { .. } => true,
    }
}

/// Whether this client is joined to a server it does not host, which takes
/// messages: whose settings an administrator may change from here.
fn joined_elsewhere() -> bool {
    crate::net::ready(enshwick::abi::NetMessage::SERVER) && !crate::world::hosting()
}

fn open_server_list(screen: &mut Screen) {
    crate::remote::forget();
    let asked = crate::net::to_server(&crate::remote::Message::Query { id: String::new() });
    screen.page = Page::ServerList { first: 0 };
    screen.message = (!asked).then(|| (texts::text("mods.server.closed").to_owned(), false));
}

/// The screen's title on `page`.
fn title(loader: &Loader, page: &Page) -> super::engine::UiText {
    match page {
        Page::List { .. } => copy(&[texts::text("mods.title").as_bytes()]),
        Page::Settings { index, .. } => loader.mods.get(*index).map_or_else(
            || copy(&[texts::text("mods.title").as_bytes()]),
            |m| copy(&[m.label().as_bytes()]),
        ),
        Page::ServerList { .. } => copy(&[texts::text("mods.server").as_bytes()]),
        Page::ServerSettings { id, .. } => {
            copy(&[texts::fill("mods.server.of", &[("id", id)]).as_bytes()])
        }
        Page::Actions(view) => copy(&[view.title().as_bytes()]),
    }
}

/// Draws the screen's window; true when the player closed it.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
unsafe fn show(loader: &Loader, engine: &Engine, context: usize, screen: &mut Screen) -> bool {
    let name = copy(&[b"enshwick/loader/mods"]);
    let title = title(loader, &screen.page);
    // SAFETY: this function's `# Safety` gives the frame, its thread and its
    // `context`; `screen.window` stays in the thread-local `SCREEN`, borrowed
    // and unmoved until `window_end` below, and `name` and `title` live in
    // the frame's text arena.
    unsafe { engine.window_begin(context, &mut screen.window, &name, &title) };
    // SAFETY: the window started one call earlier in this frame, on its
    // thread, with the frame's `context`, and `paint` is not used after
    // either `window_end` below.
    let paint = unsafe { Painter::new(engine, context) };
    if let Some((text, ok)) = &screen.message {
        let style = if *ok {
            UiWidget::STYLE_POSITIVE
        } else {
            UiWidget::STYLE_NEGATIVE
        };
        paint.label(text.as_bytes(), style, true, true);
    }
    let close = match &mut screen.page {
        Page::List { first } => {
            if joined_elsewhere() && paint.button(texts::text("mods.server").as_bytes(), true) {
                open_server_list(screen);
                // SAFETY: `window_begin` started `screen.window` in this
                // frame, on this thread, with this `context`; it did not
                // move, and the painter's widgets close every frame they
                // open.
                unsafe { engine.window_end(context, &mut screen.window) };
                return false;
            }
            match list(loader, &paint, first) {
                Listed::Settings(index) => {
                    let rows = page_rows(loader, index);
                    go(
                        screen,
                        Page::Settings {
                            index,
                            first: 0,
                            rows,
                        },
                    );
                }
                Listed::Actions(id) => {
                    go(screen, Page::Actions(super::actions::View::new(id, None)));
                }
                Listed::Nothing => {}
            }
            paint.button(texts::text("mods.back").as_bytes(), true)
        }
        Page::Settings { index, first, rows } => {
            if let Some(message) = settings(loader, &paint, *index, first, rows) {
                *rows = page_rows(loader, *index);
                screen.message = Some(message);
            }
            if paint.button(texts::text("mods.back").as_bytes(), true) {
                screen.page = Page::List { first: 0 };
                screen.message = None;
            }
            false
        }
        Page::ServerList { first } => {
            if let Some(id) = server_list(&paint, first) {
                let asked =
                    crate::net::to_server(&crate::remote::Message::Query { id: id.clone() });
                screen.message =
                    (!asked).then(|| (texts::text("mods.server.closed").to_owned(), false));
                screen.page = Page::ServerSettings {
                    id,
                    first: 0,
                    rows: Vec::new(),
                    schema: None,
                    news: 0,
                };
            } else if paint.button(texts::text("mods.back").as_bytes(), true) {
                screen.page = Page::List { first: 0 };
                screen.message = None;
            }
            false
        }
        Page::ServerSettings {
            id,
            first,
            rows,
            schema,
            news,
        } => {
            if let Some(message) = server_settings(&paint, id, first, rows, schema, news) {
                screen.message = Some(message);
            }
            if paint.button(texts::text("mods.back").as_bytes(), true) {
                open_server_list(screen);
            }
            false
        }
        Page::Actions(view) => {
            super::actions::body(&paint, view);
            if paint.button(texts::text("mods.back").as_bytes(), true) && view.back() {
                go(screen, Page::List { first: 0 });
            }
            false
        }
    };
    // SAFETY: `window_begin` started `screen.window` in this frame, on this
    // thread, with this `context`; it did not move, and the painter's
    // widgets close every frame they open.
    unsafe { engine.window_end(context, &mut screen.window) };
    close
}

/// Draws a page of the server's mods; the mod whose settings the player
/// opened.
fn server_list(paint: &Painter<'_>, first: &mut usize) -> Option<String> {
    let mods = crate::remote::heard(|h| h.mods.clone());
    let Some(mods) = mods else {
        paint.label(
            texts::text("mods.server.asking").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return None;
    };
    if mods.is_empty() {
        paint.label(
            texts::text("mods.server.none").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
    }
    let mut opened = None;
    *first = clamp_first(*first, mods.len(), MODS_PER_PAGE);
    for (id, name) in mods.iter().skip(*first).take(MODS_PER_PAGE) {
        paint.label(name.as_bytes(), UiWidget::STYLE_H3, false, false);
        if paint.button(texts::text("mods.settings").as_bytes(), true) {
            opened = Some(id.clone());
        }
    }
    pages(paint, first, mods.len(), MODS_PER_PAGE);
    opened
}

/// Draws a page of the server's settings of mod `id`, rebuilding the rows
/// when the server sent news; the outcome of a change or the server's
/// answer.
fn server_settings(
    paint: &Painter<'_>,
    id: &str,
    first: &mut usize,
    rows: &mut Vec<Row>,
    schema: &mut Option<enshwick::config::Schema>,
    news: &mut u64,
) -> Option<(String, bool)> {
    let (heard_news, result) = crate::remote::heard(|h| (h.news, h.result.clone()));
    let mut message = None;
    if heard_news != *news {
        *news = heard_news;
        let settings = crate::remote::heard(|h| h.settings.clone());
        if let Some((heard_id, admin, heard_schema, text)) = settings
            && heard_id == id
        {
            let texts = crate::host::loader()
                .and_then(|loader| loader.mods.iter().find(|m| m.id == id))
                .and_then(|m| m.texts.relock().clone());
            let words = texts.as_ref().map(|texts| rows::Words {
                texts,
                language: crate::content::language(),
            });
            *rows = rows::rows(&heard_schema, &text, None, words);
            if !admin {
                for row in rows.iter_mut() {
                    row.lock = Lock::Server;
                }
            }
            *schema = Some(heard_schema);
        }
        message = result.map(|(ok, text)| (text, ok));
    }
    let Some(known) = schema.as_ref() else {
        paint.label(
            texts::text("mods.server.asking").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return message;
    };
    let mut changed = None;
    *first = clamp_first(*first, rows.len(), ROWS_PER_PAGE);
    for row in rows.iter().skip(*first).take(ROWS_PER_PAGE) {
        if let Some(value) = draw_row(paint, id, known, row) {
            changed = Some((row.key.clone(), value));
        }
        if !row.doc.is_empty() {
            paint.label(row.doc.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
    }
    pages(paint, first, rows.len(), ROWS_PER_PAGE);
    if let Some((key, value)) = changed {
        let sent = crate::net::to_server(&crate::remote::Message::Edit {
            id: id.to_owned(),
            key,
            value,
        });
        return Some(if sent {
            (texts::text("mods.server.sent").to_owned(), true)
        } else {
            (texts::text("mods.server.closed").to_owned(), false)
        });
    }
    message
}

/// What the player did on the mod list.
enum Listed {
    Nothing,
    /// Opened mod `index`'s settings.
    Settings(usize),
    /// Opened the actions of the mod with this id, or of every mod.
    Actions(Option<String>),
}

/// Moves the screen to `page`, with no message.
fn go(screen: &mut Screen, page: Page) {
    screen.page = page;
    screen.message = None;
}

/// Draws a page of the mod list; what the player did.
fn list(loader: &Loader, paint: &Painter<'_>, first: &mut usize) -> Listed {
    let mods = &loader.mods;
    let mut done = Listed::Nothing;
    if super::actions::any_offered() && paint.button(texts::text("actions").as_bytes(), true) {
        done = Listed::Actions(None);
    }
    if crate::world::hosting() && JUMP_SHOWN.load(Ordering::Relaxed) != 0 {
        paint.label(
            texts::text("mods.world.here").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
        if paint.button(texts::text("mods.world.open").as_bytes(), true) {
            JUMP.store(1, Ordering::Relaxed);
        }
    } else {
        paint.label(
            texts::text("mods.world.elsewhere").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    let lacking = loader.build.lacking(crate::TABLE_IDS);
    if !lacking.is_empty() {
        let line = texts::fill("mods.features.off", &[("features", lacking)]);
        paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
    }
    *first = clamp_first(*first, mods.len(), MODS_PER_PAGE);
    for (index, record) in mods.iter().enumerate().skip(*first).take(MODS_PER_PAGE) {
        let heading = if record.off.load(Ordering::Acquire) {
            let name = record.label();
            texts::fill("mods.off", &[("name", &name), ("version", &record.version)])
        } else {
            format!("{} {}", record.label(), record.version)
        };
        paint.label(heading.as_bytes(), UiWidget::STYLE_H3, false, false);
        if !record.meta.description.is_empty() {
            paint.label(
                record.meta.description.as_bytes(),
                UiWidget::STYLE_BODY,
                true,
                false,
            );
        }
        let credits = record.meta.credits();
        if !credits.is_empty() {
            paint.label(credits.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
        for feature in record.features.relock().iter() {
            if let Some(reason) = &feature.off {
                let line = texts::fill(
                    "mods.feature.off",
                    &[("feature", &feature.name), ("reason", reason)],
                );
                paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
            }
        }
        let (defines, extends) = crate::points::screen(index, loader);
        for (key, points) in [("mods.points", defines), ("mods.extends", extends)] {
            if !points.is_empty() {
                let line = texts::fill(key, &[("points", &points.join(", "))]);
                paint.label(line.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
            }
        }
        if super::actions::offers(&record.id)
            && paint.button(texts::text("actions").as_bytes(), true)
        {
            done = Listed::Actions(Some(record.id.clone()));
        }
        let world = record
            .schema
            .relock()
            .as_ref()
            .is_some_and(|s| s.fields.iter().any(|f| f.scope == Scope::World));
        if client_schema(record).is_some() {
            if paint.button(texts::text("mods.settings.yours").as_bytes(), true) {
                done = Listed::Settings(index);
            }
        } else if !world {
            let note = texts::fill("mods.settings.file", &[("id", &record.id)]);
            paint.label(note.as_bytes(), UiWidget::STYLE_SECONDARY, false, false);
        }
    }
    pages(paint, first, mods.len(), MODS_PER_PAGE);
    done
}

/// The first item of the page showing `first` in a list that now holds
/// `count` items: the last page when the list shrank below it.
fn clamp_first(first: usize, count: usize, per_page: usize) -> usize {
    if first < count {
        first
    } else {
        count.saturating_sub(1) / per_page.max(1) * per_page
    }
}

/// The Previous and Next buttons of a paged list of `count` items.
fn pages(paint: &Painter<'_>, first: &mut usize, count: usize, per_page: usize) {
    if count <= per_page {
        return;
    }
    let (page, pages) = (
        (*first / per_page + 1).to_string(),
        count.div_ceil(per_page).to_string(),
    );
    let page = texts::fill("mods.page", &[("page", &page), ("pages", &pages)]);
    paint.label(page.as_bytes(), UiWidget::STYLE_SECONDARY, false, true);
    if *first + per_page < count && paint.button(texts::text("mods.page.next").as_bytes(), true) {
        *first += per_page;
    }
    if *first > 0 && paint.button(texts::text("mods.page.previous").as_bytes(), true) {
        *first = first.saturating_sub(per_page);
    }
}

/// The rows of mod `index`'s settings page.
fn page_rows(loader: &Loader, index: usize) -> Vec<Row> {
    let Some(record) = loader.mods.get(index) else {
        return Vec::new();
    };
    let Some(schema) = client_schema(record) else {
        return Vec::new();
    };
    let file = std::fs::read_to_string(loader.configs_dir.join(format!("{}.toml", record.id)))
        .unwrap_or_default();
    let texts = record.texts.relock().clone();
    let words = texts.as_ref().map(|texts| rows::Words {
        texts,
        language: crate::content::language(),
    });
    rows::rows(&schema, &file, None, words)
}

/// The settings of `record` each player keeps, the ones the Mods screen
/// edits; `None` when it has none.
fn client_schema(record: &crate::host::ModRecord) -> Option<enshwick::config::Schema> {
    let mut schema = record.schema.relock().clone()?;
    schema.fields.retain(|f| f.scope == Scope::User);
    (!schema.fields.is_empty()).then_some(schema)
}

/// A joined server's join values, an inline table, as a table.
fn server_table(values: &str) -> Option<toml::Table> {
    format!("v = {values}")
        .parse::<toml::Table>()
        .ok()?
        .remove("v")?
        .as_table()
        .cloned()
}

/// Draws a page of mod `index`'s settings; the outcome of a change.
fn settings(
    loader: &Loader,
    paint: &Painter<'_>,
    index: usize,
    first: &mut usize,
    rows: &[Row],
) -> Option<(String, bool)> {
    let record = loader.mods.get(index)?;
    if rows.iter().any(|r| r.lock == Lock::Server) {
        paint.label(
            texts::text("mods.settings.server-marked").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    let schema = record.schema.relock().clone()?;
    let mut changed = None;
    *first = clamp_first(*first, rows.len(), ROWS_PER_PAGE);
    for row in rows.iter().skip(*first).take(ROWS_PER_PAGE) {
        if let Some(value) = draw_row(paint, &record.id, &schema, row) {
            changed = Some((row.key.clone(), value));
        }
        if !row.doc.is_empty() {
            paint.label(row.doc.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
    }
    pages(paint, first, rows.len(), ROWS_PER_PAGE);
    let (key, value) = changed?;
    Some(
        match crate::configure::apply(loader, index, &key, &value, "the Mods screen") {
            Ok(change) => (crate::configure::changed(&record.id, &key, &change), true),
            Err(reply) => (reply, false),
        },
    )
}

/// Draws one row; the TOML of the player's change.
fn draw_row(
    paint: &Painter<'_>,
    id: &str,
    schema: &enshwick::config::Schema,
    row: &Row,
) -> Option<String> {
    let mut label = row.label.clone();
    match row.stage {
        Stage::Startup => label = texts::fill("row.startup", &[("label", &label)]),
        Stage::Restart => label = texts::fill("row.restart", &[("label", &label)]),
        _ => {}
    }
    if row.lock == Lock::Server {
        label = texts::fill("row.server", &[("label", &label)]);
    }
    let read_only = row.lock != Lock::Open;
    match &row.widget {
        Widget::Toggle(on) => paint
            .toggle(label.as_bytes(), *on, read_only)
            .map(rows::toggled),
        Widget::Slider {
            value,
            min,
            max,
            steps,
            whole,
        } => paint
            .setting_slider(label.as_bytes(), *value, (*min, *max), *steps, read_only)
            .map(|v| rows::slid(v, *whole)),
        Widget::Choice { options, index } => {
            let options: Vec<&[u8]> = options.iter().map(String::as_bytes).collect();
            let picked = paint.dropdown(
                label.as_bytes(),
                &options,
                u32::try_from(*index).unwrap_or(u32::MAX),
                read_only,
            )?;
            rows::picked(schema.field(&row.key)?, picked as usize)
        }
        Widget::Text(value) => {
            let line = texts::fill(
                "row.text",
                &[("label", &label), ("value", value), ("id", id)],
            );
            paint.label(line.as_bytes(), UiWidget::STYLE_BODY, true, false);
            None
        }
    }
}

/// The view's last tab, under which the mods' rows show.
const LAST_TAB: u8 = 3;

/// The mods' rows in the world settings, kept while the view shows.
struct WorldRows {
    /// The frame they last showed in.
    frame: u64,
    /// Each mod with settings: index, label, schema and rows.
    mods: Vec<(usize, String, enshwick::config::Schema, Vec<Row>)>,
    /// The last change's outcome.
    message: Option<(String, bool)>,
}

thread_local! {
    static WORLD: RefCell<Option<WorldRows>> = const { RefCell::new(None) };
}

/// Every mod's world rows: its settings but each player's own.
fn world_rows(loader: &Loader) -> Vec<(usize, String, enshwick::config::Schema, Vec<Row>)> {
    loader
        .mods
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let mut schema = record.schema.relock().clone()?;
            schema.fields.retain(|f| f.scope == Scope::World);
            if schema.fields.is_empty() {
                return None;
            }
            // Joined to another's game: its join values, shown, not changed.
            let joined = joined_elsewhere();
            let server = record
                .server_synced
                .relock()
                .clone()
                .and_then(|values| server_table(&values));
            if joined {
                schema.fields.retain(|f| f.stage == Stage::Sync);
                if schema.fields.is_empty() {
                    return None;
                }
            }
            let file = crate::worlds::show_text(loader, record);
            let texts = record.texts.relock().clone();
            let words = texts.as_ref().map(|texts| rows::Words {
                texts,
                language: crate::content::language(),
            });
            let mut rows = rows::rows(&schema, &file, server.as_ref().filter(|_| joined), words);
            if joined {
                for row in &mut rows {
                    row.lock = Lock::Server;
                }
            }
            Some((index, record.label(), schema, rows))
        })
        .collect()
}

/// The hook in the world settings' scroll area: draws the mods' rows under
/// its last tab.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread, with null or
///   the registers it saved, valid and unaliased for the call.
unsafe extern "system" fn world_settings(registers: *mut Registers, _user: *mut core::ffi::c_void) {
    // SAFETY: the mid-hook stub passes either null or the registers it saved
    // on its stack, valid and unaliased for this call.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let (Ok(view), Ok(tab_state)) = (
        usize::try_from(registers.rsi),
        usize::try_from(registers.r15),
    ) else {
        return;
    };
    let Some(layout) = ENGINE.get().and_then(|engine| engine.view) else {
        return;
    };
    if view == 0 || tab_state == 0 {
        return;
    }
    // SAFETY: `tab_state` is non-zero, checked by the guard before it; at the
    // hook at the table's `ui.world_settings_site`, `r15` holds the view's tab
    // state, whose shown tab sits at the table's `ui.view_tab`.
    let tab = unsafe { crate::memory::read::<u8>(tab_state, layout.tab) };
    if tab != LAST_TAB {
        return;
    }
    // SAFETY: `view` is non-zero, checked by the guard before it; at the hook
    // at the table's `ui.world_settings_site`, `rsi` holds the view's config,
    // whose byte at the table's `ui.view_read_only` makes every row read-only,
    // as for a player who is no host.
    let read_only = unsafe { crate::memory::read::<u8>(view, layout.read_only) } != 0;
    let drawn = std::panic::catch_unwind(|| world(read_only));
    if drawn.is_err() {
        crate::log!(
            ERROR,
            "interface: the loader panicked in the world settings"
        );
    }
}

/// Draws the mods' rows under the world settings' last tab, in its scroll
/// area; read-only for a player who may not change the world.
fn world(read_only: bool) {
    let (Some(engine), Some(loader)) = (ENGINE.get(), crate::host::loader()) else {
        return;
    };
    let context = engine.context();
    if context == 0 {
        return;
    }
    let frame = FRAME.load(Ordering::Relaxed);
    // SAFETY: `world` runs only from `world_settings`, the hook inside the
    // world settings' scroll area, where the game's own window is open in the
    // frame on its thread; `context` is non-zero, checked by the guard before
    // it, and `paint` stays inside this call.
    let paint = unsafe { Painter::new(engine, context) };
    WORLD.with_borrow_mut(|slot| {
        let fresh = slot.as_ref().is_none_or(|w| w.frame + 1 < frame);
        if fresh {
            *slot = Some(WorldRows {
                frame,
                mods: world_rows(loader),
                message: None,
            });
        }
        let Some(world) = slot.as_mut() else {
            return;
        };
        world.frame = frame;
        if world.mods.is_empty() {
            return;
        }
        paint.label(
            texts::text("mods.title").as_bytes(),
            UiWidget::STYLE_H2,
            false,
            false,
        );
        let note = if crate::worlds::current().is_some() {
            texts::text("world.belongs")
        } else {
            texts::text("world.next")
        };
        paint.label(note.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        if let Some((text, ok)) = &world.message {
            let style = if *ok {
                UiWidget::STYLE_POSITIVE
            } else {
                UiWidget::STYLE_NEGATIVE
            };
            paint.label(text.as_bytes(), style, true, false);
        }
        let mut changed = None;
        for (index, label, schema, rows) in &world.mods {
            paint.label(label.as_bytes(), UiWidget::STYLE_H3, false, false);
            for row in rows {
                let mut row = row.clone();
                if read_only && row.lock == Lock::Open {
                    row.lock = Lock::File;
                }
                let id = loader.mods.get(*index).map_or("", |m| m.id.as_str());
                if let Some(value) = draw_row(&paint, id, schema, &row) {
                    changed = Some((*index, row.key.clone(), value));
                }
            }
        }
        let Some((index, key, value)) = changed else {
            return;
        };
        let id = loader
            .mods
            .get(index)
            .map_or("", |m| m.id.as_str())
            .to_owned();
        world.message = Some(
            match crate::configure::apply(loader, index, &key, &value, "the world settings") {
                Ok(change) => (crate::configure::changed(&id, &key, &change), true),
                Err(reply) => (reply, false),
            },
        );
        world.mods = world_rows(loader);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_past_a_shrunk_list_moves_to_its_last_page() {
        assert_eq!(clamp_first(8, 20, 8), 8);
        assert_eq!(clamp_first(16, 12, 8), 8);
        assert_eq!(clamp_first(8, 5, 8), 0);
        assert_eq!(clamp_first(8, 8, 8), 0);
        assert_eq!(clamp_first(8, 0, 8), 0);
    }

    #[test]
    fn a_servers_join_values_read_as_a_table() {
        let table = server_table("{ radius = 4.0, mode = \"on\" }").unwrap();
        assert_eq!(table["radius"].as_float(), Some(4.0));
        assert!(server_table("nonsense").is_none());
    }
}
