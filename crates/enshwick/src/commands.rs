//! Commands: a second way to run a mod's [actions](crate::actions), typed
//! in chat, in the server's console or in a remote console.
//!
//! Unstable: needs the `unstable` feature and may change in any release.
//!
//! A mod names its actions as commands with
//! [`Actions::commands`](crate::actions::Actions::commands): a full name and
//! a shorter alias, so `/admin-tools online` and `/adl online` both run its
//! action `online`, and `/adl help` lists the actions the caller may run.
//! The words after the action's id are its values, as on the console. The
//! server checks the caller's rights as for any run, and the reply goes to
//! the caller alone. A command is never the only way to an action: players
//! reach every action through the game's interface too.
//!
//! [`run`] runs any mod's command line for a [`Caller`] the calling mod
//! names, such as a remote console's client, and [`list`] names the mods'
//! commands.
//!
//! # Examples
//!
//! ```
//! use enshwick::commands::{Caller, run};
//! use enshwick::world::PlayerPermission;
//!
//! let operator = Caller::new("rcon 10.0.0.2").with(PlayerPermission::CanKickBan);
//! // Outside the game no loader answers.
//! assert!(run("/adl online", &operator).is_err());
//! ```

use crate::abi::{ActionReply, CommandInfo, Header, Status, Str};
use crate::actions::Outcome;
use crate::schedule::Scheduler;
use crate::world::PlayerPermission;

/// The longest command line [`run`] takes, in bytes.
pub const MAX_LINE: usize = 1024;

/// The longest command name or alias, in bytes.
pub const MAX_NAME: usize = 32;

/// Words no mod's commands may take: the loader's own and the help word.
const RESERVED: [&str; 2] = ["enshwick", "help"];

/// Whether `name` and `alias` can name a mod's commands: each 1 to
/// [`MAX_NAME`] of `[a-z0-9-]`, the alias shorter, neither reserved.
pub(crate) fn names_fit(name: &str, alias: &str) -> bool {
    let word = |w: &str| {
        !w.is_empty()
            && w.len() <= MAX_NAME
            && !RESERVED.contains(&w)
            && w.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    word(name) && word(alias) && alias.len() < name.len()
}

/// Who runs a command line through [`run`]: a remote caller with a label
/// for the log and the handler, and the game's permissions it holds; never
/// a player, and with no character.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Caller {
    label: String,
    permissions: u8,
}

impl Caller {
    /// A caller named `label`, such as `rcon 10.0.0.2`, with no permission;
    /// the handler reads the label, cut to 64 bytes, as the player's name.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            permissions: 0,
        }
    }

    /// Gives the caller `permission`, as a user group grants it to a player.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn with(mut self, permission: PlayerPermission) -> Self {
        self.permissions |= permission.bit();
        self
    }

    /// Gives the caller every permission the game has, as the server's
    /// console holds them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::commands::Caller;
    /// use enshwick::world::PlayerPermission;
    ///
    /// let owner = Caller::new("owner").with_all();
    /// assert_eq!(owner, PlayerPermission::ALL.into_iter().fold(Caller::new("owner"), Caller::with));
    /// ```
    #[must_use]
    pub fn with_all(self) -> Self {
        PlayerPermission::ALL.into_iter().fold(self, Self::with)
    }
}

/// Why [`run`] ran nothing.
///
/// # Examples
///
/// ```
/// use enshwick::commands::{Caller, RunCommandError, run};
///
/// assert_eq!(run("  ", &Caller::new("tool")), Err(RunCommandError::Empty));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RunCommandError {
    /// The line holds no word.
    #[error("the line is empty")]
    Empty,
    /// The line is longer than [`MAX_LINE`].
    #[error("the line is longer than {MAX_LINE} bytes")]
    TooLong,
    /// No running mod has the line's first word as its command name or
    /// alias.
    #[error("no running mod's commands have that name")]
    Unknown,
    /// Not on the server's game thread, or no server runs.
    #[error("commands run on the server's game thread only")]
    NotOnGameThread,
    /// No loader runs, as in a unit test.
    #[error("no loader runs commands here")]
    NoLoader,
    /// The loader refused for another reason, with its status.
    #[error("refused by the loader (status {0})")]
    Refused(u32),
}

/// Runs the command line `line`, such as `/adl online`, for `caller`, and
/// returns the mod's reply.
///
/// The `/` is optional. The reply says whether the action was done, and
/// holds the text the caller reads. The rights checks and the refusals are
/// those of a player's run, with `caller`'s permissions.
///
/// Call it on the server's game thread, as from a scheduled task; from
/// another thread, use [`run_later`].
///
/// # Errors
///
/// [`RunCommandError`] for an empty or long line, a first word no running
/// mod's commands have, a call off the game thread or no loader.
///
/// # Examples
///
/// See the [module](self).
pub fn run(line: &str, caller: &Caller) -> Result<Outcome, RunCommandError> {
    if line.trim().is_empty() {
        return Err(RunCommandError::Empty);
    }
    if line.len() > MAX_LINE {
        return Err(RunCommandError::TooLong);
    }
    let host = crate::host().ok_or(RunCommandError::NoLoader)?;
    let mut out = ActionReply {
        header: Header::of::<ActionReply>(),
        ..bytemuck::Zeroable::zeroed()
    };
    let status = (host.api().run_command)(
        host.handle(),
        Str::new(line),
        Str::new(&caller.label),
        u32::from(caller.permissions),
        &raw mut out,
    );
    match status {
        Status::OK => {
            // SAFETY: the reply's text stays valid until the next call on this
            // thread, and it is copied at once.
            let text = unsafe { out.text.as_str() }.unwrap_or("");
            Ok(if out.done == 1 {
                Outcome::done(text)
            } else {
                Outcome::refused(text)
            })
        }
        Status::MISSING => Err(RunCommandError::Unknown),
        Status::WRONG_THREAD => Err(RunCommandError::NotOnGameThread),
        other => Err(RunCommandError::Refused(other.0)),
    }
}

/// Runs the command line `line` for `caller` as [`run`] does, from any
/// thread: `scheduler` takes it to the server's game thread on its next
/// pass, and `reply` gets the result there, never blocking the caller.
///
/// # Examples
///
/// ```
/// use std::sync::mpsc;
///
/// use enshwick::commands::{Caller, run_later};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let scheduler = load.scheduler()?;
///     let (tx, rx) = mpsc::channel();
///     std::thread::spawn(move || {
///         run_later(scheduler, "/adl online".into(), Caller::new("rcon").with_all(), move |reply| {
///             let _sent = tx.send(reply.map(|outcome| outcome.text().to_owned()));
///         });
///     });
///     # drop(rx);
///     Ok(())
/// }
/// ```
pub fn run_later(
    scheduler: Scheduler,
    line: String,
    caller: Caller,
    reply: impl FnOnce(Result<Outcome, RunCommandError>) + Send + 'static,
) {
    scheduler.next_tick(move || reply(run(&line, &caller)));
}

/// A running mod's command names, from [`list`].
///
/// # Examples
///
/// See [`list`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct CommandSet {
    /// The full name, `admin-tools` in `/admin-tools online`.
    pub name: String,
    /// The shorter alias, `adl` in `/adl online`.
    pub alias: String,
    /// The id of the mod that named them.
    pub mod_id: String,
}

/// The command names of the running mods, in load order, so a remote
/// console can answer `help` across mods: each mod's `/<alias> help` then
/// lists what a caller may run.
///
/// # Examples
///
/// ```
/// // Outside the game no loader answers.
/// assert!(enshwick::commands::list().is_empty());
/// ```
#[must_use]
pub fn list() -> Vec<CommandSet> {
    let Some(host) = crate::host() else {
        return Vec::new();
    };
    let api = host.api();
    let entry_size = u32::try_from(size_of::<CommandInfo>()).unwrap_or(0);
    let mut count = 0u32;
    let null = core::ptr::null_mut();
    if !(api.command_list)(host.handle(), null, entry_size, 0, &raw mut count).is_ok() {
        return Vec::new();
    }
    let blank = CommandInfo {
        header: Header::of::<CommandInfo>(),
        ..bytemuck::Zeroable::zeroed()
    };
    let mut infos = vec![blank; usize::try_from(count).unwrap_or(0)];
    let capacity = u32::try_from(infos.len()).unwrap_or(0);
    let status = (api.command_list)(
        host.handle(),
        infos.as_mut_ptr(),
        entry_size,
        capacity,
        &raw mut count,
    );
    if !status.is_ok() {
        return Vec::new();
    }
    infos.truncate(usize::try_from(count).unwrap_or(0));
    // SAFETY: the strings stay valid until the next call on this thread, and
    // each is copied at once.
    let text = |s: Str| unsafe { s.as_str() }.unwrap_or("").to_owned();
    infos
        .into_iter()
        .map(|info| CommandSet {
            name: text(info.name),
            alias: text(info.alias),
            mod_id: text(info.mod_id),
        })
        .collect()
}

/// The free functions of [`commands`](self) as a script's namespace
/// `commands`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct CommandFns;

#[crate::script(command_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl CommandFns {
    /// Runs the command line `line` for a caller named `label` holding
    /// `permissions`, as [`run`] does with a [`Caller`]; a script runs on
    /// the game thread, so it has no `run_later`.
    ///
    /// # Errors
    ///
    /// As [`run`].
    #[script]
    pub(crate) fn run(
        self,
        line: &str,
        label: &str,
        permissions: Vec<PlayerPermission>,
    ) -> Result<crate::actions::ScriptOutcome, RunCommandError> {
        let caller = permissions
            .into_iter()
            .fold(Caller::new(label), Caller::with);
        run(line, &caller).map(|outcome| crate::actions::ScriptOutcome {
            done: outcome.is_done(),
            text: outcome.text().to_owned(),
        })
    }

    /// The command names of the running mods, as [`list`] gives them.
    #[script]
    #[must_use]
    pub fn list(self) -> Vec<CommandSet> {
        list()
    }
}

impl crate::script::Bound for CommandFns {
    const NAME: &'static str = "commands";
    const DOC: &'static str =
        "Commands: a second way to run mods' actions, from chat, the console or a remote console.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_COMMAND_FNS];

    fn resolve(_load: &mut crate::Load<'_>) -> Result<Self, crate::Off> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_names_are_plain_words_with_a_shorter_alias() {
        assert!(names_fit("admin-tools", "adl"));
        for (name, alias) in [
            ("Admin", "a"),
            ("admin tools", "a"),
            ("adl", "admin-tools"),
            ("adl", "adl"),
            ("admin-tools", ""),
            ("enshwick", "e"),
            ("helper", "help"),
        ] {
            assert!(!names_fit(name, alias), "{name} {alias}");
        }
        assert!(!names_fit(&"x".repeat(MAX_NAME + 1), "x"));
    }
}
