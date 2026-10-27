//! `cargo enshwick`: start an Enshwick mod, build it and install it into the game,
//! and port the build rows to a new game build.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Context as _;
use clap::{Parser, Subcommand, ValueEnum};
use enshwick::content::Guid;

mod install;
mod manifest;
mod new;
mod pack;
mod pin;
mod point;
mod types;

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
        /// `ENSHROUDED_DIR` when omitted.
        #[arg(long, env = "ENSHROUDED_DIR")]
        game: PathBuf,
        /// The package to install, when the workspace has several mods.
        #[arg(long, short)]
        package: Option<String>,
    },
    /// Ports the build rows of the loader and the mods in an Enshwick
    /// checkout to a new game build. Exits with 2 when an entry needs a human.
    Pin(pin::Args),
    /// Checks, describes and packages data packs.
    #[command(subcommand)]
    Pack(pack::Command),
    /// Writes the Rust definition of another mod's extension point, or with
    /// `--luau` its Luau module, read from the owner's DLL without running it.
    Point {
        /// The point, `<mod id>:<point>`.
        name: String,
        /// The owner's DLL, a folder (such as the game's) with its DLL in it
        /// or in its `mods/`, or a schema file (`.toml`) the owner published.
        #[arg(long)]
        from: PathBuf,
        /// The file to write; the definition is printed when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Writes the definition as a Luau module for a Luau mod instead of
        /// Rust.
        #[arg(long)]
        luau: bool,
    },
    /// Writes the Luau module of another mod's event, read from the owner's
    /// DLL without running it, for a Luau mod to subscribe with.
    Event {
        /// The event, `<mod id>:<event>`.
        name: String,
        /// The owner's DLL, a folder with its DLL in it or in its `mods/`,
        /// or a schema file (`.toml`) the owner published.
        #[arg(long)]
        from: PathBuf,
        /// The file to write; the module is printed when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Writes the declaration of another mod's call, read from the server's
    /// DLL without running it, for a mod that calls it.
    Call {
        /// The call, `<mod id>:<call>`.
        name: String,
        /// The server's DLL, a folder with its DLL in it or in its `mods/`,
        /// or a schema file (`.toml`) the server published.
        #[arg(long)]
        from: PathBuf,
        /// The file to write; the declaration is printed when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Writes the Luau definitions of the API, which luau-lsp reads, for
    /// the loader this tool was built with.
    Luau {
        /// The file to write; the definitions are printed when omitted.
        #[arg(long)]
        out: Option<PathBuf>,
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
    /// Prints the GUID and game id of each content id, `<mod>:<name>`.
    Id {
        /// The content ids.
        #[arg(required = true)]
        names: Vec<String>,
    },
}

/// Where a mod runs, as `manifest!` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Side {
    Server,
    Client,
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
            Ok(format!("{name}  {guid}  {id} {id:#010x}", id = guid.id()))
        })
        .collect();
    Ok(lines?.join("\n"))
}

/// Writes the Luau definitions to `out`, or returns them to print.
fn luau_definitions(out: Option<&std::path::Path>) -> anyhow::Result<String> {
    let text = cargo_enshwick::luau::definitions();
    match out {
        Some(path) => {
            std::fs::write(path, text).with_context(|| path.display().to_string())?;
            Ok(format!("wrote {}", path.display()))
        }
        None => Ok(text),
    }
}

fn main() -> ExitCode {
    let Cargo::Enshwick(args) = Cargo::parse();
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
        Command::New { id, side, dir, .. } => new::create(&dir, &id, side.unwrap_or(Side::Server)),
        Command::Install { game, package } => install::install(&game, package.as_deref()),
        Command::Manifest {
            dll,
            expect_version,
        } => manifest::show(&dll, expect_version.as_deref()).map_err(anyhow::Error::msg),
        Command::Id { names } => ids(&names),
        Command::Point {
            name,
            from,
            out,
            luau,
        } => point::run(&name, &from, out.as_deref(), luau),
        Command::Pack(command) => pack::run(&command),
        Command::Luau { out } => luau_definitions(out.as_deref()),
        Command::Event { name, from, out } => point::run_event(&name, &from, out.as_deref()),
        Command::Call { name, from, out } => point::run_call(&name, &from, out.as_deref()),
        Command::Types {
            names,
            from,
            rust,
            luau,
        } => types::run(&from, &names, rust.as_deref(), luau.as_deref()),
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
                guid.id()
            )
        );
        assert!(ids(&["safe".to_owned()]).is_err());
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
