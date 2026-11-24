//! What the client's snapshot and commands read of each frame and do in
//! it.

use std::cell::OnceCell;

use super::{READS_FUSE, extensions};
use crate::actions::client_commands::{Applied, Kind, Seen};
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
    crate::actions::client_snapshot::frame(|| {
        view().map(|(view, tab)| FrameRead {
            play: view.play,
            screen: view.screen,
            return_to: view.return_to,
            host: view.host,
            tab,
            recipe,
        })
    });
    crate::actions::client_commands::frame(
        || {
            view().map(|(view, tab)| Seen {
                session: crate::actions::client_snapshot::session(),
                screen: view.screen,
                tab,
                recipe,
                held: crate::actions::local_held::players_held(),
            })
        },
        |kind| {
            let Some((view, _)) = view() else {
                return Applied::Busy;
            };
            // SAFETY: inside the frame on the interface thread, with its
            // own context and the interface state it read.
            unsafe { apply(context, view.state, kind) }
        },
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
            crate::actions::client_control::GROUPS
                .menu
                .get()
                .is_some_and(|menu| {
                    // SAFETY: inside the frame on the interface thread, with
                    // its own live context, as this function's contract gives.
                    unsafe { menu.post(context, post) }
                })
        },
    );
}

/// Hands `kind` to the game in this frame.
///
/// # Safety
///
/// - Called inside a frame on the interface thread; `context` is the frame's
///   own, non-zero, and `state` the interface state it read.
unsafe fn apply(context: usize, state: usize, kind: Kind) -> Applied {
    let groups = &crate::actions::client_control::GROUPS;
    match kind {
        Kind::Open(screen) | Kind::Close(screen) => {
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
        Kind::Tab(tab) => {
            let Some(layout) = groups.tab.get() else {
                return Applied::Unavailable;
            };
            // SAFETY: the caller's contract; `state` is the interface state
            // whose tab index sits at the routed `ui.menu_tab`.
            unsafe { layout.choose(core::ptr::with_exposed_provenance_mut(state), tab) };
            Applied::Taken
        }
        // Handed to their detours or the play module, never applied here.
        Kind::Look(_)
        | Kind::Move(..)
        | Kind::Press(..)
        | Kind::Highlight(_)
        | Kind::Join
        | Kind::Leave => Applied::Unavailable,
    }
}
