//! The player's say: native message boxes, shown from the updater's own
//! thread so the game never waits on them.

/// Asks `question` with Yes and No; true for Yes. Off Windows, always No.
#[must_use]
pub(crate) fn confirm(question: &str) -> bool {
    show(question, Kind::Confirm)
}

/// Tells the player `text` with an OK button.
pub(crate) fn tell(text: &str) {
    let _ok = show(text, Kind::Note);
}

/// Tells the player `text` with a warning sign and an OK button.
pub(crate) fn warn(text: &str) {
    let _ok = show(text, Kind::Warning);
}

#[derive(Clone, Copy)]
enum Kind {
    Confirm,
    Note,
    Warning,
}

#[cfg(windows)]
fn show(text: &str, kind: Kind) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IDYES, MB_ICONINFORMATION, MB_ICONQUESTION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
        MB_TOPMOST, MB_YESNO, MessageBoxW,
    };

    let style = match kind {
        Kind::Confirm => MB_YESNO | MB_ICONQUESTION,
        Kind::Note => MB_OK | MB_ICONINFORMATION,
        Kind::Warning => MB_OK | MB_ICONWARNING,
    };
    let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "Enshwick updater".encode_utf16().chain([0]).collect();
    // SAFETY: NUL-terminated strings and no owner window.
    let answer = unsafe {
        MessageBoxW(
            core::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            style | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };
    answer == IDYES
}

#[cfg(not(windows))]
fn show(_text: &str, _kind: Kind) -> bool {
    false
}
