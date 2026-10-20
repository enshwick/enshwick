//! `/safe allow <player>`, `/safe deny <player>` and `/safe list`: the owner
//! of a Safe lets other players use it, acting on their nearest own Safe
//! within reach.

use enshwick_loader_api::commands::{CommandContext, Reply};
use enshwick_loader_api::i18n::{Language, Texts};
use enshwick_loader_api::world::{Player, World};

use crate::access::{ACCESS, AllowOutcome, Cell, centre, nearest_own};

/// How far from the player their Safe may be, in metres; `texts.toml` says
/// "4 m" in its replies.
pub const REACH: f64 = 4.0;

/// The mod's texts in the server's language.
#[derive(Debug)]
pub struct Phrases {
    /// Every text the mod has.
    pub texts: Texts,
    /// The language picked in the settings.
    pub language: Language,
}

impl Phrases {
    /// The text `key` with each `{name}` replaced by its value; the key
    /// itself when there is no text.
    #[must_use]
    pub fn say(&self, key: &str, values: &[(&str, &str)]) -> String {
        fill(self.texts.get(key, self.language).unwrap_or(key), values)
    }
}

/// `template` with each `{name}` replaced by its value.
#[must_use]
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// What a `/safe` line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Let the named player use the Safe.
    Allow(String),
    /// Take it back from the named player.
    Deny(String),
    /// Name who may use it.
    List,
    /// Anything else: the usage.
    Help,
}

/// The request in a `/safe` command's words; names may hold spaces.
#[must_use]
pub fn parse(args: &[&str]) -> Request {
    match args {
        [verb, name @ ..] if !name.is_empty() => match verb.to_lowercase().as_str() {
            "allow" => Request::Allow(name.join(" ")),
            "deny" => Request::Deny(name.join(" ")),
            _ => Request::Help,
        },
        [verb] if verb.eq_ignore_ascii_case("list") => Request::List,
        _ => Request::Help,
    }
}

/// A cell as the player reads it: the Safe's position in whole metres.
fn place(at: Cell) -> String {
    let [x, y, z] = centre(at);
    format!("({x:.0}, {y:.0}, {z:.0})")
}

/// Runs a `/safe` command for the player in `ctx`; saves through `save`
/// when it changed who may use a Safe.
pub fn run(
    world: &World,
    phrases: &Phrases,
    ctx: &CommandContext,
    args: &[&str],
    save: impl Fn(),
) -> Reply {
    let request = parse(args);
    if request == Request::Help {
        return phrases.say("command.usage", &[]).into();
    }
    let me = &ctx.player;
    let Some(position) = world.position(me.entity) else {
        return phrases.say("command.no_safe", &[]).into();
    };
    let Some(at) = nearest_own(&ACCESS.owners.load(), me.key, position, REACH) else {
        return phrases.say("command.no_safe", &[]).into();
    };
    let here = place(at);
    let reply = match request {
        Request::Allow(name) => allow(world, phrases, me, at, &name, &here, &save),
        Request::Deny(name) => {
            let denied = ACCESS.change(at, |safe| safe.deny(&name)).flatten();
            match denied {
                Some(gone) => {
                    save();
                    phrases.say("command.denied", &[("name", &gone.name), ("at", &here)])
                }
                None => phrases.say("command.not_allowed", &[("name", &name), ("at", &here)]),
            }
        }
        Request::List | Request::Help => {
            let names: Vec<String> = ACCESS
                .owners
                .load()
                .get(&at)
                .map(|safe| safe.allowed.iter().map(|a| a.name.clone()).collect())
                .unwrap_or_default();
            if names.is_empty() {
                phrases.say("command.list_empty", &[("at", &here)])
            } else {
                phrases.say(
                    "command.list",
                    &[("at", &here), ("names", &names.join(", "))],
                )
            }
        }
    };
    reply.into()
}

fn allow(
    world: &World,
    phrases: &Phrases,
    me: &Player,
    at: Cell,
    name: &str,
    here: &str,
    save: &impl Fn(),
) -> String {
    let matching = world.players_named(name);
    let target = match matching.as_slice() {
        [] => return phrases.say("command.no_player", &[("name", name)]),
        [one] => one,
        _ => return phrases.say("command.several", &[("name", name)]),
    };
    if target.key == me.key {
        return phrases.say("command.self", &[]);
    }
    let outcome = ACCESS.change(at, |safe| safe.allow(target.key, &target.name));
    let values = [("name", target.name.as_str()), ("at", here)];
    match outcome {
        Some(AllowOutcome::Allowed) => {
            save();
            phrases.say("command.allowed", &values)
        }
        Some(AllowOutcome::Already) => phrases.say("command.already", &values),
        Some(AllowOutcome::Owner) => phrases.say("command.self", &[]),
        None => phrases.say("command.no_safe", &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_read_from_the_words() {
        assert_eq!(
            parse(&["allow", "Ann", "Lee"]),
            Request::Allow("Ann Lee".into())
        );
        assert_eq!(parse(&["DENY", "Bob"]), Request::Deny("Bob".into()));
        assert_eq!(parse(&["list"]), Request::List);
        assert_eq!(parse(&["allow"]), Request::Help);
        assert_eq!(parse(&[]), Request::Help);
        assert_eq!(parse(&["open", "Bob"]), Request::Help);
    }

    #[test]
    fn templates_take_their_values() {
        assert_eq!(fill("Safe · {owner}", &[("owner", "Ann")]), "Safe · Ann");
        assert_eq!(
            fill("{name} at {at}", &[("name", "Bob"), ("at", "(1, 2, 3)")]),
            "Bob at (1, 2, 3)"
        );
        assert_eq!(place((4, -4, 20)), "(2, -2, 10)");
    }

    #[test]
    fn every_text_the_mod_says_exists_in_english() {
        let texts = Texts::parse(crate::content::TEXTS).unwrap();
        for key in [
            "safe.label",
            "command.help",
            "command.usage",
            "command.no_safe",
            "command.no_player",
            "command.several",
            "command.self",
            "command.allowed",
            "command.already",
            "command.denied",
            "command.not_allowed",
            "command.list",
            "command.list_empty",
        ] {
            assert!(texts.get(key, Language::FALLBACK).is_some(), "{key}");
        }
    }
}
