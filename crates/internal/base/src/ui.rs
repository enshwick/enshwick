//! The part of the API's `ui` the layers below the interface share: the
//! answers a dialog offers, which a client command answers it by.

use crate::abi::UiDialog;

/// An answer the player gives a dialog; each has a button labelled by the
/// game in the player's language.
///
/// # Examples
///
/// ```
/// use enshwick::ui::Response;
///
/// assert_eq!(Response::from_raw(Response::Ok.to_raw()), Some(Response::Ok));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
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
    /// Every response, in the order of their raw answers.
    pub const ALL: [Self; 8] = [
        Self::Retry,
        Self::Disable,
        Self::Erase,
        Self::Reset,
        Self::Overwrite,
        Self::Cancel,
        Self::Ok,
        Self::Ignore,
    ];

    /// The loader's raw answer for this response, such as [`UiDialog::OK`],
    /// and the bit of its button in a dialog's buttons.
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
    pub const fn to_raw(self) -> u32 {
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
        Self::ALL
            .into_iter()
            .find(|response| response.to_raw() == raw)
    }

    /// The responses whose buttons the bits `buttons` name, as
    /// [`UiDialog::buttons`] holds them, in the order of their raw answers.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::UiDialog;
    /// use enshwick::ui::Response;
    ///
    /// let buttons = 1 << UiDialog::OK | 1 << UiDialog::CANCEL;
    /// assert_eq!(Response::of_buttons(buttons), [Response::Cancel, Response::Ok]);
    /// ```
    #[must_use]
    pub fn of_buttons(buttons: u32) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|response| buttons & 1 << response.to_raw() != 0)
            .collect()
    }
}
