//! The controls page: every mod's controls under the mod's name, their
//! keys in the player's layout, a click that waits for the next key, and
//! the keys two controls share.

use std::time::{Duration, Instant};

use enshwick::abi::UiWidget;

use super::list::{listing, pages};
use crate::bindings::{self, Capture, Captured, clashes};
use crate::controls::{self, Row, SetKeyError, capture_keys, key_name};
use crate::host::Loader;
use crate::texts;
use crate::ui::Painter;

/// Controls a page shows.
const PER_PAGE: usize = 6;

/// What the controls page shows and what it waits for.
#[derive(Debug, Default)]
pub(in crate::ui) struct View {
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

/// Whether any mod declared a control, so the Mods screen offers the page.
pub(super) fn any() -> bool {
    !controls::rows().is_empty()
}

/// Draws the page.
pub(super) fn body(loader: &Loader, paint: &Painter<'_>, view: &mut View) {
    let rows = controls::rows();
    look(loader, view);
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
    let shared = clashes(&rows.iter().map(|r| r.keys).collect::<Vec<_>>());
    let listing = listing(paint, PER_PAGE);
    view.first = listing.clamp_first(view.first, rows.len());
    let range = listing.shown(view.first, rows.len());
    let mut last_owner = None;
    for row in rows.get(range).unwrap_or_default() {
        if last_owner != Some(row.owner) {
            last_owner = Some(row.owner);
            let name = loader
                .mods
                .get(row.owner.get())
                .map(crate::host::ModRecord::label)
                .unwrap_or_default();
            paint.label(name.as_bytes(), UiWidget::STYLE_H3, false, false);
        }
        control(loader, paint, view, row, &rows, shared.get(row.index));
    }
    pages(paint, listing, &mut view.first, rows.len());
}

/// One control's line, its two key buttons and what it shares a key with.
fn control(
    loader: &Loader,
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
        .map(|other| {
            let name = loader
                .mods
                .get(other.owner.get())
                .map(crate::host::ModRecord::label)
                .unwrap_or_default();
            format!("{name}: {}", other.label)
        })
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
