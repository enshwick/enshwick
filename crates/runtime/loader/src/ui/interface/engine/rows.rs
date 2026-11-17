//! The game's row frame and separator line, as its lists draw their
//! entries: what an entry is at the left, its action at the right, a thin
//! line between entries.
//!
//! Engine facts from client 26430625, read in the code and seen live at
//! 1280x720 and 3840x2160: the row frame is the column frame's twin and lays its children out
//! side by side, centred vertically; a child stretched `(1, 0)` takes the
//! rest of the row; the players list pads its entries and parts them with
//! the separator of kind 3.

use std::mem::offset_of;

use super::{Buf, Engine, FRAME_SIZE, Loader, Lookup, Missing, Ptr, field, game_fn, stretch};
use crate::engine::pinned;

type FrameGapFn = unsafe extern "system" fn(frame: Ptr, gap: f32) -> Ptr;
type SeparatorFn = unsafe extern "system" fn(config: *const SeparatorConfig);

/// What the game's separator draws: the config its callers fill on the
/// stack, the same on every kept client; [`SeparatorConfig::read`] holds a
/// build to it.
#[repr(C)]
struct SeparatorConfig {
    kind: u32,
    color: u32,
    intensity: f32,
    /// Left, top, right, bottom.
    margin: [f32; 4],
    height: f32,
    offset: f32,
}

const _: () = assert!(size_of::<SeparatorConfig>() == 0x24);

impl SeparatorConfig {
    /// Checks that the build's separator reads its margin, height and
    /// offset where this config keeps them; [`Missing::Room`] on a build
    /// that moved one, so its line is never drawn from other fields.
    fn read(lookup: Lookup<'_>) -> Result<(), Missing> {
        for (id, at) in [
            (pinned::ui::SEPARATOR_MARGIN.id(), offset_of!(Self, margin)),
            (pinned::ui::SEPARATOR_HEIGHT.id(), offset_of!(Self, height)),
            (pinned::ui::SEPARATOR_OFFSET.id(), offset_of!(Self, offset)),
        ] {
            if field(lookup, id)? != at {
                return Err(Missing::Room(id));
            }
        }
        Ok(())
    }
}

/// The thin line of constant weight the players list puts between its
/// groups, with that list's colour, margin and height.
const LINE: SeparatorConfig = SeparatorConfig {
    kind: 3,
    color: 0x80ff_ffff,
    intensity: 1.0,
    margin: [0.0, 5.0, 0.0, 5.0],
    height: 3.0,
    offset: 0.0,
};
/// The gap between a row's cells, as the key bindings list's.
const CELL_GAP: f32 = 10.0;
/// The gap between the lines of a row's left cell, as the players list's.
const LINE_GAP: f32 = 5.0;
/// A row's padding above and below, the players list's.
const ROW_PADDING: [f32; 4] = [0.0, 10.0, 0.0, 10.0];

/// A row's padding: [`ROW_PADDING`], and at its right the scrollbar's width
/// when the row is in a scroll area, so its buttons stay clear of the bar.
fn row_padding(scrolled: bool) -> [f32; 4] {
    let mut padding = ROW_PADDING;
    if scrolled {
        padding[2] = super::scroll::BAR_WIDTH;
    }
    padding
}

/// The row's and the separator's functions.
#[derive(Debug)]
pub(super) struct Rows {
    row: FrameGapFn,
    column: FrameGapFn,
    separator: SeparatorFn,
}

impl Rows {
    /// Resolves the row frame, the column frame and the separator.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        SeparatorConfig::read(&|id: &str| loader.build.constant(id))?;
        macro_rules! function {
            ($name:expr, $ty:ty) => {
                // SAFETY: `$ty` is the signature of the game function `$name`
                // names, from the popup's, the slider's and the players
                // list's calls on client 26430625.
                unsafe { game_fn::<$ty>(loader, $name) }?
            };
        }
        let row = function!(pinned::ui::ROW, FrameGapFn);
        let column = function!(pinned::ui::COLUMN, FrameGapFn);
        apart(row, column)?;
        Ok(Self {
            row,
            column,
            separator: function!(pinned::ui::SEPARATOR, SeparatorFn),
        })
    }
}

/// Checks that the route found the row and not the column, its twin with
/// the same first instructions, on a build whose popup opens a column
/// where the route looks.
fn apart(row: FrameGapFn, column: FrameGapFn) -> Result<(), Missing> {
    if std::ptr::fn_addr_eq(row, column) {
        return Err(Missing::Bytes(pinned::ui::ROW.id()));
    }
    Ok(())
}

/// A row's own frames, kept in place from its start to its end: the row,
/// the column of its left cell and the column of its right cell.
pub(in crate::ui::interface) struct Row {
    frame: Buf<FRAME_SIZE>,
    column: Buf<FRAME_SIZE>,
    side: Buf<FRAME_SIZE>,
}

impl Row {
    pub(in crate::ui::interface) fn new() -> Self {
        Self {
            frame: Buf::zeroed(),
            column: Buf::zeroed(),
            side: Buf::zeroed(),
        }
    }
}

/// The game's item slot's width in layout units, rounded up from the about
/// 108 measured (80 of the 310 pixels a 420-unit panel took beside the
/// Backpack, client 26430625, 3840x2160), so a row never runs past its panel.
const SLOT_WIDTH: u16 = 112;
/// The gap between the item slots of a row.
const SLOT_GAP: u16 = 10;

/// How many item slots side by side fit in `inner` layout units, at least
/// one.
const fn slots_per_row(inner: u16) -> u16 {
    let fit = (inner + SLOT_GAP) / (SLOT_WIDTH + SLOT_GAP);
    if fit == 0 { 1 } else { fit }
}

/// A row of item slots, as the game's inventory grids lay them out: the
/// slots a frame callback draws one after another, until the row is full.
pub(in crate::ui::interface) struct SlotRow {
    frame: Buf<FRAME_SIZE>,
    /// The slots in the open row; 0 while none is open.
    slots: u16,
}

impl SlotRow {
    pub(in crate::ui::interface) const fn new() -> Self {
        Self {
            frame: Buf::zeroed(),
            slots: 0,
        }
    }

    /// Counts the next slot in a row of `per_row`: whether the open row ends
    /// first, being full, and whether the slot starts a new one.
    const fn admit(&mut self, per_row: u16) -> (bool, bool) {
        let full = self.slots >= per_row && self.slots > 0;
        if full {
            self.slots = 0;
        }
        let starts = self.slots == 0;
        self.slots += 1;
        (full, starts)
    }

    /// Takes back the last slot counted, which drew nothing: whether the row
    /// it opened is now empty, to close.
    const fn retract(&mut self) -> bool {
        let alone = self.slots == 1;
        self.slots = self.slots.saturating_sub(1);
        alone
    }

    /// Forgets an open row without closing it, for a new frame, whose layout
    /// starts with no frame of the last one open.
    pub(in crate::ui::interface) const fn forget(&mut self) {
        self.slots = 0;
    }
}

/// The gap between the buttons stacked in a row's right cell, a third of
/// an icon button's height, so they read as two.
const SIDE_GAP: f32 = 16.0;

impl Engine {
    /// Opens a row as wide as its column and the left cell in it, which
    /// takes the width the right cell leaves, clear of the scrollbar when
    /// `scrolled`; false, opening nothing, on a build without rows, where the
    /// cells stack.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `row` stays in place until [`Engine::row_end`], which follows
    ///   [`Engine::row_side`] in the same frame when this returns true.
    pub(in crate::ui::interface) unsafe fn row_begin(&self, row: &mut Row, scrolled: bool) -> bool {
        let Some(fns) = &self.rows else {
            return false;
        };
        // SAFETY: `fns.row` is the game's `ui.row`, checked in
        // `Rows::resolve`, inside the window this function's `# Safety`
        // gives; `row.frame` holds `FRAME_SIZE` bytes, the room of a frame as
        // for `ui.column`, its twin, and stays in place.
        unsafe { (fns.row)(row.frame.ptr(), CELL_GAP) };
        let node = self.frame_node(&row.frame);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the game just wrote
        // into the row.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        // SAFETY: `self.node_padding` is the game's `ui.node_padding`,
        // checked in `Engine::resolve`, on the row's node, with four live
        // floats.
        unsafe { (self.node_padding)(node, &row_padding(scrolled)) };
        // SAFETY: `fns.column` is the game's `ui.column`, checked in
        // `Rows::resolve`, inside the row just opened; `row.column` holds
        // `FRAME_SIZE` bytes and stays in place.
        unsafe { (fns.column)(row.column.ptr(), LINE_GAP) };
        let node = self.frame_node(&row.column);
        // SAFETY: `self.node_stretch` is the game's `ui.node_stretch`,
        // checked in `Engine::resolve`, on the node the game just wrote
        // into the column.
        unsafe { (self.node_stretch)(node, stretch(1.0, 0.0)) };
        true
    }

    /// Ends the row's left cell and opens its right cell, a column at the
    /// right of the row, centred down it: what is drawn next stacks there.
    ///
    /// # Safety
    ///
    /// - [`Engine::row_begin`] returned true for `row` in this frame, on
    ///   this thread, and `row` stayed in place.
    /// - Every frame opened in the left cell is closed.
    pub(in crate::ui::interface) unsafe fn row_side(&self, row: &mut Row) {
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the column `row_begin` opened, in place
        // and with nothing open inside it by this function's `# Safety`.
        unsafe { (self.frame_close)(row.column.ptr()) };
        let Some(fns) = &self.rows else {
            return;
        };
        // SAFETY: `fns.column` is the game's `ui.column`, checked in
        // `Rows::resolve`, inside the row `row_begin` opened; `row.side`
        // holds `FRAME_SIZE` bytes and stays in place.
        unsafe { (fns.column)(row.side.ptr(), SIDE_GAP) };
        let node = self.frame_node(&row.side);
        // SAFETY: `self.node_align` is the game's `ui.node_align`, checked in
        // `Engine::resolve`, on the node the game just wrote into the
        // column, with a live alignment word.
        unsafe { (self.node_align)(node, &super::widgets::BUTTON_ALIGN_RIGHT) };
    }

    /// Ends the row.
    ///
    /// # Safety
    ///
    /// - [`Engine::row_side`] ran for `row` in this frame, on this thread,
    ///   and `row` stayed in place.
    /// - Every frame opened in the right cell is closed.
    pub(in crate::ui::interface) unsafe fn row_end(&self, row: &mut Row) {
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the right cell's column `row_side` opened
        // (it returns before opening one only on a build without rows, where
        // `row_begin` returned false and this does not run), in place and
        // with nothing open inside it by this function's `# Safety`.
        unsafe { (self.frame_close)(row.side.ptr()) };
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the row `row_begin` opened, in place,
        // its columns closed and nothing else open inside it.
        unsafe { (self.frame_close)(row.frame.ptr()) };
    }

    /// Makes room for the next item slot of a screen extension's panel
    /// (`extension`) or of a window: opens a row for it, ending a full one
    /// first. On a build without rows the slots stack.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread, with every frame
    ///   opened since the row's last slot closed.
    /// - `row` stays in place until [`Engine::slot_row_end`] in this frame.
    pub(in crate::ui::interface) unsafe fn slot_row_next(
        &self,
        row: &mut SlotRow,
        extension: bool,
    ) {
        let Some(fns) = &self.rows else {
            return;
        };
        let (full, starts) = row.admit(slots_per_row(super::windows::inner_width(extension)));
        if full {
            // SAFETY: `self.frame_close` is the game's `ui.frame_close`,
            // checked in `Engine::resolve`, on the full row this function
            // opened earlier in the frame, in place and with nothing open
            // inside it by this function's `# Safety`.
            unsafe { (self.frame_close)(row.frame.ptr()) };
        }
        if starts {
            // SAFETY: `fns.row` is the game's `ui.row`, checked in
            // `Rows::resolve`, inside the window this function's `# Safety`
            // gives; `row.frame` holds `FRAME_SIZE` bytes, the room of a frame
            // as for `ui.column`, its twin, and stays in place.
            unsafe { (fns.row)(row.frame.ptr(), f32::from(SLOT_GAP)) };
        }
    }

    /// Takes back the item slot [`Engine::slot_row_next`] counted last, which
    /// drew nothing, closing the row it opened alone, so a slot that fails
    /// leaves no gap in the row.
    ///
    /// # Safety
    ///
    /// - As for [`Engine::slot_row_end`], right after the slot's
    ///   [`Engine::slot_row_next`].
    pub(in crate::ui::interface) unsafe fn slot_row_retract(&self, row: &mut SlotRow) {
        if !row.retract() {
            return;
        }
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the row `slot_row_next` opened for this
        // slot one call earlier (a count of 1 is only ever its opening, on a
        // build with rows), in place and with nothing open inside it.
        unsafe { (self.frame_close)(row.frame.ptr()) };
    }

    /// Ends the open row of item slots, if any.
    ///
    /// # Safety
    ///
    /// - Called on the frame's thread in the frame whose
    ///   [`Engine::slot_row_next`] opened the row, inside the same window,
    ///   with every frame opened since its last slot closed; `row` did not
    ///   move.
    pub(in crate::ui::interface) unsafe fn slot_row_end(&self, row: &mut SlotRow) {
        if core::mem::take(&mut row.slots) == 0 {
            return;
        }
        // SAFETY: `self.frame_close` is the game's `ui.frame_close`, checked
        // in `Engine::resolve`, on the row `slot_row_next` opened in this
        // frame (`slots` counts only from its opening), in place and with
        // nothing open inside it by this function's `# Safety`.
        unsafe { (self.frame_close)(row.frame.ptr()) };
    }

    /// Draws the thin line the game puts between the entries of a list;
    /// nothing on a build without rows.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    pub(in crate::ui::interface) unsafe fn separator(&self) {
        let Some(fns) = &self.rows else {
            return;
        };
        // SAFETY: `fns.separator` is the game's `ui.separator`, checked in
        // `Rows::resolve`, inside the window this function's `# Safety`
        // gives; it reads the 0x24 bytes of the live `LINE` and opens and
        // closes its own frame.
        unsafe { (fns.separator)(&LINE) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client whose separator reads its height at `height`.
    fn build(height: Option<u64>) -> impl Fn(&str) -> Option<u64> {
        move |id| {
            [
                (pinned::ui::SEPARATOR_MARGIN.id(), Some(0xc)),
                (pinned::ui::SEPARATOR_HEIGHT.id(), height),
                (pinned::ui::SEPARATOR_OFFSET.id(), Some(0x20)),
            ]
            .into_iter()
            .find_map(|(known, value)| (known == id).then_some(value))?
        }
    }

    #[test]
    fn rows_turn_off_when_the_route_finds_the_column_for_the_row() {
        extern "system" fn row(frame: Ptr, _: f32) -> Ptr {
            frame
        }
        extern "system" fn column(frame: Ptr, gap: f32) -> Ptr {
            std::hint::black_box(gap);
            frame.wrapping_add(1)
        }
        assert!(apart(row, column).is_ok());
        let id = pinned::ui::ROW.id();
        assert!(matches!(apart(column, column), Err(Missing::Bytes(at)) if at == id));
    }

    #[test]
    fn a_panel_beside_a_screen_holds_three_slots_a_row_and_a_window_five() {
        use super::super::windows::inner_width;
        assert_eq!(slots_per_row(inner_width(true)), 3);
        assert_eq!(slots_per_row(inner_width(false)), 5);
        assert_eq!(slots_per_row(0), 1, "a slot too wide still draws");
    }

    #[test]
    fn a_slot_starts_a_row_when_none_is_open_or_the_open_one_is_full() {
        let mut row = SlotRow::new();
        let steps: Vec<_> = (0..7).map(|_| row.admit(3)).collect();
        let (new, ends) = ((false, true), (true, true));
        let same = (false, false);
        assert_eq!(steps, [new, same, same, ends, same, same, ends]);
        assert_eq!(row.slots, 1, "the last row holds one slot to end");
    }

    #[test]
    fn a_slot_that_drew_nothing_gives_its_place_back_and_closes_a_row_it_opened_alone() {
        let mut row = SlotRow::new();
        assert!(!row.retract(), "no row open");
        row.admit(3);
        assert!(row.retract(), "it opened the row alone");
        assert_eq!(row.admit(3), (false, true), "the next slot opens a row");
        row.admit(3);
        assert!(!row.retract(), "the row keeps its first slot");
        assert_eq!(
            row.admit(3),
            (false, false),
            "and takes the next in its place"
        );
        assert_eq!(row.admit(3), (false, false));
        assert_eq!(row.admit(3), (true, true), "three drawn: the row was full");
        row.forget();
        assert_eq!(row.admit(3), (false, true), "a new frame opens its own row");
    }

    #[test]
    fn a_row_leaves_the_scrollbar_s_width_free_only_where_its_list_scrolls() {
        assert_eq!(row_padding(true), [0.0, 10.0, 24.0, 10.0]);
        assert_eq!(row_padding(false), [0.0, 10.0, 0.0, 10.0]);
    }

    #[test]
    fn rows_turn_off_on_a_build_whose_separator_reads_another_config() {
        let height = pinned::ui::SEPARATOR_HEIGHT.id();
        let read = |at| SeparatorConfig::read(&build(at));
        assert!(read(Some(0x1c)).is_ok());
        assert!(
            matches!(read(Some(0x20)), Err(Missing::Room(id)) if id == height),
            "a field moved by four bytes"
        );
        assert!(
            matches!(read(None), Err(Missing::Entry(id)) if id == height),
            "a build whose route misses"
        );
    }
}
