//! The fake's mod commands: the names the mod takes, lines run for remote
//! callers and the list of names, as the loader answers them.

use crate::test::table::base::args;
use crate::test::table::entries;
use crate::test::table::{put_headed, text, with, with_open};
use enshwick_core::abi::{
    ActionCall, ActionReply, ActionTarget, CommandInfo, PlayerInfo, Status, Str,
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
    /// account key, window id and the `change` value of the call.
    pub(crate) windows: Vec<(u64, String, u32)>,
    /// The windows each client holds: account key, window id and text.
    pub(crate) carried: Vec<(u64, String, String)>,
    /// The windows shown to clients that have not said hello, oldest
    /// first: account key and window id.
    #[cfg(feature = "unstable")]
    pub(crate) pending: Vec<(u64, String)>,
    /// Each player's waypoint as the maps' watcher saw it last.
    #[cfg(feature = "unstable")]
    pub(crate) waypoints: enshwick_base::__internal::waypoints::Waypoints,
}

#[cfg(feature = "unstable")]
impl FakeCommands {
    /// Keeps the `change` of window `id` for client `key`, which has not
    /// said hello, as the loader does: a show waits, a close takes the
    /// window back, an update waits for nothing since the hello shows the
    /// window anew. The answer is the oldest show let go past
    /// `actions::window::MAX_HELD`, which the mod hears as closed.
    pub(crate) fn pend(&mut self, key: u64, id: String, change: u32) -> Option<String> {
        use enshwick_core::abi::{WINDOW_CLOSE, WINDOW_SHOW};
        use enshwick_core::actions::window::MAX_HELD;

        let theirs = |k: u64, i: &str| k == key && i == id;
        match change {
            WINDOW_SHOW if !self.pending.iter().any(|(k, i)| theirs(*k, i)) => {
                self.pending.push((key, id));
                let mine: Vec<usize> = (0..self.pending.len())
                    .filter(|&at| self.pending[at].0 == key)
                    .collect();
                (mine.len() > MAX_HELD).then(|| self.pending.remove(mine[0]).1)
            }
            WINDOW_CLOSE => {
                self.carried.retain(|(k, i, _)| !theirs(*k, i));
                self.pending.retain(|(k, i)| !theirs(*k, i));
                None
            }
            _ => None,
        }
    }

    /// Client `key` said hello on a link that holds no window: the windows
    /// it held and those waiting for it show again, oldest first, before
    /// the mod's newer changes. All go, or none past the waiting bound;
    /// then the answer is the windows that will not show, which the mod
    /// hears as closed.
    pub(crate) fn greeted(&mut self, key: u64) -> Vec<String> {
        let held = self.carried.extract_if(.., |(k, _, _)| *k == key);
        let held = held.map(|(_, id, _)| id).collect::<Vec<_>>();
        let pending = self.pending.extract_if(.., |(k, _)| *k == key);
        let shows: Vec<String> = held.into_iter().chain(pending.map(|(_, id)| id)).collect();
        if self.windows.len() + shows.len() > MAX_WAITING {
            return shows;
        }
        let shows = shows
            .into_iter()
            .map(|id| (key, id, enshwick_core::abi::WINDOW_SHOW));
        self.windows.splice(0..0, shows);
        Vec::new()
    }
}

#[cfg(feature = "unstable")]
use enshwick_core::actions::is_valid_id as is_window_id;
use enshwick_core::peer::{MAX_CALL_PLAYERS, MAX_WAITING_WINDOWS as MAX_WAITING};

/// Without the `actions` module the fake takes any window id of 1 to 32
/// bytes.
#[cfg(not(feature = "unstable"))]
fn is_window_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
}

entries! {
    pub(crate) fn action_window(
        me as Mod,
        id: Str as args::Text,
        [players: *const enshwick_core::PlayerKey, count: u32] as args::List,
        change: u32,
    ) -> Status {
        // SAFETY: the caller keeps the id's bytes alive for the call.
        let Some(id) = (unsafe { id.as_str() }).filter(|id| is_window_id(id)) else {
            return Status::BAD_ARGS;
        };
        if count as usize > MAX_CALL_PLAYERS
            || (players.is_null() && count != 0)
            || change > enshwick_core::abi::WINDOW_UPDATE
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
            if s.host.base.side == enshwick_core::abi::Side::CLIENT && !s.host.base.hosting {
                return Status::UNAVAILABLE;
            }
            if s.actions.callback.is_none()
                || s.actions.commands.windows.len() + keys.len() > MAX_WAITING
            {
                return Status::REFUSED;
            }
            s.actions
                .commands
                .windows
                .extend(keys.into_iter().map(|key| (key, id.to_owned(), change)));
            Status::OK
        })
    }
}

use enshwick_core::command_names::{MAX_LINE, Named, Refusal, check};
/// The game's permission bits a remote caller may hold.
#[cfg(feature = "unstable")]
const ALL_BITS: u32 = enshwick_core::world::PlayerPermissions::ALL_BITS;
/// Without the `world` module the fake keeps every bit a caller names.
#[cfg(not(feature = "unstable"))]
const ALL_BITS: u32 = u32::MAX;

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
    s.base.manifest.as_ref().map_or_default(|m| m.id.clone())
}

/// The command sets granted now, and the id of the mod under test.
fn sets_now(s: &crate::test::table::State) -> (Vec<Granted>, String) {
    let id = own_id(s);
    let (sets, _) = granted(
        (&id, s.actions.commands.asked.as_deref()),
        &s.host.actions.other_commands,
    );
    (sets, id)
}

entries! {
    pub(crate) fn commands(me as Mod, alias: Str as args::Text) -> Status {
        let alias = text(alias);
        with_open(|s| {
            let id = own_id(s);
            let (_, answered) = granted((&id, Some(&alias)), &s.host.actions.other_commands);
            match answered {
                Some(Err(Refusal::Malformed)) | None => Status::BAD_ARGS,
                Some(Ok(())) if s.actions.commands.asked.is_none() => {
                    s.actions.commands.asked = Some(alias);
                    Status::OK
                }
                Some(_) => Status::REFUSED,
            }
        })
    }
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

/// The mod's reply to command line `line` asked as `kind` for `caller`,
/// `1` or `0` and the text; `None` when the line names neither the mod's
/// commands nor another mod's.
pub(crate) fn answer(line: &str, kind: u32, caller: &PlayerInfo, language: u32) -> Option<String> {
    let word = enshwick_core::command_names::command_word(line);
    let (_, rest) = enshwick_core::command_names::first_word(line);
    let (own, other, actions) = with((false, None, None), |s| {
        let (sets, id) = sets_now(s);
        let Some(set) = sets.iter().find(|set| set.is_called(&word)) else {
            return (false, None, s.actions.callback);
        };
        let own = set.id == id;
        let other = (!own).then(|| {
            s.host
                .actions
                .console_answers
                .get(&word)
                .cloned()
                .unwrap_or_default()
        });
        (own, other, s.actions.callback)
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
        rest,
        &word,
        ActionTarget::NONE,
    ))
}

/// The remote caller `label` with the game's permission bits
/// `permissions`, as the loader builds it.
fn remote_caller(label: &str, permissions: u32) -> PlayerInfo {
    PlayerInfo::without_character(
        enshwick_core::PlayerKey::new(0),
        permissions & ALL_BITS,
        label,
    )
}

entries! {
    pub(crate) fn run_command(
        me as Caller,
        line: Str as args::Text,
        label: Str as args::Text,
        permissions: u32,
        out: *mut ActionReply as args::HeadedOut,
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
            let kept = s.actions.run_out.insert(text.into());
            let reply = ActionReply {
                header: enshwick_core::abi::Header::of::<ActionReply>(),
                done: u8::from(done == "1"),
                text: Str::new(kept),
            };
            put_headed(out, reply)
        })
    }

    pub(crate) fn command_list(
        me as Caller,
        [out: *mut CommandInfo, entry_size: u32, capacity: u32, count: *mut u32] as args::SizedListOut,
    ) -> Status {
        let infos = with(Vec::new(), |s| {
            let own_off = s
                .base
                .features
                .iter()
                .any(|(name, off)| name == enshwick_core::command_names::FEATURE && off.is_some());
            let (sets, id) = sets_now(s);
            s.actions.commands.listed = sets
                .into_iter()
                .filter(|set| !own_off || set.id != id)
                .map(|set| {
                    [set.set.name, set.set.alias.unwrap_or_default(), set.id]
                        .map(String::into_boxed_str)
                })
                .collect();
            s.actions
                .commands
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
        crate::test::table::base::io::list(&infos, out, entry_size, capacity, count)
    }

    pub(crate) fn actions_changed(me as Mod) -> Status {
        with(Status::MISSING, |s| {
            if s.actions.callback.is_none() {
                return Status::MISSING;
            }
            s.actions.commands.rules_changed += 1;
            Status::OK
        })
    }
}
