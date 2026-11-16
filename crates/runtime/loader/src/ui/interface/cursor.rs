//! The mouse cursor over the loader's windows in a world. The game shows the
//! cursor there only for a HUD screen that wants it: each tick it asks
//! `ui.screen_shows_cursor` of the screen byte and shows the cursor and frees
//! its clip by the answer (client 26430625). A loader window or dialog over
//! the world is no screen, so the question answers yes while one showed in
//! the last frame, and the player can click it.

use enshwick_core::abi::HookFlags;

use crate::actions::client_input::{note_window, window_shows as wanted};
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// The cursor over loader windows, a loader feature of the client.
static CURSOR_FUSE: Fuse = Fuse::new("the cursor over loader windows");

/// Notes, once a frame on the interface thread, whether a loader window or
/// dialog showed in the last frame; the input builder holds the mouse look
/// by the same note.
pub(super) fn note(shown: bool) {
    note_window(shown);
}

/// Links the screen question, on the client.
pub(super) fn link(loader: &Loader) {
    let at = Target::Pinned(pinned::ui::SCREEN_SHOWS_CURSOR);
    let _linked = loader.link_fused(&CURSOR_FUSE, at, &NEXT, shows_cursor, HookFlags::NONE);
}

crate::engine::chains::chain_detour! {
    /// The table's `ui.screen_shows_cursor(screen) -> bool`: also yes while
    /// a loader window or dialog shows, as for a game screen that wants the
    /// mouse; the game's other askers then treat the world as under a
    /// screen, as they do for its own.
    fn shows_cursor(screen: u8) -> u8;
    next NEXT;
    fuse &CURSOR_FUSE, "the cursor question";
    |onward| {
        let shown = onward.call();
        if wanted() { 1 } else { shown }
    }
}
