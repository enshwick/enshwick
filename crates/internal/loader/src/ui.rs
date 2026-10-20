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

mod dialog;
mod engine;

use core::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use enshwick_loader_api::abi::{
    HookFlags, Side, Status, Str, UiAnswerFn, UiDialog, UiFrame, UiFrameFn, UiWidget,
};

pub(crate) use dialog::show;
use engine::{Engine, PopupFn, UiText, Window};

use crate::chains::Next;
use crate::host::{Loader, Registered};

// SAFETY: the slot joins the chain of the error popup, typed `PopupFn`.
static NEXT: Next<PopupFn> = unsafe { Next::new() };
static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Frame callbacks while mods initialise.
static PENDING: Mutex<Vec<Registered<UiFrameFn>>> = Mutex::new(Vec::new());
/// The frame callbacks once sealed.
static FRAMES: OnceLock<Box<[Registered<UiFrameFn>]>> = OnceLock::new();
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
/// The thread that draws the interface, once it drew.
static UI_THREAD: AtomicU32 = AtomicU32::new(0);

/// Frames a text copied into the arena stays readable.
const ARENA_FRAMES: usize = 3;
/// Bytes of text per frame.
const ARENA_BYTES: usize = 64 * 1024;

/// The frame callback drawing now.
#[derive(Clone, Copy)]
struct Drawing {
    token: u64,
    owner: usize,
    context: usize,
    window_open: bool,
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
}

fn thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
}

/// Registers a mod's frame callback; client only, during init.
pub(crate) fn register_frame(
    loader: &Loader,
    owner: usize,
    callback: UiFrameFn,
    user: *mut core::ffi::c_void,
) -> Status {
    if loader.side != Side::CLIENT {
        return Status::MISSING;
    }
    if FRAMES.get().is_some() {
        return Status::TOO_LATE;
    }
    PENDING
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

/// Queues a mod's dialog; client only.
pub(crate) fn mod_dialog(
    loader: &Loader,
    owner: usize,
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
        // SAFETY: the texts are borrowed for the call.
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
    let frames = crate::host::take(&PENDING);
    let count = frames.len();
    let _sealed = FRAMES.set(frames.into_boxed_slice());
    if loader.side != Side::CLIENT {
        return;
    }
    let engine = match Engine::resolve(loader) {
        Ok(engine) => engine,
        Err(missing) => {
            crate::log!(WARN, "interface off: {missing}");
            return;
        }
    };
    let popup = engine.popup;
    let _first = ENGINE.set(engine);
    if loader.link_feature(
        "interface",
        Some(popup),
        &NEXT,
        detour as PopupFn,
        HookFlags::NONE,
    ) {
        crate::log!(INFO, "interface: {count} frame callbacks from mods");
    }
}

/// After arming: dialogs show in game when the popup is wired, else as
/// system message boxes.
pub(crate) fn armed(loader: &Loader) {
    if loader.side == Side::CLIENT {
        let in_game = NEXT.wired().is_some();
        if !in_game {
            crate::log!(WARN, "interface not armed: dialogs show as system windows");
        }
        dialog::armed(in_game);
    }
}

unsafe extern "system" fn detour(state: *mut u8) {
    let Some(engine) = ENGINE.get() else {
        // SAFETY: forwarded unchanged.
        unsafe { NEXT.get()(state) };
        return;
    };
    let context = engine.context();
    let nav = if context == 0 {
        0
    } else {
        Engine::nav(context)
    };
    // SAFETY: forwarded unchanged.
    unsafe { NEXT.get()(state) };
    if context == 0 || state.is_null() {
        return;
    }
    Engine::restore_nav(context, nav);
    // SAFETY: the game's popup state, its active flag.
    let vanilla = unsafe { crate::world::read::<u8>(state.addr(), engine::POPUP_ACTIVE) } != 0;
    if vanilla {
        return;
    }
    let drawn = std::panic::catch_unwind(|| {
        UI_THREAD.store(thread_id(), Ordering::Release);
        ARENA.with_borrow_mut(Arena::next_frame);
        frames(engine, context);
        // SAFETY: inside the frame, on its thread, no popup of the game's
        // showing; the slot is the popup's chain.
        unsafe { dialog::draw(NEXT.get(), context) };
    });
    if drawn.is_err() {
        crate::log!(ERROR, "interface: the loader panicked while drawing");
    }
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
    let Some(frames) = FRAMES.get() else {
        return;
    };
    let screen = screen(Engine::app_state(context));
    for registered in frames.iter().filter(|r| r.live()) {
        let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
        DRAWING.set(Some(Drawing {
            token,
            owner: registered.owner,
            context,
            window_open: false,
        }));
        let frame = UiFrame {
            header: enshwick_loader_api::abi::Header::of::<UiFrame>(),
            token,
            screen,
            reserved: 0,
        };
        // SAFETY: the mod's callback with its own pointer; it does not unwind.
        unsafe { (registered.callback)(&raw const frame, registered.user()) };
        let left = DRAWING.take();
        if let Some(drawing) = left
            && drawing.window_open
        {
            // SAFETY: the window the callback started in this frame.
            WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
            crate::log!(DEBUG, "interface: a frame callback left its window open");
        }
    }
}

/// Draws a mod's widget in its frame callback.
pub(crate) fn widget(loader: &Loader, owner: usize, token: u64, raw: &mut UiWidget) -> Status {
    let Some(engine) = ENGINE.get() else {
        return Status::MISSING;
    };
    let Some(mut drawing) = DRAWING.get() else {
        return Status::WRONG_THREAD;
    };
    if drawing.token != token || drawing.owner != owner {
        return Status::WRONG_THREAD;
    }
    raw.result = 0;
    let status = draw(loader, engine, &mut drawing, raw);
    DRAWING.set(Some(drawing));
    status
}

fn read(value: Str) -> Option<&'static [u8]> {
    // SAFETY: the text is borrowed for the call, and copied before it ends.
    unsafe { value.as_str() }.map(str::as_bytes)
}

fn copy(parts: &[&[u8]]) -> UiText {
    ARENA.with_borrow_mut(|arena| arena.text(parts))
}

fn draw(loader: &Loader, engine: &Engine, drawing: &mut Drawing, raw: &mut UiWidget) -> Status {
    let context = drawing.context;
    let Some(text) = read(raw.text) else {
        return Status::BAD_ARGS;
    };
    let read_only = raw.flags & UiWidget::READ_ONLY != 0;
    match raw.kind {
        UiWidget::WINDOW => {
            if drawing.window_open {
                return Status::REFUSED;
            }
            let (Some(record), Some(title)) = (loader.mods.get(drawing.owner), read(raw.detail))
            else {
                return Status::BAD_ARGS;
            };
            let name = copy(&[b"enshwick/", record.id.as_bytes(), b"/", text]);
            let title = copy(&[title]);
            // SAFETY: inside the frame, on its thread; the window object
            // stays in the thread's storage until it ends.
            WINDOW.with_borrow_mut(|window| unsafe {
                engine.window_begin(context, window, &name, &title);
            });
            drawing.window_open = true;
            Status::OK
        }
        UiWidget::END => {
            if !drawing.window_open {
                return Status::REFUSED;
            }
            // SAFETY: the window this callback started.
            WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
            drawing.window_open = false;
            Status::OK
        }
        UiWidget::SOUND => {
            // SAFETY: inside the frame, on its thread.
            unsafe { engine.sound(context, raw.style) };
            Status::OK
        }
        _ if !drawing.window_open => Status::REFUSED,
        UiWidget::LABEL => {
            let text = copy(&[text]);
            // SAFETY: inside a window; the text lives in the arena.
            unsafe {
                engine.label(
                    context,
                    &text,
                    raw.style,
                    raw.flags & UiWidget::WRAP != 0,
                    raw.flags & UiWidget::CENTER != 0,
                );
            }
            Status::OK
        }
        UiWidget::BUTTON => {
            let text = copy(&[text]);
            // SAFETY: as above.
            raw.result = u32::from(unsafe { engine.button(&text, !read_only) });
            Status::OK
        }
        UiWidget::TOGGLE | UiWidget::SLIDER | UiWidget::DROPDOWN => {
            row(engine, context, copy(&[text]), raw, read_only)
        }
        UiWidget::PROGRESS => {
            if !(raw.min.is_finite() && raw.max.is_finite() && raw.value.is_finite())
                || raw.max <= raw.min
            {
                return Status::BAD_ARGS;
            }
            // SAFETY: inside a window, on the frame's thread.
            unsafe { engine.progress(raw.value.clamp(raw.min, raw.max), raw.min, raw.max) };
            Status::OK
        }
        _ => Status::MISSING,
    }
}

/// Draws a settings row, writing back the player's change.
fn row(
    engine: &Engine,
    context: usize,
    label: UiText,
    raw: &mut UiWidget,
    read_only: bool,
) -> Status {
    match raw.kind {
        UiWidget::TOGGLE => {
            // SAFETY: inside a window, on the frame's thread; the label lives
            // in the arena.
            let changed = unsafe { engine.toggle(context, &label, raw.index != 0, read_only) };
            if let Some(on) = changed {
                raw.index = u32::from(on);
                raw.result = 1;
            }
        }
        UiWidget::SLIDER => {
            if !(raw.min.is_finite() && raw.max.is_finite() && raw.value.is_finite()) {
                return Status::BAD_ARGS;
            }
            let range = (raw.min.min(raw.max), raw.max.max(raw.min));
            let value = raw.value.clamp(range.0, range.1);
            // SAFETY: as above.
            let changed = unsafe { engine.slider(&label, value, range, raw.steps, read_only) };
            if let Some(value) = changed {
                raw.value = value;
                raw.result = 1;
            }
        }
        _ => {
            let count = (raw.option_count as usize).min(enshwick_loader_api::ui::MAX_OPTIONS);
            if raw.options.is_null() || count == 0 {
                return Status::BAD_ARGS;
            }
            // SAFETY: the mod passes `option_count` texts, valid for the call.
            let given = unsafe { core::slice::from_raw_parts(raw.options, count) };
            let mut options = [UiText::EMPTY; enshwick_loader_api::ui::MAX_OPTIONS];
            for (slot, option) in options.iter_mut().zip(given) {
                *slot = read(*option).map_or(UiText::EMPTY, |bytes| copy(&[bytes]));
            }
            // SAFETY: as above; the options live in the arena too.
            let changed =
                unsafe { engine.dropdown(&label, &options[..count], raw.index, read_only) };
            if let Some(index) = changed {
                raw.index = index;
                raw.result = 1;
            }
        }
    }
    Status::OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arena_keeps_texts_until_its_buffer_comes_round() {
        let mut arena = Arena::new();
        arena.next_frame();
        let first = arena.text(&[b"enshwick/", b"probe"]);
        assert_eq!(first.len, 14);
        // SAFETY: the text points into the arena's current buffer.
        let bytes = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
        assert_eq!(bytes, b"enshwick/probe");
        let big = vec![b'x'; ARENA_BYTES];
        assert_eq!(arena.text(&[&big]).len, 0);
        arena.next_frame();
        assert_eq!(arena.text(&[b"next"]).len, 4);
        // SAFETY: two frames on, the first buffer is still untouched.
        let again = unsafe { core::slice::from_raw_parts(first.ptr, 14) };
        assert_eq!(again, b"enshwick/probe");
    }

    #[test]
    fn app_states_map_to_screens() {
        assert_eq!(screen(7), UiFrame::IN_GAME);
        assert_eq!(screen(6), UiFrame::LOADING);
        assert_eq!(screen(1), UiFrame::MENU);
        assert_eq!(screen(0), UiFrame::MENU);
    }
}
