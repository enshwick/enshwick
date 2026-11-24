//! The one way the loader asks the local player something on a client: its
//! own dialog, or the mismatch screen where the client draws it. The
//! interface that shows them sits in a layer above and names itself here
//! when it seals.

use std::sync::OnceLock;

/// What hears a dialog's answer, or `None` when it closed without one.
pub(crate) type Answer = Box<dyn FnOnce(Option<u32>) + Send>;

/// Dialogs whose texts stay readable after they showed, so a text the game
/// still draws is never freed under it.
pub(crate) const KEPT: usize = 4;

/// The GUIDs of the loader's dialog text tags: title and message per slot.
#[expect(clippy::cast_possible_truncation, reason = "the index fits a byte")]
pub(crate) const TAGS: [[u8; 16]; 2 * KEPT] = {
    let mut tags = [[0u8; 16]; 2 * KEPT];
    let mut i = 0;
    while i < 2 * KEPT {
        // `e5c8a7d2-6b1f-4f0e-9c3a-0e57<index>`, beside the notices' range.
        tags[i] = [
            0xe5, 0xc8, 0xa7, 0xd2, 0x6b, 0x1f, 0x4f, 0x0e, 0x9c, 0x3a, 0x0e, 0x57, 0x4e, 0x0c,
            0x12, i as u8,
        ];
        i += 1;
    }
    tags
};

/// The id of the dialog text tag at `index` of [`TAGS`].
pub(crate) fn tag_id(index: usize) -> u32 {
    enshwick_gamedata::reflect::fnv1a32(&TAGS[index])
}

/// The ids of the dialog text tags.
pub(crate) fn tag_ids() -> impl Iterator<Item = u32> {
    (0..TAGS.len()).map(tag_id)
}

/// What the interface shows the loader's screens with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Screens {
    /// Shows a dialog: title, message, detail, the buttons' bits and what
    /// hears the answer.
    pub(crate) show: fn(&str, &str, &str, u32, Answer),
    /// Whether the mismatch screen can show on this build.
    pub(crate) mismatch_screen_on: fn() -> bool,
    /// Whether the loader or a mod showed a window or a dialog in the last
    /// frame; on the interface thread.
    pub(crate) window_shown: fn() -> bool,
    /// The text of a dialog text tag, by its id, while its dialog is kept.
    pub(crate) text: fn(u32) -> Option<&'static str>,
}

/// What the dialogs keep for the process.
struct State {
    /// The interface's screens, once it sealed.
    screens: OnceLock<Screens>,
}

impl State {
    const fn new() -> Self {
        Self {
            screens: OnceLock::new(),
        }
    }

    /// Names `screens`; the first one named stays.
    fn on_screens(&self, screens: Screens) {
        let _first = self.screens.set(screens);
    }

    /// Shows a dialog through the named screens; nothing without them.
    fn show(&self, title: &str, message: &str, detail: &str, buttons: u32, on_answer: Answer) {
        if let Some(screens) = self.screens.get() {
            (screens.show)(title, message, detail, buttons, on_answer);
        }
    }

    /// Whether the named screens can show the mismatch screen; false
    /// without them.
    fn mismatch_screen_on(&self) -> bool {
        self.screens
            .get()
            .is_some_and(|screens| (screens.mismatch_screen_on)())
    }

    /// Whether the named screens showed a window or a dialog in the last
    /// frame; false without them.
    fn window_shown(&self) -> bool {
        self.screens
            .get()
            .is_some_and(|screens| (screens.window_shown)())
    }

    /// The named screens' text of a dialog text tag; `None` without them.
    fn text(&self, id: u32) -> Option<&'static str> {
        (self.screens.get()?.text)(id)
    }
}

static STATE: State = State::new();

/// Names `screens` as what shows the loader's dialogs and screens.
pub(crate) fn on_screens(screens: Screens) {
    STATE.on_screens(screens);
}

/// Shows the loader's own dialog, with `on_answer` hearing the answer, or
/// `None` when it closed without one.
pub(crate) fn show(
    title: &str,
    message: &str,
    detail: &str,
    buttons: u32,
    on_answer: impl FnOnce(Option<u32>) + Send + 'static,
) {
    STATE.show(title, message, detail, buttons, Box::new(on_answer));
}

/// Whether the mismatch screen can show on this build: false without an
/// interface.
pub(crate) fn mismatch_screen_on() -> bool {
    STATE.mismatch_screen_on()
}

/// Whether the loader or a mod showed a window or a dialog in the last
/// frame, which takes the player's keys from mod controls; false without an
/// interface. On the interface thread.
pub(crate) fn window_shown() -> bool {
    STATE.window_shown()
}

/// The text of a dialog text tag, by its id, while its dialog is kept;
/// `None` without an interface.
pub(crate) fn text(id: u32) -> Option<&'static str> {
    STATE.text(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dialog_and_the_mismatch_screen_go_through_the_interface_that_named_itself() {
        use std::sync::atomic::{AtomicU32, Ordering};

        static SHOWN: AtomicU32 = AtomicU32::new(0);
        static ASKED: AtomicU32 = AtomicU32::new(0);
        let state = State::new();
        assert!(!state.mismatch_screen_on(), "no interface names itself yet");
        state.show("test dialog", "message", "", 1, Box::new(|_| {}));
        state.on_screens(Screens {
            show: |title, _, _, _, on_answer| {
                if title == "test dialog" {
                    SHOWN.fetch_add(1, Ordering::Relaxed);
                    on_answer(Some(2));
                }
            },
            mismatch_screen_on: || {
                ASKED.fetch_add(1, Ordering::Relaxed);
                false
            },
            window_shown: || true,
            text: |id| (id == 7).then_some("seven"),
        });
        state.on_screens(Screens {
            show: |_, _, _, _, _| panic!("the first interface stays"),
            mismatch_screen_on: || true,
            window_shown: || false,
            text: |_| None,
        });
        state.show(
            "test dialog",
            "message",
            "",
            1,
            Box::new(|answer| assert_eq!(answer, Some(2))),
        );
        assert_eq!(SHOWN.load(Ordering::Relaxed), 1);
        assert!(!state.mismatch_screen_on());
        assert_eq!(ASKED.load(Ordering::Relaxed), 1);
        assert!(state.window_shown(), "the first interface's window answers");
        assert_eq!(state.text(7), Some("seven"));
        assert_eq!(state.text(8), None);
    }

    #[test]
    fn without_an_interface_no_window_shows_and_no_dialog_text_reads() {
        let state = State::new();
        assert!(!state.window_shown());
        assert_eq!(state.text(tag_id(0)), None);
    }

    #[test]
    fn each_dialog_text_tag_has_an_id_of_its_own() {
        let ids: rustc_hash::FxHashSet<u32> = tag_ids().collect();
        assert_eq!(ids.len(), TAGS.len());
    }
}
