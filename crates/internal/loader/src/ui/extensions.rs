//! Panels mods add to the game's own screens. The game's interface keeps no
//! window to append to and a game window has no position, so each panel is
//! a full-screen window of its own on a layer above the game's screens,
//! with a panel at one edge, drawn while the player has its screen open.

use core::cell::RefCell;
use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use enshwick::abi::{Header, Side, Status, UiExtension, UiFrame, UiFrameFn};
use enshwick_gamedata::catalogue;

use crate::host::{Loader, Registered};
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

use super::engine::{Engine, UiText, Window};
use super::{DRAWING, Drawing, NEXT_TOKEN, copy};

/// The feature's name in the log and on the Mods screen.
const FEATURE: &str = "screen extensions";

/// A mod's panel on one of the game's screens.
#[derive(Clone, Copy, Debug)]
struct Extension {
    registered: Registered<UiFrameFn>,
    /// The screen's value among the game's `HudScreenType`.
    screen: u8,
    /// [`UiExtension::LEFT`] to [`UiExtension::BOTTOM`].
    edge: u32,
}

/// Where the open screen is read, and each extension's window name, by
/// sealed index; set at link on a build that has them.
#[derive(Debug)]
struct Linked {
    context_state: usize,
    state_play_data: usize,
    hud_screen: usize,
    names: Vec<Box<[u8]>>,
}

/// The extensions: open while mods initialise, sealed after.
static EXTENSIONS: Registry<Extension> = Registry::new();
static LINKED: OnceLock<Linked> = OnceLock::new();

thread_local! {
    static WINDOW: RefCell<Window> = const { RefCell::new(Window::new()) };
}

/// Registers mod `owner`'s panel at `edge` of the game's screen named
/// `screen`; client only, during init.
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
    if edge > UiExtension::BOTTOM {
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
    EXTENSIONS
        .add(Extension {
            registered: Registered::new(owner, callback, user),
            screen,
            edge,
        })
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Seals the extensions and reads where the open screen lies; without the
/// table's entries for it, only the extensions turn off.
pub(super) fn link(loader: &Loader) {
    let extensions = EXTENSIONS.seal();
    if extensions.is_empty() {
        return;
    }
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let found = (|| {
        Ok::<_, &str>((
            table(pinned::map::CONTEXT_STATE)?,
            table(pinned::map::STATE_PLAY_DATA)?,
            table(pinned::ui::HUD_SCREEN)?,
        ))
    })();
    let (context_state, state_play_data, hud_screen) = match found {
        Ok(found) => found,
        Err(id) => {
            loader.feature_off(FEATURE, id, "this build's table lacks it");
            return;
        }
    };
    let names = extensions
        .iter()
        .enumerate()
        .map(|(index, extension)| {
            let id = loader
                .mods
                .get(extension.registered.owner.get())
                .map_or("", |record| record.id.as_str());
            format!("enshwick/{id}/screen/{index}").into_bytes().into()
        })
        .collect();
    let _first = LINKED.set(Linked {
        context_state,
        state_play_data,
        hud_screen,
        names,
    });
    crate::log!(
        INFO,
        "interface: {} screen extensions from mods",
        extensions.len()
    );
}

/// Draws the panels of the screen the player has open, each in its own
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
    // SAFETY: the caller passes the frame's live interface context on the
    // interface thread; the offsets are the table's `map.context_state` and
    // `map.state_play_data`.
    let data = unsafe { super::play_data(context, linked.context_state, linked.state_play_data) };
    if data == 0 {
        return;
    }
    // SAFETY: `data` is the non-null play data (checked before it), whose
    // HUD screen byte sits at `ui.hud_screen` (clients 410, 426 and 520).
    let screen = unsafe { crate::memory::read::<u8>(data, linked.hud_screen) };
    let open = extensions
        .iter()
        .zip(&linked.names)
        .filter(|(e, _)| e.screen == screen && e.registered.live());
    for (extension, name) in open {
        let name = copy(&[name]);
        WINDOW.with_borrow_mut(|window| {
            *window = Window::extension(extension.edge);
            // SAFETY: this function's `# Safety` gives the frame, its thread
            // and its `context`; `window` stays in the thread-local `WINDOW`,
            // borrowed and unmoved until `window_end` below, and `name` lives
            // in the frame's text arena.
            unsafe { engine.window_begin(context, window, &name, &UiText::EMPTY) };
            // SAFETY: the window started one call earlier in this frame, on
            // its thread, with this `context`; the mod's callback cannot end
            // it, since the painter refuses an extension's window widgets.
            unsafe { run(extension.registered, context) };
            // SAFETY: `window_begin` started `window` in this frame, on this
            // thread, with this `context`; it did not move, and the painter's
            // widgets close every frame they open.
            unsafe { engine.window_end(context, window) };
        });
    }
}

/// Runs an extension's callback inside its open panel.
///
/// # Safety
///
/// - Called inside the extension's window, begun in this frame on the
///   interface thread with `context`.
unsafe fn run(registered: Registered<UiFrameFn>, context: usize) {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    DRAWING.set(Some(Drawing {
        token,
        owner: registered.owner,
        context,
        window_open: true,
        hud: false,
        extension: true,
    }));
    let frame = UiFrame {
        header: Header::of::<UiFrame>(),
        token,
        screen: UiFrame::IN_GAME,
        reserved: 0,
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
