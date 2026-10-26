use enshwick::abi::{Status, Str, UiWidget};

use crate::host::Loader;
use crate::mod_index::ModIndex;

use super::engine::{Engine, UiText};
use super::{ARENA, DRAWING, WINDOW};
use super::{Drawing, ENGINE, gather};

/// Draws a mod's widget in its frame callback.
pub(crate) fn widget(loader: &Loader, owner: ModIndex, token: u64, raw: &mut UiWidget) -> Status {
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
    if drawing.hud {
        return gather(raw);
    }
    let status = draw(loader, engine, &mut drawing, raw);
    DRAWING.set(Some(drawing));
    status
}

/// The loader's own drawing inside a window it opened: the engine's widgets
/// with texts copied into the frame's arena, safe to call once made.
pub(crate) struct Painter<'a> {
    pub(super) engine: &'a Engine,
    pub(super) context: usize,
}

impl<'a> Painter<'a> {
    /// # Safety
    ///
    /// - A window is open in the frame being drawn on this thread: one
    ///   [`Engine::window_begin`] started, or the game's own window a hook
    ///   of the loader draws inside.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The painter is used only on this thread and only until that window
    ///   ends.
    pub(super) unsafe fn new(engine: &'a Engine, context: usize) -> Self {
        Self { engine, context }
    }

    pub(super) fn label(&self, text: &[u8], style: u32, wrap: bool, center: bool) {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the text lives in the frame's
        // arena.
        unsafe { self.engine.label(self.context, &text, style, wrap, center) };
    }

    pub(super) fn button(&self, text: &[u8], enabled: bool) -> bool {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the text lives in the frame's arena.
        unsafe { self.engine.button(&text, enabled) }
    }

    pub(super) fn toggle(&self, label: &[u8], on: bool, read_only: bool) -> Option<bool> {
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the label lives in the frame's
        // arena.
        unsafe { self.engine.toggle(self.context, &label, on, read_only) }
    }

    pub(super) fn slider(
        &self,
        label: &[u8],
        value: f32,
        range: (f32, f32),
        steps: u32,
        read_only: bool,
    ) -> Option<f32> {
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the label lives in the frame's arena.
        unsafe {
            self.engine
                .slider(&label, value, range, steps, read_only, false)
        }
    }

    /// A settings slider that takes its value when the player lets go.
    pub(super) fn setting_slider(
        &self,
        label: &[u8],
        value: f32,
        range: (f32, f32),
        steps: u32,
        read_only: bool,
    ) -> Option<f32> {
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the label lives in the frame's arena.
        unsafe {
            self.engine
                .slider(&label, value, range, steps, read_only, true)
        }
    }

    pub(super) fn dropdown(
        &self,
        label: &[u8],
        options: &[&[u8]],
        index: u32,
        read_only: bool,
    ) -> Option<u32> {
        let mut texts = [UiText::EMPTY; enshwick::ui::MAX_OPTIONS];
        for (slot, option) in texts.iter_mut().zip(options) {
            *slot = copy(&[option]);
        }
        let count = options.len().min(texts.len());
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the label and options live in the frame's arena.
        unsafe {
            self.engine
                .dropdown(&label, &texts[..count], index, read_only)
        }
    }

    pub(super) fn progress(&self, value: f32, min: f32, max: f32) {
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`.
        unsafe { self.engine.progress(value, min, max) };
    }

    pub(super) fn sound(&self, sound: u32) {
        // SAFETY: a window is open in this frame on this thread with
        // `self.context`, by the contract of `Painter::new`.
        unsafe { self.engine.sound(self.context, sound) };
    }
}

pub(super) fn read(value: Str) -> Option<&'static [u8]> {
    // SAFETY: a mod's widget texts stay alive and unchanged for the call by
    // the ABI's borrow rule; every caller copies the bytes into the arena
    // before the call returns and keeps no `'static` borrow.
    unsafe { value.as_str() }.map(str::as_bytes)
}

pub(super) fn copy(parts: &[&[u8]]) -> UiText {
    ARENA.with_borrow_mut(|arena| arena.text(parts))
}

pub(super) fn draw(
    loader: &Loader,
    engine: &Engine,
    drawing: &mut Drawing,
    raw: &mut UiWidget,
) -> Status {
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
            let (Some(record), Some(title)) =
                (loader.mods.get(drawing.owner.get()), read(raw.detail))
            else {
                return Status::BAD_ARGS;
            };
            let name = copy(&[b"enshwick/", record.id.as_bytes(), b"/", text]);
            let title = copy(&[title]);
            // SAFETY: `widget` found this thread's `DRAWING`, set only while
            // a frame callback runs inside the frame on the interface thread,
            // with its non-zero `context`; the window object stays in the
            // thread-local `WINDOW` until it ends, and `name` and `title` live
            // in the frame's arena.
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
            // SAFETY: `window_open`, checked by the guard before it, is set
            // only when this callback's `WINDOW` widget started the
            // thread-local window in this frame with this `context`; it stayed
            // in place, and widgets close every frame they open.
            WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
            drawing.window_open = false;
            Status::OK
        }
        UiWidget::SOUND => {
            // SAFETY: `widget` found this thread's `DRAWING`, set only while
            // a frame callback runs inside the frame on the interface thread,
            // with its non-zero `context`.
            unsafe { engine.sound(context, raw.style) };
            Status::OK
        }
        _ if !drawing.window_open => Status::REFUSED,
        UiWidget::LABEL => {
            let text = copy(&[text]);
            // SAFETY: `window_open`, checked by the guard before it, means this
            // callback's window is open in this frame on this thread with its
            // non-zero `context`; the text lives in the frame's arena.
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
            // SAFETY: `window_open`, checked by the guard before it, means this
            // callback's window is open in this frame on this thread; the text
            // lives in the frame's arena.
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
            // SAFETY: `window_open`, checked by the guard before it, means this
            // callback's window is open in this frame on this thread.
            unsafe { engine.progress(raw.value.clamp(raw.min, raw.max), raw.min, raw.max) };
            Status::OK
        }
        _ => Status::MISSING,
    }
}

/// Draws a settings row, writing back the player's change.
pub(super) fn row(
    engine: &Engine,
    context: usize,
    label: UiText,
    raw: &mut UiWidget,
    read_only: bool,
) -> Status {
    match raw.kind {
        UiWidget::TOGGLE => {
            // SAFETY: `draw` calls this only with this callback's window open
            // in this frame on this thread, with its non-zero `context`; the
            // label lives in the frame's arena.
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
            // SAFETY: `draw` calls this only with this callback's window open
            // in this frame on this thread; the label lives in the frame's
            // arena.
            let changed =
                unsafe { engine.slider(&label, value, range, raw.steps, read_only, false) };
            if let Some(value) = changed {
                raw.value = value;
                raw.result = 1;
            }
        }
        _ => {
            let count = (raw.option_count as usize).min(enshwick::ui::MAX_OPTIONS);
            if raw.options.is_null() || count == 0 {
                return Status::BAD_ARGS;
            }
            // SAFETY: `raw.options` is non-null, checked by the guard before
            // it, and the mod's `UiWidget` holds `option_count` aligned `Str`s
            // there for the call by the ABI's borrow rule; `count` is at most
            // that.
            let given = unsafe { core::slice::from_raw_parts(raw.options, count) };
            let mut options = [UiText::EMPTY; enshwick::ui::MAX_OPTIONS];
            for (slot, option) in options.iter_mut().zip(given) {
                *slot = read(*option).map_or(UiText::EMPTY, |bytes| copy(&[bytes]));
            }
            // SAFETY: `draw` calls this only with this callback's window open
            // in this frame on this thread; the label and options live in the
            // frame's arena.
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
