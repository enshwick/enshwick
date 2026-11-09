//! Keeps the mods' command names and runs their commands on the server's
//! game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{ActionCall, CommandInfo, Header, Status, Str};

use super::{MAX_LINE, Named, Refusal, check, parse, remote_caller};
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::registry::Registry;

/// Chat commands waiting for the end of the step at most; more are dropped.
const MAX_PENDING: usize = 64;

/// A command a player typed, waiting for the end of the step.
#[derive(Debug)]
struct Pending {
    key: u64,
    owner: ModIndex,
    rest: String,
}

/// What the commands keep while the game runs.
struct State {
    pending: Mutex<Vec<Pending>>,
    /// Set while `pending` holds a command, so a step with none takes no
    /// lock.
    waiting: AtomicBool,
    /// Set once a mod named commands, so a chat line takes one load while
    /// none did.
    any: AtomicBool,
    /// Each account's rate of commands, the chat's own: at most eight a
    /// second, so only a flood loses one.
    rate: Mutex<Vec<crate::chat::Rate>>,
}

static NAMED: Registry<Named> = Registry::new();
static STATE: State = State {
    pending: Mutex::new(Vec::new()),
    waiting: AtomicBool::new(false),
    any: AtomicBool::new(false),
    rate: Mutex::new(Vec::new()),
};

/// Names mod `owner`'s actions as the commands `/<name>` and `/<alias>`,
/// during init. A word another mod took first turns off only this mod's
/// commands, with a warning and a part off naming both mods.
pub(crate) fn register(loader: &Loader, owner: ModIndex, name: &str, alias: &str) -> Status {
    let ids: Vec<&str> = loader.mods.iter().map(|m| m.id.as_str()).collect();
    let checked = NAMED.edit(|named| {
        check(named, &ids, owner, name, alias).map(|()| {
            named.push(Named {
                owner,
                name: name.to_owned(),
                alias: alias.to_owned(),
            });
        })
    });
    match checked {
        Err(sealed) => sealed.into(),
        Ok(Ok(())) => {
            STATE.any.store(true, Ordering::Release);
            Status::OK
        }
        Ok(Err(Refusal::Malformed)) => Status::BAD_ARGS,
        Ok(Err(Refusal::Twice)) => Status::REFUSED,
        Ok(Err(Refusal::Taken(other))) => {
            let id = |index: ModIndex| {
                loader
                    .mods
                    .get(index.get())
                    .map_or_else(String::new, |m| m.id.clone())
            };
            let (mine, theirs) = (id(owner), id(other));
            let feature = format!("commands of {mine}");
            crate::host::report_off(
                &feature,
                (
                    format!("{feature} off: /{name} or /{alias} is taken by {theirs}"),
                    format!("/{name} /{alias} taken by {theirs}"),
                ),
            );
            Status::REFUSED
        }
    }
}

/// Seals the command names once every mod's init ran.
pub(crate) fn seal() {
    let _named = NAMED.seal();
}

/// Whether a mod named commands, so a chat line may hold one; one load.
#[inline]
pub(crate) fn any() -> bool {
    STATE.any.load(Ordering::Acquire)
}

/// The mod whose command name or alias is `word`, a leading `/` allowed.
pub(crate) fn find(word: &str) -> Option<ModIndex> {
    parse(NAMED.sealed()?, word).map(|(owner, _)| owner)
}

/// The running mod whose command `text` names, and the rest of the line.
fn owner_of<'a>(loader: &Loader, text: &'a str) -> Option<(ModIndex, &'a str)> {
    let (owner, rest) = parse(NAMED.sealed()?, text)?;
    let running = loader
        .mods
        .get(owner.get())
        .is_some_and(|m| !m.off.load(Ordering::Acquire));
    running.then_some((owner, rest))
}

/// Takes the line `text` the player of account `key` typed when it is a
/// running mod's command, a `/` and its name or alias first, and queues it
/// for the end of the step; false for any other line. A command over a full
/// queue is taken and dropped.
pub(crate) fn queue(loader: &Loader, key: u64, text: &str) -> bool {
    if !text.starts_with('/') {
        return false;
    }
    let Some((owner, rest)) = owner_of(loader, text) else {
        return false;
    };
    let admitted = crate::chat::admit(&mut STATE.rate.relock(), key, std::time::Instant::now());
    let mut pending = STATE.pending.relock();
    if !admitted || pending.len() >= MAX_PENDING {
        crate::log!(
            DEBUG,
            "{key:016x} sent a command over its rate or a full queue"
        );
        return true;
    }
    pending.push(Pending {
        key,
        owner,
        rest: rest.to_owned(),
    });
    STATE.waiting.store(true, Ordering::Release);
    true
}

/// Runs the chat commands queued during the step and tells each player the
/// reply: one line as a notice, several in chat to that player alone; on
/// the server's game thread, after the step.
pub(crate) fn after_step(loader: &Loader) {
    if !STATE.waiting.load(Ordering::Acquire) {
        return;
    }
    let pending = {
        let mut pending = STATE.pending.relock();
        STATE.waiting.store(false, Ordering::Release);
        core::mem::take(&mut *pending)
    };
    for command in pending {
        let Some((player, language)) = crate::actions::commander(loader, command.key) else {
            continue;
        };
        let Some(reply) = crate::actions::command(
            loader,
            command.owner,
            ActionCall::CHAT,
            &player,
            language,
            &command.rest,
        ) else {
            continue;
        };
        let text = reply.text.as_str();
        crate::log!(
            DEBUG,
            "{:016x} ran a command of mod {}: {}",
            command.key,
            command.owner.get(),
            if reply.done { "done" } else { "refused" }
        );
        if text.lines().nth(1).is_some() {
            let _queued = crate::chat::send(command.key, text);
        } else if !text.is_empty() {
            crate::actions::tell(loader, command.key, text);
        }
    }
}

/// Runs the command line `line` for the remote caller `label` with
/// `permissions`, on the server's game thread; the mod's answer.
///
/// # Errors
///
/// [`Status::WRONG_THREAD`] off the server's game thread,
/// [`Status::BAD_ARGS`] for an empty line or one over [`MAX_LINE`] bytes,
/// [`Status::MISSING`] when no running mod's commands have its first word.
pub(crate) fn run_remote(
    loader: &Loader,
    line: &str,
    label: &str,
    permissions: u32,
) -> Result<crate::actions::Answer, Status> {
    if line.len() > MAX_LINE || line.trim().is_empty() {
        return Err(Status::BAD_ARGS);
    }
    crate::world::server().map_err(|_| Status::WRONG_THREAD)?;
    let (owner, rest) = owner_of(loader, line).ok_or(Status::MISSING)?;
    let caller = remote_caller(label, permissions);
    crate::log!(DEBUG, "{label} runs the command {line}");
    crate::actions::command(loader, owner, ActionCall::REMOTE, &caller, 0, rest)
        .ok_or(Status::MISSING)
}

/// The running mods' command sets with their mod ids, in load order.
pub(crate) fn running(loader: &Loader) -> Vec<(&'static Named, &str)> {
    let mut named: Vec<&'static Named> = NAMED.sealed().unwrap_or_default().iter().collect();
    named.sort_by_key(|n| n.owner.get());
    named
        .into_iter()
        .filter_map(|n| {
            let record = loader.mods.get(n.owner.get())?;
            (!record.off.load(Ordering::Acquire)).then_some((n, record.id.as_str()))
        })
        .collect()
}

/// The running mods' command sets, in load order, as the ABI lists them;
/// their strings live in the sealed names and the loader's records, so for
/// the process.
pub(crate) fn list(loader: &'static Loader) -> Vec<CommandInfo> {
    running(loader)
        .into_iter()
        .map(|(n, id)| CommandInfo {
            header: Header::of::<CommandInfo>(),
            name: Str::new(&n.name),
            alias: Str::new(&n.alias),
            mod_id: Str::new(id),
        })
        .collect()
}
