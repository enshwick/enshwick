//! `enshwick ui-demo <player>` on the console: a window of every widget the loader draws, for a
//! visual check in game. An administrator's command on the server tells the
//! client's loader to open it, with where the administrator stood; the
//! window has four pages, the last one showing samples on the HUD and a
//! marker on the map, and closes with its Close button.

use core::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{UiDialog, UiWidget};

use crate::locks::Relock;
use crate::mod_index::ModIndex;

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
    *WHERE.relock() = at;
    OPEN.store(true, Ordering::Release);
}

/// The HUD samples while they show, for the HUD's frame: a label at the top
/// right, a bar under it, and a label at the bottom left.
pub(super) fn hud_samples(items: &mut Vec<super::HudItem>) {
    let Some(fill) = *HUD_SAMPLES.relock() else {
        return;
    };
    let label = |anchor: u32, key: &str, style: u32| super::HudItem {
        anchor,
        kind: UiWidget::LABEL,
        style,
        text: super::copy(&[words(key).as_bytes()]),
        value: 0.0,
    };
    items.push(label(
        UiWidget::TOP_RIGHT,
        "demo.hud.sample",
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
        "demo.hud.warning",
        UiWidget::STYLE_WARNING,
    ));
}

/// The topic of each page, by the key of its text.
const PAGES: [&str; 4] = [
    "demo.topic.text",
    "demo.topic.rows",
    "demo.topic.buttons",
    "demo.topic.hud",
];
const OPTIONS: [&str; 3] = [
    "demo.option.first",
    "demo.option.second",
    "demo.option.third",
];

/// The text of `key` in the game's language.
fn words(key: &str) -> String {
    crate::texts::fill(key, &[])
}

fn answer_name(raw: Option<u32>) -> &'static str {
    match raw {
        Some(UiDialog::OK) => "OK",
        Some(UiDialog::CANCEL) => "Cancel",
        Some(_) => "another button",
        None => "closed",
    }
}

/// The key of the text that names an answer [`answer_name`] named.
fn answer_key(name: &str) -> &'static str {
    match name {
        "OK" => "demo.answer.ok",
        "Cancel" => "demo.answer.cancel",
        "closed" => "demo.answer.closed",
        _ => "demo.answer.other",
    }
}

/// Draws the demo when it is open.
///
/// # Safety
///
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
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
        if let Some(answer) = ANSWER.relock().take() {
            demo.answer = Some(answer);
        }
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window, which `page` asks for.
        let close = unsafe { page(engine, context, demo) };
        *HUD_SAMPLES.relock() = (demo.hud && !close).then_some(demo.smooth);
        if close {
            if demo.marker {
                let _cleared = super::map::set(ModIndex::LOADER, &[]);
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
/// - Called inside a frame of the game's interface update, on its thread.
/// - `context` is the non-zero context `Engine::context` read in this frame.
/// - No other window is open.
unsafe fn page(engine: &Engine, context: usize, demo: &mut Demo) -> bool {
    let title = super::copy(&[words("demo.title").as_bytes()]);
    let name = super::copy(&[b"enshwick/loader/ui-demo"]);
    // SAFETY: this function's `# Safety` gives the frame, its thread and its
    // `context`; `demo.window` stays in the thread-local demo state, unmoved,
    // until `window_end` below, and `name` and `title` live in the frame's
    // text arena.
    unsafe { engine.window_begin(context, &mut demo.window, &name, &title) };
    // SAFETY: the window started one call earlier in this frame, on its
    // thread, with the frame's `context`, and `paint` is not used after
    // `window_end` below.
    let paint = unsafe { Painter::new(engine, context) };
    match demo.page {
        0 => text_page(&paint),
        1 => rows_page(&paint, demo),
        2 => buttons_page(&paint, demo),
        _ => hud_page(&paint, demo),
    }
    if demo.page + 1 < PAGES.len() && paint.button(words("mods.page.next").as_bytes(), true) {
        demo.page += 1;
    }
    if demo.page > 0 && paint.button(words("mods.page.previous").as_bytes(), true) {
        demo.page -= 1;
    }
    let close = paint.button(words("actions.close").as_bytes(), true);
    // SAFETY: `window_begin` started `demo.window` in this frame, on this
    // thread, with this `context`; it did not move, and the painter's widgets
    // close every frame they open.
    unsafe { engine.window_end(context, &mut demo.window) };
    close
}

/// The heading of page `page`: its number of all and its topic.
fn heading(paint: &Painter<'_>, page: usize) {
    let line = crate::texts::fill(
        "demo.page",
        &[
            ("page", &(page + 1).to_string()),
            ("pages", &PAGES.len().to_string()),
            ("topic", &words(PAGES[page])),
        ],
    );
    paint.label(line.as_bytes(), UiWidget::STYLE_H3, false, true);
}

fn text_page(paint: &Painter<'_>) {
    heading(paint, 0);
    for (key, style) in [
        ("demo.style.body", UiWidget::STYLE_BODY),
        ("demo.style.secondary", UiWidget::STYLE_SECONDARY),
        ("demo.style.h1", UiWidget::STYLE_H1),
        ("demo.style.h2", UiWidget::STYLE_H2),
        ("demo.style.warning", UiWidget::STYLE_WARNING),
        ("demo.style.positive", UiWidget::STYLE_POSITIVE),
        ("demo.style.negative", UiWidget::STYLE_NEGATIVE),
    ] {
        paint.label(words(key).as_bytes(), style, false, false);
    }
    paint.label(
        words("demo.wrap").as_bytes(),
        UiWidget::STYLE_BODY,
        true,
        false,
    );
    paint.label(
        words("demo.centred").as_bytes(),
        UiWidget::STYLE_SECONDARY,
        false,
        true,
    );
}

fn rows_page(paint: &Painter<'_>, demo: &mut Demo) {
    heading(paint, 1);
    if let Some(on) = paint.toggle(words("demo.toggle").as_bytes(), demo.on, false) {
        demo.on = on;
    }
    let _fixed = paint.toggle(words("demo.toggle.fixed").as_bytes(), demo.on, true);
    if let Some(value) = paint.slider(
        words("demo.slider.stops").as_bytes(),
        demo.stops,
        (0.0, 10.0),
        11,
        false,
    ) {
        demo.stops = value;
    }
    if let Some(value) = paint.slider(
        words("demo.slider.smooth").as_bytes(),
        demo.smooth,
        (0.0, 1.0),
        0,
        false,
    ) {
        demo.smooth = value;
    }
    let _fixed = paint.slider(
        words("demo.slider.fixed").as_bytes(),
        demo.stops,
        (0.0, 10.0),
        11,
        true,
    );
    let options = OPTIONS.map(words);
    let options = options.each_ref().map(String::as_bytes);
    if let Some(pick) = paint.dropdown(
        words("demo.dropdown").as_bytes(),
        &options,
        demo.pick,
        false,
    ) {
        demo.pick = pick;
    }
    paint.label(
        words("demo.progress").as_bytes(),
        UiWidget::STYLE_SECONDARY,
        false,
        false,
    );
    paint.progress(demo.smooth, 0.0, 1.0);
}

fn hud_page(paint: &Painter<'_>, demo: &mut Demo) {
    heading(paint, 3);
    paint.label(
        words("demo.hud.help").as_bytes(),
        UiWidget::STYLE_SECONDARY,
        true,
        false,
    );
    if let Some(on) = paint.toggle(words("demo.hud.show").as_bytes(), demo.hud, false) {
        demo.hud = on;
    }
    let at = *WHERE.relock();
    let Some(position) = at else {
        paint.label(
            words("demo.marker.none").as_bytes(),
            UiWidget::STYLE_WARNING,
            true,
            false,
        );
        return;
    };
    if let Some(on) = paint.toggle(words("demo.marker.show").as_bytes(), demo.marker, false) {
        demo.marker = on;
        let label = words("demo.marker.label");
        let marker = enshwick::abi::UiMarker {
            header: enshwick::abi::Header::of::<enshwick::abi::UiMarker>(),
            position,
            label: enshwick::abi::Str::new(&label),
            color: enshwick::abi::UiMarker::GREEN,
            reserved: 0,
        };
        let markers: &[_] = if on { &[marker] } else { &[] };
        let _set = super::map::set(ModIndex::LOADER, markers);
    }
}

fn buttons_page(paint: &Painter<'_>, demo: &Demo) {
    heading(paint, 2);
    for (key, sound) in [
        ("demo.play.click", UiWidget::SOUND_CLICK),
        ("demo.play.confirm", UiWidget::SOUND_CONFIRM),
        ("demo.play.back", UiWidget::SOUND_BACK),
        ("demo.play.error", UiWidget::SOUND_ERROR),
    ] {
        if paint.button(words(key).as_bytes(), true) {
            paint.sound(sound);
        }
    }
    let _never = paint.button(words("demo.button.disabled").as_bytes(), false);
    if paint.button(words("demo.dialog.show").as_bytes(), true) {
        super::show(
            "Enshwick",
            &words("demo.dialog.message"),
            &words("demo.dialog.detail"),
            1 << UiDialog::OK | 1 << UiDialog::CANCEL,
            |raw| {
                let name = answer_name(raw);
                crate::log!(INFO, "interface: the demo dialog was answered {name}");
                *ANSWER.relock() = Some(name);
            },
        );
    }
    let last = match demo.answer {
        Some(name) => crate::texts::fill("demo.answer", &[("answer", &words(answer_key(name)))]),
        None => words("demo.answer.none"),
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

    #[test]
    fn every_answer_has_a_text_of_its_own() {
        let raws = [Some(UiDialog::OK), Some(UiDialog::CANCEL), Some(99), None];
        let mut keys: Vec<&str> = raws.map(|raw| answer_key(answer_name(raw))).to_vec();
        keys.dedup();
        assert_eq!(keys.len(), raws.len());
    }
}
