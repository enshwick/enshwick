//! What works on a game build: which of the loader's features have every
//! table entry they name, and how given mods fare when the loader starts
//! them. A maintainer's tool of this repository; it links the loader, which
//! is not published.

mod compat;

use std::io::Write as _;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The command line.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    #[command(flatten)]
    args: Option<compat::Args>,
    #[command(subcommand)]
    step: Option<Step>,
}

/// A step the tool runs in a process of its own.
#[derive(Debug, Subcommand)]
enum Step {
    /// Runs the loader on a prepared game folder.
    #[command(hide = true)]
    Run(compat::RunArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let outcome = match (cli.step, cli.args) {
        (Some(Step::Run(args)), _) => compat::run_loader(&args).map(|()| true),
        (None, Some(args)) => compat::run(&args),
        (None, None) => Err("name a game executable; --help lists the options".to_owned()),
    };
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(compat::BROKEN),
        Err(error) => {
            let _shown = writeln!(std::io::stderr(), "error: {error}");
            ExitCode::FAILURE
        }
    }
}
