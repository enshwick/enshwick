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

use core::cell::RefCell;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use enshwick_loader_api::abi::{Registers, UiWidget};

use super::engine::{Engine, Window};
use super::{ENGINE, Painter, copy};
use crate::configure::rows::{self, Lock, Row, Widget};
use crate::host::Loader;

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
        schema: Option<enshwick_loader_api::config::Schema>,
        news: u64,
    },
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
            main_menu as enshwick_loader_api::abi::MidHookFn,
            "the main menu's Mods entry",
        ),
        (
            engine.pause_menu_site,
            pause_menu,
            "the pause menu's Mods entry",
        ),
    ] {
        let link = crate::mid::MidLink {
            owner: crate::chains::LOADER,
            handler,
            user: 0,
            priority: i32::MIN,
            flags: enshwick_loader_api::abi::HookFlags::NONE,
        };
        let status = loader
            .mids
            .lock()
            .map_or(enshwick_loader_api::abi::Status::REFUSED, |mut mids| {
                mids.add(site, link)
            });
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
    // SAFETY: inside the menu's drawing, a frame on its thread.
    if showing && unsafe { engine.back_pressed(NODE.load(Ordering::Relaxed)) } {
        BACK.store(1, Ordering::Relaxed);
    }
    let text = copy(&[b"Mods"]);
    if draw(engine, &text) && !showing {
        OPEN.store(from, Ordering::Relaxed);
    }
}

unsafe extern "system" fn main_menu(registers: *mut Registers, _user: *mut core::ffi::c_void) {
    // SAFETY: the stub passes the registers at the hook.
    let Some(registers) = (unsafe { registers.as_ref() }) else {
        return;
    };
    let menu = usize::try_from(registers.rdi).unwrap_or(0) + 8;
    let drawn = std::panic::catch_unwind(|| {
        // SAFETY: the hook sits before the main menu's Credits entry, where
        // `rdi` holds the menu page and +8 its entries' state.
        entry(FROM_MAIN, |engine, text| unsafe {
            engine.main_menu_entry(menu, text)
        });
    });
    if drawn.is_err() {
        crate::log!(ERROR, "interface: the loader panicked in the main menu");
    }
}

unsafe extern "system" fn pause_menu(_registers: *mut Registers, _user: *mut core::ffi::c_void) {
    let drawn = std::panic::catch_unwind(|| {
        // SAFETY: the hook sits right after the pause menu's Settings entry.
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
/// Inside a frame, on its thread, with no other window open; `frame` is the
/// frame [`next_frame`] counted.
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
        if BACK.swap(0, Ordering::Relaxed) != 0 && back(screen) {
            *slot = None;
            return;
        }
        // SAFETY: forwarded from the caller.
        let close = unsafe { show(loader, engine, context, screen) };
        NODE.store(screen.window.node_id(), Ordering::Relaxed);
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
        Page::List { .. } => true,
    }
}

/// Whether this client is joined to a server it does not host, which takes
/// messages: whose settings an administrator may change from here.
fn joined_elsewhere() -> bool {
    crate::net::ready(enshwick_loader_api::abi::NetMessage::SERVER) && !crate::world::hosting()
}

fn open_server_list(screen: &mut Screen) {
    crate::remote::forget();
    let asked = crate::net::to_server(&crate::remote::Message::Query { id: String::new() });
    screen.page = Page::ServerList { first: 0 };
    screen.message = (!asked).then(|| ("The server takes no messages now.".to_owned(), false));
}

/// Draws the screen's window; true when the player closed it.
unsafe fn show(loader: &Loader, engine: &Engine, context: usize, screen: &mut Screen) -> bool {
    let name = copy(&[b"enshwick/loader/mods"]);
    let title = match &screen.page {
        Page::List { .. } => copy(&[b"Mods"]),
        Page::Settings { index, .. } => loader
            .mods
            .get(*index)
            .map_or_else(|| copy(&[b"Mods"]), |m| copy(&[m.label().as_bytes()])),
        Page::ServerList { .. } => copy(&[b"Server settings"]),
        Page::ServerSettings { id, .. } => copy(&[b"Server settings: ", id.as_bytes()]),
    };
    // SAFETY: forwarded from the caller; the window lives in the screen's
    // state until it ends below.
    unsafe { engine.window_begin(context, &mut screen.window, &name, &title) };
    // SAFETY: inside the window just started, until it ends below.
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
            if joined_elsewhere() && paint.button(b"Server settings", true) {
                open_server_list(screen);
                // SAFETY: the window started above, in this frame.
                unsafe { engine.window_end(context, &mut screen.window) };
                return false;
            }
            let opened = list(loader, &paint, first);
            if let Some(index) = opened {
                screen.page = Page::Settings {
                    index,
                    first: 0,
                    rows: page_rows(loader, index),
                };
                screen.message = None;
            }
            paint.button(b"Back", true)
        }
        Page::Settings { index, first, rows } => {
            if let Some(message) = settings(loader, &paint, *index, first, rows) {
                *rows = page_rows(loader, *index);
                screen.message = Some(message);
            }
            if paint.button(b"Back", true) {
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
                    (!asked).then(|| ("The server takes no messages now.".to_owned(), false));
                screen.page = Page::ServerSettings {
                    id,
                    first: 0,
                    rows: Vec::new(),
                    schema: None,
                    news: 0,
                };
            } else if paint.button(b"Back", true) {
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
            if paint.button(b"Back", true) {
                open_server_list(screen);
            }
            false
        }
    };
    // SAFETY: the window started above, in this frame.
    unsafe { engine.window_end(context, &mut screen.window) };
    close
}

/// Draws a page of the server's mods; the mod whose settings the player
/// opened.
fn server_list(paint: &Painter<'_>, first: &mut usize) -> Option<String> {
    let mods = crate::remote::heard(|h| h.mods.clone());
    let Some(mods) = mods else {
        paint.label(
            b"Asking the server...",
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return None;
    };
    if mods.is_empty() {
        paint.label(
            b"The server shows you no settings.",
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
    }
    let mut opened = None;
    for (id, name) in mods.iter().skip(*first).take(MODS_PER_PAGE) {
        paint.label(name.as_bytes(), UiWidget::STYLE_H3, false, false);
        if paint.button(b"Settings", true) {
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
    schema: &mut Option<enshwick_loader_api::config::Schema>,
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
            *rows = rows::rows(&heard_schema, &text, None);
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
            b"Asking the server...",
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return message;
    };
    let mut changed = None;
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
            ("Sent to the server...".to_owned(), true)
        } else {
            ("The server takes no messages now.".to_owned(), false)
        });
    }
    message
}

/// Draws a page of the mod list; the mod whose settings the player opened.
fn list(loader: &Loader, paint: &Painter<'_>, first: &mut usize) -> Option<usize> {
    let mods = &loader.mods;
    let mut opened = None;
    for (index, record) in mods.iter().enumerate().skip(*first).take(MODS_PER_PAGE) {
        let off = if record.off.load(Ordering::Acquire) {
            " (off)"
        } else {
            ""
        };
        let heading = format!("{} {}{off}", record.label(), record.version);
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
        let described = record.schema.lock().is_ok_and(|s| s.is_some());
        if described {
            if paint.button(b"Settings", true) {
                opened = Some(index);
            }
        } else {
            let note = format!("Settings: mod-configs/{}.toml", record.id);
            paint.label(note.as_bytes(), UiWidget::STYLE_SECONDARY, false, false);
        }
    }
    pages(paint, first, mods.len(), MODS_PER_PAGE);
    opened
}

/// The Previous and Next buttons of a paged list of `count` items.
fn pages(paint: &Painter<'_>, first: &mut usize, count: usize, per_page: usize) {
    if count <= per_page {
        return;
    }
    let page = format!(
        "Page {} of {}",
        *first / per_page + 1,
        count.div_ceil(per_page)
    );
    paint.label(page.as_bytes(), UiWidget::STYLE_SECONDARY, false, true);
    if *first + per_page < count && paint.button(b"Next page", true) {
        *first += per_page;
    }
    if *first > 0 && paint.button(b"Previous page", true) {
        *first = first.saturating_sub(per_page);
    }
}

/// The rows of mod `index`'s settings page.
fn page_rows(loader: &Loader, index: usize) -> Vec<Row> {
    let Some(record) = loader.mods.get(index) else {
        return Vec::new();
    };
    let Some(schema) = record.schema.lock().ok().and_then(|s| s.clone()) else {
        return Vec::new();
    };
    let file = std::fs::read_to_string(loader.configs_dir.join(format!("{}.toml", record.id)))
        .unwrap_or_default();
    let server = record
        .server_synced
        .lock()
        .ok()
        .and_then(|s| s.clone())
        .and_then(|values| server_table(&values));
    rows::rows(&schema, &file, server.as_ref())
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
            b"Settings marked \"set by the server\" come from the server you joined.",
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    let schema = record.schema.lock().ok().and_then(|s| s.clone())?;
    let mut changed = None;
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
    schema: &enshwick_loader_api::config::Schema,
    row: &Row,
) -> Option<String> {
    let mut label = row.label.clone();
    if row.restart {
        label.push_str(" (after a restart)");
    }
    if row.lock == Lock::Server {
        label.push_str(" (set by the server)");
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
            let line = format!("{label}: {value} (edit mod-configs/{id}.toml)");
            paint.label(line.as_bytes(), UiWidget::STYLE_BODY, true, false);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_servers_join_values_read_as_a_table() {
        let table = server_table("{ radius = 4.0, mode = \"on\" }").unwrap();
        assert_eq!(table["radius"].as_float(), Some(4.0));
        assert!(server_table("nonsense").is_none());
    }
}
