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

use painter::{Widgets, copy, options, read};

use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::profile::Feature;
use crate::engine::registry::{JoinFeatureError, Registry};

mod arena;
pub(crate) mod carried;
pub(crate) mod demo;
mod dialog;
mod engine;
mod extensions;
mod hold;
pub(crate) mod item_slots;
mod map;
mod menu_line;
mod mods;
mod off;
mod stat_rows;
mod table_ids;
pub(crate) mod tooltips;

pub(crate) use table_ids::TABLE_IDS;

use core::cell::{Cell, RefCell};
use core::ops::ControlFlow;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use enshwick_core::abi::{
    HookFlags, Side, Str, UiAnswerFn, UiDialog, UiFrame, UiFrameFn, UiWidget,
};

use arena::{Arena, escape};
pub(crate) use demo::open as open_demo;
pub(crate) use dialog::{close_world_dialogs, show};
use engine::{Engine, HudWindow, PopupFn, UiText, Window};
pub(crate) use extensions::{ExtendScreenError, register as register_extension};

pub(crate) use map::{MarkersError, set as set_markers};
pub(crate) use mods::mismatch_screen_on;

use crate::engine::chains::{Next, Onward};
use crate::engine::fuse::Fuse;
use crate::engine::interface_parts::Part;
use crate::engine::loader::{Loader, Registered, Target};
use off::{PART_FUSES, dialogs_drawn, drawn_no_more, parts_off_with_interface};

// SAFETY: `PopupFn` is the signature of the game's `ui.popup`, and `link`
// uses this slot only as the `next` of `detour`, its link on that function.
static NEXT: Next<PopupFn> = unsafe { Next::new() };
/// The interface: the link on the game's popup, which every window and
/// dialog of the mods and the loader draws from.
static INTERFACE_FUSE: Fuse = Fuse::new("interface");
static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Frame callbacks: open while mods initialise, sealed after.
static FRAMES: Registry<Registered<UiFrameFn>> = Registry::new();
/// HUD callbacks: open while mods initialise, sealed after.
static HUDS: Registry<Registered<UiFrameFn>> = Registry::new();

type HudFn = unsafe extern "system" fn(arg: *mut u8);
/// HUD items a frame gathers at most.
const MAX_HUD_ITEMS: usize = 128;

/// What the interface keeps while the game runs. None of it belongs to a
/// world, so a hosted game's end forgets only the demo's request
/// ([`forget`]).
#[derive(Debug)]
struct State {
    /// The token of the next frame callback to draw; never reset, so a
    /// widget call holding an old token matches no later callback.
    next_token: AtomicU64,
    /// The thread that draws the interface, once it drew: the game's main
    /// thread, the same for every world.
    ui_thread: AtomicU32,
    /// The count of the next frame id a scroll area takes; never reset, so
    /// no area takes the node of another.
    next_scroll: AtomicU64,
    /// The count of the game's interface updates, every one, also those the
    /// loader draws nothing in (a game popup, no context).
    tick: AtomicU64,
}

static STATE: State = State {
    next_token: AtomicU64::new(1),
    ui_thread: AtomicU32::new(0),
    next_scroll: AtomicU64::new(1),
    tick: AtomicU64::new(0),
};

/// The game's interface update now being run, as [`frame`] counts them.
pub(in crate::ui::interface) fn tick() -> u64 {
    STATE.tick.load(Ordering::Relaxed)
}

/// A token for the frame callback about to draw, unique in the process.
fn next_token() -> u64 {
    STATE.next_token.fetch_add(1, Ordering::Relaxed)
}

/// A count for the next scroll area's frame id, unique in the process.
fn next_scroll() -> u64 {
    STATE.next_scroll.fetch_add(1, Ordering::Relaxed)
}

/// Forgets what the interface kept of a client's hosted game that ended.
pub(crate) fn forget(_loader: &'static Loader) {
    demo::forget();
    close_world_dialogs();
}

// Each of the loader's windows turns off at its first panic: a panic
// between a window's begin and end would leave the game's window stack
// unbalanced every frame.
static FRAMES_FUSE: Fuse = Fuse::new("mods' windows");
static EXTENSIONS_FUSE: Fuse = Fuse::new("screen extensions");
use crate::engine::screens::READS_FUSE;
static DEMO_FUSE: Fuse = Fuse::new("interface demo");
static CARRIED_FUSE: Fuse = Fuse::new("carried windows");
static MODS_SCREEN_FUSE: Fuse = Fuse::new("Mods screen");
static DIALOGS_FUSE: Fuse = Fuse::new("dialogs");
/// The HUD part: its link on the game's HUD and the mods' HUD items.
static HUD_FUSE: Fuse = Fuse::new(Part::HUD.name);

/// The frame callback drawing now.
#[derive(Clone, Copy)]
struct Drawing {
    token: u64,
    owner: ModIndex,
    context: usize,
    window_open: bool,
    /// Drawing the HUD: items gather for their anchors.
    hud: bool,
    /// The window of the place beside the game screen whose extension draws
    /// now: its widgets go into that place's window, which the loader opens
    /// and the callback may not end or nest.
    extension: Option<extensions::PanelWindow>,
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

thread_local! {
    static DRAWING: Cell<Option<Drawing>> = const { Cell::new(None) };
    static WINDOW: RefCell<Window> = const { RefCell::new(Window::new()) };
    static ARENA: RefCell<Arena> = const { RefCell::new(Arena::new()) };
    static HUD_ITEMS: RefCell<Vec<HudItem>> = const { RefCell::new(Vec::new()) };
    static HUD_WINDOW: RefCell<HudWindow> = const { RefCell::new(HudWindow::new()) };
    /// Whether the loader or a mod began a window in this frame and in the
    /// last one.
    static WINDOWS: Cell<[bool; 2]> = const { Cell::new([false; 2]) };
}

/// Notes that a window began in this frame, on the interface thread.
fn window_drawn() {
    let [_, last] = WINDOWS.get();
    WINDOWS.set([true, last]);
}

/// Whether the loader or a mod showed a window or a dialog in the last
/// frame, which takes the player's keys from mod controls; on the interface
/// thread.
pub(crate) fn window_shown() -> bool {
    WINDOWS.get()[1] || dialog::showing()
}

/// Registers a mod's frame callback; client only, during init.
///
/// # Errors
///
/// [`JoinFeatureError`] off a client or after init.
pub(crate) fn register_frame(
    loader: &Loader,
    owner: ModIndex,
    callback: UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Result<(), JoinFeatureError> {
    on_client(loader)?;
    Ok(FRAMES.add(Registered::new(owner, callback, user))?)
}

/// [`JoinFeatureError::Unavailable`] off a client, where no interface draws.
fn on_client(loader: &Loader) -> Result<(), JoinFeatureError> {
    if loader.side == Side::CLIENT {
        Ok(())
    } else {
        Err(JoinFeatureError::Unavailable)
    }
}

/// Registers a mod's HUD callback; client only, during init.
///
/// # Errors
///
/// [`JoinFeatureError`] off a client or after init.
pub(crate) fn register_hud(
    loader: &Loader,
    owner: ModIndex,
    callback: UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Result<(), JoinFeatureError> {
    on_client(loader)?;
    Ok(HUDS.add(Registered::new(owner, callback, user))?)
}

/// Why a mod's dialog was not queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DialogError {
    /// Dialogs show only on a client.
    #[error("dialogs show only on a client")]
    NotClient,
    /// A dialog that waits, asked from the interface thread, which it would
    /// block.
    #[error("a waiting dialog asked from the interface thread")]
    WrongThread,
    /// No buttons, or a text that does not read.
    #[error("the dialog is malformed")]
    Malformed,
    /// Too many dialogs wait.
    #[error(transparent)]
    Full(#[from] dialog::DialogsFullError),
}

/// Queues a mod's dialog; client only.
pub(crate) fn mod_dialog(
    loader: &Loader,
    owner: ModIndex,
    raw: &UiDialog,
    callback: UiAnswerFn,
    user: *mut core::ffi::c_void,
) -> Result<(), DialogError> {
    if loader.side != Side::CLIENT {
        return Err(DialogError::NotClient);
    }
    if raw.flags & UiDialog::WAITING != 0
        && STATE.ui_thread.load(Ordering::Acquire) == crate::engine::off_thread::current_thread()
    {
        return Err(DialogError::WrongThread);
    }
    if raw.buttons & UiDialog::BUTTONS == 0 {
        return Err(DialogError::Malformed);
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
        return Err(DialogError::Malformed);
    };
    let dialog = dialog::Pending::new(
        title,
        message,
        detail,
        raw.buttons,
        dialog::Answerer::Mod(Registered::new(owner, callback, user)),
    );
    Ok(dialog::queue(dialog.of_world(loader, owner))?)
}

/// The text of a dialog's text tag, for the loader's text lookup.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    dialog::text(id)
}

/// Seals the frame and HUD callbacks and the screen extensions, before any
/// link.
pub(crate) fn seal() {
    let _frames = FRAMES.seal();
    let _huds = HUDS.seal();
    extensions::seal();
    tooltips::seal();
}

/// Links the popup on the client. Without its functions on this build, the
/// interface stays off: mods draw nothing and dialogs show as system
/// message boxes.
pub(crate) fn link(loader: &'static Loader) {
    let count = FRAMES.sealed().map_or(0, <[_]>::len);
    let hud_count = HUDS.sealed().map_or(0, <[_]>::len);
    if loader.side != Side::CLIENT {
        return;
    }
    map::link(loader);
    stat_rows::link(loader);
    let engine = match Engine::resolve(loader) {
        Ok(engine) => engine,
        Err(missing) => {
            loader.fuse_off(&INTERFACE_FUSE, missing.id(), &missing.reason());
            parts_off_with_interface();
            return;
        }
    };
    let _first = ENGINE.set(engine);
    if let Some(engine) = ENGINE.get() {
        mods::link(loader, engine);
        crate::actions::client_commands::windows::register(carried::draws);
        crate::actions::client_commands::dialogs::register(dialog::on_screen);
        menu_line::link(loader, engine);
        hold::link(loader);
        item_slots::link(loader, extensions::hosts_asked(count + hud_count > 0));
        tooltips::link(loader);
    }
    if ENGINE.get().is_some_and(Engine::has_hud) {
        link_hud(loader, &HUD_FUSE);
    }
    if loader.link_feature(
        &INTERFACE_FUSE,
        Target::Pinned(pinned::ui::POPUP),
        &NEXT,
        detour as PopupFn,
        HookFlags::NONE,
    ) {
        crate::log!(
            INFO,
            "interface: {count} frame and {hud_count} HUD callbacks from mods"
        );
        extensions::link(loader, count + hud_count > 0);
    } else {
        parts_off_with_interface();
    }
}

/// Links the HUD part under `feature`, [`HUD_FUSE`] outside tests: the
/// feature its draw runs under, so a panic there wires the chain past it.
fn link_hud(loader: &Loader, feature: &'static Fuse) -> bool {
    loader.link_feature(
        feature,
        Target::Pinned(pinned::ui::HUD),
        &HUD_NEXT,
        hud_detour as HudFn,
        HookFlags::NONE,
    )
}

/// Whether the popup's link runs: wired and its feature on.
fn popup_runs() -> bool {
    runs(&NEXT, &INTERFACE_FUSE)
}

/// Whether the link whose `next` slot is `next` runs: its chain is wired
/// and its `feature` on.
fn runs<F: Copy>(next: &Next<F>, feature: &Fuse) -> bool {
    next.wired().is_some() && !feature.is_off()
}

/// After arming: dialogs show in game when the popup is wired, else as
/// system message boxes.
pub(crate) fn armed(loader: &Loader) {
    if loader.side == Side::CLIENT {
        let in_game = popup_runs() && ENGINE.get().is_some_and(|engine| engine.dialog.is_some());
        if !in_game {
            crate::log!(WARN, "interface not armed: dialogs show as system windows");
        }
        dialog::armed(in_game);
    }
}

/// Code patch tries from the interface frame; a panic turns only them off
/// there.
static RETRY_FUSE: Fuse = Fuse::new("code patch tries in the interface");

/// The loader's link on the game's error popup, `ui.popup`: the game's
/// draw, then the mods' frames, the loader's windows and its dialog. Written
/// by hand, not by `chain_detour!`: the dialogs call the rest of the chain
/// again with the loader's own popup state.
///
/// # Safety
///
/// - Called only by the game through the chain on `ui.popup`, inside a frame
///   on the interface thread, with the game's own popup state or null.
unsafe extern "system" fn detour(state: *mut u8) {
    let next = NEXT.get();
    let onward = Onward::new(|| {
        // SAFETY: `next` is the rest of `ui.popup`'s chain, wired before the
        // target jumps here, and `state` is the game's own argument, passed
        // on unchanged.
        unsafe { next(state) }
    });
    onward.run(&INTERFACE_FUSE, "the interface frame", |onward| {
        // SAFETY: this detour's own contract; `next` is the rest of
        // `ui.popup`'s chain, which `onward` calls with `state`.
        unsafe { frame(state, next, onward) }
    });
    if !dialogs_drawn() {
        drawn_no_more();
    }
}

/// One frame of the interface: the game's popup through `onward`, then the
/// mods' frames, the loader's windows and its dialog.
///
/// # Safety
///
/// - Called only from [`detour`], inside a frame on the interface thread,
///   with the game's own popup state or null as `state`.
/// - `next` is the rest of `ui.popup`'s chain, and `onward` calls it with
///   `state`.
unsafe fn frame(state: *mut u8, next: PopupFn, onward: &Onward<(), impl Fn()>) {
    STATE.tick.fetch_add(1, Ordering::Relaxed);
    let Some(engine) = ENGINE.get() else {
        return onward.call();
    };
    let context = engine.context();
    let nav = if context == 0 { 0 } else { engine.nav(context) };
    onward.call();
    ARENA.with_borrow_mut(Arena::next_frame);
    item_slots::end_frame(context);
    tooltips::end_frame();
    painter::next_frame();
    // A client that never hosts never steps, so its waiting patches are also
    // tried here; one atomic compare until a check is due.
    if let Some(loader) = crate::engine::loader::loader() {
        let _tried = RETRY_FUSE.run("the interface frame", || {
            crate::engine::retry::after_step(loader, std::time::Instant::now());
        });
    }
    if context == 0 || state.is_null() {
        // SAFETY: inside the frame on the interface thread, with the frame's
        // own context or 0.
        unsafe { crate::ui::sound::frame(context) };
        crate::engine::screens::note_window(false);
        return;
    }
    let read = READS_FUSE.run("the frame's screen", || screen(engine.app_state(context)));
    // A screen that did not read is no leave: the world's windows stay.
    if let Some(shown) = read {
        note_world(shown != UiFrame::MENU);
    }
    let shown = read.unwrap_or(UiFrame::MENU);
    let [now, _] = WINDOWS.get();
    WINDOWS.set([false, now]);
    engine::caret_next_frame(now);
    crate::engine::screens::note_window(window_shown());
    crate::actions::controls::frame(context);
    crate::actions::game_bindings::frame(context);
    // SAFETY: `state` is non-null, checked by the guard before it, and is the
    // popup state the game passed to `ui.popup`; its active byte sits at the
    // table's `ui.popup_active`.
    let vanilla = unsafe {
        crate::engine::memory::read::<u8>(state.expose_provenance(), engine.layout.popup_active)
    } != 0;
    // SAFETY: inside the frame on the interface thread, `context` non-zero
    // by the guard before it.
    unsafe { snapshot::note(engine, context, vanilla) };
    // After the session's count for this frame, so a sound started in a
    // world that just changed is not ended with the old world's.
    // SAFETY: inside the frame on the interface thread, with the frame's
    // own non-zero context.
    unsafe { crate::ui::sound::frame(context) };
    engine.restore_nav(context, nav);
    if vanilla {
        // SAFETY: inside the frame on the interface thread, `context`
        // non-zero by the guard before it; no loader window draws.
        unsafe { carried::pass_map(engine, context, false) };
        return;
    }
    STATE.ui_thread.store(
        crate::engine::off_thread::current_thread(),
        Ordering::Release,
    );
    let frame = mods::next_frame();
    map::note(context);
    let asked = extensions::hosts_asked(FRAMES.sealed().is_some_and(|f| !f.is_empty()));
    // SAFETY: the game draws its popup inside the frame on the interface
    // thread, and `context` is the frame's own, non-zero by the guard before
    // it.
    let reads = unsafe { frame_reads(engine, context, asked) };
    let drawn = Drawn {
        engine,
        context,
        screen: shown,
        reads,
        frame,
        next,
    };
    Fuse::run_in_turn(
        &DRAWS,
        |draw| draw.site,
        |draw| draw.fuse,
        |draw| {
            // SAFETY: `draw` is one of `DRAWS`, run in their order, inside the
            // frame on the interface thread; `drawn.context` is non-zero and the
            // game's popup is not active (both checked by the guards before it),
            // and `drawn.next` is the rest of `ui.popup`'s chain.
            unsafe { (draw.run)(&drawn) };
        },
    );
    // SAFETY: inside the frame on the interface thread, `context` non-zero
    // by the guard before it, after every window of the frame drew.
    unsafe { carried::pass_map(engine, context, true) };
}

/// Notes, each frame on the interface thread, whether the client shows a
/// world: leaving one for the menu, however it leaves, ends its dialogs and
/// the windows its server showed, which a link the carrier still holds
/// would otherwise keep over the menu.
fn note_world(in_world: bool) {
    if dialog::note_world(in_world) {
        let _forgot = CARRIED_FUSE.run("the carried windows' leave", carried::forget);
    }
}

/// This frame's screen, prompt and host reads, or none once they are off.
///
/// # Safety
///
/// - Called inside a frame on the interface thread, with `context` the
///   frame's own and non-zero.
unsafe fn frame_reads(engine: &Engine, context: usize, asked: bool) -> extensions::FrameReads {
    READS_FUSE
        .run("the frame's screen reads", || {
            // SAFETY: the caller's contract.
            unsafe { extensions::frame_reads(engine, context, asked) }
        })
        .unwrap_or(extensions::FrameReads::NONE)
}

/// What the loader's windows draw from in one frame.
struct Drawn<'frame> {
    engine: &'frame Engine,
    context: usize,
    /// The frame's screen, a [`UiFrame`] screen constant.
    screen: u32,
    reads: extensions::FrameReads,
    /// The count [`mods::next_frame`] gave this frame.
    frame: u64,
    /// The rest of the chain on the game's `ui.popup`.
    next: PopupFn,
}

/// One of the loader's windows, a feature of its own.
struct Draw {
    fuse: &'static Fuse,
    /// Where a panic came from, as its fault names it.
    site: &'static str,
    /// Draws the window.
    ///
    /// # Safety
    ///
    /// - Called only from [`frame`], inside the frame on the interface
    ///   thread, after every window before it in [`DRAWS`] ended.
    /// - The frame's context is non-zero and the game's popup is not active.
    run: unsafe fn(&Drawn<'_>),
}

/// The loader's windows in the order they draw. A window that panicked
/// between its begin and end leaves the game's window stack unbalanced, so
/// a panic ends this frame's windows; its own fuse keeps it off from the
/// next frame on.
static DRAWS: [Draw; 6] = [
    Draw {
        fuse: &FRAMES_FUSE,
        site: "the mods' frames",
        run: |drawn| frames(drawn.engine, drawn.context, drawn.screen, drawn.reads),
    },
    Draw {
        fuse: &EXTENSIONS_FUSE,
        site: "the screen extensions' draw",
        // SAFETY: `Draw::run`'s contract, and `frames` closed every window
        // a mod's callback left open.
        run: |drawn| unsafe { extensions::draw(drawn.engine, drawn.context, drawn.reads) },
    },
    Draw {
        fuse: &DEMO_FUSE,
        site: "the demo's draw",
        // SAFETY: `Draw::run`'s contract, and `extensions::draw` ends each
        // window before it returns.
        run: |drawn| unsafe { demo::draw(drawn.engine, drawn.context) },
    },
    Draw {
        fuse: &CARRIED_FUSE,
        site: "the carried windows' draw",
        // SAFETY: `Draw::run`'s contract, and `demo::draw` ends its window
        // before it returns.
        run: |drawn| unsafe { carried::draw(drawn.engine, drawn.context, drawn.reads.seen[0]) },
    },
    Draw {
        fuse: &MODS_SCREEN_FUSE,
        site: "the Mods screen's draw",
        run: |drawn| {
            if let Some(loader) = crate::engine::loader::loader() {
                // SAFETY: `Draw::run`'s contract, `carried::draw` ends its
                // window before it returns, and `drawn.frame` is the count
                // `mods::next_frame` gave this frame.
                unsafe { mods::draw(loader, drawn.engine, drawn.context, drawn.frame) };
            }
        },
    },
    Draw {
        fuse: &DIALOGS_FUSE,
        site: "the dialogs' draw",
        // SAFETY: `Draw::run`'s contract, and `drawn.next` is the rest of
        // `ui.popup`'s chain.
        run: |drawn| unsafe { dialog::draw(drawn.engine, drawn.next, drawn.context) },
    },
];

crate::engine::chains::chain_detour! {
    /// The loader's link on the game's HUD, `ui.hud`: the game's draw, then
    /// the mods' HUD items.
    ///
    /// # Safety
    ///
    /// - Called only by the game through the chain on `ui.hud`, inside a
    ///   frame on the interface thread, with the game's own argument.
    fn hud_detour(arg: *mut u8) -> ();
    next HUD_NEXT;
    fuse &HUD_FUSE, "the HUD's draw";
    |onward| {
        onward.call();
        let Some(engine) = ENGINE.get() else {
            return;
        };
        if hud_turns_frames(popup_runs()) {
            ARENA.with_borrow_mut(Arena::next_frame);
        }
        let context = engine.context();
        // SAFETY: the game draws its HUD inside the frame on the interface
        // thread, and this runs after the game's own HUD draw returned.
        if context != 0 && unsafe { engine.hud_visible() } {
            hud(engine, context);
        }
    }
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
    // SAFETY: `hud_detour` calls this inside the frame on the interface
    // thread with the frame's own non-zero `context`.
    let reads = unsafe { frame_reads(engine, context, !huds.is_empty()) };
    let _none: Option<(_, ())> =
        crate::engine::filters::ask(huds, Feature::Interface, |callback| {
            let token = next_token();
            DRAWING.set(Some(Drawing {
                token,
                owner: callback.owner(),
                context,
                window_open: false,
                hud: true,
                extension: None,
            }));
            let frame = reads.frame(token);
            // SAFETY: the callback lives in a mod DLL that stays loaded, and
            // it gets a live `UiFrame` and the pointer its mod registered with
            // it; an `extern "system"` function aborts rather than unwinds.
            callback.call(|callback, user| unsafe { callback(&raw const frame, user) });
            DRAWING.set(None);
            ControlFlow::Continue(())
        });
    HUD_ITEMS.with_borrow_mut(demo::hud_samples);
    HUD_ITEMS.with_borrow_mut(|items| {
        crate::actions::build_reasons::hud_line(|text| {
            items.push(HudItem {
                anchor: UiWidget::BOTTOM_CENTER,
                kind: UiWidget::LABEL,
                style: UiWidget::STYLE_WARNING,
                text: copy(&[text.as_bytes()]),
                value: 0.0,
            });
        });
    });
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
fn gather(raw: &UiWidget) -> Result<(), crate::ui::widget_checks::WidgetError> {
    if raw.anchor > UiWidget::BOTTOM_RIGHT {
        return Err(crate::ui::widget_checks::WidgetError::Malformed);
    }
    let item = match raw.kind {
        UiWidget::LABEL => {
            let Some(text) = read(raw.text) else {
                return Err(crate::ui::widget_checks::WidgetError::Malformed);
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
                return Err(crate::ui::widget_checks::WidgetError::Malformed);
            }
            HudItem {
                anchor: raw.anchor,
                kind: raw.kind,
                style: 0,
                text: UiText::EMPTY,
                value: ((raw.value - raw.min) / (raw.max - raw.min)).clamp(0.0, 1.0),
            }
        }
        _ => return Err(crate::ui::widget_checks::WidgetError::HudRefused),
    };
    HUD_ITEMS.with_borrow_mut(|items| {
        if items.capacity() == 0 {
            items.reserve(MAX_HUD_ITEMS);
        }
        if items.len() < MAX_HUD_ITEMS {
            items.push(item);
            Ok(())
        } else {
            Err(crate::ui::widget_checks::WidgetError::HudRefused)
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

fn frames(engine: &Engine, context: usize, screen: u32, reads: extensions::FrameReads) {
    let Some(frames) = FRAMES.sealed() else {
        return;
    };
    let _none: Option<(_, ())> =
        crate::engine::filters::ask(frames, Feature::Interface, |callback| {
            let token = next_token();
            DRAWING.set(Some(Drawing {
                token,
                owner: callback.owner(),
                context,
                window_open: false,
                hud: false,
                extension: None,
            }));
            let frame = UiFrame {
                screen,
                ..reads.frame(token)
            };
            // SAFETY: the callback lives in a mod DLL that stays loaded, and
            // it gets a live `UiFrame` and the pointer its mod registered with
            // it; an `extern "system"` function aborts rather than unwinds.
            callback.call(|callback, user| unsafe { callback(&raw const frame, user) });
            let left = DRAWING.take();
            if let Some(drawing) = left
                && drawing.window_open
            {
                // SAFETY: inside the frame on the interface thread, in the
                // callback's window, still open; widgets close every frame
                // they open.
                unsafe { painter::end_slots(engine) };
                // SAFETY: `window_open` is set only when the callback's
                // `WINDOW` widget started the thread-local window in this
                // frame with this `context`; it stayed in place, and widgets
                // close every frame they open.
                WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
                crate::log!(DEBUG, "interface: a frame callback left its window open");
            }
            ControlFlow::Continue(())
        });
}

mod painter;
#[cfg(test)]
mod recorder;
mod snapshot;
pub(crate) use painter::{Painter, widget};
#[cfg(test)]
mod tests;
