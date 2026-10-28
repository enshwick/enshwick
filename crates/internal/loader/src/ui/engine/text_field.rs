//! The game's text field, as its rename dialogs and its chat input draw it:
//! a label that takes typed text into a buffer its caller owns.
//!
//! Engine facts from client 426, both callers agreeing and a probe with a
//! 64-byte buffer live: the field edits the caller's text as UTF-8 up to the
//! buffer's capacity, cutting what does not fit; Enter sets the input's
//! answer to 1; its kept state says whether it has the caret.

use super::{Buf, Engine, Loader, Missing, NodeFloatFn, OneFn, Ptr, UiText, game_fn, stretch};
use crate::configure::rows::{ENTRY_BYTES, typed};
use crate::pinned;

/// The text a field edits: the game's growable byte slice, here over the
/// loader's fixed buffer and never grown.
#[repr(C)]
struct GameText {
    data: *mut u8,
    len: u64,
    capacity: u64,
    /// The function that grows `data`; none, so the field cuts instead.
    grow: usize,
}

const _: () = assert!(size_of::<GameText>() == 32);

type TextFieldFn = unsafe extern "system" fn(
    field: Ptr,
    text: *mut GameText,
    style: usize,
    wrap: u8,
    flag: u8,
) -> Ptr;
type StateFn = unsafe extern "system" fn(field: Ptr, key: u64, count: u32) -> *mut u8;
type InputFn = unsafe extern "system" fn(field: Ptr, out: *mut u64, hint: *const UiText, flag: u8);
type HitFn = unsafe extern "system" fn(frame: Ptr, out: *mut [u8; 8], mode: u32, a: u32, b: u8);

/// The field object, a label with the field's parts after it.
const FIELD_SIZE: usize = 0x280;
/// The field's pointer to the [`GameText`] it edits, which the constructor
/// writes and the input and the drawing read and write through.
const FIELD_TEXT: usize = 0x150;
/// The game's key for a field's kept state among its node's states.
const STATE_KEY: u64 = 0x64d2_7908_60d9_c7f6;
/// The kept state's size.
const STATE_SIZE: usize = 0x1990;
/// The kept state's byte that is set while the field has the caret; the
/// rename dialog clears it on Back to end the typing.
const STATE_EDITING: usize = 0x1988;

const _: () = assert!(FIELD_TEXT + 8 <= FIELD_SIZE);
const _: () = assert!(STATE_EDITING < STATE_SIZE);
/// The input's answer for Enter.
const ENTERED: u64 = 1;
/// The hit test's first byte for a click.
const CLICKED: u8 = 4;
/// The field's least height, as the rename dialog's.
const FIELD_HEIGHT: f32 = 40.0;

/// The text field's functions.
#[derive(Debug)]
pub(super) struct TextFields {
    field: TextFieldFn,
    state: StateFn,
    input: InputFn,
    focus: OneFn,
    hit: HitFn,
    min_height: NodeFloatFn,
}

impl TextFields {
    /// Resolves every function of the text field.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        macro_rules! function {
            ($name:expr, $ty:ty) => {
                // SAFETY: `$ty` is the signature of the game function `$name`
                // names, from the rename dialog's and the chat's calls on
                // client 426.
                unsafe { game_fn::<$ty>(loader, $name) }?
            };
        }
        Ok(Self {
            field: function!(pinned::ui::TEXT_FIELD, TextFieldFn),
            state: function!(pinned::ui::TEXT_FIELD_STATE, StateFn),
            input: function!(pinned::ui::TEXT_FIELD_INPUT, InputFn),
            focus: function!(pinned::ui::FOCUS_REQUEST, OneFn),
            hit: function!(pinned::ui::FRAME_HIT, HitFn),
            min_height: function!(pinned::ui::NODE_MIN_HEIGHT, NodeFloatFn),
        })
    }
}

/// The loader's buffer a text field edits, kept in place while its row
/// shows.
pub(in crate::ui) struct TextBuffer {
    text: GameText,
    bytes: [u8; ENTRY_BYTES],
}

impl TextBuffer {
    /// An empty buffer, boxed so it stays in place.
    pub(in crate::ui) fn new() -> Box<Self> {
        Box::new(Self {
            text: GameText {
                data: core::ptr::null_mut(),
                len: 0,
                capacity: 0,
                grow: 0,
            },
            bytes: [0; ENTRY_BYTES],
        })
    }

    /// The text the buffer holds.
    pub(in crate::ui) fn text(&self) -> &str {
        typed(&self.bytes, usize::try_from(self.text.len).unwrap_or(0))
    }

    /// Puts `text` into the buffer, cut to it on a whole character.
    pub(in crate::ui) fn set(&mut self, text: &str) {
        let kept = typed(text.as_bytes(), ENTRY_BYTES);
        self.bytes[..kept.len()].copy_from_slice(kept.as_bytes());
        self.text.len = kept.len() as u64;
    }
}

/// What the player did in a text field this frame.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct Typed {
    /// Pressed Enter: the buffer holds the text to take.
    pub(in crate::ui) entered: bool,
    /// The field has the caret.
    pub(in crate::ui) editing: bool,
}

impl Engine {
    /// Whether this build draws text fields; without them texts show
    /// read-only.
    pub(in crate::ui) const fn types_text(&self) -> bool {
        self.text_fields.is_some()
    }

    /// Draws a text field over `buffer`, as wide as its column, with `hint`
    /// in it while it is empty; what the player did, or `None` on a build
    /// without text fields. With `ending`, a field that has the caret gives
    /// it up, as the rename dialog's does on Back.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `context` is the non-zero context [`Engine::context`] read in this
    ///   frame.
    /// - The bytes `hint` points at outlive the frame.
    pub(in crate::ui) unsafe fn text_field(
        &self,
        context: usize,
        buffer: &mut TextBuffer,
        hint: &UiText,
        ending: bool,
    ) -> Option<Typed> {
        let fns = self.text_fields.as_ref()?;
        let bundle = self.bundle(context);
        buffer.text.data = buffer.bytes.as_mut_ptr();
        buffer.text.capacity = ENTRY_BYTES as u64;
        buffer.text.grow = 0;
        let mut field = Buf::<FIELD_SIZE>::zeroed();
        // SAFETY: `fns.field` is the game's `ui.text_field`, checked in
        // `TextFields::resolve`, inside the window this function's `# Safety`
        // gives; `field` holds the `FIELD_SIZE` bytes the field is. The
        // constructor stores the `GameText` pointer at `FIELD_TEXT`, and
        // `ui.text_field_input` and `ui.label_draw` below read and write the
        // text through it: `buffer` stays exclusively borrowed and unmoved
        // until after `label_draw`, its data points at its own
        // `ENTRY_BYTES` bytes with that capacity, and `grow` is 0, so the
        // game cuts at the capacity and never reallocates or frees the
        // bytes. The style is the reflected `defaultStyle` inside the
        // frame's look bundle, as both of the game's callers pass.
        unsafe {
            (fns.field)(
                field.ptr(),
                &raw mut buffer.text,
                bundle + self.looks.styles[0],
                1,
                0,
            );
        }
        field.u8(self.layout.label_shown, 1);
        let node = self.frame_node(&field);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the game just wrote into
        // the field.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        // SAFETY: `fns.min_height` is the game's `ui.node_min_height`,
        // checked in `TextFields::resolve`, on the field's node.
        unsafe { (fns.min_height)(node, FIELD_HEIGHT) };
        let mut pointer = [0u8; 8];
        // SAFETY: `fns.hit` is the game's `ui.frame_hit`, checked in
        // `TextFields::resolve`, on the field built above; it writes eight
        // bytes to the live `pointer`.
        unsafe { (fns.hit)(field.ptr(), &raw mut pointer, 1, 0, 0) };
        if pointer[0] == CLICKED && !ending {
            // SAFETY: `fns.focus` is the game's `ui.focus_request`, checked in
            // `TextFields::resolve`, on the field built above.
            unsafe { (fns.focus)(field.ptr()) };
        }
        let mut answer = 0u64;
        // SAFETY: `fns.input` is the game's `ui.text_field_input`, checked
        // in `TextFields::resolve`, on the field built above; it writes the
        // typed text through the `GameText` at `FIELD_TEXT`, which points at
        // `buffer`, exclusively borrowed and unmoved for this call, at most
        // `ENTRY_BYTES` bytes with no grow function, and its answer to the
        // live `answer`; `hint` outlives the frame by this function's
        // `# Safety`.
        unsafe { (fns.input)(field.ptr(), &raw mut answer, hint, 0) };
        // SAFETY: `fns.state` is the game's `ui.text_field_state`, checked in
        // `TextFields::resolve`, on the field built above, with the game's
        // own key for the field's state.
        let state = unsafe { (fns.state)(field.ptr(), STATE_KEY, 1) };
        let state = state.expose_provenance();
        let mut editing = state != 0
            // SAFETY: `state` is non-zero, checked first, and points at the
            // field's kept state of `STATE_SIZE` bytes on client 426, whose
            // editing byte sits at `STATE_EDITING`.
            && unsafe { crate::memory::read::<u8>(state, STATE_EDITING) } != 0;
        if editing && ending {
            // SAFETY: `state` is non-zero, checked by `editing`, and points at
            // the field's kept state of `STATE_SIZE` bytes, which the game
            // touches only on this thread while it draws; clearing its
            // editing byte ends the typing, as the rename dialog does on Back.
            unsafe { crate::memory::write::<u8>(state, STATE_EDITING, 0) };
            editing = false;
        }
        buffer.text.len = buffer.text.len.min(ENTRY_BYTES as u64);
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, checked in
        // `Engine::resolve`, on the field built above, which draws and closes
        // it as the game's callers do; it reads the text through the
        // `GameText` at `FIELD_TEXT`, which points at `buffer`, still
        // exclusively borrowed and unmoved, its length within its capacity.
        unsafe { (self.label_draw)(field.ptr()) };
        Some(Typed {
            entered: answer & u64::from(u32::MAX) == ENTERED,
            editing,
        })
    }
}
