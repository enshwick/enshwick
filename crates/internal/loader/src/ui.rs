//! The client's interface: mods draw windows and widgets with the game's own
//! widget functions, and dialogs show in the game's own style.
//!
//! The game redraws its interface every frame inside `GameUi::update`, on
//! the main thread, and draws its error popup last, inside the frame. The
//! loader links that popup: after the game's own call it runs the mods'
//! frame callbacks, whose widgets reach the game through
//! [`widget`], and then the loader's dialog, unless a popup of the game's
//! shows. Texts a mod hands in are copied into a per-frame arena, kept for a
//! few frames, since the game may read them after the call.

use painter::{copy, read};

use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

mod actions;
mod demo;
mod dialog;
mod engine;
mod map;
mod menu_line;
mod mods;

use core::cell::{Cell, RefCell};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use arc_swap::ArcSwapOption;

use enshwick::abi::{
    HookFlags, Side, Status, Str, UiAnswerFn, UiDialog, UiFrame, UiFrameFn, UiWidget,
};

pub(crate) use actions::configure as configure_actions_key;
pub(crate) use demo::open as open_demo;
pub(crate) use dialog::show;
use engine::{Engine, HudWindow, PopupFn, UiText, Window};
pub(crate) use map::set as set_markers;

use crate::chains::Next;
use crate::fuse::Fuse;
use crate::host::{Loader, Registered, Target};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::map::CONTEXT_STATE.id(),
    pinned::map::ENTRY_COLOR.id(),
    pinned::map::ENTRY_LABEL.id(),
    pinned::map::ENTRY_POSITION.id(),
    pinned::map::ENTRY_SIZE.id(),
    pinned::map::ENTRY_TYPE.id(),
    pinned::map::ENTRY_VISIBILITY.id(),
    pinned::map::LIST.id(),
    pinned::map::LIST_CAPACITY.id(),
    pinned::map::LIST_COUNT.id(),
    pinned::map::REGISTRY.id(),
    pinned::map::REGISTRY_BLOB.id(),
    pinned::map::STATE_PLAY_DATA.id(),
    pinned::ui::ACTION_BOUND.id(),
    pinned::ui::ACTION_EXTENT.id(),
    pinned::ui::ACTION_NEW.id(),
    pinned::ui::ACTION_NODE.id(),
    pinned::ui::ACTION_QUERY.id(),
    pinned::ui::BUTTON.id(),
    pinned::ui::BUTTON_ALIGN.id(),
    pinned::ui::BUTTON_CENTERED.id(),
    pinned::ui::BUTTON_ENABLED.id(),
    pinned::ui::BUTTON_EXTENT.id(),
    pinned::ui::BUTTON_PADDING_X.id(),
    pinned::ui::BUTTON_PADDING_Y.id(),
    pinned::ui::BUTTON_SCALE.id(),
    pinned::ui::BUTTON_SIZE_MODE.id(),
    pinned::ui::BUTTON_SOUND.id(),
    pinned::ui::COLUMN.id(),
    pinned::ui::CONTEXT.id(),
    pinned::ui::CONTEXT_BUNDLE.id(),
    pinned::ui::CONTEXT_HOST.id(),
    pinned::ui::CONTEXT_NAV.id(),
    pinned::ui::CONTEXT_STATE.id(),
    pinned::ui::DROPDOWN.id(),
    pinned::ui::DROPDOWN_CAPACITY.id(),
    pinned::ui::DROPDOWN_COUNT.id(),
    pinned::ui::DROPDOWN_ENABLED.id(),
    pinned::ui::DROPDOWN_ENTRY_TEXT.id(),
    pinned::ui::DROPDOWN_EXTENT.id(),
    pinned::ui::DROPDOWN_INIT.id(),
    pinned::ui::ENTRY_SIZE.id(),
    pinned::ui::ENTRY_TEXT.id(),
    pinned::ui::ENTRY_VALUE.id(),
    pinned::ui::FIRST_USE.id(),
    pinned::ui::FOCUS_REQUEST.id(),
    pinned::ui::FRAME_CLOSE.id(),
    pinned::ui::FRAME_COLOR.id(),
    pinned::ui::FRAME_EXTENT.id(),
    pinned::ui::FRAME_FROM_NODE.id(),
    pinned::ui::FRAME_HIT.id(),
    pinned::ui::FRAME_NODE.id(),
    pinned::ui::FRAME_OPEN.id(),
    pinned::ui::FRAME_STRETCH.id(),
    pinned::ui::HOST_ANSWER.id(),
    pinned::ui::HOST_ANSWERED.id(),
    pinned::ui::HUD.id(),
    pinned::ui::HUD_VISIBLE.id(),
    pinned::ui::LABEL_ALIGN.id(),
    pinned::ui::LABEL_DRAW.id(),
    pinned::ui::LABEL_EXTENT.id(),
    pinned::ui::LABEL_FLAGS.id(),
    pinned::ui::LABEL_FONT_SIZE.id(),
    pinned::ui::LABEL_NEW.id(),
    pinned::ui::LABEL_SHOWN.id(),
    pinned::ui::LEGAL_LINE_SITE.id(),
    pinned::ui::LOCA_ID.id(),
    pinned::ui::LOCA_TEXT.id(),
    pinned::ui::MAIN_ENTRY.id(),
    pinned::ui::MAIN_MENU_SITE.id(),
    pinned::ui::MENU_ENTRY_EXTENT.id(),
    pinned::ui::MENU_ENTRY_FONT.id(),
    pinned::ui::MENU_ENTRY_TEXT.id(),
    pinned::ui::NAV_AXIS.id(),
    pinned::ui::NAV_CONFIG.id(),
    pinned::ui::NAV_CONFIG_SIZE.id(),
    pinned::ui::NAV_DROP.id(),
    pinned::ui::NAV_END.id(),
    pinned::ui::NAV_EXTENT.id(),
    pinned::ui::NAV_FLAGS.id(),
    pinned::ui::NAV_HIGHLIGHT.id(),
    pinned::ui::NAV_INIT.id(),
    pinned::ui::NAV_INPUT.id(),
    pinned::ui::NAV_INPUT_KIND.id(),
    pinned::ui::NAV_KIND.id(),
    pinned::ui::NAV_LAYER.id(),
    pinned::ui::NAV_PUSH.id(),
    pinned::ui::NAV_SET.id(),
    pinned::ui::NAV_SOUND.id(),
    pinned::ui::NAV_WRAP.id(),
    pinned::ui::NODE_ALIGN.id(),
    pinned::ui::NODE_HEIGHT.id(),
    pinned::ui::NODE_ID.id(),
    pinned::ui::NODE_ID_GET.id(),
    pinned::ui::NODE_MIN_HEIGHT.id(),
    pinned::ui::NODE_PADDING.id(),
    pinned::ui::NODE_SPACING.id(),
    pinned::ui::NODE_STRETCH.id(),
    pinned::ui::NODE_WIDTH.id(),
    pinned::ui::NO_CONSTRUCT.id(),
    pinned::ui::PANEL.id(),
    pinned::ui::PANEL_ALPHA.id(),
    pinned::ui::PANEL_EXTENT.id(),
    pinned::ui::PANEL_KIND.id(),
    pinned::ui::PAUSE_ENTRY.id(),
    pinned::ui::PAUSE_MENU_SITE.id(),
    pinned::ui::PERSIST.id(),
    pinned::ui::POPUP.id(),
    pinned::ui::POPUP_ACTIVE.id(),
    pinned::ui::POPUP_BUTTONS.id(),
    pinned::ui::POPUP_DETAIL.id(),
    pinned::ui::POPUP_DETAIL_LEN.id(),
    pinned::ui::POPUP_EXTENT.id(),
    pinned::ui::POPUP_MESSAGE.id(),
    pinned::ui::POPUP_TITLE.id(),
    pinned::ui::PROGRESS.id(),
    pinned::ui::SCROLLBAR_LOOK.id(),
    pinned::ui::SCROLL_BEGIN.id(),
    pinned::ui::SCROLL_CONFIG.id(),
    pinned::ui::SCROLL_END.id(),
    pinned::ui::SCROLL_FOCUS.id(),
    pinned::ui::SCROLL_INPUT.id(),
    pinned::ui::SCROLL_NAV.id(),
    pinned::ui::SCROLL_SET_FOCUS.id(),
    pinned::ui::SLIDER.id(),
    pinned::ui::SLIDER_BOUNDED.id(),
    pinned::ui::SLIDER_DISCRETE.id(),
    pinned::ui::SLIDER_FORMAT.id(),
    pinned::ui::SLIDER_INIT.id(),
    pinned::ui::SLIDER_MAX.id(),
    pinned::ui::SLIDER_MIN.id(),
    pinned::ui::SLIDER_ON_RELEASE.id(),
    pinned::ui::SLIDER_READ_ONLY.id(),
    pinned::ui::SLIDER_STEPS.id(),
    pinned::ui::SLIDER_STOP_CAPACITY.id(),
    pinned::ui::SLIDER_STOP_COUNT.id(),
    pinned::ui::SLIDER_STOPS.id(),
    pinned::ui::SOUND.id(),
    pinned::ui::STATE_SCREEN.id(),
    pinned::ui::TEXT_FIELD.id(),
    pinned::ui::TEXT_FIELD_EDITING.id(),
    pinned::ui::TEXT_FIELD_EXTENT.id(),
    pinned::ui::TEXT_FIELD_INPUT.id(),
    pinned::ui::TEXT_FIELD_STATE.id(),
    pinned::ui::TEXT_FIELD_STATE_SIZE.id(),
    pinned::ui::TEXT_FIELD_TEXT.id(),
    pinned::ui::TOGGLE.id(),
    pinned::ui::TOGGLE_COUNT.id(),
    pinned::ui::TOGGLE_ENABLED.id(),
    pinned::ui::TOGGLE_EXPLAIN.id(),
    pinned::ui::TOGGLE_EXPLAINER.id(),
    pinned::ui::TOGGLE_EXTENT.id(),
    pinned::ui::TOGGLE_LABEL.id(),
    pinned::ui::VIEW_READ_ONLY.id(),
    pinned::ui::VIEW_TAB.id(),
    pinned::ui::WINDOW_BEGIN.id(),
    pinned::ui::WINDOW_END.id(),
    pinned::ui::WINDOW_EXTENT.id(),
    pinned::ui::WINDOW_NODE.id(),
    pinned::ui::WORLD_SETTINGS_SITE.id(),
];

// SAFETY: `PopupFn` is the signature of the game's `ui.popup`, and `link`
// uses this slot only as the `next` of `detour`, its link on that function.
static NEXT: Next<PopupFn> = unsafe { Next::new() };
static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Frame callbacks: open while mods initialise, sealed after.
static FRAMES: Registry<Frame> = Registry::new();
/// HUD callbacks: open while mods initialise, sealed after.
static HUDS: Registry<Frame> = Registry::new();
/// Frame and HUD callbacks of mods reloaded after sealing, swapped whole.
static LATE_FRAMES: ArcSwapOption<Vec<Frame>> = ArcSwapOption::const_empty();
static LATE_HUDS: ArcSwapOption<Vec<Frame>> = ArcSwapOption::const_empty();

/// A frame or HUD callback with the reload generation of the mod copy that
/// registered it; a reloaded mod's older copies are skipped.
#[derive(Debug, Clone, Copy)]
struct Frame {
    registered: Registered<UiFrameFn>,
    generation: u32,
}

impl Frame {
    fn new(owner: ModIndex, callback: UiFrameFn, user: *mut core::ffi::c_void) -> Self {
        Self {
            registered: Registered::new(owner, callback, user),
            generation: crate::reload::generation(owner.get()),
        }
    }

    /// Whether its mod runs, in the copy that registered it.
    fn current(&self) -> bool {
        self.registered.live()
            && self.generation == crate::reload::generation(self.registered.owner.get())
    }
}

/// Adds `frame` to `late`, for a mod a reload initialises again.
fn add_late(late: &ArcSwapOption<Vec<Frame>>, frame: Frame) {
    late.rcu(|current| {
        let mut next = current.as_deref().cloned().unwrap_or_default();
        next.push(frame);
        Some(std::sync::Arc::new(next))
    });
}

/// The current callbacks of `sealed` and `late`, in order.
fn each<'a>(
    sealed: &'a [Frame],
    late: Option<&'a Vec<Frame>>,
) -> impl Iterator<Item = &'a Registered<UiFrameFn>> {
    sealed
        .iter()
        .chain(late.into_iter().flatten())
        .filter(|frame| frame.current())
        .map(|frame| &frame.registered)
}
// SAFETY: `HudFn` is the signature of the game's `ui.hud`, and `link` uses
// this slot only as the `next` of `hud_detour`, its link on that function.
static HUD_NEXT: Next<HudFn> = unsafe { Next::new() };
type HudFn = unsafe extern "system" fn(arg: *mut u8);
/// HUD items a frame gathers at most.
const MAX_HUD_ITEMS: usize = 128;
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
/// The thread that draws the interface, once it drew.
static UI_THREAD: AtomicU32 = AtomicU32::new(0);
// Each of the loader's windows turns off at its first panic: a panic
// between a window's begin and end would leave the game's window stack
// unbalanced every frame.
static FRAMES_FUSE: Fuse = Fuse::new("mods' windows");
static DEMO_FUSE: Fuse = Fuse::new("interface demo");
static ACTIONS_FUSE: Fuse = Fuse::new("player actions window");
static MODS_SCREEN_FUSE: Fuse = Fuse::new("Mods screen");
static DIALOGS_FUSE: Fuse = Fuse::new("dialogs");
/// Turns the mods' HUD items off at their first panic.
static HUD_FUSE: Fuse = Fuse::new("HUD items");

/// Frames a text copied into the arena stays readable.
const ARENA_FRAMES: usize = 3;
/// Bytes of text per frame.
const ARENA_BYTES: usize = 64 * 1024;

/// The frame callback drawing now.
#[derive(Clone, Copy)]
struct Drawing {
    token: u64,
    owner: ModIndex,
    context: usize,
    window_open: bool,
    /// Drawing the HUD: items gather for their anchors.
    hud: bool,
}

/// A HUD item a callback drew: anchor, then a label or a bar.
#[derive(Clone, Copy)]
struct HudItem {
    anchor: u32,
    kind: u32,
    style: u32,
    text: UiText,
    value: f32,
}

/// Mod texts of the last frames, each frame's in its own buffer.
struct Arena {
    buffers: Vec<Vec<u8>>,
    current: usize,
}

impl Arena {
    const fn new() -> Self {
        Self {
            buffers: Vec::new(),
            current: 0,
        }
    }

    fn next_frame(&mut self) {
        if self.buffers.is_empty() {
            self.buffers = (0..ARENA_FRAMES)
                .map(|_| Vec::with_capacity(ARENA_BYTES))
                .collect();
        }
        self.current = (self.current + 1) % ARENA_FRAMES;
        if let Some(buffer) = self.buffers.get_mut(self.current) {
            buffer.clear();
        }
    }

    /// `parts` joined, as a text the game may read for the next frames;
    /// empty once the frame's buffer is full.
    fn text(&mut self, parts: &[&[u8]]) -> UiText {
        let Some(buffer) = self.buffers.get_mut(self.current) else {
            return UiText::EMPTY;
        };
        let len: usize = parts.iter().map(|p| p.len()).sum();
        if buffer.len() + len > buffer.capacity() {
            return UiText::EMPTY;
        }
        let start = buffer.len();
        for part in parts {
            buffer.extend_from_slice(part);
        }
        // Within capacity, the buffer never moves; it is cleared only
        // `ARENA_FRAMES` frames later.
        buffer.get(start..).map_or(UiText::EMPTY, UiText::new)
    }
}

thread_local! {
    static DRAWING: Cell<Option<Drawing>> = const { Cell::new(None) };
    static WINDOW: RefCell<Window> = const { RefCell::new(Window::new()) };
    static ARENA: RefCell<Arena> = const { RefCell::new(Arena::new()) };
    static HUD_ITEMS: RefCell<Vec<HudItem>> = const { RefCell::new(Vec::new()) };
    static HUD_WINDOW: RefCell<HudWindow> = const { RefCell::new(HudWindow::new()) };
}

fn thread_id() -> u32 {
    // SAFETY: `GetCurrentThreadId` takes no arguments and cannot fail.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

/// Registers a mod's frame callback; client only, during init.
pub(crate) fn register_frame(
    loader: &Loader,
    owner: ModIndex,
    callback: UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    if FRAMES.is_sealed() {
        if !crate::reload::reinitialising(owner.get()) {
            return Status::TOO_LATE;
        }
        add_late(&LATE_FRAMES, Frame::new(owner, callback, user));
        return Status::OK;
    }
    FRAMES
        .add(Frame::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Registers a mod's HUD callback; client only, during init.
pub(crate) fn register_hud(
    loader: &Loader,
    owner: ModIndex,
    callback: UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    if HUDS.is_sealed() {
        if !crate::reload::reinitialising(owner.get()) {
            return Status::TOO_LATE;
        }
        add_late(&LATE_HUDS, Frame::new(owner, callback, user));
        return Status::OK;
    }
    HUDS.add(Frame::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// Queues a mod's dialog; client only.
pub(crate) fn mod_dialog(
    loader: &Loader,
    owner: ModIndex,
    raw: &UiDialog,
    callback: UiAnswerFn,
    user: *mut core::ffi::c_void,
) -> Status {
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    if raw.flags & UiDialog::WAITING != 0 && UI_THREAD.load(Ordering::Acquire) == thread_id() {
        return Status::WRONG_THREAD;
    }
    if raw.buttons & UiDialog::BUTTONS == 0 {
        return Status::BAD_ARGS;
    }
    let read = |value: Str| {
        // SAFETY: the mod's `UiDialog` texts stay alive and unchanged for
        // this call by the ABI's borrow rule, and `Pending::new` copies them
        // before it returns.
        unsafe { value.as_str() }
    };
    let (Some(title), Some(message), Some(detail)) =
        (read(raw.title), read(raw.message), read(raw.detail))
    else {
        return Status::BAD_ARGS;
    };
    dialog::queue(dialog::Pending::new(
        title,
        message,
        detail,
        raw.buttons,
        dialog::Answerer::Mod(Registered::new(owner, callback, user)),
    ))
}

/// The text of a dialog's text tag, for the loader's text lookup.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    dialog::text(id)
}

/// Seals the frame callbacks and links the popup on the client. Without its
/// functions on this build, the interface stays off: mods draw nothing and
/// dialogs show as system message boxes.
pub(crate) fn link(loader: &'static Loader) {
    let count = FRAMES.seal().len();
    let hud_count = HUDS.seal().len();
    if loader.side != Side::CLIENT {
        return;
    }
    map::link(loader);
    let engine = match Engine::resolve(loader) {
        Ok(engine) => engine,
        Err(missing) => {
            crate::log!(WARN, "interface off: {missing}");
            return;
        }
    };
    let _first = ENGINE.set(engine);
    if let Some(engine) = ENGINE.get() {
        mods::link(loader, engine);
        menu_line::link(loader, engine);
    }
    loader.link_feature(
        "HUD",
        Target::Pinned(pinned::ui::HUD),
        &HUD_NEXT,
        hud_detour as HudFn,
        HookFlags::NONE,
    );
    if loader.link_feature(
        "interface",
        Target::Pinned(pinned::ui::POPUP),
        &NEXT,
        detour as PopupFn,
        HookFlags::NONE,
    ) {
        crate::log!(
            INFO,
            "interface: {count} frame and {hud_count} HUD callbacks from mods"
        );
    }
}

/// After arming: dialogs show in game when the popup is wired, else as
/// system message boxes.
pub(crate) fn armed(loader: &Loader) {
    if loader.side == Side::CLIENT {
        let in_game =
            NEXT.wired().is_some() && ENGINE.get().is_some_and(|engine| engine.dialog.is_some());
        if !in_game {
            crate::log!(WARN, "interface not armed: dialogs show as system windows");
        }
        dialog::armed(in_game);
    }
}

/// The loader's link on the game's error popup: the game's draw, then the
/// mods' frames, the loader's windows and its dialog.
///
/// # Safety
///
/// - Called only by the game through the chain on `ui.popup`, inside a frame
///   on the interface thread, with the game's own popup state or null.
unsafe extern "system" fn detour(state: *mut u8) {
    let Some(engine) = ENGINE.get() else {
        // SAFETY: `NEXT` holds the rest of `ui.popup`'s chain while this
        // detour runs, and `state` is the game's own argument, passed on
        // unchanged.
        unsafe { NEXT.get()(state) };
        return;
    };
    let context = engine.context();
    let nav = if context == 0 { 0 } else { engine.nav(context) };
    // SAFETY: `NEXT` holds the rest of `ui.popup`'s chain while this detour
    // runs, and `state` is the game's own argument, passed on unchanged.
    unsafe { NEXT.get()(state) };
    ARENA.with_borrow_mut(Arena::next_frame);
    if context == 0 || state.is_null() {
        return;
    }
    engine.restore_nav(context, nav);
    // SAFETY: `state` is non-null, checked by the guard before it, and is the
    // popup state the game passed to `ui.popup`; its active byte sits at the
    // table's `ui.popup_active`.
    let vanilla =
        unsafe { crate::memory::read::<u8>(state.expose_provenance(), engine.layout.popup_active) }
            != 0;
    if vanilla {
        return;
    }
    UI_THREAD.store(thread_id(), Ordering::Release);
    let frame = mods::next_frame();
    map::note(context);
    // A window that panicked between its begin and end leaves the game's
    // window stack unbalanced, so a panic ends this frame's windows; its own
    // fuse keeps it off from the next frame on.
    let _drawn = drew(&FRAMES_FUSE, "the mods' frames", || frames(engine, context))
        // SAFETY: the game draws its popup inside the frame on the interface
        // thread, `context` is non-zero (checked by the guard before it), and
        // `frames` closed every window a mod's callback left open.
        && drew(&DEMO_FUSE, "the demo's draw", || unsafe { demo::draw(engine, context) })
        // SAFETY: the game draws its popup inside the frame on the interface
        // thread, `context` is non-zero (checked by the guard before it), and
        // `demo::draw` ends its window before it returns.
        && drew(&ACTIONS_FUSE, "the actions' draw", || unsafe {
            actions::draw(engine, context);
        })
        && drew(&MODS_SCREEN_FUSE, "the Mods screen's draw", || {
            if let Some(loader) = crate::host::loader() {
                // SAFETY: the game draws its popup inside the frame on the
                // interface thread, `context` is non-zero (checked by the
                // guard before it), `actions::draw` ends its window before it
                // returns, and `frame` is the count `mods::next_frame` gave
                // this frame.
                unsafe { mods::draw(loader, engine, context, frame) };
            }
        })
        // SAFETY: the game draws its popup inside the frame on the interface
        // thread, `context` is non-zero and the game's popup is not active
        // (both checked by the guard before it), and `NEXT` holds the rest of
        // `ui.popup`'s chain while this detour runs.
        && drew(&DIALOGS_FUSE, "the dialogs' draw", || unsafe {
            dialog::draw(engine, NEXT.get(), context);
        });
}

/// Runs `work` under `fuse` at `site`: false only when it panicked now.
fn drew(fuse: &Fuse, site: &str, work: impl FnOnce()) -> bool {
    fuse.is_off() || fuse.run(site, work).is_some()
}

/// The loader's link on the game's HUD: the game's draw, then the mods' HUD
/// items.
///
/// # Safety
///
/// - Called only by the game through the chain on `ui.hud`, inside a frame
///   on the interface thread, with the game's own argument.
unsafe extern "system" fn hud_detour(arg: *mut u8) {
    // SAFETY: `HUD_NEXT` holds the rest of `ui.hud`'s chain while this
    // detour runs, and `arg` is the game's own argument, passed on unchanged.
    unsafe { HUD_NEXT.get()(arg) };
    let Some(engine) = ENGINE.get() else {
        return;
    };
    if hud_turns_frames(NEXT.wired().is_some()) {
        ARENA.with_borrow_mut(Arena::next_frame);
    }
    let context = engine.context();
    if context == 0 {
        return;
    }
    let _drawn = HUD_FUSE.run("the HUD's draw", || {
        // SAFETY: the game draws its HUD inside the frame on the interface
        // thread, and this runs after the game's own HUD draw returned.
        if unsafe { engine.hud_visible() } {
            hud(engine, context);
        }
    });
}

/// Whether the HUD turns the text arena to the next frame: only when the
/// popup, which turns it every frame otherwise, is not wired.
const fn hud_turns_frames(popup_wired: bool) -> bool {
    !popup_wired
}

/// The game's alignment word for a HUD anchor, and the text alignment
/// inside its column.
fn anchor_align(anchor: u32) -> (u16, u16) {
    let horizontal = [1, 2, 3, 1, 2, 3][anchor as usize % 6];
    let vertical = if anchor < 3 { 1 } else { 3 };
    (vertical << 8 | horizontal, 0x200 | horizontal)
}

fn hud(engine: &Engine, context: usize) {
    let Some(huds) = HUDS.sealed() else {
        return;
    };
    HUD_ITEMS.with_borrow_mut(Vec::clear);
    let late = LATE_HUDS.load();
    for registered in each(huds, late.as_deref()) {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        DRAWING.set(Some(Drawing {
            token,
            owner: registered.owner,
            context,
            window_open: false,
            hud: true,
        }));
        let frame = UiFrame {
            header: enshwick::abi::Header::of::<UiFrame>(),
            token,
            screen: UiFrame::IN_GAME,
            reserved: 0,
        };
        // SAFETY: the callback lives in a mod DLL that stays loaded, and it
        // gets a live `UiFrame` and the pointer its mod registered with it;
        // an `extern "system"` function aborts rather than unwinds.
        crate::profile::call(
            registered.owner,
            crate::profile::Feature::Interface,
            || unsafe { (registered.callback)(&raw const frame, registered.user()) },
        );
        DRAWING.set(None);
    }
    HUD_ITEMS.with_borrow_mut(demo::hud_samples);
    HUD_ITEMS.with_borrow_mut(actions::hud);
    HUD_ITEMS.with_borrow(|items| {
        if items.is_empty() {
            return;
        }
        let name = copy(&[b"enshwick/hud"]);
        HUD_WINDOW.with_borrow_mut(|window| {
            // SAFETY: `hud_detour` runs this inside the frame on the
            // interface thread; `window` stays in the thread-local
            // `HUD_WINDOW`, borrowed until `hud_end` below, and `name` lives
            // in the frame's text arena.
            unsafe { engine.hud_begin(window, &name) };
            for anchor in 0..6 {
                if !items.iter().any(|i| i.anchor == anchor) {
                    continue;
                }
                let (column, text) = anchor_align(anchor);
                let draw_items = || {
                    for item in items.iter().filter(|i| i.anchor == anchor) {
                        if item.kind == UiWidget::LABEL {
                            // SAFETY: `hud_column` runs this inside its
                            // column on the frame's thread, `context` is the
                            // non-zero context `hud_detour` checked, and the
                            // text lives in the frame's arena.
                            unsafe { engine.hud_label(context, &item.text, item.style, text) };
                        } else {
                            // SAFETY: `hud_column` runs this inside its
                            // column on the frame's thread.
                            unsafe { engine.progress(item.value, 0.0, 1.0) };
                        }
                    }
                };
                // SAFETY: inside the HUD window `hud_begin` started, on the
                // frame's thread; labels and bars open no frame they leave
                // open.
                unsafe { engine.hud_column(column, draw_items) };
            }
            // SAFETY: `hud_begin` started `window` in this frame, on this
            // thread; it stayed borrowed in place, and every column closed
            // before `hud_column` returned.
            unsafe { engine.hud_end(window) };
        });
    });
}

/// Gathers a HUD callback's widget for its anchor.
fn gather(raw: &UiWidget) -> Status {
    if raw.anchor > UiWidget::BOTTOM_RIGHT {
        return Status::BAD_ARGS;
    }
    let item = match raw.kind {
        UiWidget::LABEL => {
            let Some(text) = read(raw.text) else {
                return Status::BAD_ARGS;
            };
            HudItem {
                anchor: raw.anchor,
                kind: raw.kind,
                style: raw.style,
                text: copy(&[text]),
                value: 0.0,
            }
        }
        UiWidget::PROGRESS => {
            if !(raw.value.is_finite() && raw.min.is_finite() && raw.max.is_finite())
                || raw.max <= raw.min
            {
                return Status::BAD_ARGS;
            }
            HudItem {
                anchor: raw.anchor,
                kind: raw.kind,
                style: 0,
                text: UiText::EMPTY,
                value: ((raw.value - raw.min) / (raw.max - raw.min)).clamp(0.0, 1.0),
            }
        }
        _ => return Status::REFUSED,
    };
    HUD_ITEMS.with_borrow_mut(|items| {
        if items.capacity() == 0 {
            items.reserve(MAX_HUD_ITEMS);
        }
        if items.len() < MAX_HUD_ITEMS {
            items.push(item);
            Status::OK
        } else {
            Status::REFUSED
        }
    })
}

/// The screen for the app state byte.
fn screen(state: u8) -> u32 {
    match state {
        7 => UiFrame::IN_GAME,
        4..=6 => UiFrame::LOADING,
        _ => UiFrame::MENU,
    }
}

fn frames(engine: &Engine, context: usize) {
    let Some(frames) = FRAMES.sealed() else {
        return;
    };
    let screen = screen(engine.app_state(context));
    let late = LATE_FRAMES.load();
    for registered in each(frames, late.as_deref()) {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        DRAWING.set(Some(Drawing {
            token,
            owner: registered.owner,
            context,
            window_open: false,
            hud: false,
        }));
        let frame = UiFrame {
            header: enshwick::abi::Header::of::<UiFrame>(),
            token,
            screen,
            reserved: 0,
        };
        // SAFETY: the callback lives in a mod DLL that stays loaded, and it
        // gets a live `UiFrame` and the pointer its mod registered with it;
        // an `extern "system"` function aborts rather than unwinds.
        crate::profile::call(
            registered.owner,
            crate::profile::Feature::Interface,
            || unsafe { (registered.callback)(&raw const frame, registered.user()) },
        );
        let left = DRAWING.take();
        if let Some(drawing) = left
            && drawing.window_open
        {
            // SAFETY: `window_open` is set only when the callback's `WINDOW`
            // widget started the thread-local window in this frame with this
            // `context`; it stayed in place, and widgets close every frame
            // they open.
            WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
            crate::log!(DEBUG, "interface: a frame callback left its window open");
        }
    }
}

mod painter;
pub(crate) use painter::{Painter, widget};
#[cfg(test)]
mod tests;
