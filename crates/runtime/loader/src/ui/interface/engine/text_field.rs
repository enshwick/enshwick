//! The game's text field, as its rename dialogs and its chat input draw it:
//! a label that takes typed text into a buffer its caller owns.
//!
//! Engine facts from client 426, both callers agreeing and a probe with a
//! 64-byte buffer live: the field edits the caller's text as UTF-8 up to the
//! buffer's capacity, cutting what does not fit; Enter sets the input's
//! answer to 1; its kept state says whether it has the caret.
//!
//! The kept state also holds the text the field edits: the field takes the
//! caller's text into it only at its first draw, and each input writes the
//! kept text back over the caller's, so a text the loader puts in the
//! buffer later reaches the field only through `ui.text_field_take`, which
//! takes the caller's text again (client 26430625, `0x140b232c0`,
//! `0x140d28050`, `0x140d60310`, decompiler).

use super::{
    Buf, Engine, Loader, Lookup, Missing, NodeFloatFn, OneFn, Ptr, UiText, field, game_fn, stretch,
};
use core::cell::Cell;

use crate::base::configure::rows::{ENTRY_BYTES, typed};
use crate::engine::fuse::Fuse;
use crate::engine::pinned;

/// A text the loader or a client command puts in a field that drew reaches
/// the game's field; off, only the player's typing and a field's first text
/// do, and a commanded text answers off.
static TAKE_FUSE: Fuse =
    Fuse::new("texts given to drawn text fields (only typing and a field's first text reach them)");

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

/// The loader's room for the field object, a label with the field's parts
/// after it; the build's `ui.text_field_extent` must fit in it.
pub(super) const FIELD_SIZE: usize = 0x280; // bound, not a game offset: the loader's room
/// Where the field's kept state holds what the loader reads and writes,
/// from the build's table, once the field object is known to fit.
#[derive(Debug, Clone, Copy)]
pub(super) struct TextLayout {
    /// The kept state's byte that is set while the field has the caret;
    /// the rename dialog clears it on Back to end the typing.
    editing: usize,
    /// The kept state's byte that starts the typing at the field's next
    /// input, as its navigation's confirm does; `None` where its route misses
    /// or points outside the state, and a click only asks for the focus.
    start: Option<usize>,
    /// The game's key for the field's kept state among its node's states,
    /// which changes with each build.
    key: u64,
    /// Where the field object keeps its look; `None` where the route
    /// misses or points outside the object, and the field draws bare.
    look: Option<usize>,
}

impl TextLayout {
    /// Reads the layout from the build's constants. A field object that
    /// would not fit [`FIELD_SIZE`], a pointer to the [`GameText`] it edits
    /// outside the object, or a caret byte outside the kept state is
    /// [`Missing::Room`], so the game never writes past the buffer.
    pub(super) fn read(lookup: Lookup<'_>) -> Result<Self, Missing> {
        let extent = field(lookup, pinned::ui::TEXT_FIELD_EXTENT.id())?;
        if extent > FIELD_SIZE {
            return Err(Missing::Room(pinned::ui::TEXT_FIELD_EXTENT.id()));
        }
        let text = field(lookup, pinned::ui::TEXT_FIELD_TEXT.id())?;
        if text
            .checked_add(size_of::<usize>())
            .is_none_or(|end| end > extent)
        {
            return Err(Missing::Room(pinned::ui::TEXT_FIELD_TEXT.id()));
        }
        let state = field(lookup, pinned::ui::TEXT_FIELD_STATE_SIZE.id())?;
        let editing = field(lookup, pinned::ui::TEXT_FIELD_EDITING.id())?;
        if editing >= state {
            return Err(Missing::Room(pinned::ui::TEXT_FIELD_EDITING.id()));
        }
        let start = field(lookup, pinned::ui::TEXT_FIELD_START.id())
            .ok()
            .filter(|&at| at < state);
        let key = pinned::ui::TEXT_FIELD_STATE_KEY.id();
        let key = lookup(key).ok_or(Missing::Entry(key))?;
        let look = field(lookup, pinned::ui::TEXT_FIELD_LOOK.id())
            .ok()
            .filter(|at| {
                at.checked_add(size_of::<u32>())
                    .is_some_and(|end| end <= extent)
            });
        Ok(Self {
            editing,
            start,
            key,
            look,
        })
    }
}
/// Whether a field keeps its typing into the next frame: a click starts it;
/// Enter, a click elsewhere or Back ends the typing the game ran (`editing`
/// clears).
const fn keeps_typing(clicked: bool, typing: bool, editing: bool) -> bool {
    clicked || (typing && editing)
}

/// The input's answer for Enter.
const ENTERED: u64 = 1;
/// The game's hit and press tests' first byte for a click, which the text
/// field and the item slot read (client 26430625, decompiler).
pub(in crate::ui::interface) const CLICKED: u8 = 4;
/// The field's least height, as the rename dialog's.
const FIELD_HEIGHT: f32 = 40.0;
/// The look the rename dialog gives its field at `ui.text_field_look`,
/// which the constructor zeroes: 0 draws the bare text, 2 the box with its
/// hover and caret colours (clients 426 and 26430625).
const LOOK_BOXED: u32 = 2;
/// The rename dialog's text alignment in its field: left and top, where a
/// label's default is the centre.
const ALIGN_LEFT: u16 = 0x101;

/// The text field's functions.
#[derive(Debug)]
pub(super) struct TextFields {
    field: TextFieldFn,
    state: StateFn,
    input: InputFn,
    focus: OneFn,
    hit: HitFn,
    min_height: NodeFloatFn,
    /// Takes the caller's text into the field's kept state; `None` on a
    /// build whose pattern misses, where only the player's typing and a
    /// text given before the field first draws reach it.
    take: Option<OneFn>,
    layout: TextLayout,
}

impl TextFields {
    /// Resolves every function of the text field and checks its sizes.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let layout = TextLayout::read(&|id: &str| loader.build.constant(id))?;
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
            // SAFETY: `OneFn` is the signature of `ui.text_field_take`: the
            // field object in, as its own body reads it (client 26430625).
            take: unsafe { game_fn::<OneFn>(loader, pinned::ui::TEXT_FIELD_TAKE) }
                .inspect_err(|why| {
                    let _off = loader.fuse_off(&TAKE_FUSE, why.id(), &why.reason());
                })
                .ok(),
            layout,
        })
    }
}

/// The loader's buffer a text field edits, kept in place while its row
/// shows.
pub(in crate::ui::interface) struct TextBuffer {
    text: GameText,
    bytes: [u8; ENTRY_BYTES],
    /// The player clicked the field and has not ended the typing yet.
    typing: bool,
    /// The typing ends at the field's next draw, as Back asked.
    end: bool,
    /// The loader put a text in since the field last drew, or the buffer is
    /// new: its next draw hands it to the game's kept state, which outlives
    /// the buffer of a field dropped and drawn again.
    given: bool,
}

impl TextBuffer {
    /// An empty buffer, boxed so it stays in place.
    pub(in crate::ui::interface) fn new() -> Box<Self> {
        Box::new(Self {
            text: GameText {
                data: core::ptr::null_mut(),
                len: 0,
                capacity: 0,
                grow: 0,
            },
            bytes: [0; ENTRY_BYTES],
            typing: false,
            end: false,
            given: true,
        })
    }

    /// Draws the field as the game does, for the test widgets: its `kept`
    /// text takes the buffer's at the first draw or once the loader gave
    /// one, then the player's `typed` text, and is written back over the
    /// buffer.
    #[cfg(test)]
    pub(in crate::ui::interface) fn draw_as_the_game(
        &mut self,
        kept: &mut Option<String>,
        typed: Option<&str>,
    ) {
        let given = core::mem::take(&mut self.given);
        let text = match (kept.take(), typed) {
            (_, Some(typed)) => typed.to_owned(),
            (Some(kept), None) if !given => kept,
            _ => self.text().to_owned(),
        };
        self.put(&text);
        *kept = Some(text);
    }

    /// Whether the player clicked the field and has not ended the typing,
    /// nor has Back asked to end it.
    pub(in crate::ui::interface) const fn typing(&self) -> bool {
        self.typing && !self.end
    }

    /// Stands in for the player's click into the field.
    #[cfg(test)]
    pub(in crate::ui::interface) const fn start_typing(&mut self) {
        self.typing = true;
    }

    /// Ends the typing at the field's next draw, for a window whose Back
    /// reaches it while the field has the caret.
    pub(in crate::ui::interface) const fn end_typing(&mut self) {
        self.end = true;
    }

    /// The text the buffer holds.
    pub(in crate::ui::interface) fn text(&self) -> &str {
        typed(&self.bytes, usize::try_from(self.text.len).unwrap_or(0))
    }

    /// Puts `text` into the buffer, cut to it on a whole character; the
    /// field shows it from its next draw.
    pub(in crate::ui::interface) fn set(&mut self, text: &str) {
        self.put(text);
        self.given = true;
    }

    fn put(&mut self, text: &str) {
        let kept = typed(text.as_bytes(), ENTRY_BYTES);
        self.bytes[..kept.len()].copy_from_slice(kept.as_bytes());
        self.text.len = kept.len() as u64;
    }
}

thread_local! {
    /// Whether a text field had the caret in this interface frame and in
    /// the last one.
    static CARET: Cell<[bool; 2]> = const { Cell::new([false; 2]) };
    /// Whether the loader's windows hold their focus back: from a field's
    /// caret until the player's next move between a window's members.
    static FOCUS_HELD: Cell<bool> = const { Cell::new(false) };
}

/// Starts the next interface frame, on its thread: this frame's caret
/// becomes the last frame's, and a frame that drew no window (`windowed`
/// false) ends a held focus, so the next window opens with its own.
pub(in crate::ui::interface) fn caret_next_frame(windowed: bool) {
    let [now, _] = CARET.get();
    CARET.set([false, now]);
    if !windowed {
        FOCUS_HELD.set(false);
    }
}

/// Notes a field with the caret in this frame.
fn caret_seen() {
    let [_, last] = CARET.get();
    CARET.set([true, last]);
    FOCUS_HELD.set(true);
}

/// Whether the loader's windows hold their focus back, on the interface
/// thread. Enter and Escape in a field clear the focus, and a window's
/// `ui.nav_end` gives a focus nobody holds to its remembered member, the
/// window's Back, while its group's byte `+0x70` (the navigation config's
/// focus byte) is set; a second Enter would then press Back. Moves step
/// apart from that byte, and only a moved selection takes the focus once
/// the byte is set again (client 26430625, read in the decompiler).
pub(super) fn focus_held() -> bool {
    FOCUS_HELD.get()
}

/// A window's `ui.nav_end` answered `moved`, a bool the game returns in its
/// low byte: anything but 0 is the player moving between its members, which
/// ends the held focus.
pub(super) fn nav_ended(moved: u8) {
    if moved != 0 {
        FOCUS_HELD.set(false);
    }
}

/// The navigation config's focus byte: 0 gives the focus to no member
/// unasked (client 26430625).
pub(super) const fn focus_byte(held: bool) -> u16 {
    if held { 0 } else { 1 }
}

/// Whether a field had the caret in the last frame, on the interface
/// thread. The loader's windows and scroll areas then read no move, and no
/// window calls `ui.nav_end`, as the game's text popup does (client
/// 26430625, seen live): a field joins the scroll area's group, so a
/// window's `ui.nav_end` would give the focus to the window's own button
/// and Enter would press it instead of the field's confirm.
pub(super) fn typed_last_frame() -> bool {
    CARET.get()[1]
}

/// Whether a field of any of the loader's windows had the caret in this
/// frame or the last one, on the interface thread, so a letter typed there
/// stays in the field.
pub(in crate::ui::interface) fn caret_shown() -> bool {
    CARET.get().contains(&true)
}

/// The navigation config's flags: the low byte 0 reads no move from the
/// arrows and the stick (client 26430625).
pub(super) const fn nav_flags(typed_last_frame: bool) -> u16 {
    if typed_last_frame { 0x100 } else { 0x101 }
}

/// What the player did in a text field this frame.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui::interface) struct Typed {
    /// Pressed Enter: the buffer holds the text to take.
    pub(in crate::ui::interface) entered: bool,
    /// The field has the caret.
    pub(in crate::ui::interface) editing: bool,
}

impl Engine {
    /// Whether this build draws text fields; without them texts show
    /// read-only.
    pub(in crate::ui::interface) const fn types_text(&self) -> bool {
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
    pub(in crate::ui::interface) unsafe fn text_field(
        &self,
        context: usize,
        buffer: &mut TextBuffer,
        hint: &UiText,
        ending: bool,
    ) -> Option<Typed> {
        let fns = self.text_fields.as_ref()?;
        let ending = ending || core::mem::take(&mut buffer.end);
        let bundle = self.bundle(context);
        buffer.text.data = buffer.bytes.as_mut_ptr();
        buffer.text.capacity = ENTRY_BYTES as u64;
        buffer.text.grow = 0;
        let mut field = Buf::<FIELD_SIZE>::zeroed();
        // SAFETY: `fns.field` is the game's `ui.text_field`, checked in
        // `TextFields::resolve`, inside the window this function's `# Safety`
        // gives; `field` holds `FIELD_SIZE` bytes, at least the build's
        // `ui.text_field_extent` the constructor writes, as `TextLayout::read`
        // checked. The
        // constructor stores the `GameText` pointer at `ui.text_field_text`,
        // inside that extent, and
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
        field.u16(self.layout.label_align, ALIGN_LEFT);
        if let Some(look) = fns.layout.look {
            field.u32(look, LOOK_BOXED);
        }
        let node = self.frame_node(&field);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the game just wrote into
        // the field.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        // SAFETY: `fns.min_height` is the game's `ui.node_min_height`,
        // checked in `TextFields::resolve`, on the field's node.
        unsafe { (fns.min_height)(node, FIELD_HEIGHT) };
        if core::mem::take(&mut buffer.given)
            && let Some(take) = fns.take
        {
            // SAFETY: `take` is the game's `ui.text_field_take`, its pattern
            // matched once, on the field built above; it reads the `GameText`
            // at `ui.text_field_text`, which points at `buffer`, exclusively
            // borrowed and unmoved, its length within its capacity, and copies
            // the text into the field's kept state, which it looks up itself.
            unsafe { take(field.ptr()) };
        }
        // The game may make or move the field's kept state in its input, so
        // each use looks it up again rather than keep its address across it.
        let kept = |object: *mut u8| {
            // SAFETY: `fns.state` is the game's `ui.text_field_state`, checked
            // in `TextFields::resolve`, on `object`, the field built above,
            // with the build's own key for the field's state,
            // `ui.text_field_state_key`.
            unsafe { (fns.state)(object, fns.layout.key, 1) }.expose_provenance()
        };
        let at = fns.layout.editing;
        let editing_in = |state: usize| {
            state != 0
                // SAFETY: `state` is non-zero, checked first, and points at
                // the field's kept state of the build's
                // `ui.text_field_state_size` bytes, whose editing byte sits at
                // `ui.text_field_editing`, inside it as `TextLayout::read`
                // checked.
                && unsafe { crate::engine::memory::read::<u8>(state, at) } != 0
        };
        if ending {
            buffer.typing = false;
        }
        // The field keeps its typing only while it has the focus, which the
        // window's navigation takes back each frame: a field the player
        // clicked asks for it every frame, and starts its typing through its
        // own input, as the game's chat field does (client 26430625).
        if buffer.typing {
            // SAFETY: `fns.focus` is the game's `ui.focus_request`, checked in
            // `TextFields::resolve`, on the field built above.
            unsafe { (fns.focus)(field.ptr()) };
            let state = kept(field.ptr());
            if let Some(start) = fns.layout.start
                && state != 0
                && !editing_in(state)
            {
                // SAFETY: `state` is non-zero, checked first, and points at
                // the field's kept state, just looked up, whose start byte at
                // `ui.text_field_start` lies inside it as `TextLayout::read`
                // checked; the game touches the state only on this thread
                // while it draws, and its next input reads the byte.
                unsafe { crate::engine::memory::write::<u8>(state, start, 1) };
            }
        }
        let mut answer = 0u64;
        // SAFETY: `fns.input` is the game's `ui.text_field_input`, checked
        // in `TextFields::resolve`, on the field built above; it writes the
        // typed text through the `GameText` at `ui.text_field_text`, which points at
        // `buffer`, exclusively borrowed and unmoved for this call, at most
        // `ENTRY_BYTES` bytes with no grow function, and its answer to the
        // live `answer`; `hint` outlives the frame by this function's
        // `# Safety`.
        unsafe { (fns.input)(field.ptr(), &raw mut answer, hint, 0) };
        let state = kept(field.ptr());
        let mut editing = editing_in(state);
        let mut pointer = [0u8; 8];
        // SAFETY: `fns.hit` is the game's `ui.frame_hit`, checked in
        // `TextFields::resolve`, on the field built above; it writes eight
        // bytes to the live `pointer`.
        unsafe { (fns.hit)(field.ptr(), &raw mut pointer, 1, 0, 0) };
        let clicked = pointer[0] == CLICKED && !ending;
        if clicked {
            // SAFETY: `fns.focus` is the game's `ui.focus_request`, checked in
            // `TextFields::resolve`, on the field built above.
            unsafe { (fns.focus)(field.ptr()) };
        }
        buffer.typing = keeps_typing(clicked, buffer.typing, editing);
        if editing && ending {
            // SAFETY: `state` is non-zero, checked by `editing`, and points at
            // the field's kept state, whose editing byte at `at` lies inside
            // it as `TextLayout::read` checked; the game touches the state
            // only on this thread while it draws; clearing its editing byte
            // ends the typing, as the rename dialog does on Back.
            unsafe { crate::engine::memory::write::<u8>(state, at, 0) };
            editing = false;
        }
        buffer.text.len = buffer.text.len.min(ENTRY_BYTES as u64);
        // SAFETY: `self.label_draw` is the game's `ui.label_draw`, checked in
        // `Engine::resolve`, on the field built above, which draws and closes
        // it as the game's callers do; it reads the text through the
        // `GameText` at `ui.text_field_text`, which points at `buffer`, still
        // exclusively borrowed and unmoved, its length within its capacity.
        unsafe { (self.label_draw)(field.ptr()) };
        if editing {
            caret_seen();
        }
        Some(Typed {
            entered: answer & u64::from(u32::MAX) == ENTERED,
            editing,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client's text field constants, with its look at `look`.
    fn build(look: Option<u64>) -> impl Fn(&str) -> Option<u64> {
        move |id| {
            [
                (pinned::ui::TEXT_FIELD_EXTENT.id(), Some(0x272)),
                (pinned::ui::TEXT_FIELD_TEXT.id(), Some(0x150)),
                (pinned::ui::TEXT_FIELD_STATE_SIZE.id(), Some(0x1990)),
                (pinned::ui::TEXT_FIELD_EDITING.id(), Some(0x1988)),
                (pinned::ui::TEXT_FIELD_STATE_KEY.id(), Some(1)),
                (pinned::ui::TEXT_FIELD_LOOK.id(), look),
                (pinned::ui::TEXT_FIELD_START.id(), Some(0x198a)),
            ]
            .into_iter()
            .find_map(|(known, value)| (known == id).then_some(value))?
        }
    }

    #[test]
    fn navigation_stays_quiet_only_in_the_frame_after_a_field_had_the_caret() {
        caret_next_frame(true);
        caret_next_frame(true);
        assert!(!typed_last_frame(), "no field typed");
        caret_seen();
        assert!(!typed_last_frame(), "the frame the typing starts in");
        caret_next_frame(true);
        assert!(typed_last_frame(), "the frame after a field typed");
        caret_seen();
        caret_next_frame(true);
        assert!(typed_last_frame(), "a field keeps typing");
        caret_next_frame(true);
        assert!(
            !typed_last_frame(),
            "the frame after Enter ended the typing"
        );
    }

    #[test]
    fn a_window_holds_its_focus_from_a_field_s_caret_until_the_player_moves() {
        nav_ended(1);
        assert!(!focus_held(), "no field had the caret");
        assert_eq!(focus_byte(focus_held()), 1, "Back takes the focus");
        caret_seen();
        caret_next_frame(true);
        caret_next_frame(true);
        assert!(!typed_last_frame(), "Enter ended the typing");
        assert!(focus_held(), "a second Enter finds no Back focused");
        assert_eq!(focus_byte(focus_held()), 0);
        nav_ended(0);
        assert!(focus_held(), "a frame without a move keeps it held");
        nav_ended(1);
        assert!(
            !focus_held(),
            "a move lets the moved-to member take the focus"
        );
        assert_eq!(focus_byte(focus_held()), 1);
    }

    #[test]
    fn a_frame_without_a_window_ends_a_held_focus_so_the_next_window_has_its_own() {
        caret_seen();
        caret_next_frame(true);
        caret_next_frame(true);
        assert!(focus_held(), "the window with the field is still drawn");
        caret_next_frame(false);
        assert!(!focus_held(), "the window closed without a move");
    }

    #[test]
    fn navigation_reads_no_move_only_while_a_field_typed() {
        assert_eq!(nav_flags(false), 0x101);
        assert_eq!(nav_flags(true) & 0xff, 0, "no move from arrows or stick");
        assert_eq!(nav_flags(true) >> 8, 1, "the high byte stays");
    }

    #[test]
    fn the_field_draws_boxed_only_where_its_look_lies_inside_the_object() {
        let look = |at| TextLayout::read(&build(at)).map(|layout| layout.look).ok();
        assert_eq!(look(Some(0x148)), Some(Some(0x148)));
        assert_eq!(look(Some(0x26e)), Some(Some(0x26e)), "the last four bytes");
        assert_eq!(look(Some(0x26f)), Some(None), "one byte past the object");
        assert_eq!(look(None), Some(None), "a build whose route misses");
    }

    #[test]
    fn a_field_without_its_start_byte_still_types() {
        let start = |at: Option<u64>| {
            let lookup = build(None);
            let changed = move |id: &str| {
                if id == pinned::ui::TEXT_FIELD_START.id() {
                    at
                } else {
                    lookup(id)
                }
            };
            TextLayout::read(&changed).map(|layout| layout.start).ok()
        };
        assert_eq!(start(Some(0x198a)), Some(Some(0x198a)));
        assert_eq!(start(None), Some(None), "a build whose route misses");
        assert_eq!(start(Some(0x1990)), Some(None), "past the kept state");
    }

    #[test]
    fn a_click_starts_the_typing_and_the_game_s_end_of_editing_stops_it() {
        assert!(keeps_typing(true, false, false), "a click starts it");
        assert!(
            keeps_typing(false, true, true),
            "it holds while the game edits"
        );
        assert!(!keeps_typing(false, true, false), "Enter or Back ended it");
        assert!(
            !keeps_typing(false, false, true),
            "the game's own editing is not ours"
        );
    }
}
