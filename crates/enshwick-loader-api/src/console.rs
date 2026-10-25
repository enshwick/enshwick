//! The server's console: lines a tool runs on the server as an
//! administrator with no character, such as each line of a dedicated
//! server's command file.
//!
//! A line is `enshwick ...` for the loader's own tools (`enshwick help`
//! lists them), or a mod's id followed by the text form of one of its
//! [actions](crate::actions), such as `safes access Ann on`. Players never
//! type these: they use the game's interface.

use crate::abi::{Status, Str};

/// Why [`run`] ran nothing.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::console::{RunError, run};
///
/// assert_eq!(run("   "), Err(RunError::Empty));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RunError {
    /// The line holds no word.
    #[error("the line is empty")]
    Empty,
    /// Neither the loader nor a running mod with actions answers the line's
    /// first word.
    #[error("no mod with actions has that id")]
    Unknown,
    /// Not on the server's game thread, or no server runs.
    #[error("console lines run on the server's game thread only")]
    NotOnGameThread,
    /// No loader runs, as in a unit test.
    #[error("no loader runs console lines here")]
    NoLoader,
    /// The loader refused for another reason, with its status.
    #[error("refused by the loader (status {0})")]
    Refused(u32),
}

/// Runs the console line `line`, such as `enshwick status` or
/// `admin-tools world`, as the server's console, and returns its answer.
///
/// Call it on the server's game thread, from a scheduled task or a game
/// event, where action handlers expect to run.
///
/// # Errors
///
/// [`RunError`] when the line is empty, nothing answers its first word, the
/// call is not on the game thread or no loader runs.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::console::{RunError, run};
///
/// // Outside the game no loader answers.
/// assert_eq!(run("enshwick status"), Err(RunError::NoLoader));
/// ```
pub fn run(line: &str) -> Result<String, RunError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(RunError::Empty);
    }
    let host = crate::host().ok_or(RunError::NoLoader)?;
    let mut out = Str::EMPTY;
    let status = (host.api().run_console)(host.handle(), Str::new(line), &raw mut out);
    match status {
        // SAFETY: the reply stays valid until the next call on this thread.
        Status::OK => Ok(unsafe { out.as_str() }.unwrap_or("").to_owned()),
        Status::MISSING => Err(RunError::Unknown),
        Status::WRONG_THREAD => Err(RunError::NotOnGameThread),
        other => Err(RunError::Refused(other.0)),
    }
}
