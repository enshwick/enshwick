//! The game's HUD screens by name: each member of the reflected
//! `HudScreenType` by the screen byte it stands for, for every layer that
//! names a screen; and whether a loader or mod window covers the world as a
//! screen would, for the cursor and the input builder.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_gamedata::catalogue;
use windows_sys::Win32::System::SystemInformation::GetTickCount64;

use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;

/// The name of each screen byte; empty for a byte no member names.
static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
/// The interface's screen, prompt and host reads, off on a build that lacks
/// them and at their first panic; the frames then read as outside a game,
/// and the client snapshot answers off.
pub(crate) static READS_FUSE: Fuse = Fuse::new("screen reads");

/// How long a window noted in one frame holds the cursor and the player's
/// input without the next frame noting it again: several slow frames, short
/// enough that a stopped interface hands the mouse back at once.
const WINDOW_HOLD_MS: u64 = 250;

/// A loader or mod window over the world, as the interface notes it.
struct WindowShown {
    /// Until when, in `GetTickCount64` milliseconds, a window or dialog holds
    /// the cursor and the player's look and move; 0 for none. Each frame that
    /// showed one moves it on, so it lapses once the interface stops drawing.
    until: AtomicU64,
}

static WINDOW: WindowShown = WindowShown {
    until: AtomicU64::new(0),
};

fn now_ms() -> u64 {
    // SAFETY: `GetTickCount64` takes no arguments and has no preconditions.
    unsafe { GetTickCount64() }
}

/// Notes, once a frame on the interface thread, whether a loader or mod
/// window or dialog showed in the last frame.
pub(crate) fn note_window(shown: bool) {
    let until = if shown {
        now_ms().saturating_add(WINDOW_HOLD_MS)
    } else {
        0
    };
    WINDOW.until.store(until, Ordering::Relaxed);
}

/// Whether a loader or mod window or dialog shows over the world, which takes
/// the cursor and holds the player's input; from any thread.
pub(crate) fn window_shows() -> bool {
    let until = WINDOW.until.load(Ordering::Relaxed);
    until != 0 && now_ms() < until
}

/// The world held under loader windows: the cursor shows and the player's
/// input is held. One fuse for both, so the cursor never shows while the
/// camera still turns; the cursor's detour carries it, and the input
/// builder's detour, which the client commands own, reads it.
pub(crate) static HOLD_FUSE: Fuse = Fuse::new("the world held under loader windows");

/// Whether the player's input is held now: a window shows and the hold is
/// on; from any thread.
pub(crate) fn holds_world() -> bool {
    window_shows() && !HOLD_FUSE.is_off()
}

/// The interface's play data, from the interface context `context`: its G38
/// state at `context_state`, whose first word is the play state, which
/// holds the play data at `state_play_data`; 0 while any of them is null.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
/// - `context_state` and `state_play_data` are the build table's
///   `map.context_state` and `map.state_play_data`.
pub(crate) unsafe fn play_data(
    context: usize,
    context_state: usize,
    state_play_data: usize,
) -> usize {
    // SAFETY: the caller passes the live interface context, which keeps its
    // G38 state pointer at `map.context_state`.
    let state = unsafe { crate::engine::memory::read_ptr::<u8>(context, context_state) };
    if state.is_null() {
        return 0;
    }
    // SAFETY: `state` is the context's non-null G38 state (checked before
    // it), whose first word is the play state pointer or null (clients 410,
    // 426 and 520).
    let play = unsafe { crate::engine::memory::read_ptr::<u8>(state.expose_provenance(), 0) };
    if play.is_null() {
        return 0;
    }
    // SAFETY: `play` is the non-null play state (checked before it), which
    // holds its play data pointer at `map.state_play_data`.
    unsafe { crate::engine::memory::read_ptr::<u8>(play.expose_provenance(), state_play_data) }
        .expose_provenance()
}

/// Each screen byte's name on `loader`'s build, or the type's name when the
/// build does not reflect it.
fn names(loader: &Loader) -> Result<Vec<&'static str>, &'static str> {
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let info = loader.reflection.type_info(screens).map_err(|_| screens)?;
    let mut names = vec![""; usize::from(u8::MAX) + 1];
    for constant in info.enum_values() {
        if let Some(slot) = usize::try_from(constant.value())
            .ok()
            .and_then(|value| names.get_mut(value))
        {
            *slot = constant.name();
        }
    }
    Ok(names)
}

/// Keeps the screens' names of `loader`'s build.
///
/// # Errors
///
/// The reflected type's name when the build does not reflect it: every
/// screen is then unnamed.
pub(crate) fn link(loader: &Loader) -> Result<(), &'static str> {
    let _first = NAMES.set(names(loader)?);
    Ok(())
}

/// The game's name of screen byte `screen`; empty for a byte no member
/// names, and before [`link`].
pub(crate) fn name(screen: u8) -> &'static str {
    NAMES
        .get()
        .and_then(|names| names.get(usize::from(screen)).copied())
        .unwrap_or_default()
}

/// The screen byte the game names `name`; `None` for an empty or unknown
/// name, and before [`link`].
pub(crate) fn byte(name: &str) -> Option<u8> {
    let names = NAMES.get()?;
    let at = names
        .iter()
        .position(|named| !name.is_empty() && *named == name)?;
    u8::try_from(at).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_without_the_screens_reflected_names_none() {
        let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
        assert_eq!(names(&crate::start::tests::bare_loader()), Err(screens));
        assert_eq!(link(&crate::start::tests::bare_loader()), Err(screens));
        assert_eq!(name(1), "");
        assert_eq!(byte("InGame"), None);
        assert_eq!(byte(""), None);
    }

    #[test]
    fn a_window_holds_the_mouse_only_while_frames_keep_noting_it() {
        assert!(!window_shows(), "nothing noted");
        note_window(true);
        assert!(window_shows());
        assert!(holds_world(), "the hold is on while its fuse is");
        note_window(false);
        assert!(!window_shows(), "a frame without a window hands it back");
        WINDOW.until.store(now_ms(), Ordering::Relaxed);
        assert!(
            !window_shows(),
            "an interface that stopped drawing hands it back"
        );
    }
}
