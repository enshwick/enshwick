//! A mod's key bindings on its settings page: its controls, their keys in
//! the player's layout, a click that waits for the next key, and the keys
//! a control shares with any mod's.

use std::time::{Duration, Instant};

use enshwick::abi::UiWidget;

use super::list::{listing, pages};
use crate::bindings::{self, Capture, Captured, clashes};
use crate::controls::{self, Row, SetKeyError, capture_keys, key_name};
use crate::host::Loader;
use crate::texts;
use crate::ui::Painter;

/// Controls a page shows on a build without the scroll area, under the
/// settings rows of the same page.
const PER_PAGE: usize = 3;

/// What a mod's key bindings show and wait for.
#[derive(Debug, Default)]
pub(in crate::ui) struct View {
    /// The first control shown while they page.
    first: usize,
    /// The control and slot waiting for a key, and the capture.
    waiting: Option<(usize, usize, Capture)>,
    /// The last outcome, and whether it went through.
    message: Option<(String, bool)>,
    /// When Escape cancelled a capture: the screen's Back for the same key
    /// arrives a frame later and must not close the page.
    cancelled: Option<Instant>,
}

/// How long after Escape cancelled a capture a Back counts as the same key.
const SAME_ESCAPE: Duration = Duration::from_millis(500);

impl View {
    /// One step back: a capture ends first; true when the page is done.
    pub(in crate::ui) fn back(&mut self) -> bool {
        let same_escape = self
            .cancelled
            .take()
            .is_some_and(|at| at.elapsed() < SAME_ESCAPE);
        self.waiting.take().is_none() && !same_escape
    }

    /// Ends a capture when a text setting of the same page has the caret
    /// (`typing`), so what the player types there binds no key.
    pub(in crate::ui) fn yield_to(&mut self, typing: bool) {
        if typing {
            self.waiting = None;
        }
    }
}

/// The name of `bits` for a button, or the text for none.
fn shown(bits: u32) -> String {
    let name = key_name(bits);
    if name.is_empty() {
        texts::text("controls.none").to_owned()
    } else {
        name
    }
}

/// Whether mod `mod_id` declared a control, so its settings page shows
/// its key bindings.
pub(super) fn any(mod_id: &str) -> bool {
    any_in(&controls::rows(), mod_id)
}

/// [`any`] among `rows`, for a caller that asks for many mods in one frame.
pub(super) fn any_in(rows: &[Row], mod_id: &str) -> bool {
    rows.iter().any(|r| r.mod_id == mod_id)
}

/// The mods not installed here whose controls the joined server carries,
/// by id and name in the order the server sent them: they have no settings
/// page of their own, so the list offers their key bindings.
pub(super) fn carried(loader: &Loader, rows: &[Row]) -> Vec<(String, String)> {
    let mut mods: Vec<(String, String)> = Vec::new();
    for row in rows {
        let here = loader.mods.iter().any(|m| m.id == row.mod_id);
        if !here && !mods.iter().any(|(id, _)| *id == row.mod_id) {
            mods.push((row.mod_id.clone(), row.mod_name.clone()));
        }
    }
    mods
}

/// Draws mod `mod_id`'s key bindings, which belong to this player on this
/// computer, not to the world.
pub(super) fn body(loader: &Loader, paint: &Painter<'_>, view: &mut View, mod_id: &str) {
    let rows = controls::rows();
    look(loader, view);
    paint.label(
        texts::text("controls.title").as_bytes(),
        UiWidget::STYLE_H3,
        false,
        false,
    );
    paint.label(
        texts::text("controls.belongs").as_bytes(),
        UiWidget::STYLE_SECONDARY,
        true,
        false,
    );
    if let Some((text, ok)) = &view.message {
        let style = if *ok {
            UiWidget::STYLE_POSITIVE
        } else {
            UiWidget::STYLE_NEGATIVE
        };
        paint.label(text.as_bytes(), style, true, true);
    }
    if !controls::keys_on() {
        paint.label(
            texts::text("controls.off").as_bytes(),
            UiWidget::STYLE_WARNING,
            true,
            false,
        );
    }
    paint.label(
        texts::text("controls.game-keys").as_bytes(),
        UiWidget::STYLE_SECONDARY,
        true,
        false,
    );
    if view.waiting.is_some() {
        paint.label(
            texts::text("controls.press").as_bytes(),
            UiWidget::STYLE_POSITIVE,
            true,
            true,
        );
        if paint.button(texts::text("controls.clear").as_bytes(), true)
            && let Some((index, slot, _)) = view.waiting.take()
        {
            view.message = Some(saved(loader, index, slot, 0));
        }
    }
    // Clashes count every mod's keys; the page lists this mod's controls.
    let shared = clashes(&rows.iter().map(|r| r.keys).collect::<Vec<_>>());
    let mine: Vec<(usize, &Row)> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.mod_id == mod_id)
        .collect();
    let listing = listing(paint, PER_PAGE);
    view.first = listing.clamp_first(view.first, mine.len());
    let range = listing.shown(view.first, mine.len());
    for &(at, row) in mine.iter().take(range.end).skip(range.start) {
        control(paint, view, row, &rows, shared.get(at));
    }
    pages(paint, listing, &mut view.first, mine.len());
}

/// One control's line, its two key buttons and what it shares a key with.
fn control(
    paint: &Painter<'_>,
    view: &mut View,
    row: &Row,
    rows: &[Row],
    shared: Option<&Vec<usize>>,
) {
    paint.label(row.label.as_bytes(), UiWidget::STYLE_BODY, true, false);
    for (slot, key) in ["controls.key", "controls.second"].into_iter().enumerate() {
        let waiting = view
            .waiting
            .as_ref()
            .is_some_and(|(i, s, _)| *i == row.index && *s == slot);
        let name = if waiting {
            "…".to_owned()
        } else {
            shown(row.keys[slot])
        };
        let text = texts::fill(key, &[("key", &name)]);
        if paint.button(text.as_bytes(), !waiting) {
            let (_, down, _, escape) = capture_keys();
            view.waiting = Some((row.index, slot, Capture::new(&down, escape)));
            view.message = None;
        }
    }
    let others: Vec<String> = shared
        .into_iter()
        .flatten()
        .filter_map(|&other| rows.get(other))
        .map(|other| format!("{}: {}", other.mod_name, other.label))
        .collect();
    if !others.is_empty() {
        let line = texts::fill("controls.clash", &[("controls", &others.join(", "))]);
        paint.label(line.as_bytes(), UiWidget::STYLE_WARNING, true, false);
    }
}

/// One look of the capture: a key binds, Escape or a lost focus cancels.
fn look(loader: &Loader, view: &mut View) {
    let Some((index, slot, capture)) = view.waiting.as_mut() else {
        return;
    };
    if !controls::focused() {
        view.waiting = None;
        return;
    }
    let (keys, down, modifiers, escape) = capture_keys();
    match capture.look(&down, &keys, modifiers, escape) {
        Some(Captured::Key(key)) => {
            let (index, slot) = (*index, *slot);
            view.waiting = None;
            view.message = Some(saved(loader, index, slot, key.bits()));
        }
        Some(Captured::Cancelled) => {
            view.waiting = None;
            view.cancelled = Some(Instant::now());
        }
        None => {}
    }
}

/// Sets the key and says whether the file took it.
fn saved(loader: &Loader, index: usize, slot: usize, bits: u32) -> (String, bool) {
    let reason = match controls::set_key(loader, index, slot, bits) {
        Ok(()) => return (texts::text("controls.saved").to_owned(), true),
        Err(SetKeyError::Unread) => texts::fill("controls.unread", &[("file", bindings::FILE)]),
        Err(SetKeyError::Io(why)) => why.to_string(),
    };
    (
        texts::fill("controls.not-saved", &[("reason", &reason)]),
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_capture_ends_when_a_text_setting_takes_the_caret() {
        let waiting = || View {
            waiting: Some((0, 0, Capture::new(&[], false))),
            ..View::default()
        };
        let mut view = waiting();
        view.yield_to(false);
        assert!(view.waiting.is_some(), "no field has the caret");
        view.yield_to(true);
        assert!(view.waiting.is_none(), "typing binds no key");
        assert!(view.back(), "and Back leaves the page at once");
    }
}
