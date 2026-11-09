//! `cargo enshwick`: start an Enshwick mod, build it and install it into the game,
//! and port the build rows to a new game build.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context as _;
use clap::{Parser, Subcommand, ValueEnum};
use enshwick::content::Guid;

mod deps;
mod generate;
mod install;
mod loader;
mod manifest;
mod net;
mod new;
mod pack;
mod pin;
mod steam;
mod types;
mod update;

/// Cargo runs a subcommand as `cargo-enshwick enshwick <args>`.
#[derive(Debug, Parser)]
#[command(name = "cargo", bin_name = "cargo")]
enum Cargo {
    Enshwick(Args),
}

/// Start an Enshwick mod, build it and install it into the game.
#[derive(Debug, clap::Args)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Creates a mod crate in `<dir>/<name>`, or with `--data` a data pack
    /// folder and with `--luau` a Luau mod folder in `<dir>/<id>`.
    New {
        /// The mod id, `<author>.<name>`: each part of `[a-z0-9-]`, the name
        /// starting with a letter; the name names the crate.
        id: String,
        /// Creates a data pack, TOML files and no code, instead of a crate.
        #[arg(long, conflicts_with = "luau")]
        data: bool,
        /// Creates a Luau mod, a folder of scripts the loader runs, instead
        /// of a crate.
        #[arg(long)]
        luau: bool,
        /// Where the mod runs; a mod and a Luau mod run on the server and a
        /// data pack on both sides when left out.
        #[arg(long, value_enum)]
        side: Option<Side>,
        /// The folder the crate is created in.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// The game executable, or the game folder holding it, whose own
        /// types a Luau mod gets beside the API's.
        #[arg(long, requires = "luau", conflicts_with = "data")]
        game: Option<PathBuf>,
        /// A full commit hash of the platform repository to build the crate
        /// on from git, for a loader built from that commit; left out, the
        /// release this tool is from: an alpha's tag from git, as alphas are
        /// not on crates.io, or the crates.io release.
        #[arg(long, value_name = "COMMIT", conflicts_with_all = ["data", "luau"])]
        enshwick_rev: Option<new::Commit>,
    },
    /// Shows what a built mod DLL says about itself: id, version, side and
    /// dependencies. Loads the DLL, which runs its code: only for DLLs you
    /// trust.
    Manifest {
        /// The mod DLL.
        dll: PathBuf,
        /// Fails unless the DLL's version is this one.
        #[arg(long)]
        expect_version: Option<String>,
    },
    /// Builds the mod in release and copies its DLL into the game's `mods/`.
    Install {
        /// The folder holding `enshrouded.exe` or `enshrouded_server.exe`;
        /// `ENSHROUDED_DIR` when omitted, else Steam's game, else Steam's
        /// dedicated server.
        #[arg(long, env = "ENSHROUDED_DIR")]
        game: Option<PathBuf>,
        /// The package to install, when the workspace has several mods.
        #[arg(long, short)]
        package: Option<String>,
        /// Also puts in the Enshwick loader of the release this tool is
        /// from, downloaded from GitHub and checked, when the game folder
        /// has none; a different loader there is left as it is.
        #[arg(long)]
        with_loader: bool,
    },
    /// Ports the build rows of the loader and the mods in an Enshwick
    /// checkout to a new game build. Exits with 2 when an entry needs a human.
    Pin(pin::Args),
    /// Checks, describes and packages data packs.
    #[command(subcommand)]
    Pack(pack::Command),
    /// Writes what each dependency offers, its points, calls, mod events and services,
    /// as declarations in `src/deps/<alias>.rs` that `enshwick::deps!()`
    /// makes `deps::<alias>`, read from the owners' DLLs without running
    /// them.
    Deps {
        /// The dependency, by its alias in `requires` or `after`; every one
        /// when omitted.
        alias: Option<String>,
        /// The owners' DLL, a Luau mod's `pack.toml` or pack folder, a
        /// folder (such as the game's) with their DLLs and packs in it or in
        /// its `mods/`, or a schema file (`.toml`) an owner published.
        #[arg(long)]
        from: PathBuf,
        /// The mod's `Cargo.toml`, or with `--luau` its `pack.toml`; the one
        /// in the current folder when omitted.
        #[arg(long)]
        manifest_path: Option<PathBuf>,
        /// Writes a Luau module per point, call and event into
        /// `deps/<alias>/` beside `pack.toml`, for a Luau mod, instead.
        #[arg(long)]
        luau: bool,
    },
    /// Writes the Luau definitions of the API, which luau-lsp reads, for
    /// the loader this tool was built with.
    Luau {
        /// The file to write; the definitions are printed when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also writes the types of the typed API's values to this file,
        /// such as `enshwick.typed.d.luau`, for editors to load beside the
        /// definitions.
        #[arg(long)]
        typed: Option<PathBuf>,
    },
    /// Writes Rust and Luau definitions of the game's own types, read from
    /// the reflection data of a game executable: for structs the API does
    /// not wrap. Prints the Rust module when no file is named.
    Types {
        /// The types, by full name such as `keen::ecs::Health`, or a
        /// namespace ending in `::`; every runtime type when none is named.
        /// The types they need come with them.
        names: Vec<String>,
        /// The game executable, or the game folder holding it.
        #[arg(long)]
        from: PathBuf,
        /// The Rust module to write, such as `src/game.rs`.
        #[arg(long)]
        rust: Option<PathBuf>,
        /// The Luau definitions to write, such as `game.d.luau`.
        #[arg(long)]
        luau: Option<PathBuf>,
    },
    /// Writes the typed API, `enshwick::typed`, from the newest server
    /// build's snapshot of the game's reflection in the game data folder;
    /// with `--from`, snapshots that executable's reflection there first and
    /// writes from it when it is the server's.
    Generate {
        /// The game executable, or the game folder holding it, to snapshot.
        #[arg(long, conflicts_with = "check")]
        from: Option<PathBuf>,
        /// The enshwick repository to write in.
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// The game data folder outside the repository that holds the kept
        /// builds' tables in `tables/` and their snapshots in `typed/`.
        #[arg(long, env = enshwick_gamedata::DATA_VAR)]
        data: PathBuf,
        /// Writes nothing and fails when the committed files differ.
        #[arg(long)]
        check: bool,
    },
    /// Prints the GUID and game id of each content id, `<mod>:<name>`.
    Id {
        /// The content ids.
        #[arg(required = true)]
        names: Vec<String>,
    },
}

/// Where a mod runs, as `[package.metadata.enshwick]` spells it: only where
/// the world runs, or there and on every client playing in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Side {
    Server,
    Both,
}

/// A line per content id: the id, its GUID and its game id in decimal and
/// hex; an error naming the first malformed id.
fn ids(names: &[String]) -> anyhow::Result<String> {
    let lines: anyhow::Result<Vec<String>> = names
        .iter()
        .map(|name| {
            let guid = Guid::try_named(name).with_context(|| {
                format!("{name:?} is no content id: <mod>:<name>, the mod side a mod id, the name 1 to 128 of [a-z0-9_./-]")
            })?;
            Ok(format!("{name}  {guid}  {id} {id:#010x}", id = guid.fnv1a()))
        })
        .collect();
    Ok(lines?.join("\n"))
}

/// Writes the Luau definitions to `out`, or returns them to print, and the
/// typed API's types to `typed`.
fn luau_definitions(
    out: Option<&std::path::Path>,
    typed: Option<&std::path::Path>,
) -> anyhow::Result<String> {
    if let (Some(out), Some(typed)) = (out, typed)
        && std::path::absolute(out)? == std::path::absolute(typed)?
    {
        anyhow::bail!(
            "--out and --typed both name {}: each needs a file of its own",
            out.display()
        );
    }
    let mut done = Vec::new();
    if let Some(path) = typed {
        std::fs::write(path, cargo_enshwick::luau::typed_definitions())
            .with_context(|| path.display().to_string())?;
        done.push(format!("wrote {}", path.display()));
    }
    let text = cargo_enshwick::luau::definitions();
    let Some(path) = out else {
        // Printed definitions stay a file luau-lsp reads: no status line.
        return Ok(text);
    };
    std::fs::write(path, text).with_context(|| path.display().to_string())?;
    done.push(format!("wrote {}", path.display()));
    Ok(done.join("\n"))
}

fn main() -> ExitCode {
    let Cargo::Enshwick(args) = Cargo::parse();
    let check = update::start();
    let code = run(args);
    if let Some(notice) = check.and_then(update::finish) {
        let _shown = writeln!(std::io::stderr(), "note: {notice}");
    }
    code
}

/// Runs the command `args` names and prints what it did or why it failed.
fn run(args: Args) -> ExitCode {
    let outcome = match args.command {
        Command::Pin(args) => {
            return match pin::run(&args) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(pin::NEEDS_HUMAN),
                Err(error) => {
                    let _shown = writeln!(std::io::stderr(), "error: {error:#}");
                    ExitCode::FAILURE
                }
            };
        }
        Command::New {
            id,
            data: true,
            side,
            dir,
            ..
        } => new::data::create(&dir, &id, side.unwrap_or(Side::Both)),
        Command::New {
            id,
            luau: true,
            side,
            dir,
            game,
            ..
        } => new::luau::create(&dir, &id, side.unwrap_or(Side::Server), game.as_deref()),
        Command::New {
            id,
            side,
            dir,
            enshwick_rev,
            ..
        } => new::create(
            &dir,
            &id,
            side.unwrap_or(Side::Server),
            enshwick_rev.as_ref(),
        ),
        Command::Install {
            game,
            package,
            with_loader,
        } => install::install(game.as_deref(), package.as_deref(), with_loader),
        Command::Manifest {
            dll,
            expect_version,
        } => manifest::show(&dll, expect_version.as_deref()).map_err(anyhow::Error::msg),
        Command::Id { names } => ids(&names),
        Command::Deps {
            alias,
            from,
            manifest_path,
            luau,
        } => deps::run(alias.as_deref(), &from, manifest_path.as_deref(), luau),
        Command::Pack(command) => pack::run(&command),
        Command::Luau { out, typed } => luau_definitions(out.as_deref(), typed.as_deref()),
        Command::Types {
            names,
            from,
            rust,
            luau,
        } => types::run(&from, &names, rust.as_deref(), luau.as_deref()),
        Command::Generate {
            from,
            root,
            data,
            check,
        } => generate::run(&root, &data, from.as_deref(), check),
    };
    match outcome {
        Ok(done) => {
            let _shown = writeln!(std::io::stdout(), "{done}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// A fresh temporary folder for one test, named after the calling file and
/// `name`, so tests of different modules never share one.
#[cfg(test)]
#[track_caller]
fn scratch(name: &str) -> PathBuf {
    let file = std::panic::Location::caller()
        .file()
        .replace(['/', '\\', '.'], "-");
    let dir = std::env::temp_dir().join(format!("enshwick-{file}-{name}-{}", std::process::id()));
    let _old = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_prints_its_guid_and_game_id() {
        let line = ids(&["you.lockers:locker".to_owned()]).unwrap();
        let guid = Guid::named("you.lockers:locker");
        assert_eq!(
            line,
            format!(
                "you.lockers:locker  67842cb8-d551-542b-8765-fe1af9c28aca  {0} {0:#010x}",
                guid.fnv1a()
            )
        );
        assert!(ids(&["safe".to_owned()]).is_err());
    }

    #[test]
    fn one_file_for_both_definitions_is_refused() {
        let path = std::path::Path::new("types/both.d.luau");
        let error = luau_definitions(Some(path), Some(path)).unwrap_err();
        assert!(error.to_string().contains("--out and --typed"), "{error}");
        assert!(!path.exists());
    }

    #[test]
    fn printed_definitions_hold_only_luau_when_the_typed_file_is_written() {
        let typed =
            std::env::temp_dir().join(format!("enshwick-typed-{}.d.luau", std::process::id()));
        let printed = luau_definitions(None, Some(&typed)).unwrap();
        assert!(printed.starts_with("--!strict"), "{}", &printed[..40]);
        assert!(
            std::fs::read_to_string(&typed)
                .unwrap()
                .contains("export type typed_")
        );
        std::fs::remove_file(&typed).unwrap();
    }

    #[test]
    fn the_game_s_types_come_only_with_a_luau_mod() {
        let parse = |args: &[&str]| {
            Cargo::try_parse_from(
                ["cargo", "enshwick", "new", "you.greeter"]
                    .iter()
                    .chain(args),
            )
        };
        assert!(parse(&["--luau", "--game", "game"]).is_ok());
        assert!(parse(&["--game", "game"]).is_err());
        assert!(parse(&["--data", "--game", "game"]).is_err());
    }
}
