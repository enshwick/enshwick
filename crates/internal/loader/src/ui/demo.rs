//! `enshwick ui-demo <player>` on the console: a window of every widget the loader draws, for a
//! visual check in game. An administrator's command on the server tells the
//! client's loader to open it, with where the administrator stood; the
//! window has four pages, the last one showing samples on the HUD and a
//! marker on the map, and closes with its Close button.

use core::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};

use enshwick_loader_api::abi::{UiDialog, UiWidget};

use super::Painter;
use super::engine::{Engine, Window};

/// Asked to open, from the thread that takes the server's chat.
static OPEN: AtomicBool = AtomicBool::new(false);
/// Where the administrator stood when they asked, in 32.32 fixed-point
/// metres.
static WHERE: Mutex<Option<[i64; 3]>> = Mutex::new(None);
/// The HUD samples show, and how full their bar is, for the HUD's frame.
static HUD_SAMPLES: Mutex<Option<f32>> = Mutex::new(None);

/// The demo's widget state.
struct Demo {
    page: usize,
    on: bool,
    stops: f32,
    smooth: f32,
    pick: u32,
    answer: Option<&'static str>,
    hud: bool,
    marker: bool,
    window: Window,
}

thread_local! {
    static DEMO: RefCell<Option<Demo>> = const { RefCell::new(None) };
}

/// The answer to the demo's dialog, waiting for the thread that draws.
static ANSWER: Mutex<Option<&'static str>> = Mutex::new(None);

/// Opens the demo at its next frame; `at` is where the administrator stood.
pub(crate) fn open(at: Option<[i64; 3]>) {
    *WHERE.lock().unwrap_or_else(PoisonError::into_inner) = at;
    OPEN.store(true, Ordering::Release);
}

/// The HUD samples while they show, for the HUD's frame: a label at the top
/// right, a bar under it, and a label at the bottom left.
pub(super) fn hud_samples(items: &mut Vec<super::HudItem>) {
    let Some(fill) = *HUD_SAMPLES.lock().unwrap_or_else(PoisonError::into_inner) else {
        return;
    };
    let label = |anchor: u32, words: &[u8], style: u32| super::HudItem {
        anchor,
        kind: UiWidget::LABEL,
        style,
        text: super::copy(&[words]),
        value: 0.0,
    };
    items.push(label(
        UiWidget::TOP_RIGHT,
        b"Enshwick HUD sample",
        UiWidget::STYLE_BODY,
    ));
    items.push(super::HudItem {
        anchor: UiWidget::TOP_RIGHT,
        kind: UiWidget::PROGRESS,
        style: 0,
        text: super::engine::UiText::EMPTY,
        value: fill,
    });
    items.push(label(
        UiWidget::BOTTOM_LEFT,
        b"Bottom left, in the warning style",
        UiWidget::STYLE_WARNING,
    ));
}

const PAGES: usize = 4;
const OPTIONS: [&[u8]; 3] = [b"First", b"Second", b"Third"];

fn answer_name(raw: Option<u32>) -> &'static str {
    match raw {
        Some(UiDialog::OK) => "OK",
        Some(UiDialog::CANCEL) => "Cancel",
        Some(_) => "another button",
        None => "closed",
    }
}

/// Draws the demo when it is open.
///
/// # Safety
///
/// Inside a frame, on its thread, with no other window open.
pub(super) unsafe fn draw(engine: &Engine, context: usize) {
    if OPEN.swap(false, Ordering::AcqRel) {
        DEMO.set(Some(Demo {
            page: 0,
            on: true,
            stops: 5.0,
            smooth: 0.25,
            pick: 0,
            answer: None,
            hud: false,
            marker: false,
            window: Window::new(),
        }));
        crate::log!(INFO, "interface: the demo opened");
    }
    DEMO.with_borrow_mut(|slot| {
        let Some(demo) = slot.as_mut() else {
            return;
        };
        if let Some(answer) = ANSWER.lock().unwrap_or_else(PoisonError::into_inner).take() {
            demo.answer = Some(answer);
        }
        // SAFETY: forwarded from the caller.
        let close = unsafe { page(engine, context, demo) };
        *HUD_SAMPLES.lock().unwrap_or_else(PoisonError::into_inner) =
            (demo.hud && !close).then_some(demo.smooth);
        if close {
            if demo.marker {
                let _cleared = super::map::set(crate::chains::LOADER, &[]);
            }
            *slot = None;
            crate::log!(INFO, "interface: the demo closed");
        }
    });
}

/// Draws the demo's window; true when the player closed it.
///
/// # Safety
///
/// Inside a frame, on its thread, with no other window open.
unsafe fn page(engine: &Engine, context: usize, demo: &mut Demo) -> bool {
    let title = super::copy(&[b"Enshwick interface demo"]);
    let name = super::copy(&[b"enshwick/loader/ui-demo"]);
    // SAFETY: forwarded from the caller; the window object lives in the
    // demo's state until the window ends below.
    unsafe { engine.window_begin(context, &mut demo.window, &name, &title) };
    // SAFETY: inside the window just started, until it ends below.
    let paint = unsafe { Painter::new(engine, context) };
    match demo.page {
        0 => text_page(&paint),
        1 => rows_page(&paint, demo),
        2 => buttons_page(&paint, demo),
        _ => hud_page(&paint, demo),
    }
    if demo.page + 1 < PAGES && paint.button(b"Next page", true) {
        demo.page += 1;
    }
    if demo.page > 0 && paint.button(b"Previous page", true) {
        demo.page -= 1;
    }
    let close = paint.button(b"Close", true);
    // SAFETY: the window started above, in this frame.
    unsafe { engine.window_end(context, &mut demo.window) };
    close
}

fn text_page(paint: &Painter<'_>) {
    paint.label(b"Page 1 of 4: text styles", UiWidget::STYLE_H3, false, true);
    for (words, style) in [
        (&b"Body text"[..], UiWidget::STYLE_BODY),
        (b"Secondary text", UiWidget::STYLE_SECONDARY),
        (b"Heading 1", UiWidget::STYLE_H1),
        (b"Heading 2", UiWidget::STYLE_H2),
        (b"Warning", UiWidget::STYLE_WARNING),
        (b"Positive", UiWidget::STYLE_POSITIVE),
        (b"Negative", UiWidget::STYLE_NEGATIVE),
    ] {
        paint.label(words, style, false, false);
    }
    paint.label(
        "A long body line that wraps at the width of the window, with Ukrainian letters: \u{0416}\u{0438}\u{0432}\u{0456} \u{0432}\u{043e}\u{0433}\u{043d}\u{0456}, \u{0436}\u{0438}\u{0432}\u{0456} \u{0441}\u{0435}\u{043b}\u{0430}.".as_bytes(),
        UiWidget::STYLE_BODY,
        true,
        false,
    );
    paint.label(b"A centred line", UiWidget::STYLE_SECONDARY, false, true);
}

fn rows_page(paint: &Painter<'_>, demo: &mut Demo) {
    paint.label(
        b"Page 2 of 4: settings rows",
        UiWidget::STYLE_H3,
        false,
        true,
    );
    if let Some(on) = paint.toggle(b"Toggle", demo.on, false) {
        demo.on = on;
    }
    let _fixed = paint.toggle(b"Toggle, read-only", demo.on, true);
    if let Some(value) = paint.slider(
        b"Slider, 0 to 10 in 11 stops",
        demo.stops,
        (0.0, 10.0),
        11,
        false,
    ) {
        demo.stops = value;
    }
    if let Some(value) = paint.slider(b"Slider, continuous", demo.smooth, (0.0, 1.0), 0, false) {
        demo.smooth = value;
    }
    let _fixed = paint.slider(b"Slider, read-only", demo.stops, (0.0, 10.0), 11, true);
    if let Some(pick) = paint.dropdown(b"Dropdown", &OPTIONS, demo.pick, false) {
        demo.pick = pick;
    }
    paint.label(
        b"A progress bar at the continuous slider's value",
        UiWidget::STYLE_SECONDARY,
        false,
        false,
    );
    paint.progress(demo.smooth, 0.0, 1.0);
}

fn hud_page(paint: &Painter<'_>, demo: &mut Demo) {
    paint.label(
        b"Page 4 of 4: the HUD and the map",
        UiWidget::STYLE_H3,
        false,
        true,
    );
    paint.label(
        b"Close this window to see the HUD samples: a line and a bar at the top right, a line at the bottom left. They hide with the HUD, in menus and in screenshot mode.",
        UiWidget::STYLE_SECONDARY,
        true,
        false,
    );
    if let Some(on) = paint.toggle(b"Show HUD samples", demo.hud, false) {
        demo.hud = on;
    }
    let at = *WHERE.lock().unwrap_or_else(PoisonError::into_inner);
    let Some(position) = at else {
        paint.label(
            b"The server sent no position for a marker.",
            UiWidget::STYLE_WARNING,
            true,
            false,
        );
        return;
    };
    if let Some(on) = paint.toggle(
        b"Show a map marker where you typed the command",
        demo.marker,
        false,
    ) {
        demo.marker = on;
        let marker = enshwick_loader_api::abi::UiMarker {
            header: enshwick_loader_api::abi::Header::of::<enshwick_loader_api::abi::UiMarker>(),
            position,
            label: enshwick_loader_api::abi::Str::new("Enshwick demo marker"),
            color: enshwick_loader_api::abi::UiMarker::GREEN,
            reserved: 0,
        };
        let markers: &[_] = if on { &[marker] } else { &[] };
        let _set = super::map::set(crate::chains::LOADER, markers);
    }
}

fn buttons_page(paint: &Painter<'_>, demo: &Demo) {
    paint.label(
        b"Page 3 of 4: buttons, sounds and the dialog",
        UiWidget::STYLE_H3,
        false,
        true,
    );
    for (words, sound) in [
        (&b"Play click"[..], UiWidget::SOUND_CLICK),
        (b"Play confirm", UiWidget::SOUND_CONFIRM),
        (b"Play back", UiWidget::SOUND_BACK),
        (b"Play error", UiWidget::SOUND_ERROR),
    ] {
        if paint.button(words, true) {
            paint.sound(sound);
        }
    }
    let _never = paint.button(b"Disabled button", false);
    if paint.button(b"Show a dialog", true) {
        super::show(
            "Enshwick",
            "This is a dialog in the game's own style.",
            "Its buttons are the game's, in your language. The answer shows on this page and in the log.",
            1 << UiDialog::OK | 1 << UiDialog::CANCEL,
            |raw| {
                let name = answer_name(raw);
                crate::log!(INFO, "interface: the demo dialog was answered {name}");
                *ANSWER.lock().unwrap_or_else(PoisonError::into_inner) = Some(name);
            },
        );
    }
    let last = match demo.answer {
        Some(name) => format!("Last answer: {name}"),
        None => "No answer yet".to_owned(),
    };
    paint.label(last.as_bytes(), UiWidget::STYLE_SECONDARY, false, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_named() {
        assert_eq!(answer_name(Some(UiDialog::OK)), "OK");
        assert_eq!(answer_name(Some(UiDialog::CANCEL)), "Cancel");
        assert_eq!(answer_name(None), "closed");
    }
}
