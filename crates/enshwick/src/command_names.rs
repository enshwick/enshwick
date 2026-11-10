//! The names of mods' commands: the one rule the loader grants them by,
//! which the fake host follows, so a mod's test names commands as the game
//! does.

/// The longest command line `commands::run` takes, in bytes.
pub const MAX_LINE: usize = 1024;

/// The longest alias of a mod's commands, in bytes; a command's name is
/// the mod's own and may be longer.
pub const MAX_NAME: usize = 32;

/// Words no mod's commands may take: the loader's own and the help word.
const RESERVED: [&str; 2] = ["enshwick", "help"];

/// The command name of the mod `mod_id`: the part of its id after the
/// author, or the whole id when it names none.
#[must_use]
pub fn name_of(mod_id: &str) -> &str {
    mod_id.split_once('.').map_or(mod_id, |(_, name)| name)
}

/// Whether `word` can be the alias of a mod's commands: 1 to [`MAX_NAME`]
/// of `[a-z0-9-]` and not reserved.
#[must_use]
pub fn is_alias(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_NAME
        && !is_reserved(word)
        && word
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `word` is one no mod's commands may take.
#[must_use]
pub fn is_reserved(word: &str) -> bool {
    RESERVED.contains(&word)
}

/// A mod's command name and alias, the mod named by an `O`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named<O> {
    /// The mod that named them.
    pub owner: O,
    /// The full name, `admin-tools` in `/admin-tools online`: the part of
    /// the mod's id after the author.
    pub name: String,
    /// The shorter alias, `adm` in `/adm online`; `None` when another mod
    /// held the word first.
    pub alias: Option<String>,
}

impl<O> Named<O> {
    /// Whether `word` is the name or the alias, whatever its case.
    #[must_use]
    pub fn is_called(&self, word: &str) -> bool {
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
pub enum Refusal<O> {
    /// A malformed or reserved alias, an alias not shorter than the name, or
    /// a mod whose own name is reserved.
    Malformed,
    /// The mod named its commands before.
    Twice,
    /// Another mod's commands have the same name, or another mod's whole
    /// id is the name, which the console reaches that mod by.
    Taken(O),
}

/// The command names mod `owner` gets for the alias it asks, if it asks
/// one, and the mod that holds that alias, if one does.
///
/// They are checked against the reserved words, every mod's id (`ids`, each
/// mod with its id) and the names in `named`: the name is the part of the
/// mod's id after the author, and the alias is its own unless another mod's
/// commands, name or id hold the word.
///
/// An alias never takes the name of another mod, whether that mod names
/// commands or not, so a mod's full name always reaches it.
///
/// # Errors
///
/// [`Refusal`] with why the mod gets no commands.
pub fn check<O: Copy + PartialEq>(
    named: &[Named<O>],
    ids: &[(O, &str)],
    owner: O,
    alias: Option<&str>,
) -> Result<(Named<O>, Option<O>), Refusal<O>> {
    let own = ids.iter().find(|(mod_, _)| *mod_ == owner);
    let name = own.map_or("", |(_, id)| name_of(id));
    let fits = |alias: &str| is_alias(alias) && alias.len() < name.len();
    if name.is_empty() || is_reserved(name) || !alias.is_none_or(fits) {
        return Err(Refusal::Malformed);
    }
    if named.iter().any(|n| n.owner == owner) {
        return Err(Refusal::Twice);
    }
    if let Some(n) = named.iter().find(|n| n.is_called(name)) {
        return Err(Refusal::Taken(n.owner));
    }
    if let Some(&(other, _)) = ids.iter().find(|(o, id)| *o != owner && *id == name) {
        return Err(Refusal::Taken(other));
    }
    let holder = alias.and_then(|alias| {
        named
            .iter()
            .find(|n| n.is_called(alias))
            .map(|n| n.owner)
            .or_else(|| {
                ids.iter()
                    .find(|(mod_, id)| *mod_ != owner && (*id == alias || name_of(id) == alias))
                    .map(|(mod_, _)| *mod_)
            })
    });
    let granted = Named {
        owner,
        name: name.to_owned(),
        alias: alias.filter(|_| holder.is_none()).map(str::to_owned),
    };
    Ok((granted, holder))
}

/// The ABI minor from which a mod's command name comes from its id, and
/// `HostApi::commands` no longer reads the name a mod gives.
pub const NAME_FROM_ID: u16 = 23;

/// The alias to grant mod `mod_id`, built for ABI minor `abi_minor`, that
/// asks `alias`.
///
/// `None`, its commands by their name alone, for a mod built before
/// [`NAME_FROM_ID`] whose alias is a well-formed word but not shorter than
/// the name in its id, since it chose the alias against a name of its own.
#[must_use]
pub fn alias_asked<'a>(alias: &'a str, mod_id: &str, abi_minor: u16) -> Option<&'a str> {
    let older_misfit =
        abi_minor < NAME_FROM_ID && is_alias(alias) && alias.len() >= name_of(mod_id).len();
    (!older_misfit).then_some(alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(owner: usize, name: &str, alias: &str) -> Named<usize> {
        Named {
            owner,
            name: name.to_owned(),
            alias: Some(alias.to_owned()).filter(|alias| !alias.is_empty()),
        }
    }

    fn mods<'a>(ids: &[&'a str]) -> Vec<(usize, &'a str)> {
        ids.iter().copied().enumerate().collect()
    }

    #[test]
    fn a_command_is_named_after_the_part_of_the_mod_id_past_the_author() {
        assert_eq!(name_of("you.admin-tools"), "admin-tools");
        assert_eq!(name_of("sky"), "sky", "an id of the old form names itself");
    }

    #[test]
    fn an_alias_is_a_plain_word_that_is_not_reserved() {
        assert!(is_alias("adm"));
        assert!(is_alias("a-1"), "digits and hyphens");
        assert!(is_alias("2"), "a digit alone");
        assert!(is_alias(&"x".repeat(MAX_NAME)));
        for word in ["Adm", "a b", "a\tb", "", "enshwick", "help", "a.b"] {
            assert!(!is_alias(word), "{word}");
        }
        assert!(!is_alias(&"x".repeat(MAX_NAME + 1)));
    }

    #[test]
    fn a_mod_s_commands_take_the_name_in_its_id_and_the_alias_it_asks() {
        let ids = mods(&["you.admin-tools", "you.weather", "sky"]);
        assert_eq!(
            check(&[], &ids, 0, Some("adm")),
            Ok((named(0, "admin-tools", "adm"), None))
        );
        assert_eq!(
            check(&[], &ids, 2, Some("s")),
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
                check(&[], &ids, 0, Some(alias)),
                Err(Refusal::Malformed),
                "{alias}"
            );
        }
        assert_eq!(
            check(&[], &mods(&["you.help"]), 0, Some("h")),
            Err(Refusal::Malformed),
            "a mod named by a reserved word has no commands"
        );
        assert_eq!(
            check(&[], &ids, 9, Some("adm")),
            Err(Refusal::Malformed),
            "no such mod"
        );
    }

    #[test]
    fn an_alias_goes_to_the_mod_that_held_the_word_first_and_the_name_stays() {
        let ids = mods(&["you.admin-tools", "you.weather", "sky", "they.wx"]);
        let held = |by: usize| Ok((named(0, "admin-tools", ""), Some(by)));
        let taken = [named(1, "weather", "wx")];
        assert_eq!(
            check(&taken, &ids, 0, Some("wx")),
            held(1),
            "another's alias"
        );
        assert_eq!(
            check(&[], &ids, 0, Some("weather")),
            held(1),
            "another's name"
        );
        assert_eq!(
            check(&[], &ids, 0, Some("sky")),
            held(2),
            "another's whole id"
        );
        assert_eq!(
            check(&[], &ids, 1, Some("wx")),
            Ok((named(1, "weather", ""), Some(3))),
            "the name of a mod that names no commands is still its own"
        );
        assert_eq!(check(&taken, &ids, 1, Some("wi")), Err(Refusal::Twice));
    }

    #[test]
    fn two_mods_of_one_name_leave_the_commands_to_the_first() {
        let ids = mods(&["you.tools", "they.tools"]);
        let first = check(&[], &ids, 0, Some("t")).unwrap().0;
        assert_eq!(check(&[first], &ids, 1, Some("to")), Err(Refusal::Taken(0)));
    }

    #[test]
    fn a_mod_built_before_names_came_from_ids_keeps_its_name_when_its_alias_no_longer_fits() {
        let ids = mods(&["you.wx", "you.admin-tools"]);
        assert_eq!(alias_asked("wt", "you.wx", NAME_FROM_ID - 1), None);
        assert_eq!(
            check(&[], &ids, 0, None),
            Ok((named(0, "wx", ""), None)),
            "its commands run by their name alone"
        );
        assert_eq!(alias_asked("wt", "you.wx", NAME_FROM_ID), Some("wt"));
        assert_eq!(
            check(&[], &ids, 0, Some("wt")),
            Err(Refusal::Malformed),
            "a mod of this minor is told its alias does not fit"
        );
        assert_eq!(
            alias_asked("adm", "you.admin-tools", 19),
            Some("adm"),
            "an alias that fits stays"
        );
        assert_eq!(
            alias_asked("Adm", "you.wx", 19),
            Some("Adm"),
            "a malformed word is refused as before"
        );
    }

    #[test]
    fn a_name_that_is_another_mod_s_whole_id_stays_that_mod_s() {
        let ids = mods(&["tools", "you.tools"]);
        assert_eq!(
            check(&[], &ids, 1, Some("t")),
            Err(Refusal::Taken(0)),
            "the console reaches `tools` by its id, so /tools is never another mod's"
        );
        assert_eq!(
            check(&[], &ids, 0, Some("t")),
            Ok((named(0, "tools", "t"), None))
        );
    }

    #[test]
    fn the_words_of_a_command_set_are_its_name_and_its_alias_if_it_has_one() {
        assert_eq!(named(0, "admin-tools", "adm").words(), "/admin-tools /adm");
        assert_eq!(named(0, "admin-tools", "").words(), "/admin-tools");
    }

    #[test]
    fn a_command_set_is_called_by_its_name_or_alias_whatever_the_case() {
        let tools = named(0, "admin-tools", "adm");
        assert!(tools.is_called("ADM") && tools.is_called("Admin-Tools"));
        assert!(!tools.is_called("ad") && !named(0, "admin-tools", "").is_called(""));
    }
}
