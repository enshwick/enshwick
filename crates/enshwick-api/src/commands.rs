//! Chat commands: a player types `/name args` in the chat, the server keeps
//! the line out of everyone's chat and hands it to the mod that registered
//! `name` with [`Load::command`]; the mod's reply goes to that player only.
//!
//! The loader answers `/enshwick` itself: the mods and versions running,
//! with `/enshwick help` every command with its line of help, and with
//! `/enshwick status` (administrators) a short status summary.
//!
//! [`run`] runs a command line as the server's console, for tools that
//! drive the server without a player, such as a command file.

use core::ffi::c_void;
use std::cell::RefCell;

use crate::abi::{CommandCall, HostApi, RenameCall, Status, Str, read_headed};
use crate::ecs::Entity;
use crate::world::Player;
use crate::{Load, Off};

/// The longest command name.
pub const MAX_NAME: usize = 32;

/// Whether `name` can name a command: 1 to 32 characters of `[a-z0-9-]`.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::is_valid_name;
///
/// assert!(is_valid_name("safe"));
/// assert!(!is_valid_name("Safe"));
/// assert!(!is_valid_name(""));
/// ```
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// A chat line's command name, lowercased, and the rest of the line,
/// trimmed; `None` when the line is not a command.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::parse_line;
///
/// assert_eq!(parse_line("/Safe allow  Ann Lee "), Some(("safe".to_owned(), "allow  Ann Lee")));
/// assert_eq!(parse_line("/enshwick"), Some(("enshwick".to_owned(), "")));
/// assert_eq!(parse_line("hello /safe"), None);
/// assert_eq!(parse_line("/ safe"), None);
/// ```
#[must_use]
pub fn parse_line(line: &str) -> Option<(String, &str)> {
    let rest = line.trim_start().strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (name, args) = rest.split_at(end);
    if name.is_empty() {
        return None;
    }
    Some((name.to_lowercase(), args.trim()))
}

/// The words of a command's arguments, split at whitespace.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::split_args;
///
/// assert_eq!(split_args(" allow  Ann Lee"), ["allow", "Ann", "Lee"]);
/// assert!(split_args("").is_empty());
/// ```
#[must_use]
pub fn split_args(args: &str) -> Vec<&str> {
    args.split_whitespace().collect()
}

/// What a command answers the player who typed it; [`Reply::none`] sends
/// nothing. Each line becomes a chat message of its own.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::Reply;
///
/// let reply = Reply::from("Ann may open your Safe");
/// assert_eq!(reply.text(), "Ann may open your Safe");
/// assert!(Reply::none().text().is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reply(String);

impl Reply {
    /// No reply.
    ///
    /// # Examples
    ///
    /// See [`Reply`].
    #[must_use]
    pub const fn none() -> Self {
        Self(String::new())
    }

    /// The reply's text.
    ///
    /// # Examples
    ///
    /// See [`Reply`].
    #[must_use]
    pub fn text(&self) -> &str {
        &self.0
    }
}

impl From<String> for Reply {
    fn from(text: String) -> Self {
        Self(text)
    }
}

impl From<&str> for Reply {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

/// Who typed a command; a handler gets it with the command's words.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::{CommandContext, Reply};
///
/// fn whoami(ctx: &CommandContext, _args: &[&str]) -> Reply {
///     let role = if ctx.is_admin() { "an administrator" } else { "a player" };
///     format!("{} is {role}", ctx.player.name).into()
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandContext {
    /// The player who typed the command.
    pub player: Player,
    /// The command's name, as registered.
    pub name: String,
}

impl CommandContext {
    /// Whether the player is one of the server's administrators.
    ///
    /// # Examples
    ///
    /// See [`CommandContext`].
    #[must_use]
    pub const fn is_admin(&self) -> bool {
        self.player.is_admin()
    }
}

/// A player's rename of an entity, for [`Load::on_rename`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    /// The entity being renamed.
    pub entity: Entity,
    /// The renaming player, when the server could tell.
    pub player: Option<Player>,
    /// The new name the client sent.
    pub text: String,
}

/// Why [`run`] ran no command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunError {
    /// The line does not start with `/name`.
    NotACommand,
    /// No mod and not the loader answers that name.
    Unknown,
    /// Not on the server's game thread, or no server runs.
    NotOnGameThread,
    /// The loader is older than the API or not there (a unit test).
    NoLoader,
    /// The loader refused for another reason, with its status.
    Refused(u32),
}

impl core::fmt::Display for RunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotACommand => f.write_str("not a command: commands start with /"),
            Self::Unknown => f.write_str("no mod has that command"),
            Self::NotOnGameThread => f.write_str("commands run on the server's game thread only"),
            Self::NoLoader => f.write_str("commands need a newer loader"),
            Self::Refused(status) => write!(f, "refused by the loader (status {status})"),
        }
    }
}

impl std::error::Error for RunError {}

/// Runs the chat command line `line`, such as `/enshwick status`, as the
/// server's console: an administrator with no character (entity 0, account
/// key 0). Returns the reply the command gave.
///
/// Call it on the server's game thread, from a scheduled task or a game
/// event, where command handlers expect to run.
///
/// # Errors
///
/// [`RunError`] when the line is no command, no one has its name, the call
/// is not on the game thread or the loader cannot run commands.
///
/// # Examples
///
/// ```
/// use enshwick_api::commands::{RunError, run};
///
/// // Outside the game no loader answers.
/// assert_eq!(run("hello"), Err(RunError::NotACommand));
/// assert_eq!(run("/enshwick status"), Err(RunError::NoLoader));
/// ```
pub fn run(line: &str) -> Result<Reply, RunError> {
    if parse_line(line).is_none() {
        return Err(RunError::NotACommand);
    }
    let host = crate::host().ok_or(RunError::NoLoader)?;
    let end = core::mem::offset_of!(HostApi, run_command) + size_of::<usize>();
    if !host.api().header.covers(end) {
        return Err(RunError::NoLoader);
    }
    let mut out = Str::EMPTY;
    let status = (host.api().run_command)(host.handle(), Str::new(line), &raw mut out);
    match status {
        Status::OK => {
            // SAFETY: the reply stays valid until the next call on this thread.
            Ok(Reply::from(unsafe { out.as_str() }.unwrap_or("")))
        }
        Status::MISSING => Err(RunError::Unknown),
        Status::WRONG_THREAD => Err(RunError::NotOnGameThread),
        other => Err(RunError::Refused(other.0)),
    }
}

thread_local! {
    /// The last reply, alive until the next command on this thread.
    static REPLY: RefCell<String> = const { RefCell::new(String::new()) };
}

/// A borrowed string as text, empty when it is not UTF-8.
fn text_of<'a>(value: Str) -> &'a str {
    // SAFETY: strings the loader passes are valid for the call this text
    // lives in.
    unsafe { value.as_str() }.unwrap_or("")
}

unsafe extern "system" fn run_command<F>(call: *const CommandCall, user: *mut c_void, out: *mut Str)
where
    F: Fn(&CommandContext, &[&str]) -> Reply,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // commands.
    let Some(call) = (unsafe { read_headed(call, size_of::<CommandCall>()) }) else {
        return;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let ctx = CommandContext {
        player: Player::from_info(&call.player),
        name: text_of(call.name).to_owned(),
    };
    let args = split_args(text_of(call.args));
    let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&ctx, &args)));
    let reply = reply.unwrap_or_else(|_| {
        if let Some(host) = crate::host() {
            host.fault("panic in a command handler");
        }
        Reply::none()
    });
    if out.is_null() {
        return;
    }
    REPLY.with_borrow_mut(|kept| {
        *kept = reply.0;
        // SAFETY: the loader's out pointer; the text lives in this thread's
        // buffer until the next command, after the loader copied it.
        unsafe { out.write(Str::new(kept)) };
    });
}

unsafe extern "system" fn run_filter<F>(call: *const RenameCall, user: *mut c_void) -> u8
where
    F: Fn(&Rename) -> bool,
{
    // SAFETY: the loader passes a valid call for the duration of this one; an
    // all-zero call is valid. Every field is there since the minor that added
    // rename filters.
    let Some(call) = (unsafe { read_headed(call, size_of::<RenameCall>()) }) else {
        return 1;
    };
    // SAFETY: `user` is the leaked `F` this function was registered with.
    let handler = unsafe { &*user.cast::<F>() };
    let rename = Rename {
        entity: Entity(call.entity),
        player: (call.known != 0).then(|| Player::from_info(&call.player)),
        text: text_of(call.text).to_owned(),
    };
    if let Ok(allowed) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handler(&rename)))
    {
        u8::from(allowed)
    } else {
        if let Some(host) = crate::host() {
            host.fault("panic in a rename filter");
        }
        1
    }
}

impl Load<'_> {
    /// Registers the chat command `/name` with a line of help for
    /// `/enshwick help`. The server runs `handler` on its game thread, where
    /// world reads work, with the player who typed it and the words after
    /// the name; the reply goes to that player alone. A client runs it while
    /// it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] when the name is malformed or another mod holds it, or with a
    /// loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_api::commands::Reply;
    ///
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     load.command("hello", "greets you", |ctx, args| match args {
    ///         [] => Reply::from(format!("Hello, {}", ctx.player.name)),
    ///         [name, ..] => Reply::from(format!("Hello, {name}")),
    ///     })
    /// }
    /// ```
    pub fn command<F>(&mut self, name: &str, help: &str, handler: F) -> Result<(), Off>
    where
        F: Fn(&CommandContext, &[&str]) -> Reply + Send + Sync + 'static,
    {
        if !is_valid_name(name) {
            return Err(Off::new(format!(
                "command /{name}: names are 1 to 32 of [a-z0-9-]"
            )));
        }
        let host = self.host();
        let end = core::mem::offset_of!(HostApi, command) + size_of::<usize>();
        if !host.api().header.covers(end) {
            return Err(Off::new("chat commands need a newer loader"));
        }
        let user = Box::into_raw(Box::new(handler));
        let status = (host.api().command)(
            host.handle(),
            Str::new(name),
            Str::new(help),
            run_command::<F>,
            user.cast(),
        );
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
            return Err(Off::new(format!(
                "command /{name}: refused by the loader (status {}; taken by another mod?)",
                status.0
            )));
        }
        Ok(())
    }

    /// Runs `filter` before the server applies a player's rename of any
    /// entity, on its game thread; returning `false` refuses the rename. The
    /// game itself checks no permission for renames. A client runs it while
    /// it hosts a game.
    ///
    /// # Errors
    ///
    /// [`Off`] with a loader older than the API.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_api::Load<'_>) -> Result<(), enshwick_api::Off> {
    ///     // Only administrators rename anything.
    ///     load.on_rename(|rename| rename.player.as_ref().is_some_and(|p| p.is_admin()))
    /// }
    /// ```
    pub fn on_rename<F>(&mut self, filter: F) -> Result<(), Off>
    where
        F: Fn(&Rename) -> bool + Send + Sync + 'static,
    {
        let host = self.host();
        let end = core::mem::offset_of!(HostApi, rename_filter) + size_of::<usize>();
        if !host.api().header.covers(end) {
            return Err(Off::new("rename filters need a newer loader"));
        }
        let user = Box::into_raw(Box::new(filter));
        let status = (host.api().rename_filter)(host.handle(), run_filter::<F>, user.cast());
        if !status.is_ok() {
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
            return Err(Off::new(format!(
                "rename filter: refused by the loader (status {})",
                status.0
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{Header, PlayerInfo};

    /// A [`PlayerInfo`] for tests and examples.
    fn info(name: &str, permissions: u8) -> PlayerInfo {
        let mut bytes = [0u8; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 1,
            entity: 7,
            key: 42,
            permissions,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
    }

    fn command<F: Fn(&CommandContext, &[&str]) -> Reply>(
        handler: &F,
        call: &CommandCall,
    ) -> String {
        let user = core::ptr::from_ref(handler).cast_mut().cast::<c_void>();
        let mut out = Str::EMPTY;
        // SAFETY: a valid call, the handler as user, a writable reply.
        unsafe { run_command::<F>(call, user, &raw mut out) };
        // SAFETY: valid until the next command on this thread.
        unsafe { out.as_str() }.unwrap().to_owned()
    }

    fn filter_call<F: Fn(&Rename) -> bool>(filter: &F, call: &RenameCall) -> u8 {
        let user = core::ptr::from_ref(filter).cast_mut().cast::<c_void>();
        // SAFETY: a valid call and the filter as user.
        unsafe { run_filter::<F>(call, user) }
    }

    #[test]
    fn lines_split_into_a_name_and_words() {
        assert_eq!(
            parse_line("  /SAFE list"),
            Some(("safe".to_owned(), "list"))
        );
        assert_eq!(parse_line("/"), None);
        assert_eq!(parse_line("safe"), None);
        assert_eq!(split_args("deny\tAnn"), ["deny", "Ann"]);
        assert!(is_valid_name(&"a".repeat(MAX_NAME)));
        assert!(!is_valid_name(&"a".repeat(MAX_NAME + 1)));
        assert!(!is_valid_name("sa fe"));
    }

    #[test]
    fn a_handler_gets_the_player_and_words_and_its_reply_comes_back() {
        let handler = |ctx: &CommandContext, args: &[&str]| -> Reply {
            format!("{} {} {}", ctx.player.name, ctx.is_admin(), args.join("+")).into()
        };
        let call = CommandCall {
            header: Header::of::<CommandCall>(),
            player: info("Ann", PlayerInfo::CAN_KICK_BAN),
            name: Str::new("safe"),
            args: Str::new("allow  Bob"),
        };
        assert_eq!(command(&handler, &call), "Ann true allow+Bob");
    }

    #[test]
    fn a_filter_sees_the_rename_and_may_refuse_it() {
        let filter = |rename: &Rename| rename.player.is_some() && rename.text != "no";
        let mut call = RenameCall {
            header: Header::of::<RenameCall>(),
            entity: 5,
            known: 1,
            player: info("Ann", 0),
            text: Str::new("Tools"),
        };
        assert_eq!(filter_call(&filter, &call), 1);
        call.text = Str::new("no");
        assert_eq!(filter_call(&filter, &call), 0);
        call.known = 0;
        call.text = Str::new("Tools");
        assert_eq!(filter_call(&filter, &call), 0);
    }
}
