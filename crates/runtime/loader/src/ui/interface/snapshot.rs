//! What the client's snapshot and commands read of each frame and do in
//! it.

use std::cell::OnceCell;

use super::{READS_FUSE, extensions};
use crate::actions::client_commands::{Applied, ScreenChange, Seen};
use crate::actions::client_snapshot::{FrameRead, TabRead};
use crate::ui::interface::engine::Engine;

/// This frame's open screens and the menu's tab.
///
/// # Safety
///
/// - Called inside a frame on the interface thread, with `context` the
///   frame's own and non-zero.
unsafe fn view(engine: &Engine, context: usize) -> Option<(extensions::ScreenView, TabRead)> {
    let view = READS_FUSE
        .run("the client's screen reads", || {
            // SAFETY: the caller's contract.
            unsafe { extensions::screen_view(engine, context) }
        })
        .flatten()?;
    // SAFETY: `view` is this frame's, read on the interface thread by the
    // caller's contract.
    let tab = unsafe { extensions::tab(&view) };
    Some((view, tab))
}

/// Gives the client snapshot this frame's screens and runs the open client
/// commands, each only once a mod asked; the frame is read at most once.
///
/// # Safety
///
/// - Called inside a frame on the interface thread, with `context` the
///   frame's own and non-zero.
pub(super) unsafe fn note(engine: &Engine, context: usize) {
    let read = OnceCell::new();
    // SAFETY: the caller's contract.
    let view = || *read.get_or_init(|| unsafe { view(engine, context) });
    // The browser's draw since the last frame, taken once for both.
    let recipe = crate::actions::client_recipes::take_drawn();
    // The loader's own Mods screen, which reads at the main menu too; read
    // at most once, and only for a mod that asked.
    let shown = OnceCell::new();
    let loader_screen = || {
        *shown.get_or_init(|| {
            crate::engine::loader::loader().map_or_else(Default::default, super::mods::shown)
        })
    };
    crate::actions::client_snapshot::frame(
        || {
            view().map(|(view, tab)| FrameRead {
                play: view.play,
                screen: view.screen,
                return_to: view.return_to,
                host: view.host,
                tab,
                recipe,
            })
        },
        loader_screen,
    );
    crate::actions::client_commands::frame(
        || {
            view().map(|(view, tab)| Seen {
                session: crate::actions::client_snapshot::session(),
                screen: Some(view.screen),
                tab,
                recipe,
                held: crate::actions::local_held::players_held(),
                host: view.host,
                loader: loader_screen(),
            })
        },
        || Seen::menu(crate::actions::client_snapshot::session(), loader_screen()),
        // SAFETY: inside the frame on the interface thread, with its own
        // context and the interface state it read.
        |change| unsafe { apply(context, view().map(|(view, _)| view.state), change) },
    );
    // SAFETY: the caller's contract: inside the frame on the interface
    // thread, with its own non-zero context.
    unsafe { play(engine, context, || view().is_some()) };
}

/// Notes the app's stage and runs a join or leave in this frame, posting
/// through the menu's own commands; `game` answers whether the frame reads
/// the client's play data, asked only while one runs.
///
/// # Safety
///
/// - Called inside a frame on the interface thread, with `context` the
///   frame's own and non-zero.
unsafe fn play(engine: &Engine, context: usize, game: impl FnOnce() -> bool) {
    crate::actions::client_commands::play_frame(
        || engine.app_state(context),
        game,
        |post| {
            // SAFETY: inside the frame on the interface thread, with its own
            // live context, as this function's contract gives.
            unsafe { crate::actions::client_control::post_menu(context, post) }
        },
    );
}

/// Hands `change` to the game in this frame; the game's own screens change
/// only in a game, whose interface `state` the frame read.
///
/// # Safety
///
/// - Called inside a frame on the interface thread; `context` is the frame's
///   own, non-zero, and `state` the interface state it read, if any.
unsafe fn apply(context: usize, state: Option<usize>, change: ScreenChange) -> Applied {
    let groups = &crate::actions::client_control::GROUPS;
    match (change, state) {
        (ScreenChange::Mods(ask), _) => super::mods::ask(ask),
        (_, None) => Applied::Busy,
        (ScreenChange::Open(screen) | ScreenChange::Close(screen), Some(_)) => {
            let Some(toggle) = groups.toggle.get() else {
                return Applied::Unavailable;
            };
            // SAFETY: the caller's contract: the UI context is live only
            // inside the interface frame (client 26430625, seen live).
            if unsafe { toggle.toggle(context, screen) } {
                Applied::Taken
            } else {
                Applied::Busy
            }
        }
        (ScreenChange::Tab(tab), Some(state)) => {
            let Some(layout) = groups.tab.get() else {
                return Applied::Unavailable;
            };
            // SAFETY: the caller's contract; `state` is the interface state
            // whose tab index sits at the routed `ui.menu_tab`.
            unsafe { layout.choose(core::ptr::with_exposed_provenance_mut(state), tab) };
            Applied::Taken
        }
    }
}
