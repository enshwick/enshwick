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
    /// The full name, `admin-tools` in `/admin-tools online`: the part of
    /// the mod's id after the author.
    pub name: String,
    /// The shorter alias, `adm` in `/adm online`; `None` when another mod
    /// held the word first.
    pub alias: Option<String>,
}

impl Named {
    /// Whether `word` is the name or the alias, whatever its case.
    fn is_called(&self, word: &str) -> bool {
        self.name.eq_ignore_ascii_case(word)
            || self
                .alias
                .as_deref()
                .is_some_and(|alias| alias.eq_ignore_ascii_case(word))
    }

    /// The words that run the commands, each after its `/`.
    #[must_use]
    pub fn words(&self) -> String {
        match &self.alias {
            Some(alias) => format!("/{} /{alias}", self.name),
            None => format!("/{}", self.name),
        }
    }
}

/// Why a mod's commands are refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A malformed or reserved alias, an alias not shorter than the name, or
    /// a mod whose own name is reserved.
    Malformed,
    /// The mod named its commands before.
    Twice,
    /// Another mod's commands have the same name.
    Taken(ModIndex),
}

/// The names mod `owner` gets for the alias it asks, checked against the
/// reserved words, the mods' ids (`ids`, by mod index) and the names in
/// `named`: its name is the part of its id after the author, and the alias
/// is its own unless another mod's commands, name or id hold the word,
/// which the second value then names.
///
/// An alias never takes the name of another mod, whether that mod names
/// commands or not, so a mod's full name always reaches it.
///
/// # Errors
///
/// [`Refusal`] with why the mod gets no commands.
pub fn check(
    named: &[Named],
    ids: &[&str],
    owner: ModIndex,
    alias: &str,
) -> Result<(Named, Option<ModIndex>), Refusal> {
    use enshwick::commands::{is_alias, is_reserved, name_of};

    let name = ids.get(owner.get()).copied().map(name_of).unwrap_or("");
    if name.is_empty() || is_reserved(name) || !is_alias(alias) || alias.len() >= name.len() {
        return Err(Refusal::Malformed);
    }
    if named.iter().any(|n| n.owner == owner) {
        return Err(Refusal::Twice);
    }
    if let Some(n) = named.iter().find(|n| n.is_called(name)) {
        return Err(Refusal::Taken(n.owner));
    }
    let holder = named
        .iter()
        .find(|n| n.is_called(alias))
        .map(|n| n.owner)
        .or_else(|| {
            ids.iter()
                .enumerate()
                .find(|&(i, id)| i != owner.get() && (*id == alias || name_of(id) == alias))
                .map(|(i, _)| ModIndex::new(i))
        });
    let granted = Named {
        owner,
        name: name.to_owned(),
        alias: holder.is_none().then(|| alias.to_owned()),
    };
    Ok((granted, holder))
}

/// The first word of `line` and the rest, trimmed.
fn split(line: &str) -> (&str, &str) {
    let line = line.trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    (first, rest.trim())
}

/// The word that names a mod in `line`, as the mod is told it: the first
/// word in lower case, without a leading `/`.
#[must_use]
pub fn word(line: &str) -> String {
    let (first, _) = split(line);
    first.strip_prefix('/').unwrap_or(first).to_lowercase()
}

/// The mod whose command name or alias is `line`'s first word, a leading
/// `/` allowed, and the rest of the line.
#[must_use]
pub fn parse<'a>(named: &[Named], line: &'a str) -> Option<(ModIndex, &'a str)> {
    let (first, rest) = split(line);
    let word = first.strip_prefix('/').unwrap_or(first);
    named
        .iter()
        .find(|n| n.is_called(word))
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

/// How the reply `text` of a command typed in chat goes: one line of an
/// action that was `done` as a notice; several lines, and every refusal, in
/// chat, where the player typed and reads, while `chat_to_one` (lines to
/// one player) works, else joined into one notice.
#[must_use]
pub fn reply_by(text: &str, done: bool, chat_to_one: bool) -> Reply<'_> {
    if text.is_empty() {
        Reply::None
    } else if done && text.lines().nth(1).is_none() {
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
            alias: Some(alias.to_owned()).filter(|alias| !alias.is_empty()),
        }
    }

    #[test]
    fn a_mod_s_commands_take_the_name_in_its_id_and_the_alias_it_asks() {
        let ids = ["you.admin-tools", "you.weather", "sky"];
        let me = ModIndex::new(0);
        assert_eq!(
            check(&[], &ids, me, "adm"),
            Ok((named(0, "admin-tools", "adm"), None))
        );
        assert_eq!(
            check(&[], &ids, ModIndex::new(2), "s"),
            Ok((named(2, "sky", "s"), None)),
            "an id without an author names itself"
        );
        let long = "x".repeat(33);
        for alias in [
            "Adm",
            "a b",
            "",
            "enshwick",
            "help",
            "admin-tools",
            "admin-tools-x",
            long.as_str(),
        ] {
            assert_eq!(
                check(&[], &ids, me, alias),
                Err(Refusal::Malformed),
                "{alias}"
            );
        }
        assert_eq!(
            check(&[], &["you.help"], me, "h"),
            Err(Refusal::Malformed),
            "a mod named by a reserved word has no commands"
        );
        assert_eq!(
            check(&[], &ids, ModIndex::new(9), "adm"),
            Err(Refusal::Malformed),
            "no such mod"
        );
    }

    #[test]
    fn an_alias_goes_to_the_mod_that_held_the_word_first_and_the_name_stays() {
        let ids = ["you.admin-tools", "you.weather", "sky", "they.wx"];
        let me = ModIndex::new(0);
        let held = |by: usize| Ok((named(0, "admin-tools", ""), Some(ModIndex::new(by))));
        let taken = [named(1, "weather", "wx")];
        assert_eq!(check(&taken, &ids, me, "wx"), held(1), "another's alias");
        assert_eq!(check(&[], &ids, me, "weather"), held(1), "another's name");
        assert_eq!(check(&[], &ids, me, "sky"), held(2), "another's whole id");
        assert_eq!(
            check(&[], &ids, ModIndex::new(1), "wx"),
            Ok((named(1, "weather", ""), Some(ModIndex::new(3)))),
            "the name of a mod that names no commands is still its own"
        );
        assert_eq!(
            check(&taken, &ids, ModIndex::new(1), "wi"),
            Err(Refusal::Twice)
        );
    }

    #[test]
    fn two_mods_of_one_name_leave_the_commands_to_the_first() {
        let ids = ["you.tools", "they.tools"];
        let first = check(&[], &ids, ModIndex::new(0), "t").unwrap().0;
        assert_eq!(
            check(&[first], &ids, ModIndex::new(1), "to"),
            Err(Refusal::Taken(ModIndex::new(0)))
        );
    }

    #[test]
    fn the_words_of_a_command_set_are_its_name_and_its_alias_if_it_has_one() {
        assert_eq!(named(0, "admin-tools", "adm").words(), "/admin-tools /adm");
        assert_eq!(named(0, "admin-tools", "").words(), "/admin-tools");
    }

    #[test]
    fn a_line_names_a_mod_by_its_command_name_or_alias_with_or_without_a_slash() {
        let all = [named(3, "admin-tools", "adm"), named(5, "weather", "wx")];
        let at = |i| ModIndex::new(i);
        assert_eq!(parse(&all, "/adm online"), Some((at(3), "online")));
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
        let all = [named(3, "admin-tools", "adm"), named(5, "weather", "wx")];
        let running = |owner: ModIndex| owner.get() == 5;
        assert_eq!(
            typed(&all, "/adm password hunter2", running),
            Typed::Off,
            "never posted for every player to read"
        );
        assert_eq!(
            typed(&all, "/wx rain", running),
            Typed::Run(ModIndex::new(5), "rain")
        );
        assert_eq!(typed(&all, "adm online", running), Typed::Chat);
        assert_eq!(typed(&all, "/me waves", running), Typed::Chat);
    }

    #[test]
    fn a_reply_of_several_lines_goes_as_one_notice_when_chat_to_one_player_is_off() {
        let help = "online: players online\nkick: kick a player";
        assert_eq!(reply_by(help, true, true), Reply::Chat(help));
        assert_eq!(
            reply_by(help, true, false),
            Reply::Notice("online: players online / kick: kick a player".into())
        );
        assert_eq!(
            reply_by("kicked Ann", true, true),
            Reply::Notice("kicked Ann".into())
        );
        assert_eq!(reply_by("", true, false), Reply::None);
        assert_eq!(reply_by("", false, true), Reply::None);
    }

    #[test]
    fn a_refusal_of_a_typed_command_goes_into_the_chat_of_who_typed_it() {
        let unknown = "No action dance. /adm help lists them.";
        assert_eq!(reply_by(unknown, false, true), Reply::Chat(unknown));
        assert_eq!(
            reply_by(unknown, false, false),
            Reply::Notice(unknown.into()),
            "as a notice while lines to one player are off"
        );
    }

    #[test]
    fn the_word_that_names_a_mod_is_told_in_lower_case_without_its_slash() {
        assert_eq!(word("/ADM  online"), "adm");
        assert_eq!(word("you.admin-tools kick Ann"), "you.admin-tools");
        assert_eq!(word("  "), "");
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
