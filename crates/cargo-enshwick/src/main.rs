//! `cargo enshwick`: start an Enshwick mod, build it and install it into the game,
//! and port the build rows to a new game build.

use anyhow::Context as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use enshwick::content::Guid;

mod install;
mod manifest;
mod new;
mod pack;
mod pin;
mod point;

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
    /// folder in `<dir>/<id>`.
    New {
        /// The mod id, `<author>.<name>`: each part of `[a-z0-9-]`, the name
        /// starting with a letter; the name names the crate.
        id: String,
        /// Creates a data pack, TOML files and no code, instead of a crate.
        #[arg(long)]
        data: bool,
        /// Where the mod runs; a mod runs on the server and a data pack on
        /// both sides when left out.
        #[arg(long, value_enum)]
        side: Option<Side>,
        /// The folder the crate is created in.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
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
    /// Writes the Rust definition of another mod's extension point, read
    /// from the owner's DLL without running it.
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
        } => new::data::create(&dir, &id, side.unwrap_or(Side::Both)),
        Command::New { id, side, dir, .. } => new::create(&dir, &id, side.unwrap_or(Side::Server)),
        Command::Install { game, package } => install::install(&game, package.as_deref()),
        Command::Manifest {
            dll,
            expect_version,
        } => manifest::show(&dll, expect_version.as_deref()).map_err(anyhow::Error::msg),
        Command::Id { names } => ids(&names),
        Command::Point { name, from, out } => point::run(&name, &from, out.as_deref()),
        Command::Pack(command) => pack::run(&command),
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
}
