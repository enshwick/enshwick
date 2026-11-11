//! The fake's mod commands: the names the mod takes, lines run for remote
//! callers and the list of names, as the loader answers them.

use crate::test::table::{Registration, put_headed, text, with};
use enshwick_core::abi::{
    ActionCall, ActionReply, ActionTarget, CommandInfo, ModHandle, PlayerInfo, Status, Str,
};

/// What the mod did with its commands.
#[derive(Debug, Default)]
pub(crate) struct FakeCommands {
    /// The alias it asked for its commands, once the fake took them.
    pub(crate) asked: Option<String>,
    /// How often it said who may run its actions changed.
    pub(crate) rules_changed: u32,
    /// The strings `command_list` last handed out: name, alias, mod id.
    pub(crate) listed: Vec<[Box<str>; 3]>,
    /// Shows, closes and updates of the mod's windows not carried yet:
    /// account key, window id and the `close` value of the call.
    pub(crate) windows: Vec<(u64, String, u32)>,
    /// The windows each client holds: account key, window id and text.
    pub(crate) carried: Vec<(u64, String, String)>,
}

#[cfg(feature = "unstable")]
use enshwick_core::actions::{
    is_valid_id as is_window_id,
    window::{MAX_CALL_PLAYERS, MAX_WAITING},
};
/// The most players one show or close names, as
/// `actions::window::MAX_CALL_PLAYERS` holds it where that module is built.
#[cfg(not(feature = "unstable"))]
const MAX_CALL_PLAYERS: usize = 64;
/// The most shows and closes that wait, as `actions::window::MAX_WAITING`
/// holds it where that module is built.
#[cfg(not(feature = "unstable"))]
const MAX_WAITING: usize = 256;

/// Without the `actions` module the fake takes any window id of 1 to 32
/// bytes.
#[cfg(not(feature = "unstable"))]
fn is_window_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
}

pub(crate) extern "system" fn action_window(
    _me: ModHandle,
    id: Str,
    players: *const enshwick_core::PlayerKey,
    count: u32,
    close: u32,
) -> Status {
    // SAFETY: the caller keeps the id's bytes alive for the call.
    let Some(id) = (unsafe { id.as_str() }).filter(|id| is_window_id(id)) else {
        return Status::BAD_ARGS;
    };
    if count as usize > MAX_CALL_PLAYERS
        || (players.is_null() && count != 0)
        || close > enshwick_core::abi::WINDOW_UPDATE
    {
        return Status::BAD_ARGS;
    }
    let keys: Vec<u64> = if count == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller hands `count` keys at `players`, not null by
        // the guard before it, alive for the call.
        unsafe { core::slice::from_raw_parts(players, count as usize) }
            .iter()
            .map(|key| key.get())
            .collect()
    };
    with(Status::MISSING, |s| {
        if s.host.side == enshwick_core::abi::Side::CLIENT && !s.host.hosting {
            return Status::MISSING;
        }
        if s.actions.is_none() || s.commands.windows.len() + keys.len() > MAX_WAITING {
            return Status::REFUSED;
        }
        s.commands
            .windows
            .extend(keys.into_iter().map(|key| (key, id.to_owned(), close)));
        Status::OK
    })
}

use enshwick_core::command_names::{MAX_LINE, Named, Refusal, check};
/// The game's permission bits a remote caller may hold.
#[cfg(feature = "unstable")]
const ALL_BITS: u8 = enshwick_core::world::PlayerPermissions::ALL_BITS;
/// Without the `world` module the fake keeps every bit a caller names.
#[cfg(not(feature = "unstable"))]
const ALL_BITS: u8 = u8::MAX;

/// One mod's commands as the loader grants them.
struct Granted {
    id: String,
    set: Named<usize>,
}

impl Granted {
    fn is_called(&self, word: &str) -> bool {
        self.set.is_called(word)
    }
}

/// What the loader answers each mod that names commands, by the one rule
/// of [`check`], in load order: the mods loaded before the one under test,
/// that mod `own` (its id, and the alias it asked, if it did) and the mods
/// loaded after it, each `others` entry being a mod id, the alias it asks
/// and whether it loads after. The command sets granted, and what the mod
/// under test was answered.
fn granted(
    own: (&str, Option<&str>),
    others: &[(String, String, bool)],
) -> (Vec<Granted>, Option<Result<(), Refusal<usize>>>) {
    let at = |later: bool| {
        others
            .iter()
            .filter(move |(_, _, after)| *after == later)
            .map(|(id, alias, _)| (id.as_str(), Some(alias.as_str())))
    };
    let order: Vec<(&str, Option<&str>)> = at(false).chain([own]).chain(at(true)).collect();
    let me = at(false).count();
    let ids: Vec<(usize, &str)> = order.iter().map(|(id, _)| *id).enumerate().collect();
    let mut named: Vec<Named<usize>> = Vec::new();
    let mut answered = None;
    for (index, (_, alias)) in order.iter().enumerate() {
        let Some(alias) = alias else { continue };
        let checked = check(&named, &ids, index, Some(alias)).map(|(set, _)| named.push(set));
        if index == me {
            answered = Some(checked);
        }
    }
    let sets = named
        .into_iter()
        .map(|set| Granted {
            id: order[set.owner].0.to_owned(),
            set,
        })
        .collect();
    (sets, answered)
}

/// The id of the mod under test.
fn own_id(s: &crate::test::table::State) -> String {
    s.manifest
        .as_ref()
        .map(|m| m.id.clone())
        .unwrap_or_default()
}

/// The command sets granted now, and the id of the mod under test.
fn sets_now(s: &crate::test::table::State) -> (Vec<Granted>, String) {
    let id = own_id(s);
    let (sets, _) = granted((&id, s.commands.asked.as_deref()), &s.host.other_commands);
    (sets, id)
}

pub(crate) extern "system" fn commands(_me: ModHandle, _name: Str, alias: Str) -> Status {
    let alias = text(alias);
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        let id = own_id(s);
        let (_, answered) = granted((&id, Some(&alias)), &s.host.other_commands);
        match answered {
            Some(Err(Refusal::Malformed)) | None => Status::BAD_ARGS,
            Some(Ok(())) if s.commands.asked.is_none() => {
                s.commands.asked = Some(alias);
                Status::OK
            }
            Some(_) => Status::REFUSED,
        }
    })
}

/// Whether `word`, in any case, is the command name or the alias of the
/// mod under test.
#[cfg(feature = "unstable")]
pub(crate) fn is_own(word: &str) -> bool {
    let word = word.to_lowercase();
    with(false, |s| {
        let (sets, id) = sets_now(s);
        sets.iter().any(|set| set.id == id && set.is_called(&word))
    })
}

/// The first word of `line` without its `/`, and the rest.
fn split(line: &str) -> (String, String) {
    let line = line.trim();
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    (
        first.strip_prefix('/').unwrap_or(first).to_lowercase(),
        rest.trim().to_owned(),
    )
}

/// The mod's reply to command line `line` asked as `kind` for `caller`,
/// `1` or `0` and the text; `None` when the line names neither the mod's
/// commands nor another mod's.
pub(crate) fn answer(line: &str, kind: u32, caller: &PlayerInfo, language: u32) -> Option<String> {
    let (word, rest) = split(line);
    let (own, other, actions) = with((false, None, None), |s| {
        let (sets, id) = sets_now(s);
        let Some(set) = sets.iter().find(|set| set.is_called(&word)) else {
            return (false, None, s.actions);
        };
        let own = set.id == id;
        let other = (!own).then(|| {
            s.host
                .console_answers
                .get(&word)
                .cloned()
                .unwrap_or_default()
        });
        (own, other, s.actions)
    });
    if let Some(answer) = other {
        return Some(format!("1{answer}"));
    }
    if !own {
        return None;
    }
    // The mod's actions, asked with the table unlocked: they may call back.
    Some(crate::test::ask(
        actions?,
        kind,
        caller,
        language,
        &rest,
        &word,
        ActionTarget::NONE,
    ))
}

/// The remote caller `label` with the game's permission bits
/// `permissions`, as the loader builds it: no character, account key 0,
/// the label cut to 64 bytes as its name.
fn remote_caller(label: &str, permissions: u32) -> PlayerInfo {
    let label = &label[..label.floor_char_boundary(64)];
    let mut name = [0u8; 64];
    name[..label.len()].copy_from_slice(label.as_bytes());
    PlayerInfo {
        header: enshwick_core::abi::Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: enshwick_core::Entity(0),
        key: enshwick_core::PlayerKey::new(0),
        permissions: u8::try_from(permissions & u32::from(ALL_BITS)).unwrap_or(0),
        name_len: u8::try_from(label.len()).unwrap_or(64),
        name,
    }
}

pub(crate) extern "system" fn run_command(
    _me: ModHandle,
    line: Str,
    label: Str,
    permissions: u32,
    out: *mut ActionReply,
) -> Status {
    let line = text(line);
    if line.len() > MAX_LINE || line.trim().is_empty() {
        return Status::BAD_ARGS;
    }
    let caller = remote_caller(&text(label), permissions);
    let Some(reply) = answer(&line, ActionCall::REMOTE, &caller, 0) else {
        return Status::MISSING;
    };
    let (done, text) = reply.split_at_checked(1).unwrap_or(("0", ""));
    with(Status::MISSING, |s| {
        let kept = s.run_out.insert(text.into());
        let reply = ActionReply {
            header: enshwick_core::abi::Header::of::<ActionReply>(),
            done: u8::from(done == "1"),
            text: Str::new(kept),
        };
        put_headed(out, reply)
    })
}

pub(crate) extern "system" fn command_list(
    _me: ModHandle,
    out: *mut CommandInfo,
    entry_size: u32,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let infos = with(Vec::new(), |s| {
        s.commands.listed = sets_now(s)
            .0
            .into_iter()
            .map(|set| {
                [set.set.name, set.set.alias.unwrap_or_default(), set.id]
                    .map(String::into_boxed_str)
            })
            .collect();
        s.commands
            .listed
            .iter()
            .map(|[name, alias, id]| CommandInfo {
                header: enshwick_core::abi::Header::of::<CommandInfo>(),
                name: Str::new(name),
                alias: Str::new(alias),
                mod_id: Str::new(id),
            })
            .collect::<Vec<_>>()
    });
    crate::test::table::base::mods::list(&infos, out, entry_size, capacity, count)
}

pub(crate) extern "system" fn actions_changed(_me: ModHandle) -> Status {
    with(Status::MISSING, |s| {
        if s.actions.is_none() {
            return Status::MISSING;
        }
        s.commands.rules_changed += 1;
        Status::OK
    })
}
