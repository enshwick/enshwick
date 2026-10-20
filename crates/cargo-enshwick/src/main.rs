//! `cargo enshwick`: start an Enshwick mod, build it and install it into the game.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

mod install;
mod manifest;
mod new;

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
    /// Creates a mod crate in `<dir>/<id>`.
    New {
        /// The mod id: 1 to 64 characters of `[a-z0-9-]`.
        id: String,
        /// Where the mod runs.
        #[arg(long, value_enum, default_value_t = Side::Server)]
        side: Side,
        /// The folder the crate is created in.
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// Shows what a built mod DLL says about itself: id, version, side and
    /// dependencies.
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
}

/// Where a mod runs, as `manifest!` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum Side {
    Server,
    Client,
    Both,
}

fn main() -> ExitCode {
    let Cargo::Enshwick(args) = Cargo::parse();
    let outcome = match args.command {
        Command::New { id, side, dir } => new::create(&dir, &id, side),
        Command::Install { game, package } => install::install(&game, package.as_deref()),
        Command::Manifest {
            dll,
            expect_version,
        } => manifest::show(&dll, expect_version.as_deref()),
    };
    match outcome {
        Ok(done) => {
            let _shown = writeln!(std::io::stdout(), "{done}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "error: {error}");
            ExitCode::FAILURE
        }
    }
}
