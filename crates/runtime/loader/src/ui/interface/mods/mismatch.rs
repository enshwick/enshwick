//! The mismatch screen's page: the mods that differ from a server's that
//! refused or kicked this client, a heading per kind, scrolling.

use enshwick_core::abi::UiWidget;

use super::list::{listing, pages};
use crate::base::join::Row;
use crate::engine::texts;
use crate::ui::interface::Painter;

/// Rows a page shows on a build without scroll areas.
const ROWS_PER_PAGE: usize = 8;

/// Draws what to do, then `rows`, built once when the difference came, from
/// `first` while they page.
pub(super) fn body(paint: &Painter<'_>, rows: &[Row], first: &mut usize) {
    let what = texts::text("join.dialog.message");
    paint.label(what.as_bytes(), UiWidget::STYLE_BODY, true, false);
    let listing = listing(paint, ROWS_PER_PAGE);
    *first = listing.clamp_first(*first, rows.len());
    let shown = listing.shown(*first, rows.len());
    for row in rows.iter().take(shown.end).skip(shown.start) {
        let (text, style) = match row {
            Row::Heading(text) => (text, UiWidget::STYLE_H3),
            Row::Line(text) => (text, UiWidget::STYLE_BODY),
            Row::Quiet(text) => (text, UiWidget::STYLE_SECONDARY),
        };
        paint.label(text.as_bytes(), style, true, false);
    }
    pages(paint, listing, first, rows.len());
}
