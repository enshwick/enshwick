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
use crate::pinned;

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
/// A row's padding above and below, the players list's, and at its right,
/// which keeps a row's button clear of the scroll bar beside the list.
const ROW_PADDING: [f32; 4] = [0.0, 10.0, 24.0, 10.0];

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
pub(in crate::ui) struct Row {
    frame: Buf<FRAME_SIZE>,
    column: Buf<FRAME_SIZE>,
    side: Buf<FRAME_SIZE>,
}

impl Row {
    pub(in crate::ui) fn new() -> Self {
        Self {
            frame: Buf::zeroed(),
            column: Buf::zeroed(),
            side: Buf::zeroed(),
        }
    }
}

/// The alignment word that puts the right cell at the right of its row,
/// centred down it (client 26430625): the low byte across (3 right), the
/// high byte down (2 centre).
const SIDE_ALIGN: u16 = 0x203;
/// The gap between the buttons stacked in a row's right cell, a third of
/// an icon button's height, so they read as two.
const SIDE_GAP: f32 = 16.0;

impl Engine {
    /// Opens a row as wide as its column and the left cell in it, which
    /// takes the width the right cell leaves; false, opening nothing, on a
    /// build without rows, where the cells stack.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    /// - `row` stays in place until [`Engine::row_end`], which follows
    ///   [`Engine::row_side`] in the same frame when this returns true.
    pub(in crate::ui) unsafe fn row_begin(&self, row: &mut Row) -> bool {
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
        unsafe { (self.node_padding)(node, &ROW_PADDING) };
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
    pub(in crate::ui) unsafe fn row_side(&self, row: &mut Row) {
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
        unsafe { (self.node_align)(node, &SIDE_ALIGN) };
    }

    /// Ends the row.
    ///
    /// # Safety
    ///
    /// - [`Engine::row_side`] ran for `row` in this frame, on this thread,
    ///   and `row` stayed in place.
    /// - Every frame opened in the right cell is closed.
    pub(in crate::ui) unsafe fn row_end(&self, row: &mut Row) {
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

    /// Draws the thin line the game puts between the entries of a list;
    /// nothing on a build without rows.
    ///
    /// # Safety
    ///
    /// - Called inside a window, on the frame's thread.
    pub(in crate::ui) unsafe fn separator(&self) {
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
