//! The allowlist at work: its file, the console's `enshwick allowlist` and
//! the check at each join.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::console::accounts::{self, Online, Refusal, Target};
use crate::host::Loader;
use crate::locks::Relock;
use crate::settings::LOADER_ID;
use crate::texts;

use super::{FILE, HEADER, add, admits, parse, remove};

/// The allowlist's state.
struct State {
    /// The `allowlist` setting.
    on: AtomicBool,
    /// The file's accounts as last read, with its time of change.
    listed: Mutex<Option<(SystemTime, Vec<u64>)>>,
    /// Held while a change reads and writes the file.
    writing: Mutex<()>,
}

static STATE: State = State {
    on: AtomicBool::new(false),
    listed: Mutex::new(None),
    writing: Mutex::new(()),
};

/// Takes the loader settings' switch, before the loader starts.
pub(crate) fn configure(settings: &crate::settings::Settings) {
    STATE.on.store(settings.allowlist, Ordering::Release);
}

/// The file in the game's `data_dir`.
fn path(loader: &Loader) -> PathBuf {
    loader.data_dir.join(LOADER_ID).join(FILE)
}

/// Writes the file's header when the allowlist is on and the file is
/// missing, so the admin finds where the accounts go.
pub(crate) fn start(loader: &Loader) {
    if !STATE.on.load(Ordering::Acquire) {
        return;
    }
    let path = path(loader);
    if !path.exists() {
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, HEADER));
        if let Err(e) = written {
            crate::log!(WARN, "allowlist: {} not written: {e}", path.display());
        }
    }
    let count = listed(loader).len();
    crate::log!(
        INFO,
        "allowlist on: {count} accounts and the administrators may stay"
    );
}

/// The accounts the file lists now, read again only when it changed.
fn listed(loader: &Loader) -> Vec<u64> {
    let path = path(loader);
    let changed = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut cached = STATE.listed.relock();
    if let (Some(changed), Some((seen, keys))) = (changed, cached.as_ref())
        && *seen == changed
    {
        return keys.clone();
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let (keys, bad) = parse(&text);
    if !bad.is_empty() {
        crate::log!(
            WARN,
            "allowlist: lines {bad:?} of {} spell no account",
            path.display()
        );
    }
    *cached = changed.map(|changed| (changed, keys.clone()));
    keys
}

/// Whether the player `key` joining now must leave: the allowlist is on,
/// the account is not listed and the player is no administrator.
pub(crate) fn refuses(loader: &Loader, key: u64, admin: bool) -> bool {
    let on = STATE.on.load(Ordering::Acquire);
    on && !admits(on, &listed(loader), key, admin)
}

/// `enshwick allowlist [add|remove <player or account>]`.
pub(crate) fn command(loader: &Loader, args: &str) -> String {
    let (word, rest) = args
        .trim()
        .split_once(char::is_whitespace)
        .map_or((args.trim(), ""), |(word, rest)| (word, rest.trim()));
    let shown = format!("mod-data/{LOADER_ID}/{FILE}");
    let on = texts::fill(
        if STATE.on.load(Ordering::Acquire) {
            "allowlist.is_on"
        } else {
            "allowlist.is_off"
        },
        &[],
    );
    let on = on.as_str();
    if word.is_empty() {
        let count = listed(loader).len().to_string();
        return texts::fill(
            "allowlist.state",
            &[("state", on), ("count", &count), ("file", &shown)],
        );
    }
    let adding = match word.to_ascii_lowercase().as_str() {
        "add" => true,
        "remove" => false,
        _ => return texts::fill("allowlist.usage", &[]),
    };
    let online: Vec<Online> = crate::world::players(loader)
        .map(|players| {
            players
                .iter()
                .map(|p| Online {
                    name: crate::console::player_name(p),
                    key: p.key,
                    admin: crate::console::player_is_admin(p),
                })
                .collect()
        })
        .unwrap_or_default();
    let (key, name) = match accounts::target(rest, &online) {
        Ok(Target::Player(player)) => (player.key, Some(player.name)),
        Ok(Target::Account(key)) => (key, None),
        Err(Refusal::Shared(word, keys)) => {
            return texts::fill(
                "console.account.shared",
                &[("word", &word), ("accounts", &accounts::listed(&keys))],
            );
        }
        Err(Refusal::Unknown(word)) => {
            return texts::fill("console.account.unknown", &[("word", &word)]);
        }
        Err(_) => return texts::fill("allowlist.usage", &[]),
    };
    let account = format!("{key:016x}");
    match set(loader, key, adding, name.as_deref(), "the console") {
        Ok(true) => {
            let done = if adding {
                "allowlist.added"
            } else {
                "allowlist.removed"
            };
            texts::fill(done, &[("account", &account), ("state", on)])
        }
        Ok(false) => {
            let same = if adding {
                "allowlist.listed"
            } else {
                "allowlist.unlisted"
            };
            texts::fill(same, &[("account", &account), ("file", &shown)])
        }
        Err(e) => texts::fill(
            "allowlist.unwritten",
            &[("file", &shown), ("why", &e.to_string())],
        ),
    }
}

/// Whether the file lists `key` now, the setting on or not.
pub(crate) fn has(loader: &Loader, key: u64) -> bool {
    listed(loader).contains(&key)
}

/// Lists `key` in the file, its note `name`, or takes it off, for `who`
/// in the log; whether the file changed. One change at a time, so two
/// writers never lose each other's line.
pub(crate) fn set(
    loader: &Loader,
    key: u64,
    listed: bool,
    name: Option<&str>,
    who: &str,
) -> std::io::Result<bool> {
    let _one = STATE.writing.relock();
    let path = path(loader);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let changed = if listed {
        add(&text, key, name)
    } else {
        remove(&text, key)
    };
    let Some(changed) = changed else {
        return Ok(false);
    };
    path.parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, changed))?;
    *STATE.listed.relock() = None;
    crate::log!(
        INFO,
        "{who} {} {key:016x} {} the allowlist",
        if listed { "added" } else { "removed" },
        if listed { "to" } else { "from" }
    );
    Ok(true)
}
