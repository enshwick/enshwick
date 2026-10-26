//! The server's console: lines a mod runs on the server as an administrator
//! with no character, such as each line of a dedicated server's command file.
//!
//! `enshwick ...` reaches the loader's own tools; any other first word names a
//! mod, whose actions take the rest of the line.
//!
//! Reading a line is pure logic here; the run module answers it.

use enshwick::abi::Level;

use crate::settings::LOADER_ID;

#[cfg(windows)]
mod run;
#[cfg(windows)]
pub(crate) use run::{run, the_console};

/// The most log lines `enshwick log` shows.
pub const MAX_LOG_LINES: usize = 16;

/// What a console line asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Line<'a> {
    /// No word at all.
    Empty,
    /// One of the loader's own tools.
    Loader(Tool<'a>),
    /// A mod's actions.
    Mod {
        /// The mod's id as written.
        id: &'a str,
        /// The text form of one of its actions, or `help`.
        rest: &'a str,
    },
}

/// One of the loader's own tools, after `enshwick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool<'a> {
    /// `enshwick`: the mods running.
    Mods,
    /// `enshwick help`: the tools and the mods with actions.
    Help,
    /// `enshwick <mod id>`: what a mod is.
    About(&'a str),
    /// `enshwick status`: the status summary.
    Status,
    /// `enshwick profile [reset]`: what each mod costs.
    Profile {
        /// Whether the costs count from now.
        reset: bool,
    },
    /// `enshwick conflicts`: what mods share.
    Conflicts,
    /// `enshwick log [mod] [level] [n]`: the latest log lines.
    Log(&'a str),
    /// `enshwick recipes ...`: disabled recipes.
    Recipes(&'a str),
    /// `enshwick config [mod [key [value]]]`: mods' settings.
    Config(&'a str),
    /// `enshwick ui-demo <player>`: the interface demo on a player's client.
    UiDemo(&'a str),
}

/// The first word of `text` and the rest, trimmed.
fn split(text: &str) -> (&str, &str) {
    let text = text.trim();
    let (first, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    (first, rest.trim())
}

/// Reads a console line.
#[must_use]
pub fn parse(line: &str) -> Line<'_> {
    let (first, rest) = split(line);
    if first.is_empty() {
        return Line::Empty;
    }
    if !first.eq_ignore_ascii_case(LOADER_ID) {
        return Line::Mod { id: first, rest };
    }
    let (word, args) = split(rest);
    Line::Loader(match word.to_ascii_lowercase().as_str() {
        "" => Tool::Mods,
        "help" => Tool::Help,
        "status" => Tool::Status,
        "profile" => Tool::Profile {
            reset: args.eq_ignore_ascii_case("reset"),
        },
        "conflicts" => Tool::Conflicts,
        "log" => Tool::Log(args),
        "recipes" => Tool::Recipes(args),
        "config" => Tool::Config(args),
        "ui-demo" => Tool::UiDemo(args),
        _ => Tool::About(word),
    })
}

/// The source, level and count `enshwick log` asks for, in any order; a
/// word that is no number, level or known source (`known`, or the loader's
/// own id) is returned as the error.
///
/// # Errors
///
/// The first word that is none of them.
pub fn log_query(
    args: &str,
    known: impl Fn(&str) -> bool,
) -> Result<(Option<String>, Level, usize), String> {
    let mut source = None;
    let mut level = Level::DEBUG;
    let mut count = 10;
    for word in args.split_whitespace() {
        if let Ok(n) = word.parse::<usize>() {
            count = n.clamp(1, MAX_LOG_LINES);
        } else if let Ok(named) = word.to_ascii_lowercase().parse::<Level>() {
            level = named;
        } else if word.eq_ignore_ascii_case(LOADER_ID) || known(word) {
            source = Some(word.to_ascii_lowercase());
        } else {
            return Err(word.to_owned());
        }
    }
    Ok((source, level, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loader_s_word_reaches_its_tools_and_any_other_names_a_mod() {
        assert_eq!(parse("   "), Line::Empty);
        assert_eq!(parse("enshwick"), Line::Loader(Tool::Mods));
        assert_eq!(parse("Enshwick  HELP"), Line::Loader(Tool::Help));
        assert_eq!(
            parse("enshwick you.trading-post"),
            Line::Loader(Tool::About("you.trading-post"))
        );
        assert_eq!(
            parse("enshwick profile reset"),
            Line::Loader(Tool::Profile { reset: true })
        );
        assert_eq!(
            parse("enshwick config you.trading-post language uk"),
            Line::Loader(Tool::Config("you.trading-post language uk"))
        );
        assert_eq!(
            parse("enshwick ui-demo Ann Lee"),
            Line::Loader(Tool::UiDemo("Ann Lee"))
        );
        assert_eq!(
            parse("you.my-mod give 7 1  Ann "),
            Line::Mod {
                id: "you.my-mod",
                rest: "give 7 1  Ann"
            }
        );
        assert_eq!(
            parse("you.trading-post"),
            Line::Mod {
                id: "you.trading-post",
                rest: ""
            }
        );
    }

    #[test]
    fn a_log_query_takes_its_words_in_any_order() {
        let known = |id: &str| id == "you.trading-post";
        assert_eq!(log_query("", known), Ok((None, Level::DEBUG, 10)));
        assert_eq!(
            log_query("warn 5 you.trading-post", known),
            Ok((Some("you.trading-post".to_owned()), Level::WARN, 5))
        );
        assert_eq!(
            log_query("Enshwick 999", known),
            Ok((Some("enshwick".to_owned()), Level::DEBUG, MAX_LOG_LINES))
        );
        assert_eq!(log_query("nobody", known), Err("nobody".to_owned()));
    }
}
