//! Dialogs: a message with buttons, and the player's answer.

use core::ffi::c_void;

use crate::abi::{Header, Str, UiDialog};

/// An answer the player gives a dialog; each has a button labelled by the
/// game in the player's language.
///
/// # Examples
///
/// See [`Dialog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Response {
    /// "Retry".
    Retry,
    /// "Disable".
    Disable,
    /// "Erase".
    Erase,
    /// "Reset".
    Reset,
    /// "Overwrite".
    Overwrite,
    /// "Cancel".
    Cancel,
    /// "OK".
    Ok,
    /// "Ignore".
    Ignore,
}

impl Response {
    /// The loader's raw answer for this response, such as [`UiDialog::OK`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::UiDialog;
    /// use enshwick::ui::Response;
    ///
    /// assert_eq!(Response::Cancel.to_raw(), UiDialog::CANCEL);
    /// ```
    #[must_use]
    pub fn to_raw(self) -> u32 {
        self.bit()
    }

    pub(super) fn bit(self) -> u32 {
        match self {
            Self::Retry => UiDialog::RETRY,
            Self::Disable => UiDialog::DISABLE,
            Self::Erase => UiDialog::ERASE,
            Self::Reset => UiDialog::RESET,
            Self::Overwrite => UiDialog::OVERWRITE,
            Self::Cancel => UiDialog::CANCEL,
            Self::Ok => UiDialog::OK,
            Self::Ignore => UiDialog::IGNORE,
        }
    }

    /// The response for the loader's answer `raw`; `None` for
    /// [`UiDialog::CLOSED`] or an answer this crate does not know.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::UiDialog;
    /// use enshwick::ui::Response;
    ///
    /// assert_eq!(Response::from_raw(UiDialog::OK), Some(Response::Ok));
    /// assert_eq!(Response::from_raw(UiDialog::CLOSED), None);
    /// ```
    #[must_use]
    pub fn from_raw(raw: u32) -> Option<Self> {
        Some(match raw {
            UiDialog::RETRY => Self::Retry,
            UiDialog::DISABLE => Self::Disable,
            UiDialog::ERASE => Self::Erase,
            UiDialog::RESET => Self::Reset,
            UiDialog::OVERWRITE => Self::Overwrite,
            UiDialog::CANCEL => Self::Cancel,
            UiDialog::OK => Self::Ok,
            UiDialog::IGNORE => Self::Ignore,
            _ => return None,
        })
    }
}

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
        let bits = buttons.iter().fold(0, |bits, r| bits | 1 << r.bit());
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
