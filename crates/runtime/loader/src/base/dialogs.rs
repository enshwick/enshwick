//! The one way the loader asks the local player something on a client: its
//! own dialog, or the mismatch screen where the client draws it. The
//! interface that shows them sits in a layer above and names itself here
//! when it seals.

use std::sync::OnceLock;

/// What hears a dialog's answer, or `None` when it closed without one.
pub(crate) type Answer = Box<dyn FnOnce(Option<u32>) + Send>;

/// What the interface shows the loader's screens with.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Screens {
    /// Shows a dialog: title, message, detail, the buttons' bits and what
    /// hears the answer.
    pub(crate) show: fn(&str, &str, &str, u32, Answer),
    /// Whether the mismatch screen can show on this build.
    pub(crate) mismatch_screen_on: fn() -> bool,
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
        });
        state.on_screens(Screens {
            show: |_, _, _, _, _| panic!("the first interface stays"),
            mismatch_screen_on: || true,
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
    }
}
