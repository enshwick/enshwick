//! The server's console: lines a mod runs on the server as an administrator
//! with no character, such as each line of a dedicated server's command file.
//!
//! `enshwick ...` reaches the loader's own tools; any other first word names a
//! mod, whose actions take the rest of the line.
//!
//! On a dedicated server the loader feeds it from the command file
//! ([`file`]) and from the server's own console window or terminal
//! ([`input`]). Reading a line is pure logic here; the run module answers it
//! on the game thread, after the step.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{BuildingData, Level};

use crate::locks::Relock;
use crate::settings::LOADER_ID;

pub mod accounts;
#[cfg(windows)]
mod admin;
pub mod answers;
pub mod file;
pub mod input;
pub mod orders;
#[cfg(windows)]
mod run;
#[cfg(windows)]
pub(crate) use admin::TABLE_IDS;
#[cfg(windows)]
mod tools;
#[cfg(windows)]
pub(crate) use tools::{is_admin as player_is_admin, name as player_name};
#[cfg(windows)]
mod world_orders;
#[cfg(windows)]
pub(crate) use run::{after_step, run, start, the_console};

/// The command files a server reads in `data_dir`: the loader's own, then
/// the one of each mod whose work the loader took over
/// ([`RETIRED`](crate::resolve::RETIRED)), where scripts may still write.
#[must_use]
pub fn command_files(data_dir: &Path) -> Vec<PathBuf> {
    std::iter::once(LOADER_ID)
        .chain(crate::resolve::RETIRED.iter().copied())
        .map(|id| data_dir.join(id).join(file::FILE))
        .collect()
}

/// Where a console line came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// A command file, as the log names it.
    File(String),
    /// The server's console window or terminal, which also reads the answer.
    Input,
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(name) => f.write_str(name),
            Self::Input => f.write_str("console"),
        }
    }
}

/// A console line waiting for the game thread, and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    /// The line, trimmed.
    pub line: String,
    /// Where it came from.
    pub source: Source,
}

/// What the console keeps while the game runs.
struct State {
    /// Lines waiting for the game thread.
    pending: Mutex<Vec<Pending>>,
    /// Set while `pending` holds a line, so a step with none takes no lock.
    waiting: AtomicBool,
    /// The pieces placed lately, newest last.
    placed: Mutex<VecDeque<answers::Placed>>,
    /// The game's account action handler and its action's layout, or why
    /// this build lacks them.
    #[cfg(windows)]
    handler: std::sync::OnceLock<Result<admin::Handler, String>>,
}

static STATE: State = State {
    pending: Mutex::new(Vec::new()),
    waiting: AtomicBool::new(false),
    placed: Mutex::new(VecDeque::new()),
    #[cfg(windows)]
    handler: std::sync::OnceLock::new(),
};

/// Queues `line` from `source` for the game thread.
pub fn submit(line: String, source: Source) {
    let mut pending = STATE.pending.relock();
    pending.push(Pending { line, source });
    STATE.waiting.store(true, Ordering::Release);
}

/// Keeps the pieces placed among `built`, one step's building events, among
/// the [`answers::REMEMBERED`] placed lately.
pub fn remember(built: &[BuildingData]) {
    if built.is_empty() {
        return;
    }
    let pieces = built
        .iter()
        .filter(|data| data.kind == BuildingData::PLACED)
        .map(|data| answers::Placed {
            entity: data.entity,
            item: data.item,
            template: data.template,
            position: data.position,
        });
    let mut placed = STATE.placed.relock();
    for piece in pieces {
        if placed.len() == answers::REMEMBERED {
            placed.pop_front();
        }
        placed.push_back(piece);
    }
}

/// The `count` pieces placed last, newest first.
#[must_use]
pub fn newest(count: usize) -> Vec<answers::Placed> {
    STATE
        .placed
        .relock()
        .iter()
        .rev()
        .take(count)
        .copied()
        .collect()
}

/// The lines queued since the last call, oldest first.
#[must_use]
pub fn take() -> Vec<Pending> {
    if !STATE.waiting.load(Ordering::Acquire) {
        return Vec::new();
    }
    let mut pending = STATE.pending.relock();
    STATE.waiting.store(false, Ordering::Release);
    std::mem::take(&mut *pending)
}

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
    /// `enshwick`: the loader's version and the mods running.
    Overview,
    /// `enshwick mods`: each mod with its side, join need and state.
    Mods,
    /// `enshwick world`: the players online.
    World,
    /// `enshwick entity <id>`: what the server knows of an entity.
    Entity(&'a str),
    /// `enshwick placed [n]`: the pieces placed lately.
    Placed(&'a str),
    /// `enshwick reload <mod id>`: a mod reads its settings again.
    Reload(&'a str),
    /// `enshwick kick|ban|unban <player or account>`: the game's account
    /// actions.
    Account(accounts::Act, &'a str),
    /// `enshwick give|give-to|spawn|teleport ...`: the game's admin
    /// orders on the world.
    Order(Order, &'a str),
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
    /// `enshwick allowlist [add|remove <player or account>]`: who may join.
    Allowlist(&'a str),
    /// `enshwick config [mod [key [value]]]`: mods' settings.
    Config(&'a str),
    /// `enshwick ui-demo <player>`: the interface demo on a player's client.
    UiDemo(&'a str),
}

/// The game's admin orders on the world the console gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Items for every player online.
    Give,
    /// Items for one player.
    GiveTo,
    /// Entities from a template.
    Spawn,
    /// A player moved.
    Teleport,
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
        "" => Tool::Overview,
        "help" => Tool::Help,
        "mods" => Tool::Mods,
        "world" => Tool::World,
        "entity" => Tool::Entity(args),
        "placed" => Tool::Placed(args),
        "reload" => Tool::Reload(args),
        "kick" => Tool::Account(accounts::Act::Kick, args),
        "ban" => Tool::Account(accounts::Act::Ban, args),
        "unban" => Tool::Account(accounts::Act::Unban, args),
        "give" => Tool::Order(Order::Give, args),
        "give-to" => Tool::Order(Order::GiveTo, args),
        "spawn" => Tool::Order(Order::Spawn, args),
        "teleport" => Tool::Order(Order::Teleport, args),
        "status" => Tool::Status,
        "profile" => Tool::Profile {
            reset: args.eq_ignore_ascii_case("reset"),
        },
        "conflicts" => Tool::Conflicts,
        "log" => Tool::Log(args),
        "recipes" => Tool::Recipes(args),
        "allowlist" => Tool::Allowlist(args),
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
        assert_eq!(parse("enshwick"), Line::Loader(Tool::Overview));
        assert_eq!(parse("enshwick mods"), Line::Loader(Tool::Mods));
        assert_eq!(parse("enshwick World"), Line::Loader(Tool::World));
        assert_eq!(
            parse("enshwick entity 12"),
            Line::Loader(Tool::Entity("12"))
        );
        assert_eq!(parse("enshwick placed"), Line::Loader(Tool::Placed("")));
        assert_eq!(
            parse("enshwick reload you.safes"),
            Line::Loader(Tool::Reload("you.safes"))
        );
        assert_eq!(
            parse("enshwick ban Ann Lee"),
            Line::Loader(Tool::Account(accounts::Act::Ban, "Ann Lee"))
        );
        assert_eq!(parse("Enshwick  HELP"), Line::Loader(Tool::Help));
        assert_eq!(
            parse("enshwick give-to 7 1 Ann"),
            Line::Loader(Tool::Order(Order::GiveTo, "7 1 Ann"))
        );
        assert_eq!(
            parse("enshwick teleport 1 2 3 Ann"),
            Line::Loader(Tool::Order(Order::Teleport, "1 2 3 Ann"))
        );
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
    fn a_server_reads_its_own_command_file_and_each_retired_mod_s() {
        let data = Path::new("mod-data");
        let files = command_files(data);
        assert_eq!(files[0], data.join(LOADER_ID).join(file::FILE));
        assert_eq!(files.len(), 1 + crate::resolve::RETIRED.len());
        for id in crate::resolve::RETIRED {
            assert!(files.contains(&data.join(id).join(file::FILE)));
        }
    }

    #[test]
    fn only_the_latest_placed_pieces_are_kept_newest_first() {
        let piece = |entity, kind| BuildingData {
            header: enshwick::abi::Header::of::<BuildingData>(),
            entity,
            player: 7,
            item: 1,
            kind,
            replay: 0,
            template: [0; 16],
            position: [0.0; 3],
        };
        let built: Vec<BuildingData> = (0..40)
            .map(|entity| piece(entity, BuildingData::PLACED))
            .chain([piece(99, BuildingData::TORN_DOWN)])
            .collect();
        remember(&built);
        let newest: Vec<u32> = newest(usize::MAX).iter().map(|p| p.entity).collect();
        assert_eq!(newest.len(), answers::REMEMBERED);
        assert_eq!(newest[..2], [39, 38]);
    }

    #[test]
    fn queued_lines_come_out_once_in_order() {
        submit("enshwick status".into(), Source::File("a".into()));
        submit("enshwick help".into(), Source::Input);
        let lines: Vec<String> = take().into_iter().map(|p| p.line).collect();
        assert_eq!(lines, ["enshwick status", "enshwick help"]);
        assert!(take().is_empty());
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
