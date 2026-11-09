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
pub(crate) use run::{
    after_step, any, find, forget, list, queue, register, run_remote, running, seal,
};

pub use enshwick::commands::MAX_LINE;
/// Every permission bit the game gives, for the console's full rights.
pub const ALL_PERMISSIONS: u8 = enshwick::world::PlayerPermissions::ALL_BITS;

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
    if !enshwick::commands::names_fit(name, alias) {
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

/// What a line a player typed is to the commands.
#[derive(Debug, PartialEq, Eq)]
pub enum Typed<'a> {
    /// No command: the chat posts it.
    Chat,
    /// A command of a mod that is off: it leaves the chat, so its words
    /// reach no other player, and runs nowhere.
    Off,
    /// A command of a running mod, and the rest of the line.
    Run(ModIndex, &'a str),
}

/// What the line `text` is: a command when it starts with `/` and a name or
/// alias in `named`, run when `running` says its mod runs.
pub fn typed<'a>(named: &[Named], text: &'a str, running: impl Fn(ModIndex) -> bool) -> Typed<'a> {
    if !text.starts_with('/') {
        return Typed::Chat;
    }
    match parse(named, text) {
        None => Typed::Chat,
        Some((owner, rest)) if running(owner) => Typed::Run(owner, rest),
        Some(_) => Typed::Off,
    }
}

/// How a command's reply reaches the player who typed it.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply<'a> {
    /// No text: nothing goes.
    None,
    /// One notice.
    Notice(std::borrow::Cow<'a, str>),
    /// Several lines in chat, to that player alone.
    Chat(&'a str),
}

/// How the reply `text` goes: one line as a notice, several in chat while
/// `chat_to_one` (lines to one player) works, else joined into one notice.
#[must_use]
pub fn reply_by(text: &str, chat_to_one: bool) -> Reply<'_> {
    if text.is_empty() {
        Reply::None
    } else if text.lines().nth(1).is_none() {
        Reply::Notice(text.into())
    } else if chat_to_one {
        Reply::Chat(text)
    } else {
        Reply::Notice(text.lines().collect::<Vec<_>>().join(" / ").into())
    }
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
    fn a_command_of_a_mod_that_is_off_leaves_the_chat_and_runs_nowhere() {
        let all = [named(3, "admin-tools", "adl"), named(5, "weather", "wx")];
        let running = |owner: ModIndex| owner.get() == 5;
        assert_eq!(
            typed(&all, "/adl password hunter2", running),
            Typed::Off,
            "never posted for every player to read"
        );
        assert_eq!(
            typed(&all, "/wx rain", running),
            Typed::Run(ModIndex::new(5), "rain")
        );
        assert_eq!(typed(&all, "adl online", running), Typed::Chat);
        assert_eq!(typed(&all, "/me waves", running), Typed::Chat);
    }

    #[test]
    fn a_reply_of_several_lines_goes_as_one_notice_when_chat_to_one_player_is_off() {
        let help = "online: players online\nkick: kick a player";
        assert_eq!(reply_by(help, true), Reply::Chat(help));
        assert_eq!(
            reply_by(help, false),
            Reply::Notice("online: players online / kick: kick a player".into())
        );
        assert_eq!(
            reply_by("kicked Ann", true),
            Reply::Notice("kicked Ann".into())
        );
        assert_eq!(reply_by("", false), Reply::None);
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
