//! The player's key file, `mod-controls.toml`: read at link and again when
//! the loader gives up on a rebind kept for it, written by each rebind.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use enshwick_base::Relock;

use super::{Bound, DECLARED, Declared, STATE, bindings, game_dir, in_force, with_carried};
use crate::engine::loader::Loader;

/// The key file's text, a rebind still waiting for a held file included;
/// a file that does not exist reads as empty.
fn read(path: &Path) -> std::io::Result<String> {
    match crate::base::frame_writes::waiting(path).map_or_else(|| std::fs::read_to_string(path), Ok)
    {
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        read => read,
    }
}

/// Reads the player's key file at `path` into the keys in force and the
/// entries kept for controls no running mod declares; the server's carried
/// controls stay, with the file's keys. Its overrides, which a file that
/// does not read has none of.
pub(super) fn take_file(
    declared: &[Declared],
    path: &Path,
) -> BTreeMap<String, bindings::Override> {
    let mut kept = STATE.kept.relock();
    let read = read(path)
        .inspect_err(|why| crate::log!(WARN, "{}: {why}; the mods' keys apply", bindings::FILE))
        .ok();
    take(declared, read, &mut kept)
}

/// Takes the key file's text `read`, `None` for a file that does not read,
/// into the keys in force and `kept`, under the lock every rebind holds.
fn take(
    declared: &[Declared],
    read: Option<String>,
    kept: &mut Option<toml::Table>,
) -> BTreeMap<String, bindings::Override> {
    let text = read.clone().unwrap_or_default();
    let (overrides, problems) = bindings::parse(&text);
    let table = read.and_then(|text| text.parse::<toml::Table>().ok());
    for problem in &problems {
        crate::log!(WARN, "{}: {problem}; the mod's key applies", bindings::FILE);
    }
    *kept = table.map(|table| {
        table
            .into_iter()
            .filter(|(id, _)| !declared.iter().any(|d| d.id == *id))
            .collect()
    });
    let keys: Vec<[u32; 2]> = declared
        .iter()
        .map(|d| {
            in_force(
                d.defaults,
                overrides.get(&d.id).copied().unwrap_or_default(),
            )
        })
        .collect();
    STATE.keys.rcu(|bound| {
        let own = Bound {
            keys: keys.clone(),
            carried: Vec::new(),
            carried_at: bound.as_ref().map_or(0, |b| b.carried_at),
        };
        Some(Arc::new(match bound.as_ref() {
            // The same carried controls keep their trackers, so a held key
            // does not press again.
            Some(bound) if !bound.carried.is_empty() => Bound {
                carried_at: bound.carried_at,
                ..with_carried(&own, declared, bound.carried.clone(), &overrides)
            },
            _ => own,
        }))
    });
    overrides
}

/// Reads `mod-controls.toml` again once the loader gave up on a rebind kept
/// for it, so the keys in force are the file's and the next rebind starts
/// from its text; another file is left alone. On the held files' writer
/// thread.
pub(crate) fn given_up(path: &Path) {
    let (Some(file), Some(declared)) = (STATE.file.get(), DECLARED.sealed()) else {
        return;
    };
    if file != path {
        return;
    }
    reread(declared, path);
}

/// Reads the key file at `path` again after a give-up; a file that does not
/// read now leaves the keys in force and the kept entries as they are, so a
/// failed read loses nothing the player bound.
pub(super) fn reread(declared: &[Declared], path: &Path) {
    let mut kept = STATE.kept.relock();
    match read(path) {
        Ok(text) => {
            crate::log!(
                INFO,
                "controls: back to {}, whose change was given up",
                path.display()
            );
            let _overrides = take(declared, Some(text), &mut kept);
        }
        Err(why) => crate::log!(
            WARN,
            "controls: {} does not read again: {why}; the keys in force stay",
            path.display()
        ),
    }
}

/// Sets control `index`'s key in `slot` to `bits` (0 unbinds) and writes
/// the player's file, keeping its entries for controls no running mod
/// declares, a server's carried ones among them.
///
/// # Errors
///
/// The reason the file could not be written, or that it did not read at
/// start, so writing would lose it; the key holds for this run.
pub(crate) fn set_key(
    loader: &Loader,
    index: usize,
    slot: usize,
    bits: u32,
) -> Result<(), SetKeyError> {
    let Some(declared) = DECLARED.sealed() else {
        return Ok(());
    };
    let changed = |defaults: [u32; 2], k: [u32; 2]| {
        let at = |slot: usize| (k[slot] != defaults[slot]).then_some(k[slot]);
        [at(0), at(1)]
    };
    let mut entries: Vec<(String, bindings::Override)> = Vec::new();
    let mut carried_set = None;
    let mut bound_once = false;
    // Held to the write: a re-read after a give-up on the writer thread
    // then sees this key in the file's waiting text, never in memory only.
    let mut kept = STATE.kept.relock();
    // A server's message may change the carried list meanwhile, on the chat
    // thread: this key goes on top of whatever list stands.
    STATE.keys.rcu(|bound| {
        let bound = bound.as_ref()?;
        bound_once = true;
        let carried = index
            .checked_sub(declared.len())
            .and_then(|at| bound.carried.get(at));
        // A carried control this client declares too sets its own control's key.
        let twin = carried.and_then(|heard| {
            let id = heard.id();
            declared.iter().position(|d| d.id == id)
        });
        let mut keys = bound.keys.clone();
        for at in [Some(index), twin].into_iter().flatten() {
            if let Some(slots) = keys.get_mut(at)
                && let Some(key) = slots.get_mut(slot)
            {
                *key = bits;
            }
        }
        entries = declared
            .iter()
            .zip(&keys)
            .map(|(d, k)| (d.id.clone(), changed(d.defaults, *k)))
            .collect();
        carried_set = carried
            .filter(|_| twin.is_none())
            .zip(keys.get(index).copied())
            .map(|(heard, k)| (heard.id(), changed(heard.control.keys, k)));
        Some(Arc::new(Bound {
            keys,
            carried: bound.carried.clone(),
            carried_at: bound.carried_at,
        }))
    });
    if !bound_once {
        return Ok(());
    }
    let Some(kept) = kept.as_mut() else {
        return Err(SetKeyError::Unread);
    };
    if let Some((id, set)) = &carried_set {
        bindings::set_kept(kept, id, *set);
    }
    let path = game_dir(loader).join(bindings::FILE);
    crate::base::frame_writes::write(
        &path,
        &bindings::render(&entries, kept),
        crate::base::frame_writes::whole,
    )?;
    Ok(())
}

/// Why a key was not saved.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SetKeyError {
    /// The file did not read when the game started; writing would lose it.
    #[error("{} did not read when the game started", bindings::FILE)]
    Unread,
    /// Windows refused the write.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
