//! Keeps the mods' command names and runs their commands on the server's
//! game thread.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick_core::abi::{CommandInfo, Header, Status, Str};

use enshwick_core::command_names::{alias_asked, check};

use super::{
    MAX_LINE, NameCommandsError, Named, Refusal, Typed, parse, remote_caller, typed, word,
};
use crate::actions::admission::{self, Path, Verdict};
use crate::engine::fuse::{Fuse, Off};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::pending::Pending;
use crate::engine::registry::{Registry, Sealed};

/// Chat commands waiting for the end of the step at most; more are dropped.
const MAX_PENDING: usize = 64;

/// A command a player typed, waiting for the end of the step.
#[derive(Debug)]
struct Queued {
    key: u64,
    owner: ModIndex,
    /// The word that named the mod, as [`word`] reads it.
    word: String,
    rest: String,
}

/// What the commands keep while the game runs.
struct State {
    pending: Pending<Queued>,
    /// The commands the admission refused while the game posted the chat,
    /// answered at the end of the step, outside the game's chat post.
    refused: Pending<(Queued, admission::Refusal)>,
    /// Set once a mod named commands, so a chat line takes one load while
    /// none did.
    any: AtomicBool,
    /// Commands refused over a rate or a full queue, so the log names only
    /// the first and every hundredth.
    dropped: AtomicU32,
}

static NAMED: Registry<Named> = Registry::new();
static STATE: State = State {
    pending: Pending::new(),
    refused: Pending::new(),
    any: AtomicBool::new(false),
    dropped: AtomicU32::new(0),
};

/// Names mod `owner`'s actions as commands during init: `/<name>`, the name
/// in the mod's id, and `/<alias>`. An alias another mod held first stays
/// that mod's, with one warning and a part off naming both mods, and this
/// mod's commands run by their name alone. A name another mod's commands
/// have turns off only this mod's commands, reported the same way. A mod
/// built before names came from ids whose alias no longer fits runs its
/// commands by their name, reported the same way too.
///
/// # Errors
///
/// [`NameCommandsError`] after init, for a malformed alias, a second ask,
/// or a name another mod's commands have.
pub(crate) fn register(
    loader: &Loader,
    owner: ModIndex,
    alias: &str,
) -> Result<(), NameCommandsError> {
    let ids: Vec<(ModIndex, &str)> = loader
        .mods
        .iter()
        .enumerate()
        .map(|(index, m)| (ModIndex::new(index), m.id.as_str()))
        .collect();
    let id = |index: ModIndex| ids.get(index.get()).map_or("", |(_, id)| *id);
    let asked = loader.mods.get(owner.get()).map_or(Some(alias), |m| {
        alias_asked(alias, &m.id, m.manifest.header.abi_minor)
    });
    let checked = NAMED.edit(|named| {
        check(named, &ids, owner, asked).map(|(granted, holder)| {
            let name = granted.name.clone();
            named.push(granted);
            (name, holder)
        })
    });
    match checked {
        Err(Sealed) => Err(NameCommandsError::TooLate),
        Ok(Ok((name, holder))) => {
            STATE.any.store(true, Ordering::Release);
            if asked.is_none() {
                let mine = id(owner);
                let feature = format!("alias /{alias} of {mine}");
                let line = format!(
                    "{feature} off: it is not shorter than /{name}, the name in the mod's id; /{name} runs its commands"
                );
                let entry = format!("/{alias} is not shorter than /{name}; use /{name}");
                part_off(&feature, line, entry);
            }
            if let Some(holder) = holder {
                let (mine, theirs) = (id(owner), id(holder));
                let feature = format!("alias /{alias} of {mine}");
                let line = format!(
                    "{feature} off: /{alias} is {theirs}'s; /{name} runs the commands of {mine}"
                );
                part_off(
                    &feature,
                    line,
                    format!("/{alias} is {theirs}'s; use /{name}"),
                );
            }
            Ok(())
        }
        Ok(Err(Refusal::Malformed)) => Err(NameCommandsError::Malformed),
        Ok(Err(Refusal::Twice)) => Err(NameCommandsError::Twice),
        Ok(Err(Refusal::Taken(other))) => {
            let (mine, theirs) = (id(owner), id(other));
            let name = enshwick_core::commands::name_of(mine);
            let feature = format!("commands of {mine}");
            let line = format!("{feature} off: /{name} is taken by {theirs}");
            part_off(&feature, line, format!("/{name} taken by {theirs}"));
            Err(NameCommandsError::Taken)
        }
    }
}

/// Turns `feature`, a part of one mod's commands, off for the log `line`
/// and the status `entry`, through [`Fuse::turn_off`] with no error code:
/// two mods asked for one name, neither the build's fault nor a panic.
/// The part is the one fuse its name keeps, so a mod asking again reports
/// nothing more.
#[cold]
fn part_off(feature: &str, line: String, entry: String) {
    Fuse::named(feature).turn_off(&Off::reported(line, entry));
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
/// end of the step past the [admission](admission) of chat; false for any
/// other line. A command of a mod that is off is taken, runs nowhere and is
/// answered as unavailable, without its words; one the admission refuses,
/// over the rate or a full queue, is answered at the end of the step.
pub(crate) fn queue(loader: &Loader, key: u64, text: &str) -> bool {
    let Some(named) = NAMED.sealed() else {
        return false;
    };
    let (owner, rest, off) = match typed(named, text, |owner| running_mod(loader, owner)) {
        Typed::Chat => return false,
        Typed::Off(owner) => (owner, "", true),
        Typed::Run(owner, rest) => (owner, rest, false),
    };
    let command = || Queued {
        key,
        owner,
        word: word(text),
        rest: rest.to_owned(),
    };
    let refusal = match admission::admit(loader, Path::Chat, key) {
        Verdict::Run if off => Some(admission::Refusal::Unavailable),
        Verdict::Run => {
            if STATE
                .pending
                .push_within(MAX_PENDING, core::iter::once_with(command))
            {
                return true;
            }
            Some(admission::Refusal::Busy)
        }
        Verdict::Refuse(refusal) => Some(refusal),
        Verdict::Drop => None,
    };
    if let Some(refusal) = refusal {
        let _answered = STATE
            .refused
            .push_within(MAX_PENDING, core::iter::once_with(|| (command(), refusal)));
    }
    if off {
        return true;
    }
    let dropped = STATE.dropped.fetch_add(1, Ordering::Relaxed);
    if dropped.is_multiple_of(100) {
        crate::log!(
            DEBUG,
            "{key:016x} sent a command over its rate or a full queue ({} refused)",
            dropped + 1
        );
    }
    true
}

/// Forgets the commands waiting and their refusals, when a hosted game
/// ends, so none runs or answers in the next one.
pub(crate) fn forget() {
    STATE.pending.clear();
    STATE.refused.clear();
}

/// Runs the chat commands queued during the step and gives each player the
/// reply the one way replies go, then answers the ones refused, which came
/// after them; on the server's game thread, after the step.
pub(crate) fn after_step(loader: &Loader) {
    let refused = STATE.refused.take();
    for command in STATE.pending.take() {
        let Some(player) = crate::actions::player_actions::player(loader, command.key) else {
            continue;
        };
        let Some(reply) = crate::actions::player_actions::command(
            loader,
            command.owner,
            Path::Chat,
            &player,
            crate::base::net::language_number(command.key),
            (&command.word, &command.rest),
        ) else {
            continue;
        };
        crate::log!(
            DEBUG,
            "{:016x} ran a command of mod {}: {}",
            command.key,
            command.owner.get(),
            if reply.done { "done" } else { "refused" }
        );
        crate::actions::reply::answer(loader, Path::Chat, command.key, reply.done, &reply.text);
    }
    for (command, refusal) in refused {
        crate::actions::player_actions::refuse(
            loader,
            Path::Chat,
            command.key,
            refusal,
            Some(command.owner),
            (&command.rest, "", &command.word),
        );
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
) -> Result<crate::actions::player_actions::Answer, Status> {
    if line.len() > MAX_LINE || line.trim().is_empty() {
        return Err(Status::BAD_ARGS);
    }
    crate::engine::world::server().map_err(|_| Status::WRONG_THREAD)?;
    let (owner, rest) = owner_of(loader, line).ok_or(Status::MISSING)?;
    let caller = remote_caller(label, permissions);
    crate::log!(DEBUG, "{label} runs the command {line}");
    let word = word(line);
    crate::actions::player_actions::command(loader, owner, Path::Remote, &caller, 0, (&word, rest))
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
    fn a_name_clash_is_logged_once_without_a_build_code_and_listed_off() {
        let turn = crate::engine::log::capture_turn();
        for _ in 0..2 {
            part_off(
                "test alias /lamp",
                "test alias /lamp off: /lamp is taken".to_owned(),
                "/lamp is taken".to_owned(),
            );
        }
        let lines = crate::engine::log::captured();
        drop(turn);
        let logged = lines
            .iter()
            .filter(|(_, _, line)| line == "test alias /lamp off: /lamp is taken")
            .count();
        assert_eq!(logged, 1, "a mod asking again logs nothing more: {lines:?}");
        let listed = crate::engine::diag::parts_off();
        let off = listed.iter().find(|p| p.part == "test alias /lamp");
        assert_eq!(off.map(|p| p.reason.as_str()), Some("/lamp is taken"));
    }

    #[test]
    fn a_command_typed_in_one_hosted_game_never_runs_in_the_next() {
        STATE.pending.push(Queued {
            key: 7,
            owner: ModIndex::new(0),
            word: "adm".to_owned(),
            rest: "tp spawn".to_owned(),
        });
        forget();
        assert!(!STATE.pending.any());
        assert!(STATE.pending.take().is_empty());
    }
}
