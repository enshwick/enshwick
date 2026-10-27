//! The server's allowlist, ours since the game has none: with the
//! `allowlist` setting on, only the accounts `mod-data/enshwick/allowlist.txt`
//! lists, and administrators, stay in the game.

use std::fmt::Write as _;

use crate::console::accounts;

#[cfg(windows)]
mod runtime;
#[cfg(windows)]
pub(crate) use runtime::{command, configure, refuses, start};

/// The file under the loader's data folder.
pub const FILE: &str = "allowlist.txt";

/// What the file says when the loader writes it first.
pub const HEADER: &str = "\
# Accounts that may join while the allowlist setting is on, one per line:
# the 16 hex digits `enshwick world` shows, or hex after 0x. Text after #
# is a note, such as the player's name. Administrators always may join.
";

/// The accounts `text` lists, in file order, each once; lines that spell no
/// account are left out and returned apart, numbered from 1.
#[must_use]
pub fn parse(text: &str) -> (Vec<u64>, Vec<usize>) {
    let mut listed = Vec::new();
    let mut bad = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let word = line.split('#').next().unwrap_or("").trim();
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
        let word = line.split('#').next().unwrap_or("").trim();
        if accounts::account(word) == Some(key) {
            found = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    found.then_some(out)
}

/// Whether the player `key` may stay: always with the allowlist off, and
/// for an administrator or a listed account with it on.
#[must_use]
pub fn admits(on: bool, listed: &[u64], key: u64, admin: bool) -> bool {
    !on || admin || listed.contains(&key)
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

    #[test]
    fn with_the_allowlist_on_only_listed_accounts_and_administrators_stay() {
        assert!(admits(false, &[], 7, false));
        assert!(admits(true, &[7], 7, false));
        assert!(admits(true, &[], 7, true));
        assert!(!admits(true, &[8], 7, false));
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
