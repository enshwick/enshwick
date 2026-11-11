//! Chat mutes: the accounts whose typed lines the server drops.
//!
//! They belong to the world: `<stem>-mod-data/enshwick/muted.txt` beside its
//! save, read as the world loads. Mods mute and lift mutes through the host
//! table; the chat post link asks about every line a player types, before
//! any other player reads it.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use crate::console::accounts;

/// The file in the loader's folder of a world's data.
pub const FILE: &str = "muted.txt";

/// What the file says when the loader writes it first.
pub const HEADER: &str = "\
# Accounts muted in chat in this world, one per line: the 16 hex digits
# `enshwick world` shows, or hex after 0x. Text after # is a note. The
# server drops what a muted player types; administrators and a hosted
# game's host are never muted. The server reads this file as the world
# loads: a change by hand applies then.
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

/// The account word of a file line: before any `#`, without blanks or the
/// byte-order mark an editor may put first.
fn word(line: &str) -> &str {
    line.split('#')
        .next()
        .unwrap_or("")
        .trim()
        .trim_start_matches('\u{feff}')
        .trim()
}

/// The accounts `text` lists, in file order, each once; lines that spell no
/// account are left out and returned apart, numbered from 1.
#[must_use]
pub fn parse(text: &str) -> (Vec<u64>, Vec<usize>) {
    let mut listed = Vec::new();
    let mut bad = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let word = word(line);
        if word.is_empty() {
            continue;
        }
        match accounts::account(word) {
            Some(key) if !listed.contains(&key) => listed.push(key),
            Some(_) => {}
            None => bad.push(i + 1),
        }
    }
    (listed, bad)
}

/// `text`, the file, with `key` muted or not; `None` when it says so already.
#[must_use]
pub fn changed(text: &str, key: u64, muted: bool) -> Option<String> {
    let listed = parse(text).0.contains(&key);
    if muted {
        if listed {
            return None;
        }
        let mut out = if text.is_empty() {
            HEADER.to_owned()
        } else {
            text.to_owned()
        };
        if !out.ends_with('\n') {
            out.push('\n');
        }
        let _w = writeln!(out, "{key:016x}");
        return Some(out);
    }
    listed.then(|| {
        text.lines()
            .filter(|line| accounts::account(word(line)) != Some(key))
            .flat_map(|line| [line, "\n"])
            .collect()
    })
}

/// Why a mute was not changed.
#[derive(Debug, thiserror::Error)]
pub enum SetMuteError {
    /// The player is an administrator online or the hosted game's host.
    #[error("administrators and the host are never muted")]
    Protected,
    /// No world runs, so there is no world to keep the mute.
    #[error("no world runs")]
    NoWorld,
    /// The file was not read or written.
    #[error("{FILE} was not written: {0}")]
    Write(#[from] std::io::Error),
}

#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub(crate) use runtime::{drops, has, serving, set, world_event};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

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
        assert_eq!(parse(&first), (vec![0x2a], Vec::new()));
        assert_eq!(changed(&first, 0x2a, true), None);
        let second = changed(&first, 0x2b, true).unwrap();
        assert_eq!(parse(&second).0, [0x2a, 0x2b]);
        let lifted = changed(&second, 0x2a, false).unwrap();
        assert_eq!(parse(&lifted).0, [0x2b]);
        assert!(lifted.starts_with(HEADER), "comments stay: {lifted}");
        assert_eq!(changed(&lifted, 0x2a, false), None);
    }

    #[test]
    fn the_file_lists_accounts_with_notes_and_names_bad_lines() {
        let text = "\u{feff}# muted\n0123456789abcdef # Ann\n\n0x2a\nBob\n0x2a # again\n";
        assert_eq!(parse(text), (vec![0x0123_4567_89ab_cdef, 0x2a], vec![5]));
        assert_eq!(parse(HEADER), (Vec::new(), Vec::new()));
        assert_eq!(
            changed(
                "# note\n0x2a\n0x2b # Bob\n000000000000002a # Ann\n",
                0x2a,
                false
            )
            .unwrap(),
            "# note\n0x2b # Bob\n"
        );
    }

    proptest! {
        #[test]
        fn a_muted_account_reads_back_and_a_lifted_one_is_gone(
            keys in proptest::collection::vec(1u64.., 0..8),
            key in 1u64..,
        ) {
            let mut text = String::new();
            for &k in &keys {
                if let Some(next) = changed(&text, k, true) {
                    text = next;
                }
            }
            let with = changed(&text, key, true).unwrap_or(text);
            prop_assert!(parse(&with).0.contains(&key));
            let without = changed(&with, key, false).unwrap();
            prop_assert!(!parse(&without).0.contains(&key));
            prop_assert!(parse(&without).1.is_empty());
        }
    }
}
