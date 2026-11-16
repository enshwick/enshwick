//! The mouse cursor over the loader's windows in a world. The game shows the
//! cursor there only for a HUD screen that wants it: each tick it asks
//! `ui.screen_shows_cursor` of the screen byte and shows the cursor and frees
//! its clip by the answer (client 26430625). A loader window or dialog over
//! the world is no screen, so the question answers yes while one shows
//! ([`window_shows`]), and the player can click it; the input builder holds
//! the player's look and move by the same answer.

use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_core::abi::HookFlags;

use crate::actions::local_held;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;

/// The cursor over loader windows, a loader feature of the client.
static CURSOR_FUSE: Fuse = Fuse::new("the cursor over loader windows");

/// How long a window noted in one frame holds the cursor and the camera
/// without the next frame noting it again: several slow frames, short enough
/// that a stopped interface hands the mouse back at once.
const HOLD_MS: u64 = 250;

/// What the cursor keeps while the game runs.
struct State {
    /// Until when, as [`local_held::now_ms`] counts, a loader or mod window
    /// or dialog holds the cursor and the camera; 0 for none. Each frame that
    /// showed one moves it on, so it lapses once the interface stops drawing.
    until: AtomicU64,
}

static STATE: State = State {
    until: AtomicU64::new(0),
};

/// Notes, once a frame on the interface thread, whether a loader or mod
/// window or dialog showed in the last frame.
pub(super) fn note(shown: bool) {
    let until = if shown {
        local_held::now_ms().saturating_add(HOLD_MS)
    } else {
        0
    };
    STATE.until.store(until, Ordering::Relaxed);
}

/// Whether a loader or mod window or dialog shows over the world, which takes
/// the cursor and holds the player's look and move; from any thread.
pub(crate) fn window_shows() -> bool {
    let until = STATE.until.load(Ordering::Relaxed);
    until != 0 && local_held::now_ms() < until
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
        if window_shows() { 1 } else { shown }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_holds_the_mouse_only_while_frames_keep_noting_it() {
        assert!(!window_shows(), "nothing noted");
        note(true);
        assert!(window_shows());
        note(false);
        assert!(!window_shows(), "a frame without a window hands it back");
        STATE.until.store(local_held::now_ms(), Ordering::Relaxed);
        assert!(
            !window_shows(),
            "an interface that stopped drawing hands it back"
        );
    }
}
