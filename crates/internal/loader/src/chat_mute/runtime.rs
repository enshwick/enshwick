//! Chat mutes at work: the file, the host table's changes, and the check on
//! every line a player types.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::host::Loader;
use crate::locks::Relock;
use crate::settings::LOADER_ID;
use crate::texts;

use super::{FILE, SetMuteError, changed, protected, tell};

/// The mutes' state.
struct State {
    /// The muted accounts, as last read or written.
    keys: Mutex<Vec<u64>>,
    /// Whether any account is muted, so a line costs no lock while none is.
    any: AtomicBool,
    /// Held while a change reads and writes the file.
    writing: Mutex<()>,
    /// When each muted player was last told.
    told: Mutex<rustc_hash::FxHashMap<u64, Instant>>,
}

static STATE: State = State {
    keys: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    writing: Mutex::new(()),
    told: Mutex::new(rustc_hash::FxHashMap::with_hasher(
        rustc_hash::FxBuildHasher,
    )),
};

/// The file in the game's `data_dir`.
fn path(loader: &Loader) -> PathBuf {
    loader.data_dir.join(LOADER_ID).join(FILE)
}

fn keep(keys: Vec<u64>) {
    STATE.any.store(!keys.is_empty(), Ordering::Release);
    STATE.told.relock().retain(|key, _| keys.contains(key));
    *STATE.keys.relock() = keys;
}

/// Whether the player of `key` online is an administrator; `false` off the
/// game thread, where the players do not read.
fn admin_online(loader: &Loader, key: u64) -> bool {
    crate::world::players(loader).is_ok_and(|players| {
        players
            .iter()
            .any(|p| p.key == enshwick::PlayerKey::new(key) && crate::console::player_is_admin(p))
    })
}

/// Turns the mute check off alone at its first panic, so chat and mod
/// messages keep working.
static MUTE_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("chat mutes");

/// Whether this process runs a server whose chat the mutes hold: the
/// dedicated one, or the one a client hosts.
pub(crate) fn serving(loader: &Loader) -> bool {
    loader.side == enshwick::abi::Side::SERVER || crate::world::hosting()
}

/// Reads the file at start; a file that does not read mutes nobody and
/// says so.
pub(crate) fn start(loader: &Loader) {
    let path = path(loader);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => return,
        Err(e) => {
            crate::log!(WARN, "chat mutes: {} does not read: {e}", path.display());
            return;
        }
    };
    let (keys, bad) = crate::allowlist::parse(&text);
    if !bad.is_empty() {
        crate::log!(
            WARN,
            "chat mutes: lines {bad:?} of {} spell no account",
            path.display()
        );
    }
    if !keys.is_empty() {
        crate::log!(INFO, "chat mutes: {} accounts muted", keys.len());
    }
    keep(keys);
}

/// Whether `key` is muted.
pub(crate) fn has(_loader: &Loader, key: u64) -> bool {
    STATE.any.load(Ordering::Acquire) && STATE.keys.relock().contains(&key)
}

/// Mutes `key` or lifts its mute, for `who` in the log. One change at a
/// time, so two writers never lose each other's line.
///
/// # Errors
///
/// [`SetMuteError::Protected`] for a mute of an administrator online or the
/// hosted game's host; [`SetMuteError::Write`] when the file does not read
/// or is not written.
pub(crate) fn set(loader: &Loader, key: u64, muted: bool, who: &str) -> Result<(), SetMuteError> {
    if muted && protected(key, admin_online(loader, key), crate::admission::host()) {
        return Err(SetMuteError::Protected);
    }
    let _one = STATE.writing.relock();
    let path = path(loader);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    let Some(text) = changed(&text, key, muted) else {
        // The file says so already, perhaps edited by hand: the list follows it.
        keep(crate::allowlist::parse(&text).0);
        return Ok(());
    };
    enshwick::files::write_atomically(&path, &text)?;
    keep(crate::allowlist::parse(&text).0);
    crate::log!(
        INFO,
        "{who} {} {key:016x} in chat",
        if muted { "muted" } else { "lifted the mute of" }
    );
    Ok(())
}

/// Whether the server drops a line the player of `key` typed, telling the
/// player at most every [`super::NOTICE_GAP`]; on the game thread, from the
/// chat post link. An administrator or the host is never dropped, whenever
/// the mute was set. Off after a panic in it.
pub(crate) fn drops(loader: &Loader, key: u64) -> bool {
    MUTE_FUSE
        .run("a muted line", || dropped(loader, key))
        .unwrap_or(false)
}

fn dropped(loader: &Loader, key: u64) -> bool {
    // A typed line is a person's, not a frame's: one uncontended lock per line.
    if key == 0
        || !has(loader, key)
        || protected(key, admin_online(loader, key), crate::admission::host())
    {
        return false;
    }
    let now = Instant::now();
    let due = {
        let mut told = STATE.told.relock();
        let due = tell(told.get(&key).copied(), now);
        if due {
            told.insert(key, now);
        }
        due
    };
    if due {
        let line = texts::for_player(key, || texts::text("muted.notice"));
        if crate::notices::send(loader, key, line).is_err() {
            let _queued = crate::chat::send(key, line);
        }
    }
    true
}
