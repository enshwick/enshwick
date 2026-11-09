//! Mod commands: typed lines that run a mod's player actions, with the
//! caller's rights.
//!
//! A mod names its actions as a command set, `/<name>` with a shorter
//! alias. A line of that form runs one of them from a player's chat, the
//! server's console or a mod's remote console. The chat post takes a player's line that starts with `/` and a command
//! name out of the chat and queues it for the end of the step; the words
//! after the name, the rights check and the reply are the mod's own action
//! code, as for a console line. Reading names and lines is pure logic here;
//! the run module keeps them and runs the commands on the game thread.

use enshwick::abi::{Header, PlayerInfo};

use crate::mod_index::ModIndex;

#[cfg(windows)]
mod run;
#[cfg(windows)]
pub(crate) use run::{after_step, any, find, list, queue, register, run_remote, running, seal};

/// The longest command line, from chat, the console or a remote caller.
pub const MAX_LINE: usize = 1024;
/// The longest command name or alias.
const MAX_WORD: usize = 32;
/// Words no mod may take: the loader's own and the help word.
const RESERVED: [&str; 2] = [crate::settings::LOADER_ID, "help"];
/// Every permission bit the game gives, for the console's full rights.
pub const ALL_PERMISSIONS: u8 = PlayerInfo::CAN_KICK_BAN
    | PlayerInfo::CAN_ACCESS_INVENTORIES
    | PlayerInfo::CAN_EDIT_BASE
    | PlayerInfo::CAN_EDIT_WORLD
    | PlayerInfo::CAN_EXTEND_BASE
    | PlayerInfo::CAN_RECEIVE_EXP;

/// A mod's command name and alias.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
    /// The mod that named them.
    pub owner: ModIndex,
    /// The full name, `admin-tools` in `/admin-tools online`.
    pub name: String,
    /// The shorter alias, `adl` in `/adl online`.
    pub alias: String,
}

/// Whether `word` can be a command name or alias: 1 to 32 of `[a-z0-9-]`.
fn is_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD
        && word
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Why a mod's command names are refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A malformed or reserved word, or an alias not shorter than the name.
    Malformed,
    /// The mod named its commands before.
    Twice,
    /// Another mod's name, alias or id is one of the words.
    Taken(ModIndex),
}

/// Checks mod `owner`'s `name` and `alias` against the reserved words, the
/// mods' ids (`ids`, by mod index) and the names in `named`.
///
/// # Errors
///
/// [`Refusal`] with why the words cannot be the mod's.
pub fn check(
    named: &[Named],
    ids: &[&str],
    owner: ModIndex,
    name: &str,
    alias: &str,
) -> Result<(), Refusal> {
    let reserved = |w: &str| RESERVED.contains(&w);
    if !is_word(name) || !is_word(alias) || alias.len() >= name.len() {
        return Err(Refusal::Malformed);
    }
    if reserved(name) || reserved(alias) {
        return Err(Refusal::Malformed);
    }
    if named.iter().any(|n| n.owner == owner) {
        return Err(Refusal::Twice);
    }
    let words = [name, alias];
    if let Some(n) = named
        .iter()
        .find(|n| words.contains(&n.name.as_str()) || words.contains(&n.alias.as_str()))
    {
        return Err(Refusal::Taken(n.owner));
    }
    let other_id = ids
        .iter()
        .enumerate()
        .find(|&(i, id)| i != owner.get() && words.iter().any(|w| id.eq_ignore_ascii_case(w)));
    match other_id {
        Some((i, _)) => Err(Refusal::Taken(ModIndex::new(i))),
        None => Ok(()),
    }
}

/// The first word of `line` and the rest, trimmed.
fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    (first, rest.trim())
}

/// The mod whose command name or alias is `line`'s first word, a leading
/// `/` allowed, and the rest of the line.
#[must_use]
pub fn parse<'a>(named: &[Named], line: &'a str) -> Option<(ModIndex, &'a str)> {
    let (first, rest) = split(line);
    let word = first.strip_prefix('/').unwrap_or(first);
    named
        .iter()
        .find(|n| n.name.eq_ignore_ascii_case(word) || n.alias.eq_ignore_ascii_case(word))
        .map(|n| (n.owner, rest))
}

/// The remote caller `label` with the game's permission bits
/// `permissions`: no character, account key 0, the label, cut to 64 bytes,
/// as its name.
#[must_use]
pub fn remote_caller(label: &str, permissions: u32) -> PlayerInfo {
    let label = &label[..label.floor_char_boundary(64)];
    let mut name = [0u8; 64];
    name[..label.len()].copy_from_slice(label.as_bytes());
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: enshwick::Entity(0),
        key: enshwick::PlayerKey::new(0),
        permissions: u8::try_from(permissions & u32::from(ALL_PERMISSIONS)).unwrap_or(0),
        name_len: u8::try_from(label.len()).unwrap_or(64),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(owner: usize, name: &str, alias: &str) -> Named {
        Named {
            owner: ModIndex::new(owner),
            name: name.to_owned(),
            alias: alias.to_owned(),
        }
    }

    #[test]
    fn a_name_and_a_shorter_alias_of_plain_words_are_taken() {
        let ids = ["you.admin-tools", "you.weather"];
        let me = ModIndex::new(0);
        assert_eq!(check(&[], &ids, me, "admin-tools", "adl"), Ok(()));
        let long = "x".repeat(33);
        for (name, alias) in [
            ("Admin", "adl"),
            ("admin tools", "adl"),
            ("adl", "admin-tools"),
            ("adl", "adl"),
            ("admin-tools", ""),
            ("enshwick", "en"),
            ("helper", "help"),
            (long.as_str(), "x"),
        ] {
            assert_eq!(
                check(&[], &ids, me, name, alias),
                Err(Refusal::Malformed),
                "{name} {alias}"
            );
        }
    }

    #[test]
    fn a_word_another_mod_took_is_refused_naming_that_mod() {
        let ids = ["you.admin-tools", "you.weather", "sky"];
        let taken = [named(1, "weather", "wx")];
        let me = ModIndex::new(0);
        let by = |i| Err(Refusal::Taken(ModIndex::new(i)));
        assert_eq!(check(&taken, &ids, me, "admin-tools", "wx"), by(1));
        assert_eq!(check(&taken, &ids, me, "weather", "adl"), by(1));
        assert_eq!(check(&taken, &ids, me, "skies", "sky"), by(2));
        assert_eq!(
            check(&taken, &ids, ModIndex::new(1), "winds", "wi"),
            Err(Refusal::Twice)
        );
        assert_eq!(
            check(&taken, &ids, ModIndex::new(2), "sky-mod", "sky"),
            Ok(()),
            "its own id"
        );
    }

    #[test]
    fn a_line_names_a_mod_by_its_command_name_or_alias_with_or_without_a_slash() {
        let all = [named(3, "admin-tools", "adl"), named(5, "weather", "wx")];
        let at = |i| ModIndex::new(i);
        assert_eq!(parse(&all, "/adl online"), Some((at(3), "online")));
        assert_eq!(
            parse(&all, "  ADMIN-TOOLS   kick  Ann Lee "),
            Some((at(3), "kick  Ann Lee"))
        );
        assert_eq!(parse(&all, "/wx"), Some((at(5), "")));
        assert_eq!(parse(&all, "/me waves"), None);
        assert_eq!(parse(&all, "/"), None);
        assert_eq!(parse(&all, ""), None);
    }

    #[test]
    fn a_remote_caller_has_no_character_and_only_the_game_s_bits() {
        assert_eq!(ALL_PERMISSIONS, 0x3f, "the game's six permission bits");
        let caller = remote_caller("rcon 10.0.0.2", u32::MAX);
        assert_eq!(caller.key, enshwick::PlayerKey::new(0));
        assert_eq!(caller.entity, enshwick::Entity(0));
        assert_eq!(caller.permissions, ALL_PERMISSIONS);
        assert_eq!(
            &caller.name[..usize::from(caller.name_len)],
            b"rcon 10.0.0.2"
        );
        let long = remote_caller(&"й".repeat(40), 0);
        assert_eq!(long.name_len, 64);
        assert!(core::str::from_utf8(&long.name[..64]).is_ok());
    }
}
