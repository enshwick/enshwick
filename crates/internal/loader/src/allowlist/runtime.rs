//! The allowlist at work: its file, the console's `enshwick allowlist` and
//! the check at each join.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::console::accounts::{self, Online, Refusal, Target};
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::locks::Relock;
use crate::settings::LOADER_ID;
use crate::texts;

use super::{FILE, HEADER, Joining, add, parse, remove};

/// The allowlist's state.
struct State {
    /// The `allowlist` setting.
    on: AtomicBool,
    /// The file's accounts as last read well.
    listed: Mutex<Option<Listed>>,
    /// Held while a change reads and writes the file.
    writing: Mutex<()>,
    /// The join check's own switch, apart from the join admission's.
    fuse: Fuse,
}

/// The file's accounts as last read well.
struct Listed {
    /// The file's time of change and length then; `None` reads it again.
    stamp: Option<(SystemTime, u64)>,
    keys: Vec<u64>,
}

static STATE: State = State {
    on: AtomicBool::new(false),
    listed: Mutex::new(None),
    writing: Mutex::new(()),
    fuse: Fuse::new("the allowlist (every player but the host is removed while it is off)"),
};

/// Takes the loader settings' switch, before the loader starts.
pub(crate) fn configure(settings: &crate::settings::Settings) {
    STATE.on.store(settings.allowlist, Ordering::Release);
}

/// Whether the allowlist applies: on in the loader settings, on a
/// dedicated server.
pub(crate) fn is_on() -> bool {
    STATE.on.load(Ordering::Acquire)
        && crate::host::loader().is_some_and(|loader| loader.side == enshwick::abi::Side::SERVER)
}

/// The file in the game's `data_dir`.
fn path(loader: &Loader) -> PathBuf {
    loader.data_dir.join(LOADER_ID).join(FILE)
}

/// Writes the file's header when the allowlist is on and the file is
/// missing, so the admin finds where the accounts go; `can_kick` says
/// whether this build can remove a player at all.
pub(crate) fn start(loader: &Loader, can_kick: bool) {
    if !STATE.on.load(Ordering::Acquire) {
        return;
    }
    let path = path(loader);
    if !path.exists()
        && let Err(e) = write(&path, HEADER)
    {
        crate::log!(WARN, "allowlist: {} not written: {e}", path.display());
    }
    if !can_kick {
        crate::log!(
            WARN,
            "allowlist on but not enforced: this build lacks the session's kick, so every account may join"
        );
        return;
    }
    if let Some(keys) = listed(loader) {
        crate::log!(
            INFO,
            "allowlist on: {} accounts and the administrators may stay",
            keys.len()
        );
    } else {
        crate::log!(
            WARN,
            "allowlist on, but {} does not read: only administrators may stay",
            path.display()
        );
    }
}

/// Writes `text` to `path` whole, so a reader never sees it half written.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    enshwick::files::write_atomically(path, text)
}

/// The accounts the file lists now, read again only when it changed; a
/// missing file lists none. A file that does not read keeps the accounts
/// last read well, or `None` when none were.
fn listed(loader: &Loader) -> Option<Vec<u64>> {
    let path = path(loader);
    let stamp = std::fs::metadata(&path)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())));
    let mut cached = STATE.listed.relock();
    if let (
        Some(stamp),
        Some(Listed {
            stamp: Some(seen),
            keys,
        }),
    ) = (stamp, cached.as_ref())
        && *seen == stamp
    {
        return Some(keys.clone());
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => {
            crate::log!(
                WARN,
                "allowlist: {} does not read: {e}; the accounts last read stay",
                path.display()
            );
            let last = cached.as_mut()?;
            last.stamp = None;
            return Some(last.keys.clone());
        }
    };
    let (keys, bad) = parse(&text);
    if !bad.is_empty() {
        crate::log!(
            WARN,
            "allowlist: lines {bad:?} of {} spell no account",
            path.display()
        );
    }
    *cached = Some(Listed {
        stamp,
        keys: keys.clone(),
    });
    Some(keys)
}

/// Runs `check`, a join's allowlist check, under the allowlist's own
/// switch: `None` once a panic turned it off.
pub(crate) fn guarded<R>(check: impl FnOnce() -> R) -> Option<R> {
    STATE.fuse.run("a join's check", check)
}

/// Whether the player `key` entering the world now must leave, as
/// [`super::refused`] decides for this loader's setting and file.
pub(crate) fn refuses(loader: &Loader, key: u64, host: Option<u64>, admin: bool) -> bool {
    let on = STATE.on.load(Ordering::Acquire);
    if !on {
        return false;
    }
    let listed = listed(loader);
    super::refused(Joining {
        on,
        server: loader.side == enshwick::abi::Side::SERVER,
        key,
        host,
        admin,
        listed: listed.as_deref(),
    })
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
        let count = listed(loader).map_or(0, |keys| keys.len()).to_string();
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
                    key: p.key.get(),
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
    listed(loader).is_some_and(|keys| keys.contains(&key))
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
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
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
    write(&path, &changed)?;
    if let Some(last) = STATE.listed.relock().as_mut() {
        last.stamp = None;
    }
    crate::log!(
        INFO,
        "{who} {} {key:016x} {} the allowlist",
        if listed { "added" } else { "removed" },
        if listed { "to" } else { "from" }
    );
    Ok(true)
}
