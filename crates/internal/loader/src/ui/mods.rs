//! The Mods screen: a "Mods" entry in the title screen's main menu and in the
//! pause menu, beside Settings, opens a screen of the loader's own that lists
//! the mods. Each mod's Settings button opens its settings as the game's
//! settings rows, built from the schema the mod handed the loader: each
//! player's own, then the world's. In a world this process runs those are
//! the world's; in the main menu, the defaults of new worlds. Joined to
//! another's server, the server's settings of the mod are a page of their
//! own, which only its administrators change.
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
//! Long lists scroll in the game's own scroll area, and a text setting
//! takes typed text in the game's own text field, set on Enter; typing left
//! without Enter is dropped. A build without either pages its lists and
//! shows its texts read-only.

use core::cell::RefCell;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use enshwick::abi::{Registers, UiWidget};
use list::{Listed, list};
use server::{server_list, server_settings};
use settings::{page_rows, settings};

use crate::configure::rows::Row;
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::interface_parts::Part;
use crate::listing;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::mods_screen::{self, BACK_PRESSED, Back, Heard, Kind};
use crate::pinned;
use crate::texts;

use super::engine::{Engine, Scroll, TextBuffer, Window};
use super::{ENGINE, Painter, copy};

/// Mods a list page shows on a build without scroll areas.
const MODS_PER_PAGE: usize = 4;
/// Settings a mod's page shows on a build without scroll areas.
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
/// Turns the main menu's Mods entry off at its first panic.
static MAIN_MENU_FUSE: Fuse = Fuse::new("main menu Mods entry");
/// Turns the pause menu's Mods entry off at its first panic.
static PAUSE_MENU_FUSE: Fuse = Fuse::new("pause menu Mods entry");
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
    /// The mods, from `first` while they page.
    List { first: usize },
    /// Mod `index`'s settings, from row `first` while they page, where
    /// `place` says, with the mod's id when the joined server keeps its
    /// world settings (`server`).
    Settings {
        index: usize,
        first: usize,
        rows: Vec<Row>,
        entries: Entries,
        place: mods_screen::Place,
        server: Option<String>,
    },
    /// The joined server's mods with settings, from `first` while they page.
    ServerList { first: usize },
    /// The joined server's settings of mod `id`, from row `first` while
    /// they page, as last heard (`news`).
    ServerSettings {
        id: String,
        first: usize,
        rows: Vec<Row>,
        schema: Option<enshwick::config::Schema>,
        news: u64,
        entries: Entries,
    },
    /// The actions the server offers.
    Actions(super::actions::View),
}

impl Page {
    /// Which page this is, as far as Back cares.
    const fn kind(&self) -> Kind {
        match self {
            Self::List { .. } => Kind::List,
            Self::Settings { .. } => Kind::Settings,
            Self::ServerList { .. } => Kind::ServerList,
            Self::ServerSettings { .. } => Kind::ServerSettings,
            Self::Actions(_) => Kind::Actions,
        }
    }

    /// The text fields of the page, on a settings page.
    fn entries(&mut self) -> Option<&mut Entries> {
        match self {
            Self::Settings { entries, .. } | Self::ServerSettings { entries, .. } => Some(entries),
            _ => None,
        }
    }

    /// Whether one of the page's text fields has the caret, so Back ends
    /// the typing and not the page.
    fn editing(&mut self) -> bool {
        self.entries().is_some_and(|e| e.editing)
    }

    /// Back while a text field has the caret: the field gives it up in the
    /// next frame, and the page stays.
    fn end_typing(&mut self) {
        if let Some(entries) = self.entries() {
            entries.ending = true;
        }
    }
}

/// The text fields of a settings page by row key, each with its buffer,
/// kept while the page shows.
#[derive(Default)]
struct Entries {
    fields: Vec<(String, Box<TextBuffer>)>,
    /// One of them had the caret in the last frame.
    editing: bool,
    /// Back came while one had the caret: the next frame takes it away.
    ending: bool,
}

impl Entries {
    /// The buffer of row `key`, made with `value` the first time the row
    /// shows.
    fn buffer(&mut self, key: &str, value: &str) -> &mut TextBuffer {
        let at = self.fields.iter().position(|(k, _)| k == key);
        let at = at.unwrap_or_else(|| {
            let mut buffer = TextBuffer::new();
            buffer.set(value);
            self.fields.push((key.to_owned(), buffer));
            self.fields.len() - 1
        });
        &mut self.fields[at].1
    }
}

struct Screen {
    from: u8,
    page: Page,
    /// The last change's outcome, and whether it went through.
    message: Option<(String, bool)>,
    window: Window,
    scroll: Scroll,
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
    for (site, handler, part, id) in [
        (
            engine.main_menu_site,
            main_menu as enshwick::abi::MidHookFn,
            Part::MAIN_MENU,
            pinned::ui::MAIN_MENU_SITE,
        ),
        (
            engine.pause_menu_site,
            pause_menu,
            Part::PAUSE_MENU,
            pinned::ui::PAUSE_MENU_SITE,
        ),
    ] {
        if site == 0 {
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
            let reason = format!("refused the link ({})", status.0);
            loader.feature_off(part.name, id.id(), &reason);
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
        BACK.store(BACK_PRESSED, Ordering::Relaxed);
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
    let Some(menu) = usize::try_from(registers.rdi)
        .ok()
        .filter(|&page| page != 0)
    else {
        return;
    };
    let menu = menu + 8;
    let _drawn = MAIN_MENU_FUSE.run("the main menu's entry", || {
        // SAFETY: the hook at the table's `ui.main_menu_site` sits before the
        // main menu's Credits entry, on the frame's thread, where `rdi` holds
        // the menu page and +8 its entries' state; the text lives in the
        // frame's arena.
        entry(FROM_MAIN, |engine, text| unsafe {
            engine.main_menu_entry(menu, text)
        });
    });
}

/// The hook after the pause menu's Settings entry: draws the Mods entry.
///
/// # Safety
///
/// - Called only by the mid-hook stub, on the hooked thread.
unsafe extern "system" fn pause_menu(_registers: *mut Registers, _user: *mut core::ffi::c_void) {
    let _drawn = PAUSE_MENU_FUSE.run("the pause menu's entry", || {
        // SAFETY: the hook at the table's `ui.pause_menu_site` sits right
        // after the pause menu's Settings entry, on the frame's thread; the
        // text lives in the frame's arena.
        entry(FROM_PAUSE, |engine, text| unsafe {
            engine.pause_menu_entry(text)
        });
    });
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
                scroll: Scroll::new(),
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
        if mods_screen::missed_frame(seen, frame) {
            *slot = None;
            return;
        }
        if let Some(entries) = screen.page.entries() {
            entries.ending = false;
        }
        let editing = screen.page.editing();
        match mods_screen::heard(BACK.swap(0, Ordering::Relaxed), editing) {
            Heard::EndTyping => screen.page.end_typing(),
            Heard::Back if back(screen) => {
                *slot = None;
                return;
            }
            Heard::Back | Heard::Nothing => {}
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
    match mods_screen::back(screen.page.kind()) {
        Back::Close => return true,
        Back::List => screen.page = Page::List { first: 0 },
        Back::ServerList => open_server_list(screen),
        Back::Inside => {
            if let Page::Actions(view) = &mut screen.page
                && view.back()
            {
                screen.page = Page::List { first: 0 };
            }
        }
    }
    false
}

/// Whether this client is joined to a server it does not host, which takes
/// messages: whose settings an administrator may change from here.
fn joined_elsewhere() -> bool {
    crate::net::ready(enshwick::abi::NetMessage::SERVER.get()) && !crate::world::hosting()
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
    // `window_end` below.
    let paint = unsafe { Painter::new(engine, context) };
    if let Some((text, ok)) = &screen.message {
        let style = if *ok {
            UiWidget::STYLE_POSITIVE
        } else {
            UiWidget::STYLE_NEGATIVE
        };
        paint.label(text.as_bytes(), style, true, true);
    }
    let height = listing::scroll_height(screen.message.is_some());
    // SAFETY: the window started above in this frame, on its thread, with
    // its `context`; `screen.scroll` stays in the thread-local `SCREEN`,
    // unmoved until `scroll_end` below.
    let scrolled = unsafe { engine.scroll_begin(context, &mut screen.scroll, height, GAP) };
    paint.set_scrolled(scrolled);
    let next = content(loader, &paint, screen.from, &mut screen.page);
    // SAFETY: `scroll_begin` started `screen.scroll` in this frame inside
    // this window; it did not move, and the painter's widgets close every
    // frame they open.
    unsafe { engine.scroll_end(&mut screen.scroll) };
    let back = paint.button(texts::text("mods.back").as_bytes(), true);
    // SAFETY: `window_begin` started `screen.window` in this frame, on this
    // thread, with this `context`; it did not move, and the scroll area and
    // the painter's widgets closed every frame they opened.
    unsafe { engine.window_end(context, &mut screen.window) };
    match next {
        Next::Stay => {}
        Next::ServerList => {
            open_server_list(screen);
            return false;
        }
        Next::Page(page, message) => {
            screen.page = page;
            screen.message = message;
            return false;
        }
        Next::Message(message) => screen.message = Some(message),
    }
    if !back {
        return false;
    }
    if screen.page.editing() {
        // Taken as a Back press in the next frame, which ends the typing.
        BACK.store(BACK_PRESSED, Ordering::Relaxed);
        return false;
    }
    self::back(screen)
}

/// What the screen does after a frame of its page.
enum Next {
    /// Nothing.
    Stay,
    /// Opens the server's list of mods.
    ServerList,
    /// Opens a page, with a message or none.
    Page(Page, Option<(String, bool)>),
    /// Shows a message.
    Message((String, bool)),
}

/// Draws the page's content, inside the scroll area where the build has
/// one; what the screen does next. `from` is the menu the screen opened
/// over.
fn content(loader: &Loader, paint: &Painter<'_>, from: u8, page: &mut Page) -> Next {
    match page {
        Page::List { first } => {
            if joined_elsewhere() && paint.button(texts::text("mods.server").as_bytes(), true) {
                return Next::ServerList;
            }
            match list(loader, paint, first) {
                Listed::Settings(index) => settings_page(loader, from, index),
                Listed::Actions(id) => {
                    Next::Page(Page::Actions(super::actions::View::new(id, None)), None)
                }
                Listed::Nothing => Next::Stay,
            }
        }
        Page::Settings {
            index,
            first,
            rows,
            entries,
            place,
            server,
        } => {
            let joined = if FRAME.load(Ordering::Relaxed).is_multiple_of(JOINED_EVERY) {
                joined_elsewhere()
            } else {
                place.joined
            };
            let now = self::place(from, joined);
            if now != *place {
                *place = now;
                *rows = page_rows(loader, *index, now);
                *server = server_id(loader, *index, now);
                *entries = Entries::default();
            }
            if let Some(id) = server
                && paint.button(texts::text("mods.server").as_bytes(), true)
            {
                return server_page(id.clone());
            }
            match settings(loader, paint, *index, first, rows, entries, place.world) {
                Some(message) => {
                    *rows = page_rows(loader, *index, *place);
                    Next::Message(message)
                }
                None => Next::Stay,
            }
        }
        Page::ServerList { first } => server_list(paint, first).map_or(Next::Stay, server_page),
        Page::ServerSettings {
            id,
            first,
            rows,
            schema,
            news,
            entries,
        } => server_settings(paint, id, first, rows, schema, news, entries)
            .map_or(Next::Stay, Next::Message),
        Page::Actions(view) => {
            super::actions::body(paint, view);
            Next::Stay
        }
    }
}

/// Opens mod `index`'s settings page; `from` is the menu under the screen.
fn settings_page(loader: &Loader, from: u8, index: usize) -> Next {
    if loader.mods.get(index).is_none() {
        return Next::Stay;
    }
    let place = place(from, joined_elsewhere());
    Next::Page(
        Page::Settings {
            index,
            first: 0,
            rows: page_rows(loader, index, place),
            entries: Entries::default(),
            place,
            server: server_id(loader, index, place),
        },
        None,
    )
}

/// Where the screen opened over menu `from` stands now, joined to a server
/// that takes messages (`joined`) or not.
fn place(from: u8, joined: bool) -> mods_screen::Place {
    mods_screen::Place::new(from == FROM_PAUSE, crate::worlds::running(), joined)
}

/// The id of mod `index` when the joined server at `place` keeps its world
/// settings on a page of its own.
fn server_id(loader: &Loader, index: usize, place: mods_screen::Place) -> Option<String> {
    let record = loader.mods.get(index)?;
    (place.joined
        && record
            .schema
            .relock()
            .as_ref()
            .is_some_and(mods_screen::has_world_settings))
    .then(|| record.id.clone())
}

/// Frames between two looks at whether the joined server takes messages,
/// which needs a lock; a world starting or stopping is seen at once.
const JOINED_EVERY: u64 = 60;

/// Asks the joined server for its settings of mod `id` and opens their page.
fn server_page(id: String) -> Next {
    let asked = crate::net::to_server(&crate::remote::Message::Query { id: id.clone() });
    Next::Page(
        Page::ServerSettings {
            id,
            first: 0,
            rows: Vec::new(),
            schema: None,
            news: 0,
            entries: Entries::default(),
        },
        (!asked).then(|| (texts::text("mods.server.closed").to_owned(), false)),
    )
}

mod list;
mod server;
mod settings;
