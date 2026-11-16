//! The world held under the loader's windows, as under a game screen: the
//! mouse cursor shows and the player's character takes no input while a
//! loader or mod window or dialog shows over the world
//! ([`screens::window_shows`]).
//!
//! The game shows the cursor only for a HUD screen that wants it: each tick
//! it asks `ui.screen_shows_cursor` of the screen byte and shows the cursor
//! and frees its clip by the answer. A loader window is no screen, so the
//! question answers yes while one shows. The player's input is held as the
//! input builder leaves it while the game blocks the input (client
//! 26430625, seen live). Both run under one fuse, so the cursor never shows
//! while the camera still turns, or the other way round.

use enshwick_core::abi::HookFlags;

use crate::actions::client_control::GROUPS;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::{pinned, screens};

/// The world held under loader windows, a loader feature of the client.
static FUSE: Fuse = Fuse::new("the world held under loader windows");

/// Links the input hold, then the cursor question, on the client; neither
/// links without the other.
pub(super) fn link(loader: &Loader) {
    let build = pinned::input::CLIENT_BUILD;
    if GROUPS.input.get().is_none() {
        let _off = loader.fuse_off(&FUSE, build.id(), "the input group is off");
        return;
    }
    // Linked after the client commands' link on the builder, so this link
    // runs inside it: the hold comes before a mod's handed look or move.
    if !loader.link_fused(
        &FUSE,
        Target::Pinned(build),
        &BUILD_NEXT,
        held,
        HookFlags::NONE,
    ) {
        return;
    }
    let at = Target::Pinned(pinned::ui::SCREEN_SHOWS_CURSOR);
    let _linked = loader.link_fused(&FUSE, at, &NEXT, shows_cursor, HookFlags::NONE);
}

crate::engine::chains::chain_detour! {
    /// The table's `ui.screen_shows_cursor(screen) -> bool`: also yes while
    /// a loader window or dialog shows, as for a game screen that wants the
    /// mouse; the game's other askers then treat the world as under a
    /// screen, as they do for its own.
    fn shows_cursor(screen: u8) -> u8;
    next NEXT;
    fuse &FUSE, "the cursor question";
    |onward| {
        let shown = onward.call();
        if screens::window_shows() { 1 } else { shown }
    }
}

crate::engine::chains::chain_detour! {
    /// The client's input builder, as the client commands link it: after the
    /// game's pass, the player's input is held while a loader window shows.
    fn held(
        input: *mut u8,
        consumer: usize,
        other: usize,
        a4: usize,
        mapping: *const u8,
        settings: usize,
        camera: usize,
        mode: u32,
        a9: usize,
        a10: usize,
        a11: u32,
        a12: usize,
    ) -> ();
    next BUILD_NEXT;
    fuse &FUSE, "the input hold";
    |onward| {
        onward.call();
        if screens::window_shows()
            && !input.is_null()
            && let Some(layout) = GROUPS.input.get()
        {
            // SAFETY: the builder's own non-null input record, right after
            // its pass on its thread, which nothing else touches meanwhile.
            unsafe { layout.hold(input, mode) };
        }
    }
}
