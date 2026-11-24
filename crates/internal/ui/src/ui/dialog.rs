//! Dialogs: a message with buttons, and the player's answer.

use core::ffi::c_void;

use crate::abi::{Header, Str, UiDialog};

pub use enshwick_base::ui::Response;

/// A dialog in the game's own style: a title, a message, smaller detail text
/// and buttons the game labels in the player's language.
///
/// # Examples
///
/// ```
/// use enshwick::ui::{Dialog, Response};
///
/// let dialog = Dialog::new("Lockers", "Open every locker in the world?")
///     .detail("12 lockers")
///     .buttons(&[Response::Ok, Response::Cancel]);
/// assert_eq!(dialog.title(), "Lockers");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dialog<'a> {
    pub(super) title: &'a str,
    pub(super) message: &'a str,
    pub(super) detail: &'a str,
    pub(super) buttons: u32,
}

impl<'a> Dialog<'a> {
    /// A dialog with an OK button.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn new(title: &'a str, message: &'a str) -> Self {
        Self {
            title,
            message,
            detail: "",
            buttons: 1 << UiDialog::OK,
        }
    }

    /// Smaller text under the message, wrapped.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn detail(mut self, detail: &'a str) -> Self {
        self.detail = detail;
        self
    }

    /// The buttons, which show in the game's own order whatever the order
    /// here; none keeps OK.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub fn buttons(mut self, buttons: &[Response]) -> Self {
        let bits = buttons.iter().fold(0, |bits, r| bits | 1 << r.to_raw());
        if bits != 0 {
            self.buttons = bits;
        }
        self
    }

    /// The title.
    ///
    /// # Examples
    ///
    /// See [`Dialog`].
    #[must_use]
    pub const fn title(&self) -> &'a str {
        self.title
    }

    pub(super) fn to_raw(self, flags: u32) -> UiDialog {
        UiDialog {
            header: Header::of::<UiDialog>(),
            title: Str::new(self.title),
            message: Str::new(self.message),
            detail: Str::new(self.detail),
            buttons: self.buttons,
            flags,
        }
    }
}

/// A dialog's answer callback as the loader keeps it until the answer.
pub(super) struct Answer<F> {
    pub(super) on_answer: F,
}

pub(super) unsafe extern "system" fn answered<F>(answer: u32, user: *mut c_void)
where
    F: FnOnce(Option<Response>),
{
    // SAFETY: `user` is the `Answer<F>` boxed for this dialog, handed back
    // exactly once.
    let answer_box = unsafe { Box::from_raw(user.cast::<Answer<F>>()) };
    crate::host::guard(
        "panic in a dialog's answer",
        || (),
        move || (answer_box.on_answer)(Response::from_raw(answer)),
    );
}
