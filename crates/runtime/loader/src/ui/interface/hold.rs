//! The world held under the loader's windows, as under a game screen: the
//! mouse cursor shows and the player's character takes no input while a
//! loader or mod window or dialog shows over the world
//! ([`screens::window_shows`]).
//!
//! The game shows the cursor only for a HUD screen that wants it: each tick
//! it asks `ui.screen_shows_cursor` of the screen byte and shows the cursor
//! and frees its clip by the answer. A loader window is no screen, so the
//! question answers yes while one shows. The player's input is held in the
//! input builder's one loader detour, which the client commands own
//! ([`crate::actions::client_input`]), as the builder leaves it while the
//! game blocks the input (client 26430625, seen live). Both run under
//! [`screens::HOLD_FUSE`], and the cursor answers yes only while that
//! detour runs, so the cursor never shows while the camera still turns.

use enshwick_core::abi::HookFlags;

use crate::actions::client_input;
use crate::engine::loader::{Loader, Target};
use crate::engine::pinned;
use crate::engine::screens::{self, HOLD_FUSE};

/// Links the cursor question on the client, where the input builder's
/// detour, linked before the interface, holds the input.
pub(super) fn link(loader: &Loader) {
    if !client_input::runs() {
        let build = pinned::input::CLIENT_BUILD.id();
        let _off = loader.fuse_off(&HOLD_FUSE, build, "the input builder is not linked");
        return;
    }
    let at = Target::Pinned(pinned::ui::SCREEN_SHOWS_CURSOR);
    let _linked = loader.link_fused(&HOLD_FUSE, at, &NEXT, shows_cursor, HookFlags::NONE);
}

crate::engine::chains::chain_detour! {
    /// The table's `ui.screen_shows_cursor(screen) -> bool`: also yes while
    /// a loader window or dialog shows and the input is held, as for a game
    /// screen that wants the mouse; the game's other askers then treat the
    /// world as under a screen, as they do for its own.
    fn shows_cursor(screen: u8) -> u8;
    next NEXT;
    fuse &HOLD_FUSE, "the cursor question";
    |onward| {
        let shown = onward.call();
        if screens::window_shows() && client_input::runs() { 1 } else { shown }
    }
}
