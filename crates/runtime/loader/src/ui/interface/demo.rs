//! `enshwick ui-demo <player>` on the console: a window of every widget the loader draws, for a
//! visual check in game. An administrator's command on the server tells the
//! client's loader to open it, with where the administrator stood; the
//! window has four pages, the last one showing samples on the HUD and a
//! marker on the map, and closes with its Close button.

use core::cell::RefCell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::{UiDialog, UiWidget};

use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

use super::engine::{Engine, Window};
use super::{Painter, Widgets};

/// What other threads hand the interface's frames: the server's chat thread
/// a request to open, the dialog's thread its answer.
#[derive(Debug)]
struct State {
    /// Asked to open: read without the lock each frame, set and cleared
    /// only under it, so a request and its place change together.
    open: AtomicBool,
    shared: Mutex<Shared>,
}

/// What the demo shares behind one lock.
#[derive(Debug)]
struct Shared {
    /// Where the administrator stood when they asked, in 32.32 fixed-point
    /// metres.
    at: Option<[i64; 3]>,
    /// The answer to the demo's dialog, waiting for the thread that draws.
    answer: Option<&'static str>,
}

static STATE: State = State {
    open: AtomicBool::new(false),
    shared: Mutex::new(Shared {
        at: None,
        answer: None,
    }),
};

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
    /// Where the administrator stood when they opened this demo.
    at: Option<[i64; 3]>,
    window: Window,
}

thread_local! {
    static DEMO: RefCell<Option<Demo>> = const { RefCell::new(None) };
}

/// Opens the demo at its next frame; `at` is where the administrator stood.
pub(crate) fn open(at: Option<[i64; 3]>) {
    let mut shared = STATE.shared.relock();
    shared.at = at;
    STATE.open.store(true, Ordering::Release);
}

/// Forgets a request to open that no frame drew yet, with the place it was
/// asked from, when the client changes servers or a hosted game ends; an
/// open demo keeps its own place and marker toggle.
pub(crate) fn forget() {
    let mut shared = STATE.shared.relock();
    STATE.open.store(false, Ordering::Release);
    shared.at = None;
}

/// A request to open the demo that a frame took.
#[derive(Debug, PartialEq, Eq)]
struct Opening {
    /// Where the administrator stood when they asked.
    at: Option<[i64; 3]>,
}

/// Takes a request to open with its place, dropping an answer meant for an
/// earlier demo's dialog.
fn take_open() -> Option<Opening> {
    if !STATE.open.load(Ordering::Acquire) {
        return None;
    }
    let mut shared = STATE.shared.relock();
    if !STATE.open.swap(false, Ordering::AcqRel) {
        return None;
    }
    shared.answer = None;
    Some(Opening {
        at: shared.at.take(),
    })
}

/// Starts a fresh demo at `at`; true when the demo it replaced had its map
/// marker on, which the caller then clears.
fn start(at: Option<[i64; 3]>) -> bool {
    let old = DEMO.replace(Some(Demo {
        page: 0,
        on: true,
        stops: 5.0,
        smooth: 0.25,
        pick: 0,
        answer: None,
        hud: false,
        marker: false,
        at,
        window: Window::new(),
    }));
    old.is_some_and(|old| old.marker)
}

/// The HUD samples while they show, for the HUD's frame: a label at the top
/// right, a bar under it, and a label at the bottom left.
pub(super) fn hud_samples(items: &mut Vec<super::HudItem>) {
    // The HUD draws under its own fuse: a demo turned off by a panic must not
    // leave its samples on the HUD.
    if super::DEMO_FUSE.is_off() {
        return;
    }
    let Some(fill) = DEMO.with_borrow(|demo| {
        demo.as_ref()
            .filter(|demo| demo.hud)
            .map(|demo| demo.smooth)
    }) else {
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
    crate::engine::texts::fill(key, &[])
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
    if let Some(opening) = take_open() {
        if start(opening.at) {
            let _cleared = super::map::set(ModIndex::LOADER, &[]);
        }
        crate::log!(INFO, "interface: the demo opened");
    }
    DEMO.with_borrow_mut(|slot| {
        let Some(demo) = slot.as_mut() else {
            return;
        };
        if let Some(answer) = STATE.shared.relock().answer.take() {
            demo.answer = Some(answer);
        }
        // SAFETY: this function's `# Safety` gives the frame, its thread, its
        // `context` and no other open window, which `page` asks for.
        let close = unsafe { page(engine, context, demo) };
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
    let line = crate::engine::texts::fill(
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
    let Some(position) = demo.at else {
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
        let marker = enshwick_core::abi::UiMarker {
            header: enshwick_core::abi::Header::of::<enshwick_core::abi::UiMarker>(),
            position,
            label: enshwick_core::abi::Str::new(&label),
            color: enshwick_core::abi::UiMarker::GREEN,
            marker_type: 0,
        };
        let fast_travel = crate::engine::loader::loader()
            .and_then(|loader| crate::world::scene::marker_types(loader).ok())
            .and_then(|types| {
                types
                    .iter()
                    .find(|ty| ty.is_fast_travel_destination != 0)
                    .map(|ty| ty.marker_id)
            });
        let mut markers = Vec::new();
        if on {
            markers.push(marker);
            markers.extend(fast_travel.map(|marker_type| enshwick_core::abi::UiMarker {
                position: [
                    position[0].saturating_add(20 << 32),
                    position[1],
                    position[2],
                ],
                marker_type,
                ..marker
            }));
        }
        let _set = super::map::set(ModIndex::LOADER, &markers);
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
                STATE.shared.relock().answer = Some(name);
            },
        );
    }
    let last = match demo.answer {
        Some(name) => {
            crate::engine::texts::fill("demo.answer", &[("answer", &words(answer_key(name)))])
        }
        None => words("demo.answer.none"),
    };
    paint.label(last.as_bytes(), UiWidget::STYLE_SECONDARY, false, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One test at a time on the demo's process-wide request.
    static SHARED: Mutex<()> = Mutex::new(());

    #[test]
    fn answers_are_named() {
        assert_eq!(answer_name(Some(UiDialog::OK)), "OK");
        assert_eq!(answer_name(Some(UiDialog::CANCEL)), "Cancel");
        assert_eq!(answer_name(None), "closed");
    }

    #[test]
    fn a_hosted_games_end_forgets_the_demos_request_and_its_place() {
        let _shared = SHARED.relock();
        open(Some([1, 2, 3]));
        let loader: &'static crate::engine::loader::Loader =
            Box::leak(Box::new(crate::start::tests::bare_loader()));
        crate::engine::parts::Part::forget_in(crate::ui::PARTS, "ui", loader);
        assert_eq!(take_open(), None, "no open is left to draw");
        assert_eq!(
            STATE.shared.relock().at,
            None,
            "the old world's place went with it"
        );
    }

    #[test]
    fn joining_another_server_forgets_the_demos_request_and_its_place() {
        let _shared = SHARED.relock();
        open(Some([1, 2, 3]));
        crate::base::net::joining(false);
        assert_eq!(take_open(), None, "no open is left to draw");
        assert_eq!(
            STATE.shared.relock().at,
            None,
            "the old server's place went with it"
        );
    }

    #[test]
    fn a_new_demo_takes_its_place_and_drops_an_earlier_demos_answer() {
        let _shared = SHARED.relock();
        STATE.shared.relock().answer = Some("OK");
        open(Some([4, 5, 6]));
        assert_eq!(
            take_open(),
            Some(Opening {
                at: Some([4, 5, 6])
            })
        );
        assert_eq!(
            STATE.shared.relock().answer,
            None,
            "the old answer is not shown"
        );
        assert_eq!(take_open(), None, "a request opens one demo");
    }

    #[test]
    fn a_new_demo_over_one_with_its_marker_on_asks_to_clear_the_marker() {
        assert!(!start(Some([1, 2, 3])), "no demo was open");
        DEMO.with_borrow_mut(|demo| demo.as_mut().map(|demo| demo.marker = true));
        assert!(start(None), "the old demo's marker is cleared");
        assert!(
            DEMO.with_borrow(|demo| demo.as_ref().is_some_and(|d| !d.marker && d.at.is_none())),
            "the new demo has no marker and no place"
        );
        DEMO.set(None);
    }

    #[test]
    fn every_answer_has_a_text_of_its_own() {
        let raws = [Some(UiDialog::OK), Some(UiDialog::CANCEL), Some(99), None];
        let mut keys: Vec<&str> = raws.map(|raw| answer_key(answer_name(raw))).to_vec();
        keys.dedup();
        assert_eq!(keys.len(), raws.len());
    }
}
