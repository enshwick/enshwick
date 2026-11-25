use core::cell::{Cell, RefCell};

use enshwick_core::abi::{Str, UiWidget};

use crate::ui::widget_checks::WidgetError;
use enshwick_core::ui::MAX_OPTIONS;

use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::ui::widget_checks;

use super::engine::{Engine, Icon, Row, SlotRow, Takes, TextBuffer, Typed, UiText};
use super::{ARENA, DRAWING, WINDOW};
use super::{Drawing, ENGINE, gather};

/// Draws a mod's widget in its frame callback.
pub(crate) fn widget(
    loader: &Loader,
    owner: ModIndex,
    token: u64,
    raw: &mut UiWidget,
) -> Result<(), WidgetError> {
    let Some(engine) = ENGINE.get() else {
        return Err(WidgetError::NoEngine);
    };
    if let Some(gathered) = super::tooltips::gather(owner, token, raw) {
        return gathered;
    }
    let Some(mut drawing) = DRAWING.get() else {
        return Err(WidgetError::WrongThread);
    };
    if drawing.token != token || drawing.owner != owner {
        return Err(WidgetError::WrongThread);
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
    /// The window's list is inside a scroll area this frame.
    scrolled: Cell<bool>,
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
        Self {
            engine,
            context,
            scrolled: Cell::new(false),
        }
    }

    /// A list entry's name, above the entry's text.
    pub(super) fn name(&self, text: &[u8]) {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the text lives in the frame's
        // arena.
        unsafe { self.engine.name(self.context, &text) };
    }

    pub(super) fn button_compact(&self, text: &[u8], enabled: bool) -> bool {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the text lives in the frame's arena.
        unsafe { self.engine.button_compact(&text, enabled) }
    }

    /// A small button showing the game's `icon`, or a compact button with
    /// the text `fallback` on a build whose icons do not read.
    pub(super) fn icon_button(&self, icon: Icon, fallback: &[u8], enabled: bool) -> bool {
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`.
        unsafe { self.engine.icon_button(self.context, icon, enabled) }
            .unwrap_or_else(|| self.button_compact(fallback, enabled))
    }

    /// Draws what `draw` draws in a frame at the right of the column; with
    /// what it gives, the height the game laid it out at in the last frame,
    /// for a [`Painter::button_slot`] of the same height.
    pub(super) fn measured<R>(&self, draw: impl FnOnce() -> R) -> (R, f32) {
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`.
        unsafe { self.engine.measured(draw) }
    }

    /// The free place, `height` high, of a button that is not there.
    pub(super) fn button_slot(&self, height: f32) {
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`.
        unsafe { self.engine.button_slot(height) };
    }

    /// The game's label of `action` in the player's language; `None` on a
    /// build without the game's labels.
    pub(super) fn action_label(
        &self,
        action: crate::actions::game_bindings::ActionNumber,
    ) -> Option<String> {
        // SAFETY: the painter draws inside the frame on this thread, by the
        // contract of `Painter::new`, so the UI context global holds its live
        // context.
        unsafe { self.engine.action_label(action) }
    }

    /// A settings slider that takes its value when the player lets go; one
    /// of a `whole` setting shows its value without decimals.
    pub(super) fn setting_slider(
        &self,
        label: &[u8],
        value: f32,
        range: (f32, f32),
        steps: u32,
        read_only: bool,
        whole: bool,
    ) -> Option<f32> {
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the label lives in the frame's arena.
        unsafe {
            self.engine.slider(
                &label,
                value,
                range,
                steps,
                read_only,
                Takes::OnRelease { whole },
            )
        }
    }

    /// Draws a list entry as a row: what `about` draws at the left, what
    /// `side` draws at its right, centred beside it; `side`'s answer. On a
    /// build without rows the two stack.
    pub(super) fn row<R>(&self, about: impl FnOnce(), side: impl FnOnce() -> R) -> R {
        let mut row = Row::new();
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; `row` stays in place on this stack until
        // `row_end` below, which follows `row_side` whenever this opens.
        let open = unsafe { self.engine.row_begin(&mut row, self.scrolled()) };
        about();
        if open {
            // SAFETY: `row_begin` returned true for `row` in this frame, on
            // this thread; the painter's widgets close every frame they
            // open, so nothing `about` drew is open.
            unsafe { self.engine.row_side(&mut row) };
        }
        let done = side();
        if open {
            // SAFETY: `row_side` ran for `row` above; the painter's widgets
            // close every frame they open, so nothing `side` drew is open.
            unsafe { self.engine.row_end(&mut row) };
        }
        done
    }

    /// The thin line between the entries of a list.
    pub(super) fn separator(&self) {
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`.
        unsafe { self.engine.separator() };
    }

    /// Whether the list drawn now is inside a scroll area.
    pub(super) fn scrolled(&self) -> bool {
        self.scrolled.get()
    }

    /// Notes whether the list drawn now is inside a scroll area.
    pub(super) fn set_scrolled(&self, scrolled: bool) {
        self.scrolled.set(scrolled);
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

/// The widgets the loader's own windows draw, so their drawing runs on the
/// game's engine or, in a test, on a recorder of what it drew.
pub(super) trait Widgets {
    /// A text in `style`, wrapped and centred as asked.
    fn label(&self, text: &[u8], style: u32, wrap: bool, center: bool);

    /// A button; whether the player pressed it this frame.
    fn button(&self, text: &[u8], enabled: bool) -> bool;

    /// A switch showing `on`; its new state when the player flipped it.
    fn toggle(&self, label: &[u8], on: bool, read_only: bool) -> Option<bool>;

    /// A slider over `range` in `steps`, 0 for any value; its new value
    /// while the player moves it.
    fn slider(
        &self,
        label: &[u8],
        value: f32,
        range: (f32, f32),
        steps: u32,
        read_only: bool,
    ) -> Option<f32>;

    /// A dropdown of at most [`MAX_OPTIONS`] `options` with `index`
    /// picked, past them for none; the row the player picked.
    fn dropdown(&self, label: &[u8], options: &[&[u8]], index: u32, read_only: bool)
    -> Option<u32>;

    /// A text field over `buffer` with `hint` in it while it is empty,
    /// which gives up the caret when `ending`; `None` on a build without
    /// text fields.
    fn text_field(&self, buffer: &mut TextBuffer, hint: &[u8], ending: bool) -> Option<Typed>;
}

impl Widgets for Painter<'_> {
    fn label(&self, text: &[u8], style: u32, wrap: bool, center: bool) {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the text lives in the frame's
        // arena.
        unsafe { self.engine.label(self.context, &text, style, wrap, center) };
    }

    fn button(&self, text: &[u8], enabled: bool) -> bool {
        let text = copy(&[text]);
        // SAFETY: a window is open on this thread, by the contract of
        // `Painter::new`; the text lives in the frame's arena.
        unsafe { self.engine.button(&text, enabled) }
    }

    fn toggle(&self, label: &[u8], on: bool, read_only: bool) -> Option<bool> {
        let label = copy(&[label]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the label lives in the frame's
        // arena.
        unsafe { self.engine.toggle(self.context, &label, on, read_only) }
    }

    fn slider(
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
                .slider(&label, value, range, steps, read_only, Takes::Live)
        }
    }

    fn dropdown(
        &self,
        label: &[u8],
        options: &[&[u8]],
        index: u32,
        read_only: bool,
    ) -> Option<u32> {
        let mut texts = [UiText::EMPTY; MAX_OPTIONS];
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

    fn text_field(&self, buffer: &mut TextBuffer, hint: &[u8], ending: bool) -> Option<Typed> {
        let hint = copy(&[hint]);
        // SAFETY: a window is open on this thread with `self.context`, by
        // the contract of `Painter::new`; the hint lives in the frame's
        // arena.
        unsafe { self.engine.text_field(self.context, buffer, &hint, ending) }
    }
}

/// A dropdown's options on the stack: the bytes of the first
/// [`MAX_OPTIONS`] of `texts`, and how many there are.
pub(super) fn options<'a>(
    texts: impl IntoIterator<Item = &'a str>,
) -> ([&'a [u8]; MAX_OPTIONS], usize) {
    let mut options: [&[u8]; MAX_OPTIONS] = [&[]; MAX_OPTIONS];
    let mut count = 0;
    for (slot, text) in options.iter_mut().zip(texts) {
        *slot = text.as_bytes();
        count += 1;
    }
    (options, count)
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

thread_local! {
    /// The row of item slots the frame callback drawing now has open.
    static SLOTS: RefCell<SlotRow> = const { RefCell::new(SlotRow::new()) };
}

/// Ends the open row of a frame callback's item slots, if any: before its
/// next widget that is not a slot, and before its window or the callback
/// ends.
///
/// # Safety
///
/// - Called on the interface thread, in the frame and inside the window the
///   row's slots drew in, with every frame opened since its last slot
///   closed.
pub(super) unsafe fn end_slots(engine: &Engine) {
    // SAFETY: this function's `# Safety`; the row stayed in place in the
    // thread-local `SLOTS`.
    SLOTS.with_borrow_mut(|row| unsafe { engine.slot_row_end(row) });
}

/// Forgets a row of item slots a path left open in the last frame, so the
/// next frame never closes a frame it did not open.
pub(super) fn next_frame() {
    SLOTS.with_borrow_mut(SlotRow::forget);
}

pub(super) fn draw(
    loader: &Loader,
    engine: &Engine,
    drawing: &mut Drawing,
    raw: &mut UiWidget,
) -> Result<(), WidgetError> {
    let context = drawing.context;
    let Some(text) = read(raw.text) else {
        return Err(WidgetError::Malformed);
    };
    let read_only = raw.flags & UiWidget::READ_ONLY != 0;
    if let Some(window) = drawing.extension {
        if matches!(raw.kind, UiWidget::WINDOW | UiWidget::END) {
            return Err(WidgetError::OutOfOrder);
        }
        if !drawing.window_open && widget_checks::needs_window(raw.kind) {
            // SAFETY: `widget` found this thread's `DRAWING`, which
            // `extensions::run` sets only while it runs an extension's
            // callback inside the frame on the interface thread, with its
            // non-zero `context` and no window but the extensions' own open.
            drawing.window_open = unsafe { super::extensions::open(engine, context, window) };
        }
    }
    widget_checks::order(raw.kind, drawing.window_open)?;
    if !matches!(raw.kind, UiWidget::ITEM_SLOT | UiWidget::SOUND) {
        // SAFETY: `widget` found this thread's `DRAWING`, set only while a
        // frame callback runs inside the frame on the interface thread; a
        // row is open only in the window its slots drew in, and widgets close
        // every frame they open.
        unsafe { end_slots(engine) };
    }
    match raw.kind {
        UiWidget::WINDOW => {
            let (Some(record), Some(title)) =
                (loader.mods.get(drawing.owner.get()), read(raw.detail))
            else {
                return Err(WidgetError::Malformed);
            };
            let name = copy(&[b"enshwick/", record.id.as_bytes(), b"/", text]);
            let title = copy(&[title]);
            let back = WINDOW.with_borrow_mut(|window| {
                // SAFETY: `widget` found this thread's `DRAWING`, set only
                // while a frame callback runs inside the frame on the
                // interface thread, with its non-zero `context`; the window
                // object stays in the thread-local `WINDOW` until it ends, and
                // `name` and `title` live in the frame's arena.
                unsafe { engine.window_begin(context, window, &name, &title) };
                // SAFETY: in the same frame on the interface thread, of the
                // window begun one call earlier: Back binds to its own node,
                // since the slot is shared by every mod window of the frame.
                unsafe { super::dialog::back_over(engine, window) }
            });
            raw.result = u32::from(back);
            drawing.window_open = true;
            Ok(())
        }
        UiWidget::END => {
            // SAFETY: `window_open`, checked by `widget_checks::order`, is set
            // only when this callback's `WINDOW` widget started the
            // thread-local window in this frame with this `context`; it stayed
            // in place, and widgets close every frame they open.
            WINDOW.with_borrow_mut(|window| unsafe { engine.window_end(context, window) });
            drawing.window_open = false;
            Ok(())
        }
        UiWidget::SOUND if raw.sound == enshwick_core::content::Guid::NIL => {
            // SAFETY: `widget` found this thread's `DRAWING`, set only while
            // a frame callback runs inside the frame on the interface thread,
            // with its non-zero `context`.
            unsafe { engine.sound(context, raw.style) };
            Ok(())
        }
        UiWidget::SOUND => {
            if !crate::ui::sound::holds_sound(raw.sound.0) {
                return Err(WidgetError::Unavailable);
            }
            // SAFETY: `widget` found this thread's `DRAWING`, set only while
            // a frame callback runs inside the frame on the interface thread,
            // with its non-zero `context`.
            unsafe { engine.play(context, &raw.sound.0) };
            Ok(())
        }
        UiWidget::LABEL => {
            let text = copy(&[text]);
            // SAFETY: `window_open`, checked by `widget_checks::order`, means this
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
            Ok(())
        }
        UiWidget::BUTTON => {
            let text = copy(&[text]);
            // SAFETY: `window_open`, checked by `widget_checks::order`, means this
            // callback's window is open in this frame on this thread; the text
            // lives in the frame's arena.
            raw.result = u32::from(unsafe { engine.button(&text, !read_only) });
            Ok(())
        }
        UiWidget::TOGGLE | UiWidget::SLIDER | UiWidget::DROPDOWN => {
            row(engine, context, copy(&[text]), raw, read_only)
        }
        UiWidget::INPUT_HINT => input_hint(engine, text, raw),
        UiWidget::ITEM_SLOT => {
            let extension = drawing.extension.is_some();
            // SAFETY: `window_open`, checked by `widget_checks::order`, means
            // this callback's window, or its screen extension's, is open in
            // this frame on the interface thread; the row's slots drew only
            // in it, and widgets close every frame they open; the row stays in
            // the thread-local `SLOTS` until `end_slots` ends it in this frame,
            // before the window or the callback ends.
            SLOTS.with_borrow_mut(|row| unsafe { engine.slot_row_next(row, extension) });
            // SAFETY: `window_open`, checked by `widget_checks::order`, means
            // this callback's window, or its screen extension's, is open in
            // this frame on the interface thread, with its non-zero `context`.
            let drawn = unsafe { super::item_slots::draw(context, raw) };
            if drawn.is_err() {
                // SAFETY: in the same window and frame, right after the
                // slot's `slot_row_next`, with nothing opened since; `SLOTS`
                // kept the row in place.
                SLOTS.with_borrow_mut(|row| unsafe { engine.slot_row_retract(row) });
            }
            drawn
        }
        UiWidget::PROGRESS => {
            let Some(value) = widget_checks::progress(raw.value, raw.min, raw.max) else {
                return Err(WidgetError::Malformed);
            };
            // SAFETY: `window_open`, checked by `widget_checks::order`, means this
            // callback's window is open in this frame on this thread.
            unsafe { engine.progress(value, raw.min, raw.max) };
            Ok(())
        }
        _ => Err(WidgetError::Unknown),
    }
}

/// Draws the key bound to the game action named `action`, with the
/// widget's `detail` beside it; `result` 1 when the game drew a key.
fn input_hint(engine: &Engine, action: &[u8], raw: &mut UiWidget) -> Result<(), WidgetError> {
    let Some(hint) = &engine.hint else {
        return Err(WidgetError::Unavailable);
    };
    let (Ok(action), Some(label)) = (core::str::from_utf8(action), read(raw.detail)) else {
        return Err(WidgetError::Malformed);
    };
    let label = if label.is_empty() {
        UiText::EMPTY
    } else {
        copy(&[label])
    };
    // SAFETY: `draw` calls this only with `window_open`, checked by
    // `widget_checks::order`: this callback's window, or its screen
    // extension's, is open in this frame on the interface thread, so its
    // layout node is open; `label` is empty or its bytes live in the
    // frame's arena, kept to the end of the frame.
    match unsafe { hint.draw(action, &label) } {
        Some(drawn) => {
            raw.result = u32::from(drawn);
            Ok(())
        }
        None => Err(WidgetError::Unavailable),
    }
}

/// Draws a settings row, writing back the player's change.
pub(super) fn row(
    engine: &Engine,
    context: usize,
    label: UiText,
    raw: &mut UiWidget,
    read_only: bool,
) -> Result<(), WidgetError> {
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
            let Some((range, value)) = widget_checks::slider(raw.value, raw.min, raw.max) else {
                return Err(WidgetError::Malformed);
            };
            // SAFETY: `draw` calls this only with this callback's window open
            // in this frame on this thread; the label lives in the frame's
            // arena.
            let changed =
                unsafe { engine.slider(&label, value, range, raw.steps, read_only, Takes::Live) };
            if let Some(value) = changed {
                raw.value = value;
                raw.result = 1;
            }
        }
        _ => {
            let Some(count) = widget_checks::options(raw.option_count, !raw.options.is_null())
            else {
                return Err(WidgetError::Malformed);
            };
            // SAFETY: `raw.options` is non-null, checked by
            // `widget_checks::options`, and the mod's `UiWidget` holds `option_count` aligned `Str`s
            // there for the call by the ABI's borrow rule; `count` is at most
            // that.
            let given = unsafe { core::slice::from_raw_parts(raw.options, count) };
            let mut options = [UiText::EMPTY; MAX_OPTIONS];
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
    Ok(())
}
