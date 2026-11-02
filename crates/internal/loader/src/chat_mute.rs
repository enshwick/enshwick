//! Chat mutes: the accounts whose typed lines the server drops.
//!
//! They are kept in `mod-data/enshwick/muted.txt`. Mods mute and lift mutes
//! through the host table; the chat post link asks about every line a
//! player types, before any other player reads it.

use std::time::{Duration, Instant};

/// The file under the loader's data folder.
pub const FILE: &str = "muted.txt";

/// What the file says when the loader writes it first.
pub const HEADER: &str = "\
# Accounts muted in chat, one per line: the 16 hex digits `enshwick world`
# shows, or hex after 0x. Text after # is a note. The server drops what a
# muted player types; administrators and a hosted game's host are never muted.
# The server reads this file as it starts: a change by hand applies then.
";

/// The least time between two notices that tell a muted player why their
/// lines go nowhere.
pub const NOTICE_GAP: Duration = Duration::from_secs(30);

/// Whether the player of `key` online may not be muted: an administrator,
/// or the host of the hosted game.
#[must_use]
pub fn protected(key: u64, admin: bool, host: Option<u64>) -> bool {
    admin || host == Some(key)
}

/// Whether a muted player who typed at `now` hears why, last told at `last`.
#[must_use]
pub fn tell(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|last| now.saturating_duration_since(last) >= NOTICE_GAP)
}

/// `text`, the file, with `key` muted or not; `None` when it says so already.
#[must_use]
pub fn changed(text: &str, key: u64, muted: bool) -> Option<String> {
    if muted {
        let base = if text.is_empty() { HEADER } else { text };
        crate::allowlist::add(base, key, None)
    } else {
        crate::allowlist::remove(text, key)
    }
}

/// Why a mute was not changed.
#[derive(Debug, thiserror::Error)]
pub enum SetMuteError {
    /// The player is an administrator online or the hosted game's host.
    #[error("administrators and the host are never muted")]
    Protected,
    /// The file was not read or written.
    #[error("{FILE} was not written: {0}")]
    Write(#[from] std::io::Error),
}

#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub(crate) use runtime::{drops, has, serving, set, start};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn administrators_and_the_host_are_never_muted() {
        assert!(protected(7, true, None));
        assert!(protected(7, false, Some(7)));
        assert!(!protected(7, false, Some(8)));
        assert!(!protected(7, false, None));
    }

    #[test]
    fn a_muted_player_hears_why_at_most_every_thirty_seconds() {
        let now = Instant::now();
        assert!(tell(None, now));
        assert!(!tell(Some(now), now + NOTICE_GAP / 2));
        assert!(tell(Some(now), now + NOTICE_GAP));
    }

    #[test]
    fn the_file_starts_with_its_header_and_lists_each_account_once() {
        let first = changed("", 0x2a, true).unwrap();
        assert!(first.starts_with(HEADER), "{first}");
        assert_eq!(crate::allowlist::parse(&first), (vec![0x2a], Vec::new()));
        assert_eq!(changed(&first, 0x2a, true), None);
        let second = changed(&first, 0x2b, true).unwrap();
        assert_eq!(crate::allowlist::parse(&second).0, [0x2a, 0x2b]);
        let lifted = changed(&second, 0x2a, false).unwrap();
        assert_eq!(crate::allowlist::parse(&lifted).0, [0x2b]);
        assert!(lifted.starts_with(HEADER), "comments stay: {lifted}");
        assert_eq!(changed(&lifted, 0x2a, false), None);
    }
}
