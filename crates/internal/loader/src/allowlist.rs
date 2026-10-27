//! The server's allowlist, ours since the game has none: with the
//! `allowlist` setting on, only the accounts `mod-data/enshwick/allowlist.txt`
//! lists, and administrators, stay in the game.

use std::fmt::Write as _;

use crate::console::accounts;

#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub(crate) use runtime::{command, configure, guarded, has, refuses, set, start};

/// The file under the loader's data folder.
pub const FILE: &str = "allowlist.txt";

/// What the file says when the loader writes it first.
pub const HEADER: &str = "\
# Accounts that may join while the allowlist setting is on, one per line:
# the 16 hex digits `enshwick world` shows, or hex after 0x. Text after #
# is a note, such as the player's name. Administrators always may join.
";

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

/// `text` with `key` added as its own line, its note `name`; `None` when
/// it lists `key` already.
#[must_use]
pub fn add(text: &str, key: u64, name: Option<&str>) -> Option<String> {
    if parse(text).0.contains(&key) {
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
    let _w = write!(out, "{key:016x}");
    if let Some(name) = name {
        out.push_str(" # ");
        out.push_str(&accounts::shown(name).replace('\n', " "));
    }
    out.push('\n');
    Some(out)
}

/// `text` without the lines that list `key`; `None` when none does.
#[must_use]
pub fn remove(text: &str, key: u64) -> Option<String> {
    let mut found = false;
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let word = word(line);
        if accounts::account(word) == Some(key) {
            found = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    found.then_some(out)
}

/// Who is asking at a join, for [`refused`].
#[derive(Clone, Copy, Debug)]
pub struct Joining<'a> {
    /// The `allowlist` setting.
    pub on: bool,
    /// Whether this is a dedicated server; a game a client hosts keeps its
    /// own invitations.
    pub server: bool,
    /// The joining player's account; 0 when the game gave none.
    pub key: u64,
    /// A hosted game's host, when known.
    pub host: Option<u64>,
    /// Whether the player may kick and ban, the game's administrators.
    pub admin: bool,
    /// The accounts the file lists, `None` when it never read.
    pub listed: Option<&'a [u64]>,
}

/// Whether the joining player must leave. A player without an account and
/// a list that never read are refused, so a broken file or a missing key
/// never opens the server.
#[must_use]
pub fn refused(joining: Joining<'_>) -> bool {
    let Joining {
        on,
        server,
        key,
        host,
        admin,
        listed,
    } = joining;
    if !on || !server || (key != 0 && host == Some(key)) {
        return false;
    }
    if key == 0 {
        return true;
    }
    !admin && listed.is_none_or(|listed| !listed.contains(&key))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn the_file_lists_accounts_with_notes_and_names_bad_lines() {
        let text = "# who may join\n0123456789abcdef # Ann\n\n0x2a\nBob\n0x2a # again\n";
        assert_eq!(parse(text), (vec![0x0123_4567_89ab_cdef, 0x2a], vec![5]));
        assert_eq!(parse(HEADER), (Vec::new(), Vec::new()));
    }

    #[test]
    fn adding_writes_the_header_once_and_never_lists_an_account_twice() {
        let first = add("", 0x2a, Some("Ann\n# forged")).unwrap();
        assert!(first.starts_with(HEADER));
        assert!(
            first.ends_with("000000000000002a # Ann?# forged\n"),
            "{first}"
        );
        assert_eq!(add(&first, 0x2a, None), None);
        let second = add(&first, 0x2b, None).unwrap();
        assert_eq!(parse(&second).0, [0x2a, 0x2b]);
        assert_eq!(add("0x1", 0x2, None).unwrap(), "0x1\n0000000000000002\n");
    }

    #[test]
    fn removing_drops_every_line_of_the_account_and_keeps_the_rest() {
        let text = "# note\n0x2a\n0x2b # Bob\n000000000000002a # Ann\n";
        assert_eq!(remove(text, 0x2a).unwrap(), "# note\n0x2b # Bob\n");
        assert_eq!(remove(text, 0x99), None);
    }

    fn joining(key: u64) -> Joining<'static> {
        Joining {
            on: true,
            server: true,
            key,
            host: None,
            admin: false,
            listed: Some(&[7]),
        }
    }

    #[test]
    fn a_join_is_refused_only_on_a_server_with_the_allowlist_on() {
        assert!(refused(joining(8)));
        assert!(!refused(joining(7)));
        assert!(!refused(Joining {
            on: false,
            ..joining(8)
        }));
        assert!(!refused(Joining {
            server: false,
            ..joining(8)
        }));
    }

    #[test]
    fn administrators_and_the_host_stay_unlisted() {
        assert!(!refused(Joining {
            admin: true,
            ..joining(8)
        }));
        assert!(!refused(Joining {
            host: Some(8),
            ..joining(8)
        }));
    }

    #[test]
    fn a_player_without_an_account_or_a_list_that_never_read_is_refused() {
        assert!(refused(joining(0)));
        assert!(refused(Joining {
            admin: true,
            host: Some(0),
            ..joining(0)
        }));
        assert!(refused(Joining {
            listed: None,
            ..joining(7)
        }));
        assert!(!refused(Joining {
            listed: None,
            admin: true,
            ..joining(7)
        }));
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_first_account() {
        let text = "\u{feff}0x2a # Ann\n0x2b\n";
        assert_eq!(parse(text), (vec![0x2a, 0x2b], Vec::new()));
        assert_eq!(remove(text, 0x2a).unwrap(), "0x2b\n");
    }

    proptest! {
        #[test]
        fn an_added_account_reads_back_and_a_removed_one_is_gone(
            keys in proptest::collection::vec(1u64.., 0..8),
            key in 1u64..,
        ) {
            let mut text = String::new();
            for &k in &keys {
                if let Some(next) = add(&text, k, Some("x")) {
                    text = next;
                }
            }
            let with = add(&text, key, None).unwrap_or(text);
            prop_assert!(parse(&with).0.contains(&key));
            let without = remove(&with, key).unwrap();
            prop_assert!(!parse(&without).0.contains(&key));
            prop_assert!(parse(&without).1.is_empty());
        }
    }
}
