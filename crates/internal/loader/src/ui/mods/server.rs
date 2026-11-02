//! The joined server's pages: its mods with settings, and a mod's settings it sends.

use enshwick::abi::UiWidget;

use super::list::{listing, pages};
use super::settings::draw_row;
use super::{Entries, MODS_PER_PAGE, ROWS_PER_PAGE};
use crate::configure::rows::{self, Row};
use crate::locks::Relock;
use crate::mods_screen;
use crate::texts;
use crate::ui::Painter;

/// Draws a page of the server's mods; the mod whose settings the player
/// opened.
pub(super) fn server_list(paint: &Painter<'_>, first: &mut usize) -> Option<String> {
    let mods = crate::remote::heard(|h| h.mods.clone());
    let Some(mods) = mods else {
        paint.label(
            texts::text("mods.server.asking").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return None;
    };
    if mods.is_empty() {
        paint.label(
            texts::text("mods.server.none").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
    }
    let mut opened = None;
    let listing = listing(paint, MODS_PER_PAGE);
    *first = listing.clamp_first(*first, mods.len());
    let shown = listing.shown(*first, mods.len());
    for (id, name) in mods.iter().take(shown.end).skip(shown.start) {
        paint.label(name.as_bytes(), UiWidget::STYLE_H3, false, false);
        if paint.button(texts::text("mods.settings").as_bytes(), true) {
            opened = Some(id.clone());
        }
    }
    pages(paint, listing, first, mods.len());
    opened
}

/// Draws a page of the server's settings of mod `id`, rebuilding the rows
/// when the server sent news; the outcome of a change or the server's
/// answer.
pub(super) fn server_settings(
    paint: &Painter<'_>,
    id: &str,
    first: &mut usize,
    rows: &mut Vec<Row>,
    schema: &mut Option<enshwick::config::Schema>,
    news: &mut u64,
    entries: &mut Entries,
) -> Option<(String, bool)> {
    entries.editing = false;
    let (heard_news, result) = crate::remote::heard(|h| (h.news, h.result.clone()));
    let mut message = None;
    if heard_news != *news {
        *news = heard_news;
        let settings = crate::remote::heard(|h| h.settings.clone());
        if let Some((heard_id, admin, heard_schema, text)) = settings
            && heard_id == id
        {
            let texts = crate::host::loader()
                .and_then(|loader| loader.mods.iter().find(|m| m.id == id))
                .and_then(|m| m.texts.relock().clone());
            let words = texts.as_ref().map(|texts| rows::Words {
                texts,
                language: texts::language(),
            });
            *rows = rows::rows(&heard_schema, &text, None, words);
            if !admin {
                mods_screen::lock_to_server(rows);
            }
            *schema = Some(heard_schema);
        }
        message = result.map(|(ok, text)| (text, ok));
    }
    let Some(known) = schema.as_ref() else {
        paint.label(
            texts::text("mods.server.asking").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            false,
            true,
        );
        return message;
    };
    let mut changed = None;
    let listing = listing(paint, ROWS_PER_PAGE);
    *first = listing.clamp_first(*first, rows.len());
    let shown = listing.shown(*first, rows.len());
    for row in rows.iter().take(shown.end).skip(shown.start) {
        if let Some(value) = draw_row(paint, id, known, row, entries) {
            changed = Some((row.key.clone(), value));
        }
        if !row.doc.is_empty() {
            paint.label(row.doc.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
    }
    pages(paint, listing, first, rows.len());
    if let Some((key, value)) = changed {
        let sent = crate::net::to_server(&crate::remote::Message::Edit {
            id: id.to_owned(),
            key,
            value,
        });
        return Some(if sent {
            (texts::text("mods.server.sent").to_owned(), true)
        } else {
            (texts::text("mods.server.closed").to_owned(), false)
        });
    }
    message
}
