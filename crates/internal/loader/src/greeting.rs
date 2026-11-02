//! The lines a player reads in chat on joining a server or loading a world:
//! the warning that developer tools are on, the message of the day and what
//! the platform turned off.

use std::sync::Mutex;

use crate::locks::Relock;

/// Lines the message of the day keeps at most.
pub(crate) const MOTD_LINES: usize = 4;
/// Bytes of text the message of the day keeps at most, over all its lines.
pub(crate) const MOTD_BYTES: usize = 500;

/// The message of the day as players read it: non-empty lines without
/// control characters, at most [`MOTD_LINES`] of them and [`MOTD_BYTES`] in
/// all.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Motd(Vec<String>);

impl Motd {
    /// The message of the day the setting's `text` gives, and whether it was
    /// cut to fit.
    #[must_use]
    pub(crate) fn parse(text: &str) -> (Self, bool) {
        let mut lines = Vec::new();
        let mut left = MOTD_BYTES;
        let mut cut = false;
        let given = text.lines().map(|line| {
            line.chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim()
                .to_owned()
        });
        for line in given.filter(|line| !line.is_empty()) {
            if lines.len() == MOTD_LINES || left == 0 {
                cut = true;
                break;
            }
            let kept = &line[..line.floor_char_boundary(left)];
            cut |= kept.len() < line.len();
            left -= kept.len();
            if !kept.is_empty() {
                lines.push(kept.to_owned());
            }
        }
        (Self(lines), cut)
    }

    /// Its lines, in order.
    #[must_use]
    pub(crate) fn lines(&self) -> &[String] {
        &self.0
    }
}

/// What a player reads, in order: the developer tools warning `developer`,
/// the message of the day, then the line naming what is off.
#[must_use]
pub(crate) fn lines(developer: Option<String>, motd: &Motd, off: Option<String>) -> Vec<String> {
    developer
        .into_iter()
        .chain(motd.lines().iter().cloned())
        .chain(off)
        .collect()
}

/// What the loaded game greets a player with.
#[cfg(windows)]
#[must_use]
pub(crate) fn of(loader: &crate::host::Loader) -> Vec<String> {
    let developer = loader
        .developer_tools
        .then(|| crate::developer_tools::warning().to_owned());
    let off = crate::off_line::line(&crate::off_line::names(loader));
    lines(developer, &motd(), off)
}

/// Greets the player with account key `key`, who just joined this server.
#[cfg(windows)]
pub(crate) fn joined(loader: &crate::host::Loader, key: u64) {
    for line in of(loader) {
        let _sent = crate::chat::send(key, &line);
    }
}

/// The greeting's state: the message of the day from the loader settings.
#[derive(Debug)]
struct Greeting {
    motd: Mutex<Motd>,
}

static STATE: Greeting = Greeting {
    motd: Mutex::new(Motd(Vec::new())),
};

/// Takes the loader settings' message of the day `text`, at start and on every
/// change of the settings file; logs when it had to be cut.
pub(crate) fn configure(text: &str) {
    let (motd, cut) = Motd::parse(text);
    if cut {
        crate::log!(
            WARN,
            "loader settings: motd is cut to {MOTD_LINES} lines and {MOTD_BYTES} bytes"
        );
    }
    *STATE.motd.relock() = motd;
}

/// The message of the day in force.
#[must_use]
pub(crate) fn motd() -> Motd {
    STATE.motd.relock().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_setting_is_no_message() {
        assert_eq!(Motd::parse(""), (Motd::default(), false));
        assert_eq!(Motd::parse(" \n\t\n"), (Motd::default(), false));
    }

    #[test]
    fn the_message_keeps_its_lines_without_blanks_or_control_characters() {
        let (motd, cut) = Motd::parse("  Welcome to Ember Vale!  \n\n\tRules: be kind.\u{7}\r\n");
        assert!(!cut);
        assert_eq!(motd.lines(), ["Welcome to Ember Vale!", "Rules: be kind."]);
    }

    #[test]
    fn lines_past_the_limit_are_cut() {
        let text = (1..=MOTD_LINES + 2)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (motd, cut) = Motd::parse(&text);
        assert!(cut);
        assert_eq!(motd.lines().len(), MOTD_LINES);
        assert_eq!(motd.lines()[0], "line 1");
    }

    #[test]
    fn bytes_past_the_limit_are_cut_at_a_character_boundary() {
        let text = "Сейф ".repeat(MOTD_BYTES);
        let (motd, cut) = Motd::parse(&text);
        assert!(cut);
        let kept: usize = motd.lines().iter().map(String::len).sum();
        assert!(kept <= MOTD_BYTES, "{kept}");
        assert!(kept > MOTD_BYTES - 4, "{kept}");
    }

    #[test]
    fn a_message_at_the_limit_is_whole() {
        let text = "x".repeat(MOTD_BYTES);
        assert_eq!(Motd::parse(&text), (Motd(vec![text]), false));
    }

    #[test]
    fn the_warning_comes_first_then_the_message_then_what_is_off() {
        let (motd, _) = Motd::parse("Welcome\nBe kind");
        let lines = lines(
            Some("DEVELOPER TOOLS ARE ON".to_owned()),
            &motd,
            Some("Off: map markers".to_owned()),
        );
        assert_eq!(
            lines,
            [
                "DEVELOPER TOOLS ARE ON",
                "Welcome",
                "Be kind",
                "Off: map markers"
            ]
        );
    }

    #[test]
    fn nothing_to_say_is_no_line() {
        assert!(lines(None, &Motd::default(), None).is_empty());
    }

    #[test]
    fn a_new_setting_replaces_the_message() {
        configure("First");
        assert_eq!(motd().lines(), ["First"]);
        configure("");
        assert!(motd().lines().is_empty());
    }
}
