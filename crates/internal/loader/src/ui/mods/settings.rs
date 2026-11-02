//! A mod's settings page, behind its gear on the Mods screen: each player's
//! own settings, then the running world's, or with no world the defaults of
//! new worlds; and each settings row drawn with its text field.

use enshwick::abi::UiWidget;

use super::list::{listing, pages};
use super::{Entries, ROWS_PER_PAGE};
use crate::configure::rows::{self, Lock, Row, Widget};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mods_screen;
use crate::texts;
use crate::ui::Painter;

/// The rows of mod `index`'s settings page at `place`: in another's world,
/// its world rows hold the server's join values, read-only.
pub(super) fn page_rows(loader: &Loader, index: usize, place: mods_screen::Place) -> Vec<Row> {
    let Some(record) = loader.mods.get(index) else {
        return Vec::new();
    };
    let Some(schema) = record
        .schema
        .relock()
        .clone()
        .and_then(|schema| mods_screen::page_settings(schema, place.away))
    else {
        return Vec::new();
    };
    let file = crate::worlds::show_text(loader, record);
    let texts = record.texts.relock().clone();
    let words = texts.as_ref().map(|texts| rows::Words {
        texts,
        language: texts::language(),
    });
    if !place.away {
        return rows::rows(&schema, &file, None, words);
    }
    let server = record
        .server_synced
        .relock()
        .clone()
        .and_then(|values| mods_screen::server_table(&values));
    let mut rows = rows::rows(&schema, &file, server.as_ref(), words);
    mods_screen::lock_world_rows(&schema, &mut rows);
    rows
}

/// Draws a page of mod `index`'s settings, the world's those of the world
/// this process runs (`world`) or of new worlds; the outcome of a change.
pub(super) fn settings(
    loader: &Loader,
    paint: &Painter<'_>,
    index: usize,
    first: &mut usize,
    rows: &[Row],
    entries: &mut Entries,
    world: bool,
) -> Option<(String, bool)> {
    entries.editing = false;
    let record = loader.mods.get(index)?;
    if rows.iter().any(|r| r.lock == Lock::Server) {
        paint.label(
            texts::text("mods.settings.server-marked").as_bytes(),
            UiWidget::STYLE_SECONDARY,
            true,
            false,
        );
    }
    let schema = record.schema.relock().clone()?;
    let mut changed = None;
    let listing = listing(paint, ROWS_PER_PAGE);
    *first = listing.clamp_first(*first, rows.len());
    let shown = listing.shown(*first, rows.len());
    let mut section = None;
    for row in rows.iter().take(shown.end).skip(shown.start) {
        let scope = schema
            .field(crate::configure::element(&row.key).0)
            .map(|f| f.scope);
        if let Some(scope) = scope
            && section != Some(scope)
        {
            section = Some(scope);
            let (heading, note) = mods_screen::section(scope, world);
            paint.label(
                texts::text(heading).as_bytes(),
                UiWidget::STYLE_H3,
                false,
                false,
            );
            if let Some(note) = note {
                paint.label(
                    texts::text(note).as_bytes(),
                    UiWidget::STYLE_SECONDARY,
                    true,
                    false,
                );
            }
        }
        if let Some(value) = draw_row(paint, &record.id, &schema, row, entries) {
            changed = Some((row.key.clone(), value));
        }
        if !row.doc.is_empty() {
            paint.label(row.doc.as_bytes(), UiWidget::STYLE_SECONDARY, true, false);
        }
    }
    pages(paint, listing, first, rows.len());
    let (key, value) = changed?;
    Some(
        match crate::configure::apply(loader, index, &key, &value, "the Mods screen") {
            Ok(change) => (crate::configure::changed(&record.id, &key, &change), true),
            Err(why) => (why.reply(), false),
        },
    )
}

/// Draws one row, its text field kept in `entries`; the TOML of the
/// player's change.
pub(super) fn draw_row(
    paint: &Painter<'_>,
    id: &str,
    schema: &enshwick::config::Schema,
    row: &Row,
    entries: &mut Entries,
) -> Option<String> {
    let label = mods_screen::row_label(row);
    let read_only = row.lock != Lock::Open;
    match &row.widget {
        Widget::Toggle(on) => paint
            .toggle(label.as_bytes(), *on, read_only)
            .map(rows::toggled),
        Widget::Slider {
            value,
            min,
            max,
            steps,
            whole,
        } => paint
            .setting_slider(label.as_bytes(), *value, (*min, *max), *steps, read_only)
            .map(|v| rows::slid(v, *whole)),
        Widget::Choice { options, index } => {
            let options: Vec<&[u8]> = options.iter().map(String::as_bytes).collect();
            let picked = paint.dropdown(
                label.as_bytes(),
                &options,
                u32::try_from(*index).unwrap_or(u32::MAX),
                read_only,
            )?;
            rows::picked(schema.field(&row.key)?, picked as usize)
        }
        Widget::Entry { text, toml } => {
            if !read_only && paint.engine.types_text() {
                paint.label(label.as_bytes(), UiWidget::STYLE_BODY, true, false);
                return text_row(paint, &row.key, text, entries);
            }
            let line = if read_only {
                texts::fill("row.value", &[("label", &label), ("value", text)])
            } else {
                texts::fill(
                    "row.text",
                    &[("label", &label), ("value", toml), ("id", id)],
                )
            };
            paint.label(line.as_bytes(), UiWidget::STYLE_BODY, true, false);
            None
        }
        Widget::Text(value) => {
            let line = texts::fill(
                "row.text",
                &[("label", &label), ("value", value), ("id", id)],
            );
            paint.label(line.as_bytes(), UiWidget::STYLE_BODY, true, false);
            None
        }
    }
}

/// Draws the text field of row `key`, whose setting holds `value`; the
/// TOML of the typed text when the player pressed Enter. Typing that ends
/// without Enter sets nothing, and the field's buffer gets the setting's
/// value back; `entries.ending` takes the caret from the field.
fn text_row(paint: &Painter<'_>, key: &str, value: &str, entries: &mut Entries) -> Option<String> {
    let ending = entries.ending;
    let buffer = entries.buffer(key, value);
    let hint = texts::text("row.text.hint").as_bytes();
    let typed = paint.text_field(buffer, hint, ending)?;
    let entered = typed.entered.then(|| rows::entered(buffer.text()));
    if !typed.editing && entered.is_none() && buffer.text() != value {
        buffer.set(value);
    }
    entries.editing |= typed.editing;
    entered
}
