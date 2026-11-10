//! Keeps the mods' command names and runs their commands on the server's
//! game thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick::abi::{ActionCall, CommandInfo, Header, Status, Str};

use super::{
    MAX_LINE, Named, Refusal, Reply, Typed, check, messages, parse, remote_caller, reply_by, typed,
    word,
};
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
    /// The word that named the mod, as [`word`] reads it.
    word: String,
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
    /// Commands dropped over a rate or a full queue, so the log names only
    /// the first and every hundredth.
    dropped: AtomicU32,
}

static NAMED: Registry<Named> = Registry::new();
static STATE: State = State {
    pending: Mutex::new(Vec::new()),
    waiting: AtomicBool::new(false),
    any: AtomicBool::new(false),
    rate: Mutex::new(Vec::new()),
    dropped: AtomicU32::new(0),
};

/// Names mod `owner`'s actions as commands during init: `/<name>`, the name
/// in the mod's id, and `/<alias>`. An alias another mod held first stays
/// that mod's, with one warning and a part off naming both mods, and this
/// mod's commands run by their name alone. A name another mod's commands
/// have turns off only this mod's commands, reported the same way.
pub(crate) fn register(loader: &Loader, owner: ModIndex, alias: &str) -> Status {
    let ids: Vec<&str> = loader.mods.iter().map(|m| m.id.as_str()).collect();
    let checked = NAMED.edit(|named| {
        check(named, &ids, owner, alias).map(|(granted, holder)| {
            let name = granted.name.clone();
            named.push(granted);
            (name, holder)
        })
    });
    let id = |index: ModIndex| ids.get(index.get()).copied().unwrap_or("");
    match checked {
        Err(sealed) => sealed.into(),
        Ok(Ok((name, holder))) => {
            STATE.any.store(true, Ordering::Release);
            if let Some(holder) = holder {
                let (mine, theirs) = (id(owner), id(holder));
                let feature = format!("alias /{alias} of {mine}");
                crate::host::report_off(
                    &feature,
                    (
                        format!(
                            "{feature} off: /{alias} is {theirs}'s; /{name} runs the commands of {mine}"
                        ),
                        format!("/{alias} is {theirs}'s; use /{name}"),
                    ),
                );
            }
            Status::OK
        }
        Ok(Err(Refusal::Malformed)) => Status::BAD_ARGS,
        Ok(Err(Refusal::Twice)) => Status::REFUSED,
        Ok(Err(Refusal::Taken(other))) => {
            let (mine, theirs) = (id(owner), id(other));
            let name = enshwick::commands::name_of(mine);
            let feature = format!("commands of {mine}");
            crate::host::report_off(
                &feature,
                (
                    format!("{feature} off: /{name} is taken by {theirs}"),
                    format!("/{name} taken by {theirs}"),
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

/// Whether mod `owner` runs.
fn running_mod(loader: &Loader, owner: ModIndex) -> bool {
    loader
        .mods
        .get(owner.get())
        .is_some_and(|m| !m.off.load(Ordering::Acquire))
}

/// The running mod whose command `text` names, and the rest of the line.
fn owner_of<'a>(loader: &Loader, text: &'a str) -> Option<(ModIndex, &'a str)> {
    let (owner, rest) = parse(NAMED.sealed()?, text)?;
    running_mod(loader, owner).then_some((owner, rest))
}

/// Takes the line `text` the player of account `key` typed when it is a
/// mod's command, a `/` and its name or alias first, and queues it for the
/// end of the step; false for any other line. A command of a mod that is
/// off, or over the rate or a full queue, is taken and dropped.
pub(crate) fn queue(loader: &Loader, key: u64, text: &str) -> bool {
    let Some(named) = NAMED.sealed() else {
        return false;
    };
    let (owner, rest) = match typed(named, text, |owner| running_mod(loader, owner)) {
        Typed::Chat => return false,
        Typed::Off => return true,
        Typed::Run(owner, rest) => (owner, rest),
    };
    let admitted = crate::chat::admit(&mut STATE.rate.relock(), key, std::time::Instant::now());
    let queued = admitted && {
        let mut pending = STATE.pending.relock();
        let room = pending.len() < MAX_PENDING;
        if room {
            pending.push(Pending {
                key,
                owner,
                word: word(text),
                rest: rest.to_owned(),
            });
            STATE.waiting.store(true, Ordering::Release);
        }
        room
    };
    if !queued {
        let dropped = STATE.dropped.fetch_add(1, Ordering::Relaxed);
        if dropped.is_multiple_of(100) {
            crate::log!(
                DEBUG,
                "{key:016x} sent a command over its rate or a full queue ({} dropped)",
                dropped + 1
            );
        }
    }
    true
}

/// Forgets the commands waiting and each account's rate, when a hosted game
/// ends, so none runs in the next one.
pub(crate) fn forget() {
    STATE.pending.relock().clear();
    STATE.waiting.store(false, Ordering::Release);
    STATE.rate.relock().clear();
}

/// Runs the chat commands queued during the step and tells each player the
/// reply as [`reply_by`] sends it; on the server's game thread, after the
/// step.
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
            (&command.word, &command.rest),
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
        match reply_by(text, reply.done, crate::chat::to_one_player()) {
            Reply::Chat(text) => {
                for message in messages(text, crate::chat::LINES_PER_REPLY) {
                    let _queued = crate::chat::send(command.key, &message);
                }
            }
            Reply::Notice(text) => crate::actions::tell(loader, command.key, &text),
            Reply::None => {}
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
    let word = word(line);
    crate::actions::command(loader, owner, ActionCall::REMOTE, &caller, 0, (&word, rest))
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
            alias: n.alias.as_deref().map_or(Str::EMPTY, Str::new),
            mod_id: Str::new(id),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_typed_in_one_hosted_game_never_runs_in_the_next() {
        STATE.pending.relock().push(Pending {
            key: 7,
            owner: ModIndex::new(0),
            word: "adm".to_owned(),
            rest: "tp spawn".to_owned(),
        });
        STATE.waiting.store(true, Ordering::Release);
        forget();
        assert!(STATE.pending.relock().is_empty());
        assert!(!STATE.waiting.load(Ordering::Acquire));
    }
}
