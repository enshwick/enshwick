//! Answers console lines on the server's game thread.

use std::sync::atomic::Ordering;

use enshwick::abi::{Header, PlayerInfo, Status};

use crate::host::Loader;
use crate::locks::Relock;
use crate::texts;

use super::file::{Watched, watch};
use super::input::{self, End};
use super::{Line, Source, Tool, command_files, log_query, parse, submit, take};

/// Starts reading the command files and the console input of a dedicated
/// server.
pub(crate) fn start(loader: &'static Loader) {
    let Some(game) = loader.data_dir.parent() else {
        return;
    };
    let files: Vec<Watched> = command_files(&loader.data_dir, |id| {
        loader
            .find_mod(id)
            .is_some_and(|(_, m)| !m.off.load(Ordering::Acquire))
    })
    .into_iter()
    .map(Watched::new)
    .collect();
    for file in &files {
        crate::log!(INFO, "the console reads {}", shown(game, file.path()));
    }
    let game = game.to_path_buf();
    let started = std::thread::Builder::new()
        .name("enshwick-command-file".into())
        .spawn(move || {
            watch(files, |path, line| {
                submit(line, Source::File(shown(&game, path)));
            })
        });
    if let Err(e) = started {
        crate::log!(WARN, "the command file is not read: {e}");
    }
    let started = std::thread::Builder::new()
        .name("enshwick-console-input".into())
        .spawn(|| {
            match input::read(std::io::stdin().lock(), |line| {
                submit(line, Source::Input);
            }) {
                End::Closed => crate::log!(DEBUG, "the console has no more input"),
                End::Failed(why) => crate::log!(WARN, "the console input stopped: {why}"),
            }
        });
    if let Err(e) = started {
        crate::log!(WARN, "the console input is not read: {e}");
    }
}

/// `path` as the log names it: from the game's folder, with `/`.
fn shown(game: &std::path::Path, path: &std::path::Path) -> String {
    path.strip_prefix(game)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Runs the console lines queued since the last step and logs each answer;
/// on the game thread, after the step.
pub(crate) fn after_step(loader: &Loader) {
    for pending in take() {
        let line = &pending.line;
        let from = &pending.source;
        let answer = match run(loader, line) {
            Ok(answer) if answer.is_empty() => {
                crate::log!(INFO, "{from}: {line}: no answer");
                texts::fill("console.no-answer", &[])
            }
            Ok(answer) => {
                crate::log!(INFO, "{from}: {line}:\n{answer}");
                answer
            }
            Err(status) => {
                let why = refusal(line, status);
                crate::log!(WARN, "{from}: {line}: {why}");
                why
            }
        };
        if *from == Source::Input {
            show(&answer);
        }
    }
}

/// Writes `answer` to the server's console window or terminal, where the
/// line was typed; the server's own output shares it.
fn show(answer: &str) {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    let _shown = writeln!(out, "{answer}").and_then(|()| out.flush());
}

/// Why the console did not answer `line`.
fn refusal(line: &str, status: Status) -> String {
    let word = line.split_whitespace().next().unwrap_or_default();
    if status == Status::MISSING {
        texts::fill("enshwick.unknown", &[("id", word)])
    } else {
        format!("not run ({status:?})")
    }
}

/// The server's console as a player: an administrator with no character.
pub(crate) fn the_console() -> PlayerInfo {
    let mut name = [0u8; 64];
    name[..7].copy_from_slice(b"console");
    PlayerInfo {
        header: Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: 0,
        key: 0,
        permissions: PlayerInfo::CAN_KICK_BAN,
        name_len: 7,
        name,
    }
}

/// Runs `line` as the server's console, on the game thread; the answer.
/// [`Status::MISSING`] when no running mod with actions has the line's
/// first word as its id.
pub(crate) fn run(loader: &Loader, line: &str) -> Result<String, Status> {
    crate::world::server().map_err(|_| Status::WRONG_THREAD)?;
    match parse(line) {
        Line::Empty => Err(Status::BAD_ARGS),
        Line::Mod { id, rest } => {
            let (index, _) = loader
                .find_mod(&id.to_ascii_lowercase())
                .ok_or(Status::MISSING)?;
            crate::log!(DEBUG, "the console runs {id} {rest}");
            crate::actions::console(loader, index, rest).ok_or(Status::MISSING)
        }
        Line::Loader(tool) => {
            crate::log!(DEBUG, "the console runs {line}");
            Ok(answer(loader, tool))
        }
    }
}

/// What the loader's tool `tool` answers.
fn answer(loader: &Loader, tool: Tool<'_>) -> String {
    match tool {
        Tool::Mods => mods(loader),
        Tool::Help => help(loader),
        Tool::About(id) => loader
            .mods
            .iter()
            .find(|m| m.id.eq_ignore_ascii_case(id))
            .map_or_else(|| texts::fill("enshwick.unknown", &[("id", id)]), about),
        Tool::Status => {
            crate::status::request();
            crate::status::summary(loader).short()
        }
        Tool::Profile { reset: true } => {
            crate::status::reset_profile(loader);
            texts::fill("enshwick.profile.reset", &[])
        }
        Tool::Profile { reset: false } => {
            let mut lines = crate::status::cost_lines(loader, 5, 2);
            lines.push(texts::fill("enshwick.profile.full", &[]));
            lines.join("\n")
        }
        Tool::Conflicts => {
            let lines = crate::status::conflicts(loader);
            if lines.is_empty() {
                texts::fill("enshwick.conflicts.none", &[])
            } else {
                lines.join("\n")
            }
        }
        Tool::Log(args) => log_lines(loader, args),
        Tool::Recipes(args) => crate::recipes::command(loader, args),
        Tool::Config(args) => config(loader, args),
        Tool::UiDemo(name) => ui_demo(loader, name),
    }
}

/// `enshwick`: the loader's version and the mods running.
fn mods(loader: &Loader) -> String {
    let mods: Vec<String> = loader.mods.iter().map(name).collect();
    let version = env!("CARGO_PKG_VERSION");
    let mut sentences = vec![if mods.is_empty() {
        texts::fill("enshwick.mods.none", &[("version", version)])
    } else {
        texts::fill(
            "enshwick.mods",
            &[("version", version), ("mods", &mods.join(", "))],
        )
    }];
    let disabled = crate::modlist::disabled();
    if !disabled.is_empty() {
        sentences.push(texts::fill(
            "enshwick.off-in",
            &[
                ("file", crate::modlist::LIST_PATH),
                ("mods", &disabled.join(", ")),
            ],
        ));
    }
    sentences.extend(crate::modlist::replaced().iter().map(|old| {
        texts::fill(
            "enshwick.replaced",
            &[("id", &old.id), ("by", &old.by), ("file", &old.file)],
        )
    }));
    sentences.push(texts::fill("enshwick.more", &[]));
    sentences.join(" ")
}

/// A mod's name and version, marked when it is off.
fn name(record: &crate::host::ModRecord) -> String {
    let label = record.label();
    if record.off.load(Ordering::Acquire) {
        texts::fill(
            "mods.off",
            &[("name", &label), ("version", &record.version)],
        )
    } else {
        format!("{label} {}", record.version)
    }
}

/// What `record` is, for `enshwick <mod id>`.
fn about(record: &crate::host::ModRecord) -> String {
    let mut lines = vec![name(record)];
    if !record.meta.description.is_empty() {
        lines.push(record.meta.description.clone());
    }
    let credits = record.meta.credits();
    if !credits.is_empty() {
        lines.push(credits);
    }
    lines.extend(record.notes.iter().cloned());
    lines.join("\n")
}

/// `enshwick help`: the loader's tools and the mods whose actions the
/// console runs.
fn help(loader: &Loader) -> String {
    let mut lines = vec![texts::fill("enshwick.help", &[])];
    let with: Vec<&str> = crate::actions::declaring(loader)
        .into_iter()
        .filter_map(|index| loader.mods.get(index.get()).map(|m| m.id.as_str()))
        .collect();
    lines.push(if with.is_empty() {
        texts::fill("enshwick.actions.none", &[])
    } else {
        texts::fill("enshwick.actions", &[("mods", &with.join(", "))])
    });
    lines.join("\n")
}

/// `enshwick log [mod] [level] [n]`: the latest `n` lines the loader kept,
/// from one mod or the loader or all, at `level` or more severe.
fn log_lines(loader: &Loader, args: &str) -> String {
    match log_query(args, |id| loader.mods.iter().any(|m| m.id == id)) {
        Ok((source, level, count)) => {
            let lines = crate::log::recent(source.as_deref(), level, count);
            if lines.is_empty() {
                texts::fill("enshwick.log.none", &[])
            } else {
                lines.join("\n")
            }
        }
        Err(word) => texts::fill("enshwick.log.usage", &[("word", &word)]),
    }
}

/// `enshwick config`: read and change mods' settings.
fn config(loader: &Loader, args: &str) -> String {
    use crate::configure::{self, Request};

    let request = configure::parse(args);
    let id = match request {
        Request::Mods => {
            let with: Vec<&str> = loader
                .mods
                .iter()
                .filter(|m| m.schema.relock().is_some())
                .map(|m| m.id.as_str())
                .collect();
            return if with.is_empty() {
                texts::fill("config.none", &[])
            } else {
                texts::fill("config.mods", &[("mods", &with.join(", "))])
            };
        }
        Request::List(id) | Request::Get(id, _) | Request::Set(id, _, _) => id,
    };
    let Some((index, record)) = loader.find_mod(id) else {
        return texts::fill("config.unknown", &[("id", id)]);
    };
    let id = record.id.as_str();
    let Some(schema) = record.schema.relock().clone() else {
        return texts::fill("config.no-schema", &[("id", id)]);
    };
    let path = loader.configs_dir.join(format!("{id}.toml"));
    if !path.is_file() {
        return texts::fill("config.missing", &[("id", id)]);
    }
    let text = crate::worlds::show_text(loader, record);
    match request {
        Request::Set(_, key, value) => match configure::apply(loader, index, key, value, "console")
        {
            Ok(change) => configure::changed(id, key, &change),
            Err(reply) => reply,
        },
        Request::Get(_, key) => configure::get(id, &schema, &text, key).unwrap_or_else(|e| e),
        _ => configure::list(id, &schema, &text),
    }
}

/// `enshwick ui-demo <player>`: opens the interface demo on the client of
/// the player online with that name.
fn ui_demo(loader: &Loader, name: &str) -> String {
    let wanted = name.to_lowercase();
    let player = crate::world::players(loader).ok().and_then(|players| {
        players.into_iter().find(|p| {
            let named = &p.name[..usize::from(p.name_len).min(p.name.len())];
            String::from_utf8_lossy(named).to_lowercase() == wanted
        })
    });
    let Some(player) = player.filter(|_| !wanted.is_empty()) else {
        return texts::fill("ui-demo.player", &[("name", name)]);
    };
    match crate::net::ui_demo(
        player.key,
        crate::world::fixed_position(loader, player.entity),
    ) {
        Ok(()) => texts::fill("ui-demo.opening", &[("name", name)]),
        Err(_) => texts::fill("ui-demo.no-client", &[("name", name)]),
    }
}
