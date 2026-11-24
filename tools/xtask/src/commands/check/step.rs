//! One command line of the checks and how it runs.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use crate::{cargo, say, tool_path, workspace_root};

/// The variable that moves cargo's target folder.
const TARGET_DIR_VAR: &str = "CARGO_TARGET_DIR";

/// One command line: cargo, or another tool of the checks.
#[derive(Debug)]
pub(crate) struct Step {
    pub(super) program: Option<String>,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(&'static str, String)>,
}

impl Step {
    pub(crate) fn new(words: &[&str]) -> Self {
        Self {
            program: None,
            args: words.iter().map(|w| (*w).to_owned()).collect(),
            env: Vec::new(),
        }
    }

    /// A step that runs `program` instead of cargo.
    pub(crate) fn tool(program: &'static str, words: &[&str]) -> Self {
        Self {
            program: Some(program.to_owned()),
            ..Self::new(words)
        }
    }

    /// A step that runs this xtask binary again, without a cargo start and
    /// its rebuild check of the binary that runs.
    pub(super) fn xtask(words: &[&str]) -> Self {
        match std::env::current_exe() {
            Ok(exe) => Self {
                program: Some(exe.display().to_string()),
                ..Self::new(words)
            },
            Err(_) => Self::new(&["xtask"]).then(words),
        }
    }

    pub(super) fn then_owned(mut self, words: &[String]) -> Self {
        self.args.extend_from_slice(words);
        self
    }

    pub(crate) fn then(mut self, words: &[&str]) -> Self {
        self.args.extend(words.iter().map(|w| (*w).to_owned()));
        self
    }

    pub(crate) fn env(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.env.push((name, value.into()));
        self
    }

    /// Whether the step runs cargo's `first` subcommand.
    pub(super) fn runs(&self, first: &str) -> bool {
        self.program.is_none() && self.args.first().is_some_and(|arg| arg == first)
    }

    /// The search index a `cargo doc` step writes, which it removes first:
    /// rustdoc merges each run into the index the runs before left and never
    /// prunes it, so in a tree that ran the check many times the stable
    /// surface's rustdoc took 103 s, and 5 s from a clean folder, growing
    /// about 3 s and 2.5 MiB a run (rustdoc 1.100, 2026-11-24). An index then
    /// holds only the crates its run documented; no check reads it. The
    /// target folder is cargo's: `--target-dir`, else `CARGO_TARGET_DIR` of
    /// the step or else `inherited`, relative to `cwd`, else `root`'s
    /// `target`; a `--target` build writes only under its triple.
    ///
    /// shortcut: the index goes before cargo takes its build lock, so a
    /// second check documenting in the same tree at the same moment can
    /// lose its index mid-run; take rustdoc's own merge switch once stable.
    pub(super) fn search_index(
        &self,
        root: &Path,
        cwd: &Path,
        inherited: Option<&str>,
    ) -> Option<PathBuf> {
        if !self.runs("doc") {
            return None;
        }
        let flag = |name: &str| {
            let joined = format!("{name}=");
            self.args.iter().enumerate().find_map(|(at, arg)| {
                arg.strip_prefix(&joined).or_else(|| {
                    (arg == name)
                        .then(|| self.args.get(at + 1).map(String::as_str))
                        .flatten()
                })
            })
        };
        let env = self
            .env
            .iter()
            .find(|(name, _)| *name == TARGET_DIR_VAR)
            .map(|(_, value)| value.as_str());
        let target = flag("--target-dir")
            .or(env)
            .or(inherited)
            .map_or_else(|| root.join("target"), |dir| cwd.join(dir));
        let doc = flag("--target").map_or_else(|| target.clone(), |triple| target.join(triple));
        Some(doc.join("doc").join("search.index"))
    }

    pub(super) fn line(&self) -> String {
        format!(
            "{} {}",
            self.program.as_deref().unwrap_or("cargo"),
            self.args.join(" ")
        )
    }

    /// Runs the step with its output shown, or captured when `quiet` (for a
    /// step running beside others) and shown only when it fails.
    pub(crate) fn run(&self, quiet: bool) -> bool {
        let started = Instant::now();
        let cwd = std::env::current_dir().unwrap_or_else(|_| workspace_root());
        let inherited = std::env::var(TARGET_DIR_VAR).ok();
        if let Some(index) = self.search_index(&workspace_root(), &cwd, inherited.as_deref()) {
            match std::fs::remove_dir_all(&index) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => say(&format!(
                    "could not remove {}: {error}; rustdoc merges into it",
                    index.display()
                )),
                _ => {}
            }
        }
        let mut command = Command::new(self.program.clone().unwrap_or_else(cargo));
        command
            .args(&self.args)
            .env("PATH", tool_path())
            .envs(self.env.iter().map(|(name, value)| (name, value)));
        let (ok, output) = if quiet {
            match command.output() {
                Ok(output) => (
                    output.status.success(),
                    [output.stdout, output.stderr].concat(),
                ),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        } else {
            say(&format!("> {}", self.line()));
            match command.status() {
                Ok(status) => (status.success(), Vec::new()),
                Err(error) => (false, error.to_string().into_bytes()),
            }
        };
        let seconds = started.elapsed().as_secs_f32();
        if ok {
            say(&format!("ok in {seconds:.1} s: {}", self.line()));
        } else {
            let _shown = std::io::stderr().write_all(&output);
            say(&format!("FAILED in {seconds:.1} s: {}", self.line()));
        }
        ok
    }
}
