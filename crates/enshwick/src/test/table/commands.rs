//! The fake's mod commands: the names the mod takes, lines run for remote
//! callers and the list of names, as the loader answers them.

use crate::abi::{
    ActionCall, ActionReply, ActionTarget, CommandInfo, ModHandle, PlayerInfo, Status, Str,
};
use crate::test::table::{Registration, put_headed, text, with};

/// What the mod did with its commands.
#[derive(Debug, Default)]
pub(crate) struct FakeCommands {
    /// The name and alias it took.
    pub(crate) named: Option<(String, String)>,
    /// How often it said who may run its actions changed.
    pub(crate) rules_changed: u32,
    /// The strings `command_list` last handed out: name, alias, mod id.
    pub(crate) listed: Vec<[Box<str>; 3]>,
    /// Shows and closes of the mod's windows not carried yet: account key,
    /// window id and whether it closes.
    pub(crate) windows: Vec<(u64, String, bool)>,
    /// The windows each client holds: account key, window id and text.
    #[cfg_attr(
        not(feature = "unstable"),
        expect(
            dead_code,
            reason = "only `Installed::carried_windows`, unstable, reads it"
        )
    )]
    pub(crate) carried: Vec<(u64, String, String)>,
}

/// The most players one show or close names, as the loader takes them.
const MAX_WINDOW_PLAYERS: u32 = 64;
/// The most shows and closes that wait, as the loader keeps them.
const MAX_WINDOWS_WAITING: usize = 256;

/// Whether `id` names a window as the loader takes it: 1 to 32 of
/// `[a-z0-9-]`.
fn is_window_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(crate) extern "system" fn action_window(
    _me: ModHandle,
    id: Str,
    players: *const crate::PlayerKey,
    count: u32,
    close: u32,
) -> Status {
    // SAFETY: the caller keeps the id's bytes alive for the call.
    let Some(id) = (unsafe { id.as_str() }).filter(|id| is_window_id(id)) else {
        return Status::BAD_ARGS;
    };
    if count > MAX_WINDOW_PLAYERS || (players.is_null() && count != 0) || close > 1 {
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
        if s.host.side == crate::abi::Side::CLIENT && !s.host.hosting {
            return Status::MISSING;
        }
        if s.actions.is_none() || s.commands.windows.len() + keys.len() > MAX_WINDOWS_WAITING {
            return Status::REFUSED;
        }
        s.commands
            .windows
            .extend(keys.into_iter().map(|key| (key, id.to_owned(), close == 1)));
        Status::OK
    })
}

#[cfg(feature = "unstable")]
use crate::commands::MAX_LINE;
/// The longest command line, as `commands::MAX_LINE` holds it where that
/// module is built.
#[cfg(not(feature = "unstable"))]
const MAX_LINE: usize = 1024;
/// The game's permission bits a remote caller may hold.
#[cfg(feature = "unstable")]
const ALL_BITS: u8 = crate::world::PlayerPermissions::ALL_BITS;
/// Without the `world` module the fake keeps every bit a caller names.
#[cfg(not(feature = "unstable"))]
const ALL_BITS: u8 = u8::MAX;

/// Whether `name` and `alias` are words the loader takes.
fn fit(name: &str, alias: &str) -> bool {
    #[cfg(feature = "unstable")]
    return crate::commands::names_fit(name, alias);
    #[cfg(not(feature = "unstable"))]
    return !name.is_empty() && !alias.is_empty();
}

pub(crate) extern "system" fn commands(_me: ModHandle, name: Str, alias: Str) -> Status {
    let (name, alias) = (text(name), text(alias));
    with(Status::MISSING, |s| {
        if s.registration == Registration::Closed {
            return Status::TOO_LATE;
        }
        if !fit(&name, &alias) {
            return Status::BAD_ARGS;
        }
        let words = [name.as_str(), alias.as_str()];
        let taken = s
            .host
            .other_commands
            .iter()
            .any(|(_, n, a)| words.contains(&n.as_str()) || words.contains(&a.as_str()));
        if s.commands.named.is_some() || taken {
            return Status::REFUSED;
        }
        s.commands.named = Some((name, alias));
        Status::OK
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
        let own = s
            .commands
            .named
            .as_ref()
            .is_some_and(|(n, a)| *n == word || *a == word);
        let other = s
            .host
            .other_commands
            .iter()
            .any(|(_, n, a)| *n == word || *a == word)
            .then(|| {
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
        "",
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
        header: crate::abi::Header::of::<PlayerInfo>(),
        slot: u32::MAX,
        entity: crate::Entity(0),
        key: crate::PlayerKey::new(0),
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
            header: crate::abi::Header::of::<ActionReply>(),
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
        let own_id = s
            .manifest
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_default();
        let own = s
            .commands
            .named
            .iter()
            .map(|(name, alias)| (own_id.clone(), name.clone(), alias.clone()));
        let sets: Vec<(String, String, String)> =
            own.chain(s.host.other_commands.iter().cloned()).collect();
        s.commands.listed = sets
            .into_iter()
            .map(|(id, name, alias)| [name, alias, id].map(String::into_boxed_str))
            .collect();
        s.commands
            .listed
            .iter()
            .map(|[name, alias, id]| CommandInfo {
                header: crate::abi::Header::of::<CommandInfo>(),
                name: Str::new(name),
                alias: Str::new(alias),
                mod_id: Str::new(id),
            })
            .collect::<Vec<_>>()
    });
    super::mods::list(&infos, out, entry_size, capacity, count)
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
