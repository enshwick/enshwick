//! The player's say: dialogs in the game's own style, waited for on the
//! updater's own thread so the game never waits on them.

use std::sync::OnceLock;

use enshwick_loader_api::ui::{Dialog, Response, Ui};

/// Every dialog's title.
const TITLE: &str = "Enshwick updater";

static UI: OnceLock<Ui> = OnceLock::new();

/// Keeps the interface the updater asks through.
pub(crate) fn set(ui: Ui) {
    let _first = UI.set(ui);
}

/// The first paragraph as the dialog's message, the rest as its detail.
fn split(text: &str) -> (&str, &str) {
    text.split_once("\n\n").unwrap_or((text, ""))
}

fn ask(text: &str, buttons: &[Response]) -> Option<Response> {
    let Some(ui) = UI.get() else {
        enshwick_loader_api::warn!("no interface to ask through: {text}");
        return None;
    };
    let (message, detail) = split(text);
    let dialog = Dialog::new(TITLE, message).detail(detail).buttons(buttons);
    match ui.ask(&dialog) {
        Ok(answer) => answer,
        Err(e) => {
            enshwick_loader_api::warn!("cannot ask the player ({e}): {text}");
            None
        }
    }
}

/// Asks `question` with OK and Cancel; true for OK.
#[must_use]
pub(crate) fn confirm(question: &str) -> bool {
    ask(question, &[Response::Ok, Response::Cancel]) == Some(Response::Ok)
}

/// Tells the player `text` with an OK button.
pub(crate) fn tell(text: &str) {
    let _ok = ask(text, &[Response::Ok]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_paragraph_is_the_message() {
        assert_eq!(split("Updates:\n\na\nb"), ("Updates:", "a\nb"));
        assert_eq!(split("Just one line"), ("Just one line", ""));
    }
}
