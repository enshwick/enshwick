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
#[derive(Clone, Debug)]
struct Extension {
    registered: Registered<UiFrameFn>,
    /// The screen's value among the game's `HudScreenType`.
    screen: u8,
    /// [`UiExtension::LEFT`] to [`UiExtension::BOTTOM`].
    edge: u32,
    /// The host templates the panel shows for; empty for every host.
    hosts: Box<[[u8; 16]]>,
}

impl Extension {
    /// Whether the panel shows for a screen whose host has `template`.
    fn shows_for(&self, template: [u8; 16]) -> bool {
        self.hosts.is_empty() || self.hosts.contains(&template)
    }
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
    /// Where the play data keeps the interaction prompt, on a build whose
    /// table has it.
    prompt: Option<PromptAt>,
    /// Where the play data keeps the open screen's `u32` host entity, on a
    /// build whose table has it.
    screen_host: Option<usize>,
    /// A bit for each screen byte with no window open, as
    /// [`crate::controls::open_screens`] reads them; 0 on a build without.
    open_screens: u64,
}

/// The interaction prompt's offsets in the play data: its title, a
/// `UiText` (pointer, length, hash), the `u32` verb and the valid byte.
#[derive(Clone, Copy, Debug)]
struct PromptAt {
    title: usize,
    verb: usize,
    valid: usize,
}

/// The longest prompt title a frame reads, in bytes; a longer one is cut at
/// its last whole character before it.
const TITLE_MAX: usize = 256;

/// What the frames mods draw read of the game the player is in, each empty
/// or 0 outside a game and on a build without its read.
#[derive(Clone, Copy, Debug)]
pub(super) struct FrameReads {
    pub(super) hud_screen: Str,
    pub(super) prompt_title: Str,
    pub(super) prompt_verb: u32,
    pub(super) screen_host: enshwick::Entity,
    /// The host's template, all zero before a step read it.
    pub(super) screen_host_template: [u8; 16],
    /// The host's fixed-point position, from the same step.
    pub(super) screen_host_position: [i64; 3],
    /// The HUD screen byte, `None` outside a game.
    screen: Option<u8>,
}

impl FrameReads {
    pub(super) const NONE: Self = Self {
        screen: None,
        hud_screen: Str::EMPTY,
        prompt_title: Str::EMPTY,
        prompt_verb: 0,
        screen_host: enshwick::Entity(0),
        screen_host_template: [0; 16],
        screen_host_position: [0; 3],
    };

    /// The in-game frame `token` with these reads.
    pub(super) fn frame(self, token: u64) -> UiFrame {
        UiFrame {
            header: Header::of::<UiFrame>(),
            token,
            screen: UiFrame::IN_GAME,
            reserved: 0,
            hud_screen: self.hud_screen,
            prompt_title: self.prompt_title,
            prompt_verb: self.prompt_verb,
            screen_host: self.screen_host,
            screen_host_template: self.screen_host_template,
            screen_host_position: self.screen_host_position,
        }
    }
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
    /// The frame's copy of the prompt's title, which the frame's
    /// [`FrameReads::prompt_title`] views until the next frame reads it.
    static TITLE: RefCell<[u8; TITLE_MAX]> = const { RefCell::new([0; TITLE_MAX]) };
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
    hosts: &[[u8; 16]],
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
    if crate::controls::open_screens(loader)
        .is_some_and(|open| crate::controls::is_open_screen(open, screen))
    {
        return Status::BAD_ARGS;
    }
    EXTENSIONS
        .add(Extension {
            registered: Registered::new(owner, callback, user),
            screen,
            edge,
            hosts: hosts.into(),
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
            prompt: prompt_at(loader),
            screen_host: loader.offset(pinned::ui::SCREEN_HOST),
            open_screens: crate::controls::open_screens(loader).unwrap_or(0),
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

/// Where the play data keeps the interaction prompt, or `None` with a log
/// line when the table lacks an entry of it.
fn prompt_at(loader: &Loader) -> Option<PromptAt> {
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let found = (|| {
        Ok::<_, &str>(PromptAt {
            title: table(pinned::ui::PROMPT_TITLE)?,
            verb: table(pinned::ui::PROMPT_VERB)?,
            valid: table(pinned::ui::PROMPT_VALID)?,
        })
    })();
    found
        .inspect_err(|id| {
            crate::log!(
                INFO,
                "interface: mods read no prompt on this build: it lacks {id}"
            );
        })
        .ok()
}

/// Runs `read` only for the in-game app state: a menu or loading frame may
/// still point at the last game's play data.
fn in_game_only<T>(app_state: u8, read: impl FnOnce() -> Option<T>) -> Option<T> {
    (super::screen(app_state) == UiFrame::IN_GAME)
        .then(read)
        .flatten()
}

/// The play data of the game the player is in; `None` outside one.
///
/// # Safety
///
/// - `context` is 0 or the live interface context of the frame being
///   drawn, on the interface thread.
unsafe fn play(engine: &Engine, context: usize, linked: &Linked) -> Option<usize> {
    if context == 0 {
        return None;
    }
    in_game_only(engine.app_state(context), || {
        // SAFETY: the caller passes the frame's live interface context on
        // the interface thread, non-zero by the check before it; the offsets
        // are the table's `map.context_state` and `map.state_play_data`.
        let data =
            unsafe { super::play_data(context, linked.context_state, linked.state_play_data) };
        (data != 0).then_some(data)
    })
}

/// The prompt's title and verb in the play data `data`; empty and 0 while
/// the prompt offers nothing. The title is copied into [`TITLE`] and cut at
/// its last whole character within [`TITLE_MAX`] bytes.
///
/// # Safety
///
/// - `data` is the non-null play data of the frame being drawn, on the
///   interface thread, with the prompt at `at`.
unsafe fn prompt(data: usize, at: PromptAt) -> (Str, u32) {
    // SAFETY: the caller's contract: `data` is the live play data, whose
    // prompt valid byte sits at `ui.prompt_valid` (every kept client).
    if unsafe { crate::memory::read::<u8>(data, at.valid) } == 0 {
        return (Str::EMPTY, 0);
    }
    // SAFETY: the caller's contract; the verb's u32 sits at `ui.prompt_verb`.
    let verb = unsafe { crate::memory::read::<u32>(data, at.verb) };
    // SAFETY: the caller's contract; the title's `UiText` starts with its
    // pointer word at `ui.prompt_title`.
    let text = unsafe { crate::memory::read::<usize>(data, at.title) };
    // SAFETY: the caller's contract; the length word follows the pointer.
    let len = unsafe { crate::memory::read::<usize>(data, at.title + size_of::<usize>()) };
    if text == 0 || len == 0 {
        return (Str::EMPTY, verb);
    }
    let title = TITLE.with_borrow_mut(|buffer| {
        let copied = &mut buffer[..len.min(TITLE_MAX)];
        // The kernel's copy: a route that reads wrong on a new build gives
        // no title rather than a fault.
        if !crate::pages::copy_into(text, copied) {
            return Str::EMPTY;
        }
        let whole = match core::str::from_utf8(copied) {
            Ok(whole) => whole,
            // SAFETY: `valid_up_to` bytes of `copied` are UTF-8.
            Err(cut) => unsafe { core::str::from_utf8_unchecked(&copied[..cut.valid_up_to()]) },
        };
        // The view outlives this borrow: `TITLE` is this thread's and only
        // the next frame's read writes it again.
        Str::new(whole)
    });
    (title, verb)
}

/// What the frames mods draw read of the game the player is in: the open
/// screen, the interaction prompt and the open screen's host.
///
/// # Safety
///
/// - `context` is 0 or the live interface context of the frame being
///   drawn, on the interface thread.
pub(super) unsafe fn frame_reads(engine: &Engine, context: usize, hosts_asked: bool) -> FrameReads {
    let Some(linked) = LINKED.get() else {
        return FrameReads::NONE;
    };
    // SAFETY: the caller's contract.
    let Some(data) = (unsafe { play(engine, context, linked) }) else {
        return FrameReads::NONE;
    };
    // SAFETY: `data` is the non-null play data of this frame, on the
    // interface thread, whose HUD screen byte sits at `ui.hud_screen`.
    let screen = unsafe { crate::memory::read::<u8>(data, linked.hud_screen) };
    // SAFETY: `data` is the live play data of this frame, on the interface
    // thread, with the prompt at the table's `ui.prompt_*`.
    let (prompt_title, prompt_verb) = linked
        .prompt
        .map_or((Str::EMPTY, 0), |at| unsafe { prompt(data, at) });
    let host = open_host(screen, linked.open_screens, || {
        // SAFETY: `data` is the live play data of this frame, whose screen
        // host `u32` sits at the table's `ui.screen_host` (every kept client).
        linked
            .screen_host
            .map_or(0, |at| unsafe { crate::memory::read::<u32>(data, at) })
    });
    // Each step walks the host only for a mod that draws on frames.
    let (template, position) = if hosts_asked {
        crate::local_held::screen_host(host).unwrap_or_default()
    } else {
        ([0; 16], [0; 3])
    };
    FrameReads {
        screen: Some(screen),
        hud_screen: screen_name(linked, Some(screen)),
        prompt_title,
        prompt_verb,
        screen_host: enshwick::Entity(host),
        screen_host_template: template,
        screen_host_position: position,
    }
}

/// The open screen's host as `read` gives it, or 0 on a screen with no
/// window open (a bit of `open_screens`), whatever the play data still
/// holds of a screen that closed.
fn open_host(screen: u8, open_screens: u64, read: impl FnOnce() -> u32) -> u32 {
    if crate::controls::is_open_screen(open_screens, screen) {
        0
    } else {
        read()
    }
}

/// Whether a panel or frame callback of a mod could read the host, so the
/// steps walk it.
pub(super) fn hosts_asked(frames: bool) -> bool {
    frames || EXTENSIONS.sealed().is_some_and(|e| !e.is_empty())
}

/// The game's name of screen byte `screen`; empty for `None`.
fn screen_name(linked: &Linked, screen: Option<u8>) -> Str {
    screen
        .and_then(|s| linked.names.get(usize::from(s)))
        .map_or(Str::EMPTY, |name| Str::new(name))
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
/// - `reads` are [`frame_reads`] of this frame.
pub(super) unsafe fn draw(engine: &Engine, context: usize, reads: FrameReads) {
    let Some(extensions) = EXTENSIONS.sealed() else {
        return;
    };
    let Some(screen) = reads.screen.filter(|_| !extensions.is_empty()) else {
        return;
    };
    for extension in by_edge(extensions, screen, |e| (e.screen, e.edge)) {
        if extension.registered.live() && extension.shows_for(reads.screen_host_template) {
            // SAFETY: this function's `# Safety` gives the frame, its thread
            // and its `context`; the only window open is the extensions' own,
            // which `open` and `close` keep one at a time.
            unsafe { run(extension.registered, context, extension.edge, reads) };
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
unsafe fn run(registered: Registered<UiFrameFn>, context: usize, edge: u32, reads: FrameReads) {
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    DRAWING.set(Some(Drawing {
        token,
        owner: registered.owner,
        context,
        window_open: false,
        hud: false,
        extension: Some(edge),
    }));
    let frame = reads.frame(token);
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
            let read: Option<usize> =
                in_game_only(menu_or_loading, || panic!("read the last game's play data"));
            assert_eq!(read, None, "app state {menu_or_loading}");
        }
        assert_eq!(in_game_only(7, || Some(12)), Some(12));
        assert_eq!(in_game_only::<usize>(7, || None), None, "no play data");
    }

    #[test]
    fn the_world_with_no_screen_open_has_no_host_whatever_the_play_data_holds() {
        let (in_game, no_ui) = (3, 40);
        let open = (1 << in_game) | (1 << no_ui);
        assert_eq!(open_host(in_game, open, || 77), 0);
        assert_eq!(open_host(no_ui, open, || 77), 0, "a build tool");
        assert_eq!(open_host(9, open, || 77), 77);
        assert_eq!(open_host(in_game, 0, || 77), 77);
        assert_eq!(open_host(200, open, || 77), 77);
    }

    #[test]
    fn a_panel_with_hosts_shows_only_for_them_and_one_without_for_every_host() {
        unsafe extern "system" fn nothing(_: *const UiFrame, _: *mut c_void) {}
        let panel = |hosts: &[[u8; 16]]| Extension {
            registered: Registered::new(ModIndex::new(0), nothing, core::ptr::null_mut()),
            screen: 0,
            edge: UiExtension::RIGHT,
            hosts: hosts.into(),
        };
        assert!(panel(&[[7; 16]]).shows_for([7; 16]));
        assert!(!panel(&[[7; 16]]).shows_for([8; 16]));
        assert!(
            !panel(&[[7; 16]]).shows_for([0; 16]),
            "a host no step read yet"
        );
        assert!(panel(&[]).shows_for([0; 16]));
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
