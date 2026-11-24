//! Panels mods add to the game's own screens. The game's interface keeps no
//! window to append to and a game window has no position, so each place a
//! panel asks for (an edge, where along it and an offset) gets one
//! full-screen window of its own on a layer above the game's screens,
//! shared by every panel at that place, drawn while the player has its
//! screen open and only once a panel draws a widget there.

use core::cell::{Cell, RefCell};
use core::ffi::c_void;
use std::sync::OnceLock;

use enshwick_core::abi::{Header, Side, Str, UiFrame, UiFrameFn};
use enshwick_core::ui::HudScreenType;
use enshwick_gamedata::catalogue;

use crate::actions::client_snapshot::TabRead;
use crate::engine::builds::ABSENT;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Registered};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::registry::{JoinFeatureError, Registry};
use crate::ui::place::Place;

use super::engine::{Engine, UiText, Window};
use super::{DRAWING, Drawing, copy, next_token};

/// The open screen's name in what mods' frames read.
static SCREEN_NAMES_FUSE: Fuse = Fuse::new("open screen names");
/// The interaction prompt in what mods' frames read.
static PROMPT_FUSE: Fuse = Fuse::new("interaction prompt reads");
/// Why a reflected type the frames read fails on a build without it.
const NOT_REFLECTED: &str = "is not reflected on this build";

/// The start of each place's window name, which its index ends, outside the
/// `enshwick/<mod id>/` names of mods' own windows.
const NAME_PREFIX: &str = "enshwick:screen/";

/// A panel's layer beside a screen that draws on a window of its own, as the
/// chest screens on 6 (client 520): above it, under the button hints (0xa)
/// and the split stack menu (0xb).
const SCREEN_LAYER: u32 = 8;
/// A panel's layer beside a screen the in-game menu draws on its layer 9
/// (`window_begin` at `0x140be8960`, client 26430625): above the menu, under
/// the split stack menu (0xb). It shares 0xa with the button hints.
const MENU_LAYER: u32 = 0xa;
/// A panel's layer beside the Skill Tree, whose window draws on 0xb (client
/// 520): above it, under the tooltips (0xe) and the dragged item (0xf).
const SKILL_TREE_LAYER: u32 = 0xc;

/// The layer a panel draws on beside the screen the game names `screen`.
fn layer(screen: &str) -> u32 {
    if screen == HudScreenType::SkillTree.name() {
        SKILL_TREE_LAYER
    } else if crate::actions::client_control::MENU_SCREENS.contains(&screen) {
        MENU_LAYER
    } else {
        SCREEN_LAYER
    }
}

/// A mod's panel on one of the game's screens.
#[derive(Clone, Debug)]
struct Extension {
    registered: Registered<UiFrameFn>,
    /// The screen's value among the game's `HudScreenType`.
    screen: u8,
    place: Place,
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
    /// Where the play data keeps the interaction prompt, on a build whose
    /// table has it.
    prompt: Option<PromptAt>,
    /// Where the play data keeps the open screen's `u32` host entity, on a
    /// build whose table has it.
    screen_host: Option<usize>,
    /// Where the play data keeps the screen a stacked screen returns to, on
    /// a build whose table has it.
    return_screen: Option<usize>,
    /// A bit for each screen byte with no window open, as
    /// [`crate::actions::controls::open_screens`] reads them; 0 on a build without.
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
    pub(super) screen_host: enshwick_core::Entity,
    /// The host's template, all zero before a step read it.
    pub(super) screen_host_template: [u8; 16],
    /// The host's fixed-point position, from the same step.
    pub(super) screen_host_position: [i64; 3],
    /// The screens the player sees, as [`Tab::seen`] gives them: the shown
    /// tab's on the in-game menu, and the menu's own where it opened as a
    /// whole; both `None` outside a game.
    ///
    /// [`Tab::seen`]: crate::actions::client_control::Tab::seen
    pub(super) seen: [Option<u8>; 2],
}

impl FrameReads {
    pub(super) const NONE: Self = Self {
        seen: [None; 2],
        hud_screen: Str::EMPTY,
        prompt_title: Str::EMPTY,
        prompt_verb: 0,
        screen_host: enshwick_core::Entity(0),
        screen_host_template: [0; 16],
        screen_host_position: [0; 3],
    };

    /// The in-game frame `token` with these reads and the slot of the game's
    /// screen under the cursor in the last whole frame; on the interface
    /// thread.
    pub(super) fn frame(self, token: u64) -> UiFrame {
        let hovered = super::item_slots::hovered();
        UiFrame {
            header: Header::of::<UiFrame>(),
            token,
            screen: UiFrame::IN_GAME,
            reserved: 0,
            hud_screen: self.hud_screen,
            prompt_title: self.prompt_title,
            prompt_verb: self.prompt_verb,
            screen_host: self.screen_host,
            screen_host_template: enshwick_core::content::Guid(self.screen_host_template),
            screen_host_position: enshwick_core::ecs::metres(self.screen_host_position),
            hovered_slot: hovered.slot,
            hovered_item: enshwick_core::ItemId::new(hovered.item),
            hovered_count: hovered.count,
            hovered_pide: enshwick_core::Entity(hovered.pide),
            reserved_hovered: 0,
        }
    }
}

/// The extensions: open while mods initialise, sealed after.
static EXTENSIONS: Registry<Extension> = Registry::new();
static LINKED: OnceLock<Linked> = OnceLock::new();

/// A place's window, its name, which the game hashes into the window's node
/// ids, and the extensions drawn there, as indices of the sealed list in
/// registration order.
struct Placed {
    window: Window,
    name: Box<[u8]>,
    panels: Box<[usize]>,
    /// The screen the window last drew beside: one window serves a place on
    /// every screen.
    shown: Option<u8>,
}

impl Placed {
    /// Moves the window beside screen `shown` on `layer`, holding its focus
    /// back when the screen changed, as when it opens.
    const fn beside(&mut self, shown: u8, layer: u32) {
        self.window.set_layer(layer);
        if !matches!(self.shown, Some(was) if was == shown) {
            self.shown = Some(shown);
            self.window.hold_focus();
        }
    }
}

/// One place's window among the extensions' windows, in drawing order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PanelWindow(usize);

thread_local! {
    /// One window per place, built at the first draw after the extensions
    /// seal and kept in place across frames: the list never grows after.
    static WINDOWS: RefCell<Vec<Placed>> = const { RefCell::new(Vec::new()) };
    /// The place whose window is open in this frame.
    static OPEN: Cell<Option<PanelWindow>> = const { Cell::new(None) };
    /// The frame's copy of the prompt's title, which the frame's
    /// [`FrameReads::prompt_title`] views until the next frame reads it.
    static TITLE: RefCell<[u8; TITLE_MAX]> = const { RefCell::new([0; TITLE_MAX]) };
}

/// Why a mod's screen extension was not registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ExtendScreenError {
    /// The registration itself was refused: after init.
    #[error(transparent)]
    Register(#[from] JoinFeatureError),
    /// Only a client draws screens.
    #[error("screens are extended on a client")]
    NotClient,
    /// No screen of the game has the name on this build.
    #[error("no screen has the name")]
    UnknownScreen,
    /// The edge, the alignment or an offset is none the loader takes.
    #[error("no such edge, alignment or offset")]
    BadPlace,
    /// On the screen the player is in the world with no window open.
    #[error("a screen of play")]
    ScreenOfPlay,
}

/// Registers mod `owner`'s panel at `place` beside the game's screen named
/// `screen`, `None` for a place [`Place::of`] refused; client only, during
/// init. A screen on which the player is in the world with no window open
/// is refused: a full-screen window there would cover play while the mods'
/// keys still fire.
///
/// # Errors
///
/// [`ExtendScreenError`] off a client, after init, for an unknown screen,
/// a refused place or a screen of play.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    screen: &str,
    place: Option<Place>,
    hosts: &[[u8; 16]],
    callback: UiFrameFn,
    user: *mut c_void,
) -> Result<(), ExtendScreenError> {
    if loader.side != Side::CLIENT {
        return Err(ExtendScreenError::NotClient);
    }
    let place = place.ok_or(ExtendScreenError::BadPlace)?;
    if crate::actions::controls::OPEN_SCREENS.contains(&screen) {
        return Err(ExtendScreenError::ScreenOfPlay);
    }
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let screen = loader
        .reflection
        .enum_value(screens, screen)
        .ok()
        .and_then(|value| u8::try_from(value).ok())
        .ok_or(ExtendScreenError::UnknownScreen)?;
    if crate::actions::controls::open_screens(loader)
        .is_some_and(|open| crate::actions::controls::is_open_screen(open, screen))
    {
        return Err(ExtendScreenError::ScreenOfPlay);
    }
    EXTENSIONS
        .add(Extension {
            registered: Registered::new(owner, callback, user),
            screen,
            place,
            hosts: hosts.into(),
        })
        .map_err(|sealed| ExtendScreenError::Register(sealed.into()))
}

/// Seals the extensions, so a registration after init is too late on every
/// side.
pub(super) fn seal() {
    let _sealed = EXTENSIONS.seal();
}

/// Turns a feature of what mods' frames read off: the feature, the entry
/// the build lacks and why, as [`Loader::fuse_off`] takes them.
type OffFn<'a> = &'a dyn Fn(&'static Fuse, &str, &str);

/// Reads where the screen, the prompt and the hosts sit. What this build
/// lacks is listed off when mods' frames, HUD items (`frames`), the
/// extensions sealed before or, on a client, the client snapshot and
/// commands read it, else only logged.
pub(super) fn link(loader: &Loader, frames: bool) {
    let extensions = EXTENSIONS.sealed().unwrap_or_default();
    // Any mod may ask a client's snapshot or command after init.
    let used = frames || !extensions.is_empty() || loader.side == Side::CLIENT;
    let off = |fuse: &'static Fuse, id: &str, reason: &str| {
        if used {
            loader.fuse_off(fuse, id, reason);
        } else {
            let reason = loader.build.reason(id, reason);
            crate::log!(
                INFO,
                "interface: no mod reads {}, which this build lacks: {id} {reason}",
                fuse.feature()
            );
        }
    };
    match linked(loader, &off) {
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
        Err(id) => screen_lacked(id, &off),
    }
}

/// Where the screen, the prompt and the hosts sit on this build, or the
/// first table entry the screen reads lack.
fn linked(loader: &Loader, off: OffFn<'_>) -> Result<Linked, &'static str> {
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let context_state = table(pinned::map::CONTEXT_STATE)?;
    let state_play_data = table(pinned::map::STATE_PLAY_DATA)?;
    let hud_screen = table(pinned::ui::HUD_SCREEN)?;
    // After the reads' own entries: a build without them lists one cause.
    link_names(loader, off);
    Ok(Linked {
        context_state,
        state_play_data,
        hud_screen,
        prompt: prompt_at(loader, off),
        screen_host: loader.offset(pinned::ui::SCREEN_HOST),
        return_screen: loader.offset(pinned::ui::RETURN_SCREEN),
        open_screens: crate::actions::controls::open_screens(loader).unwrap_or(0),
    })
}

/// Turns the screen reads off, and the screen extensions placed by them,
/// for the table entry `id` this build lacks.
fn screen_lacked(id: &str, off: OffFn<'_>) {
    off(&super::READS_FUSE, id, ABSENT);
    off(&super::EXTENSIONS_FUSE, id, ABSENT);
}

/// Links the screens' names, listing them off on a build that does not
/// reflect them.
fn link_names(loader: &Loader, off: OffFn<'_>) {
    if let Err(screens) = crate::engine::screens::link(loader) {
        off(&SCREEN_NAMES_FUSE, screens, NOT_REFLECTED);
    }
}

/// Where the play data keeps the interaction prompt, or `None` with its
/// feature off when the table lacks an entry of it.
fn prompt_at(loader: &Loader, off: OffFn<'_>) -> Option<PromptAt> {
    let table = |id: pinned::Constant| loader.offset(id).ok_or(id.id());
    let found = (|| {
        Ok::<_, &str>(PromptAt {
            title: table(pinned::ui::PROMPT_TITLE)?,
            verb: table(pinned::ui::PROMPT_VERB)?,
            valid: table(pinned::ui::PROMPT_VALID)?,
        })
    })();
    found.inspect_err(|id| off(&PROMPT_FUSE, id, ABSENT)).ok()
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
        let data = unsafe {
            crate::engine::screens::play_data(context, linked.context_state, linked.state_play_data)
        };
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
    if unsafe { crate::engine::memory::read::<u8>(data, at.valid) } == 0 {
        return (Str::EMPTY, 0);
    }
    // SAFETY: the caller's contract; the verb's u32 sits at `ui.prompt_verb`.
    let verb = unsafe { crate::engine::memory::read::<u32>(data, at.verb) };
    // SAFETY: the caller's contract; the title's `UiText` starts with its
    // pointer word at `ui.prompt_title`.
    let text = unsafe { crate::engine::memory::read::<usize>(data, at.title) };
    // SAFETY: the caller's contract; the length word follows the pointer.
    let len = unsafe { crate::engine::memory::read::<usize>(data, at.title + size_of::<usize>()) };
    if text == 0 || len == 0 {
        return (Str::EMPTY, verb);
    }
    let title = TITLE.with_borrow_mut(|buffer| {
        let copied = &mut buffer[..len.min(TITLE_MAX)];
        // The kernel's copy: a route that reads wrong on a new build gives
        // no title rather than a fault.
        if !crate::engine::pages::copy_into(text, copied) {
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
    let Some(view) = (unsafe { screen_view(engine, context) }) else {
        return FrameReads::NONE;
    };
    let (screen, host) = (view.screen, view.host);
    // SAFETY: `view.play` is the live play data of this frame, on the
    // interface thread, with the prompt at the table's `ui.prompt_*`.
    let (prompt_title, prompt_verb) = linked
        .prompt
        .map_or((Str::EMPTY, 0), |at| unsafe { prompt(view.play, at) });
    // Each step walks the host only for a mod that draws on frames.
    let (template, position) = if hosts_asked {
        crate::actions::local_held::screen_host(host).unwrap_or_default()
    } else {
        ([0; 16], [0; 3])
    };
    // SAFETY: `view` is this frame's, read on the interface thread by the
    // caller's contract.
    let seen = unsafe { seen(&view) };
    FrameReads {
        seen,
        hud_screen: screen_name(Some(seen[0].unwrap_or(screen))),
        prompt_title,
        prompt_verb,
        screen_host: enshwick_core::Entity(host),
        screen_host_template: template,
        screen_host_position: position,
    }
}

/// The open screen's host as `read` gives it, or 0 on a screen with no
/// window open (a bit of `open_screens`), whatever the play data still
/// holds of a screen that closed.
fn open_host(screen: u8, open_screens: u64, read: impl FnOnce() -> u32) -> u32 {
    if crate::actions::controls::is_open_screen(open_screens, screen) {
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
fn screen_name(screen: Option<u8>) -> Str {
    screen.map_or(Str::EMPTY, |s| Str::new(screen_named(s)))
}

/// The game's name of screen byte `screen` on this build: empty for a byte
/// no constant names, and before the screen reads link.
pub(crate) fn screen_named(screen: u8) -> &'static str {
    crate::engine::screens::name(screen)
}

/// One frame's open screens, read in the game the player is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ScreenView {
    /// The play data's address, inside the client's game object, so a new
    /// object moves it.
    pub(crate) play: usize,
    /// The HUD screen byte.
    pub(crate) screen: u8,
    /// The screen byte a stacked screen returns to, 0 for the world and on
    /// a build without the read.
    pub(crate) return_to: u8,
    /// The open screen's host entity, 0 for none.
    pub(crate) host: u32,
    /// The interface state the in-game menu draws from.
    pub(crate) state: usize,
}

/// This frame's open screens; `None` outside a game and before the screen
/// reads link.
///
/// # Safety
///
/// - `context` is 0 or the live interface context of the frame being
///   drawn, on the interface thread.
pub(super) unsafe fn screen_view(engine: &Engine, context: usize) -> Option<ScreenView> {
    let linked = LINKED.get()?;
    // SAFETY: the caller's contract.
    let data = unsafe { play(engine, context, linked) }?;
    // SAFETY: `data` is the non-null play data of this frame, on the
    // interface thread, whose HUD screen byte sits at `ui.hud_screen`.
    let screen = unsafe { crate::engine::memory::read::<u8>(data, linked.hud_screen) };
    // SAFETY: the same play data, whose return-to byte sits at the table's
    // `ui.return_screen` (every kept client).
    let return_to = linked.return_screen.map_or(0, |at| unsafe {
        crate::engine::memory::read::<u8>(data, at)
    });
    let host = open_host(screen, linked.open_screens, || {
        // SAFETY: `data` is the live play data of this frame, whose screen
        // host `u32` sits at the table's `ui.screen_host` (every kept client).
        linked.screen_host.map_or(0, |at| unsafe {
            crate::engine::memory::read::<u32>(data, at)
        })
    });
    // SAFETY: `play` above read the context's state pointer at
    // `map.context_state` of this live, non-zero context.
    let state = unsafe { crate::engine::memory::read::<usize>(context, linked.context_state) };
    Some(ScreenView {
        play: data,
        screen,
        return_to,
        host,
        state,
    })
}

/// What `view`'s frame reads of the in-game menu's tab; [`TabRead::Unread`]
/// on a build without the tab read.
///
/// # Safety
///
/// - `view` is this frame's, read on the interface thread.
pub(super) unsafe fn tab(view: &ScreenView) -> TabRead {
    crate::actions::client_control::GROUPS
        .tab
        .get()
        .map_or(TabRead::Unread, |tab| {
            // SAFETY: `view.state` is this frame's interface state, the one
            // the in-game menu draws from, read on the interface thread; its
            // tab index sits at the routed `ui.menu_tab`.
            unsafe { tab.shown(core::ptr::with_exposed_provenance(view.state), view.screen) }
        })
}

/// The screens the player sees in `view`'s frame, as [`Tab::seen`] gives
/// them; the HUD screen alone on a build without the tab read.
///
/// [`Tab::seen`]: crate::actions::client_control::Tab::seen
///
/// # Safety
///
/// - `view` is this frame's, read on the interface thread.
unsafe fn seen(view: &ScreenView) -> [Option<u8>; 2] {
    crate::actions::client_control::GROUPS
        .tab
        .get()
        .map_or([Some(view.screen), None], |tab| {
            // SAFETY: as in `tab`: this frame's interface state, read on the
            // interface thread, its tab index at the routed `ui.menu_tab`.
            unsafe { tab.seen(core::ptr::with_exposed_provenance(view.state), view.screen) }
        })
}

/// The places of `panels`, each once, in the order their windows draw: by
/// [`Place::rank`], then by the first panel registered there.
fn places<T>(panels: &[T], place: impl Fn(&T) -> Place) -> Vec<Place> {
    let mut places: Vec<Place> = Vec::new();
    for panel in panels {
        let at = place(panel);
        if !places.contains(&at) {
            places.push(at);
        }
    }
    places.sort_by_key(|at| at.rank());
    places
}

/// The indices of the panels at `place`, in registration order.
fn at_place<T>(panels: &[T], place: Place, of: impl Fn(&T) -> Place) -> Box<[usize]> {
    (0..panels.len())
        .filter(|&index| of(&panels[index]) == place)
        .collect()
}

/// A window for each place of `extensions`, in drawing order, on `layer`,
/// with the extensions drawn there.
fn placed(extensions: &[Extension], layer: u32) -> Vec<Placed> {
    places(extensions, |e| e.place)
        .into_iter()
        .enumerate()
        .map(|(index, place)| Placed {
            window: Window::extension(place, layer),
            name: format!("{NAME_PREFIX}{index}").into_bytes().into(),
            panels: at_place(extensions, place, |e| e.place),
            shown: None,
        })
        .collect()
}

/// The extension at `panel` of the place's window `window`; `None` past its
/// last.
fn panel_at(window: PanelWindow, panel: usize) -> Option<usize> {
    WINDOWS.with_borrow(|windows| windows.get(window.0)?.panels.get(panel).copied())
}

/// Draws the panels of the screen the player has open, each place's in one
/// window, so each window opens at most once a frame.
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
    let Some(shown) = reads.seen[0]
        .or(reads.seen[1])
        .filter(|_| !extensions.is_empty())
    else {
        return;
    };
    let on = layer(screen_named(shown));
    let count = WINDOWS.with_borrow_mut(|windows| {
        if windows.is_empty() {
            *windows = placed(extensions, on);
        }
        for placed in windows.iter_mut() {
            placed.beside(shown, on);
        }
        windows.len()
    });
    for window in (0..count).map(PanelWindow) {
        // A panel's callback opens its window through `WINDOWS`, so each
        // panel is read apart from the panels' runs.
        let panels = (0..).map_while(|panel| panel_at(window, panel));
        for extension in panels.filter_map(|at| extensions.get(at)) {
            if reads.seen.contains(&Some(extension.screen))
                && extension.registered.live()
                && extension.shows_for(reads.screen_host_template)
            {
                // SAFETY: this function's `# Safety` gives the frame, its
                // thread and its `context`; the only window open is the
                // extensions' own, which `open` and `close` keep one at a
                // time.
                unsafe { run(extension.registered, context, window, reads) };
            }
        }
    }
    // SAFETY: this function's `# Safety` gives the frame, its thread and its
    // `context`, in which `open` began any window still open.
    unsafe { close(engine, context) };
}

/// Opens the place's window `window` for a panel's first widget that needs
/// one, ending another place's window first; an already open one stays.
/// False when the extensions have no such window, so none is open.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread,
///   with that frame's non-zero `context`, from a panel's callback that
///   [`draw`] runs, so no window but the extensions' own is open.
pub(super) unsafe fn open(engine: &Engine, context: usize, window: PanelWindow) -> bool {
    if OPEN.get() == Some(window) {
        return true;
    }
    // SAFETY: the caller's frame, thread and `context`.
    unsafe { close(engine, context) };
    let opened = WINDOWS.with_borrow_mut(|windows| {
        let placed = windows.get_mut(window.0)?;
        let name = copy(&[&placed.name]);
        // SAFETY: the caller's frame, thread and `context`; no other window
        // is open (`close` ended the last one), the window stays in the
        // thread-local `WINDOWS`, which never grows after `draw` built it,
        // until `close` ends it, and `name` lives in the frame's text arena.
        unsafe { engine.window_begin(context, &mut placed.window, &name, &UiText::EMPTY) };
        Some(())
    });
    if opened.is_some() {
        OPEN.set(Some(window));
    }
    opened.is_some()
}

/// Ends the open place's window, if any.
///
/// # Safety
///
/// - Called inside the frame of the game's interface update in which
///   [`open`] began the window, on its thread, with that frame's `context`.
unsafe fn close(engine: &Engine, context: usize) {
    let Some(window) = OPEN.take() else {
        return;
    };
    WINDOWS.with_borrow_mut(|windows| {
        if let Some(placed) = windows.get_mut(window.0) {
            // SAFETY: `open` began the window in this frame, on this thread,
            // with this `context`; it did not move, and the painter's widgets
            // close every frame they open.
            unsafe { engine.window_end(context, &mut placed.window) };
        }
    });
}

/// Runs an extension's callback; its first widget that needs a window
/// opens the place's window `window` through [`open`].
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread,
///   with that frame's non-zero `context`, while no window but the
///   extensions' own is open.
unsafe fn run(
    registered: Registered<UiFrameFn>,
    context: usize,
    window: PanelWindow,
    reads: FrameReads,
) {
    let token = next_token();
    DRAWING.set(Some(Drawing {
        token,
        owner: registered.owner,
        context,
        window_open: false,
        hud: false,
        extension: Some(window),
    }));
    let frame = reads.frame(token);
    // SAFETY: the callback lives in a mod DLL that stays loaded, and it gets
    // a live `UiFrame` and the pointer its mod registered with it; an
    // `extern "system"` function aborts rather than unwinds.
    crate::engine::profile::call(
        registered.owner,
        crate::engine::profile::Feature::Interface,
        || unsafe { (registered.callback)(&raw const frame, registered.user()) },
    );
    if let Some(engine) = super::ENGINE.get() {
        // SAFETY: this function's `# Safety` gives the frame and its thread;
        // the edge's window the callback's slots drew in stays open until
        // `draw` closes it after every panel ran.
        unsafe { super::painter::end_slots(engine) };
    }
    DRAWING.set(None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_core::abi::UiExtension;

    /// What `off` was asked to turn off: the feature, entry and reason.
    type Asked = RefCell<Vec<(&'static str, String, String)>>;

    fn recorder(asked: &Asked) -> impl Fn(&'static Fuse, &str, &str) + '_ {
        |fuse, id, reason| {
            asked
                .borrow_mut()
                .push((fuse.feature(), id.to_owned(), reason.to_owned()));
        }
    }

    #[test]
    fn a_place_s_window_holds_its_focus_again_when_another_screen_shows() {
        let mut placed = Placed {
            window: Window::extension(Place::edge(UiExtension::RIGHT), 0xa),
            name: Box::default(),
            panels: Box::default(),
            shown: Some(3),
        };
        placed.beside(3, 0xa);
        assert!(
            !placed.window.held(),
            "the same screen keeps the focus as it is"
        );
        placed.beside(4, 0xa);
        assert!(placed.window.held(), "the next screen's panel opens held");
        assert_eq!(placed.shown, Some(4));
    }

    #[test]
    fn a_panel_draws_just_above_its_screen_and_under_that_screen_s_popups() {
        let on = |screen: HudScreenType| layer(screen.name());
        // The in-game menu draws on 9 and the split stack menu on 0xb.
        assert!((10..0xb).contains(&on(HudScreenType::Backpack)));
        for menu in [
            HudScreenType::InGameMenu,
            HudScreenType::Map,
            HudScreenType::Social,
        ] {
            assert_eq!(on(menu), on(HudScreenType::Backpack), "{menu:?}");
        }
        // The Skill Tree's own window draws on 0xb, the tooltips on 0xe.
        assert!((0xc..0xe).contains(&on(HudScreenType::SkillTree)));
        // A chest screen draws on 6, its button hints on 0xa.
        assert!((7..0xa).contains(&on(HudScreenType::StorageChest)));
        assert_eq!(layer(""), on(HudScreenType::StorageChest));
    }

    #[test]
    fn a_build_without_the_screens_reflected_names_none_and_lists_the_names_off() {
        let asked = Asked::default();
        link_names(&crate::start::tests::bare_loader(), &recorder(&asked));
        assert_eq!(screen_named(1), "");
        let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
        assert_eq!(
            asked.into_inner(),
            [(
                SCREEN_NAMES_FUSE.feature(),
                screens.to_owned(),
                NOT_REFLECTED.to_owned()
            )]
        );
    }

    #[test]
    fn a_build_without_the_prompt_reads_none_and_lists_the_prompt_off() {
        let asked = Asked::default();
        assert!(prompt_at(&crate::start::tests::bare_loader(), &recorder(&asked)).is_none());
        assert_eq!(
            asked.into_inner(),
            [(
                PROMPT_FUSE.feature(),
                pinned::ui::PROMPT_TITLE.id().to_owned(),
                ABSENT.to_owned()
            )],
            "a missing row passes the build's diagnosis, not a text of its own"
        );
    }

    #[test]
    fn a_build_without_the_screen_reads_turns_them_and_the_extensions_off() {
        let asked = Asked::default();
        let lacked = linked(&crate::start::tests::bare_loader(), &recorder(&asked));
        let Err(id) = lacked else {
            panic!("a bare build has no screen reads");
        };
        screen_lacked(id, &recorder(&asked));
        let features: Vec<_> = asked.borrow().iter().map(|(f, ..)| *f).collect();
        assert_eq!(
            features,
            [
                super::super::READS_FUSE.feature(),
                super::super::EXTENSIONS_FUSE.feature()
            ]
        );
        assert!(asked.borrow().iter().all(|(_, _, reason)| reason == ABSENT));
    }

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
            place: Place::edge(UiExtension::RIGHT),
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

    /// The panels of `seen` in the order [`draw`] runs them: place by place
    /// as [`places`] orders them, each place's by [`at_place`].
    fn drawn(panels: &[(u8, Place)], seen: [Option<u8>; 2]) -> Vec<usize> {
        places(panels, |p| p.1)
            .into_iter()
            .flat_map(|place| at_place(panels, place, |p| p.1))
            .filter(|&panel| seen.contains(&Some(panels[panel].0)))
            .collect()
    }

    #[test]
    fn panels_of_the_open_screen_draw_edge_by_edge_in_registration_order() {
        let [left, right, top] =
            [UiExtension::LEFT, UiExtension::RIGHT, UiExtension::TOP].map(Place::edge);
        let panels = [(5, right), (5, left), (7, left), (5, right), (5, top)];
        assert_eq!(drawn(&panels, [Some(5), None]), [1, 0, 3, 4]);
    }

    #[test]
    fn each_place_on_an_edge_draws_in_a_window_of_its_own_after_the_edges_before_it() {
        let place = |edge, align, shift| {
            Place::of(&UiExtension {
                edge,
                align,
                shift,
                ..bytemuck::Zeroable::zeroed()
            })
            .unwrap()
        };
        let low = place(UiExtension::LEFT, UiExtension::START, 360.0);
        let left = Place::edge(UiExtension::LEFT);
        let right = Place::edge(UiExtension::RIGHT);
        let panels = [(5, right), (5, low), (5, left), (5, low)];
        assert_eq!(
            places(&panels, |p| p.1),
            [low, left, right],
            "edge by edge, then as first registered"
        );
        assert_eq!(drawn(&panels, [Some(5), None]), [1, 3, 2, 0]);
    }

    #[test]
    fn the_in_game_menu_s_panels_draw_beside_the_tab_it_shows_and_a_hidden_tab_s_do_not() {
        let [left, right] = [UiExtension::LEFT, UiExtension::RIGHT].map(Place::edge);
        let (menu, character, backpack) = (4, 7, 9);
        let panels = [(backpack, left), (menu, right), (character, left)];
        assert_eq!(drawn(&panels, [Some(character), Some(menu)]), [2, 1]);
        assert_eq!(drawn(&panels, [Some(character), None]), [2]);
        assert_eq!(drawn(&panels, [None, Some(menu)]), [1]);
        assert!(drawn(&panels, [None, None]).is_empty());
    }
}
