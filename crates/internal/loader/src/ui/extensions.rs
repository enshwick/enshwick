//! Panels mods add to the game's own screens. The game's interface keeps no
//! window to append to and a game window has no position, so each edge of
//! the screen gets one full-screen window of its own on a layer above the
//! game's screens, shared by every panel at that edge, drawn while the
//! player has its screen open and only once a panel draws a widget there.

use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick::abi::{Header, Side, Status, Str, UiExtension, UiFrame, UiFrameFn};
use enshwick_gamedata::catalogue;

use crate::host::{Loader, Registered};
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

use super::engine::{Engine, UiText, Window};
use super::{DRAWING, Drawing, NEXT_TOKEN, copy};

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "screen extensions";

/// Each edge's window name, by edge, outside the `enshwick/<mod id>/` names
/// of mods' own windows.
const NAMES: [&[u8]; 4] = [
    b"enshwick:screen/left",
    b"enshwick:screen/right",
    b"enshwick:screen/top",
    b"enshwick:screen/bottom",
];

/// A mod's panel on one of the game's screens.
#[derive(Clone, Copy, Debug)]
struct Extension {
    registered: Registered<UiFrameFn>,
    /// The screen's value among the game's `HudScreenType`.
    screen: u8,
    /// [`UiExtension::LEFT`] to [`UiExtension::BOTTOM`].
    edge: u32,
}

/// Where the open screen is read; set at link on a build that has it.
#[derive(Debug)]
struct Linked {
    context_state: usize,
    state_play_data: usize,
    hud_screen: usize,
    /// The game's name of each screen by its value; empty for a value no
    /// constant names.
    names: Vec<&'static str>,
}

/// The extensions: open while mods initialise, sealed after.
static EXTENSIONS: Registry<Extension> = Registry::new();
static LINKED: OnceLock<Linked> = OnceLock::new();

thread_local! {
    /// One window per edge, built once and kept in place across frames.
    static WINDOWS: RefCell<[Window; 4]> = const {
        RefCell::new([
            Window::extension(UiExtension::LEFT),
            Window::extension(UiExtension::RIGHT),
            Window::extension(UiExtension::TOP),
            Window::extension(UiExtension::BOTTOM),
        ])
    };
    /// The edge whose window is open in this frame.
    static OPEN: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Registers mod `owner`'s panel at `edge` of the game's screen named
/// `screen`; client only, during init. A screen on which the player is in
/// the world with no window open is refused: a full-screen window there
/// would cover play while the mods' keys still fire.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    screen: &str,
    edge: u32,
    callback: UiFrameFn,
    user: *mut c_void,
) -> Status {
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    if edge > UiExtension::BOTTOM || crate::controls::OPEN_SCREENS.contains(&screen) {
        return Status::BAD_ARGS;
    }
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let Some(screen) = loader
        .reflection
        .enum_value(screens, screen)
        .ok()
        .and_then(|value| u8::try_from(value).ok())
    else {
        return Status::MISSING;
    };
    let in_game = catalogue::constant(catalogue::ids::HUD_SCREEN_IN_GAME, &loader.reflection)
        .and_then(|value| u8::try_from(value).ok());
    if in_game == Some(screen) {
        return Status::BAD_ARGS;
    }
    EXTENSIONS
        .add(Extension {
            registered: Registered::new(owner, callback, user),
            screen,
            edge,
        })
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Seals the extensions, so a registration after init is too late on every
/// side.
pub(super) fn seal() {
    let _sealed = EXTENSIONS.seal();
}

pub(super) fn link(loader: &Loader) {
    let extensions = EXTENSIONS.seal();
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let found = (|| {
        Ok::<_, &str>(Linked {
            context_state: table(pinned::map::CONTEXT_STATE)?,
            state_play_data: table(pinned::map::STATE_PLAY_DATA)?,
            hud_screen: table(pinned::ui::HUD_SCREEN)?,
            names: screen_names(loader),
        })
    })();
    match found {
        Ok(linked) => {
            let _first = LINKED.set(linked);
            if !extensions.is_empty() {
                crate::log!(
                    INFO,
                    "interface: {} screen extensions from mods",
                    extensions.len()
                );
            }
        }
        Err(id) if !extensions.is_empty() => {
            loader.feature_off(FEATURE, id, "this build's table lacks it");
        }
        Err(id) => crate::log!(
            INFO,
            "interface: mods read no open screen on this build: it lacks {id}"
        ),
    }
}

fn screen_names(loader: &Loader) -> Vec<&'static str> {
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let mut names = vec![""; usize::from(u8::MAX) + 1];
    let Ok(info) = loader.reflection.type_info(screens) else {
        crate::log!(
            INFO,
            "interface: mods read no open screen on this build: reflection lacks {screens}"
        );
        return names;
    };
    for constant in info.enum_values() {
        if let Some(slot) = usize::try_from(constant.value())
            .ok()
            .and_then(|value| names.get_mut(value))
        {
            *slot = constant.name();
        }
    }
    names
}

/// Runs `read` only for the in-game app state: a menu or loading frame may
/// still point at the last game's play data.
fn in_game_only(app_state: u8, read: impl FnOnce() -> Option<u8>) -> Option<u8> {
    (super::screen(app_state) == UiFrame::IN_GAME)
        .then(read)
        .flatten()
}

/// The HUD screen byte of the game the player is in; `None` outside one.
///
/// # Safety
///
/// - `context` is 0 or the live interface context of the frame being
///   drawn, on the interface thread.
unsafe fn screen_byte(engine: &Engine, context: usize, linked: &Linked) -> Option<u8> {
    if context == 0 {
        return None;
    }
    in_game_only(engine.app_state(context), || {
        // SAFETY: the caller passes the frame's live interface context on
        // the interface thread, non-zero by the check before it; the offsets
        // are the table's `map.context_state` and `map.state_play_data`.
        let data =
            unsafe { super::play_data(context, linked.context_state, linked.state_play_data) };
        // SAFETY: `data` is the non-null play data (checked before it), whose
        // HUD screen byte sits at `ui.hud_screen` (clients 410, 426 and 520).
        (data != 0).then(|| unsafe { crate::memory::read::<u8>(data, linked.hud_screen) })
    })
}

/// The game's name of screen byte `screen`; empty for `None`.
fn screen_name(linked: &Linked, screen: Option<u8>) -> Str {
    screen
        .and_then(|s| linked.names.get(usize::from(s)))
        .map_or(Str::EMPTY, |name| Str::new(name))
}

/// The game's name of the screen the player has open, for the frames mods
/// draw; empty outside a game and on a build without the screen's read.
///
/// # Safety
///
/// - `context` is 0 or the live interface context of the frame being
///   drawn, on the interface thread.
pub(super) unsafe fn open_screen(engine: &Engine, context: usize) -> Str {
    let Some(linked) = LINKED.get() else {
        return Str::EMPTY;
    };
    // SAFETY: the caller's contract.
    screen_name(linked, unsafe { screen_byte(engine, context, linked) })
}

/// The panels of `screen`, edge by edge and in registration order within
/// an edge, so each edge's window opens at most once a frame.
fn by_edge<T>(
    panels: &[T],
    screen: u8,
    place: impl Fn(&T) -> (u8, u32) + Copy,
) -> impl Iterator<Item = &T> {
    (UiExtension::LEFT..=UiExtension::BOTTOM)
        .flat_map(move |edge| panels.iter().filter(move |p| place(p) == (screen, edge)))
}

/// Draws the panels of the screen the player has open, each edge's in one
/// window.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window of the loader's or a mod's is open.
pub(super) unsafe fn draw(engine: &Engine, context: usize) {
    let (Some(extensions), Some(linked)) = (EXTENSIONS.sealed(), LINKED.get()) else {
        return;
    };
    if extensions.is_empty() {
        return;
    }
    // SAFETY: the caller passes the frame's live interface context on the
    // interface thread.
    let Some(screen) = (unsafe { screen_byte(engine, context, linked) }) else {
        return;
    };
    let name = screen_name(linked, Some(screen));
    for extension in by_edge(extensions, screen, |e| (e.screen, e.edge)) {
        if extension.registered.live() {
            // SAFETY: this function's `# Safety` gives the frame, its thread
            // and its `context`; the only window open is the extensions' own,
            // which `open` and `close` keep one at a time.
            unsafe { run(extension.registered, context, extension.edge, name) };
        }
    }
    // SAFETY: this function's `# Safety` gives the frame, its thread and its
    // `context`, in which `open` began any window still open.
    unsafe { close(engine, context) };
}

/// Opens `edge`'s window for a panel's first widget that needs one, ending
/// another edge's window first; an already open one stays.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread,
///   with that frame's non-zero `context`, from a panel's callback that
///   [`draw`] runs, so no window but the extensions' own is open.
pub(super) unsafe fn open(engine: &Engine, context: usize, edge: u32) {
    if OPEN.get() == Some(edge) {
        return;
    }
    // SAFETY: the caller's frame, thread and `context`.
    unsafe { close(engine, context) };
    let Some(index) = usize::try_from(edge).ok().filter(|&i| i < NAMES.len()) else {
        return;
    };
    let name = copy(&[NAMES[index]]);
    WINDOWS.with_borrow_mut(|windows| {
        // SAFETY: the caller's frame, thread and `context`; no other window
        // is open (`close` ended the last one), the window stays in the
        // thread-local `WINDOWS` until `close` ends it, and `name` lives in
        // the frame's text arena.
        unsafe { engine.window_begin(context, &mut windows[index], &name, &UiText::EMPTY) };
    });
    OPEN.set(Some(edge));
}

/// Ends the open edge's window, if any.
///
/// # Safety
///
/// - Called inside the frame of the game's interface update in which
///   [`open`] began the window, on its thread, with that frame's `context`.
unsafe fn close(engine: &Engine, context: usize) {
    let Some(index) = OPEN.take().and_then(|edge| usize::try_from(edge).ok()) else {
        return;
    };
    WINDOWS.with_borrow_mut(|windows| {
        if let Some(window) = windows.get_mut(index) {
            // SAFETY: `open` began `window` in this frame, on this thread,
            // with this `context`; it did not move, and the painter's widgets
            // close every frame they open.
            unsafe { engine.window_end(context, window) };
        }
    });
}

/// Runs an extension's callback; its first widget that needs a window
/// opens `edge`'s through [`open`].
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread,
///   with that frame's non-zero `context`, while no window but the
///   extensions' own is open.
unsafe fn run(registered: Registered<UiFrameFn>, context: usize, edge: u32, hud_screen: Str) {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    DRAWING.set(Some(Drawing {
        token,
        owner: registered.owner,
        context,
        window_open: false,
        hud: false,
        extension: Some(edge),
    }));
    let frame = UiFrame {
        header: Header::of::<UiFrame>(),
        token,
        screen: UiFrame::IN_GAME,
        reserved: 0,
        hud_screen,
    };
    // SAFETY: the callback lives in a mod DLL that stays loaded, and it gets
    // a live `UiFrame` and the pointer its mod registered with it; an
    // `extern "system"` function aborts rather than unwinds.
    crate::profile::call(
        registered.owner,
        crate::profile::Feature::Interface,
        || unsafe { (registered.callback)(&raw const frame, registered.user()) },
    );
    DRAWING.set(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_open_screen_is_read_only_in_a_game() {
        for menu_or_loading in [0, 3, 4, 5, 6, 8] {
            let read = in_game_only(menu_or_loading, || panic!("read the last game's play data"));
            assert_eq!(read, None, "app state {menu_or_loading}");
        }
        assert_eq!(in_game_only(7, || Some(12)), Some(12));
        assert_eq!(in_game_only(7, || None), None, "no play data");
    }

    #[test]
    fn panels_of_the_open_screen_draw_edge_by_edge_in_registration_order() {
        let (left, right, top) = (UiExtension::LEFT, UiExtension::RIGHT, UiExtension::TOP);
        let panels = [(5, right), (5, left), (7, left), (5, right), (5, top)];
        let order: Vec<_> = by_edge(&panels, 5, |p| *p)
            .map(core::ptr::from_ref)
            .collect();
        let expected = [1, 0, 3, 4].map(|i| core::ptr::from_ref(&panels[i]));
        assert_eq!(order, expected);
    }
}
