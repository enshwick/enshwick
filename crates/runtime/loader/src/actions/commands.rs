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

use enshwick_core::abi::PlayerInfo;

use crate::engine::mod_index::ModIndex;

#[cfg(windows)]
mod run;
#[cfg(windows)]
pub(crate) use run::{
    after_step, any, find, forget, list, queue, register, run_remote, running, seal,
};

pub use enshwick_core::commands::MAX_LINE;
/// Every permission bit the game gives, for the console's full rights.
pub const ALL_PERMISSIONS: u8 = enshwick_core::world::PlayerPermissions::ALL_BITS;

/// A mod's command name and alias, as the one rule of
/// [`enshwick_core::command_names`] grants them.
pub type Named = enshwick_core::command_names::Named<ModIndex>;
/// Why a mod's commands are refused.
pub type Refusal = enshwick_core::command_names::Refusal<ModIndex>;

pub use enshwick_core::command_names::command_word as word;
use enshwick_core::command_names::first_word;

/// The mod whose command name or alias is `line`'s first word, a leading
/// `/` allowed, and the rest of the line.
#[must_use]
pub fn parse<'a>(named: &[Named], line: &'a str) -> Option<(ModIndex, &'a str)> {
    let (first, rest) = first_word(line);
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
    /// A command of mod `0`, which is off: it leaves the chat, so its words
    /// reach no other player, and runs nowhere.
    Off(ModIndex),
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
        Some((owner, _)) => Typed::Off(owner),
    }
}

/// The remote caller `label` with the game's permission bits
/// `permissions`: no character, account key 0, the label as its name.
#[must_use]
pub fn remote_caller(label: &str, permissions: u32) -> PlayerInfo {
    PlayerInfo::without_character(
        enshwick_core::PlayerKey::new(0),
        u8::try_from(permissions & u32::from(ALL_PERMISSIONS)).unwrap_or(0),
        label,
    )
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
            Typed::Off(ModIndex::new(3)),
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
    fn the_word_that_names_a_mod_is_told_in_lower_case_without_its_slash() {
        assert_eq!(word("/ADM  online"), "adm");
        assert_eq!(word("you.admin-tools kick Ann"), "you.admin-tools");
        assert_eq!(word("  "), "");
    }

    #[test]
    fn a_remote_caller_has_no_character_and_only_the_game_s_bits() {
        assert_eq!(ALL_PERMISSIONS, 0x3f, "the game's six permission bits");
        let caller = remote_caller("rcon 10.0.0.2", u32::MAX);
        assert_eq!(caller.key, enshwick_core::PlayerKey::new(0));
        assert_eq!(caller.entity, enshwick_core::Entity(0));
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
