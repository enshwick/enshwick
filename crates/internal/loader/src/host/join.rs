//! The mods a server's join needs, and the join values it applies to a client.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use enshwick::abi::{Event, Level, ManifestFlags};

use crate::host::loader;
use crate::locks::Relock;

/// The running join mods as the join check counts them, `(id, version)`:
/// read each time, since a mod may turn off while the game runs.
pub(crate) fn join_mods() -> Vec<(String, String)> {
    loader().map_or_else(Vec::new, |loader| {
        loader
            .mods
            .iter()
            .filter(|m| m.joins())
            .map(|m| (m.id.clone(), m.join_version()))
            .collect()
    })
}

/// The join mods this process loaded but turned off since: a server that
/// runs one cannot be joined until the game starts again.
pub(crate) fn join_mods_off() -> Vec<String> {
    loader().map_or_else(Vec::new, |loader| {
        loader
            .mods
            .iter()
            .filter(|m| {
                m.off.load(Ordering::Acquire) && m.manifest.flags.contains(ManifestFlags::JOIN)
            })
            .map(|m| m.id.clone())
            .collect()
    })
}

/// The join values of the running join mods, by id, as their settings files
/// hold them.
pub(crate) fn join_values() -> BTreeMap<String, String> {
    let Some(loader) = loader() else {
        return BTreeMap::new();
    };
    loader
        .mods
        .iter()
        .filter(|m| m.joins())
        .filter_map(|m| Some((m.id.clone(), m.synced.relock().clone()?)))
        .collect()
}

/// On a client joining a server: makes `values`, the server's join values by
/// mod id, the ones each join mod runs with, and raises
/// [`Event::SETTINGS_CHANGED`] for every mod whose values change, so its
/// handler reads its settings again. A mod the server sends none for runs
/// with its file's.
/// On a client, takes the joined server's new join values of mod `id`, which
/// it pushed after a change, and tells the mod.
pub(crate) fn apply_server_value(id: &str, values: &str) {
    // A hosting client's own server: its file's values are the server's.
    let Some(loader) = loader().filter(|_| !crate::world::hosting()) else {
        return;
    };
    let Some((index, record)) = loader
        .mods
        .iter()
        .enumerate()
        .find(|(_, m)| m.id == id && m.joins())
    else {
        return;
    };
    let now = Some(values.to_owned());
    {
        let mut held = record.server_synced.relock();
        if *held == now {
            return;
        }
        held.clone_from(&now);
    }
    crate::log::write(
        Level::INFO,
        &record.id,
        &format!("the server changed its join values: {}", shown(values)),
    );
    loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
}

/// Join values a server sent, as a log line shows them: on one line, cut
/// at [`SHOWN_VALUES`] characters, so a server writes no lines of its own
/// into the log.
pub(crate) fn shown(values: &str) -> String {
    let mut shown = String::with_capacity(values.len().min(SHOWN_VALUES));
    for c in values.chars().take(SHOWN_VALUES) {
        if c.is_control() {
            shown.extend(c.escape_default());
        } else {
            shown.push(c);
        }
    }
    if values.chars().nth(SHOWN_VALUES).is_some() {
        shown.push_str("...");
    }
    shown
}

/// The most characters of a server's join values a log line shows.
pub(crate) const SHOWN_VALUES: usize = 512;

pub(crate) fn apply_join_values(values: &BTreeMap<String, String>) {
    let Some(loader) = loader() else {
        return;
    };
    let current: Vec<(String, Option<String>)> = loader
        .mods
        .iter()
        .map(|m| {
            let held = m.server_synced.relock().clone();
            (
                if m.joins() {
                    m.id.clone()
                } else {
                    String::new()
                },
                held,
            )
        })
        .collect();
    let changes = crate::join::changes(
        current
            .iter()
            .map(|(id, held)| (id.as_str(), held.as_deref())),
        values,
    );
    for (index, now) in changes {
        let Some(record) = loader.mods.get(index) else {
            continue;
        };
        {
            let mut held = record.server_synced.relock();
            held.clone_from(&now);
        }
        let line = match &now {
            Some(values) => format!("the server's join values apply: {}", shown(values)),
            None => "the settings file's join values apply again".to_owned(),
        };
        crate::log::write(Level::INFO, &record.id, &line);
        loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
    }
}

/// One of the server's join mods the last refused join found missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Missing {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) version: &'static str,
    pub(crate) file: &'static str,
    pub(crate) page: &'static str,
}

/// The server's join mods the last refused join found missing.
pub(crate) static JOIN_MISSING: Mutex<Vec<Missing>> = Mutex::new(Vec::new());

/// Every string [`HostApi::join_missing`] has handed out, kept for the process.
pub(crate) static KEPT: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `text` kept for the process; the same text is kept once.
pub(crate) fn keep(text: &str) -> &'static str {
    let mut kept = KEPT.relock();
    if let Some(&held) = kept.iter().find(|held| **held == text) {
        return held;
    }
    let held: &'static str = Box::leak(text.into());
    kept.push(held);
    held
}

/// One of the server's join mods the join check found missing on this
/// client, as the server advertises it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wanted {
    /// The mod id.
    pub(crate) id: String,
    /// The name players read; empty for none.
    pub(crate) name: String,
    /// The server's version.
    pub(crate) version: String,
    /// The mod's homepage; empty for none.
    pub(crate) page: String,
}

/// On a client the join check kept out: records `missing`, the server's
/// join mods this client lacks or runs in another version, and raises
/// [`Event::JOIN_REFUSED`].
pub(crate) fn report_join_missing(missing: &[Wanted]) {
    let Some(loader) = loader() else {
        return;
    };
    let entries = missing
        .iter()
        .map(|wanted| {
            let Wanted {
                id,
                name,
                version,
                page,
            } = wanted;
            let ours = loader.mods.iter().find(|m| &m.id == id);
            let file = ours.map_or("", |m| m.file.as_str());
            Missing {
                id: keep(id),
                name: keep(name),
                version: keep(version),
                file: keep(file),
                page: keep(page),
            }
        })
        .collect();
    {
        let mut held = JOIN_MISSING.relock();
        *held = entries;
    }
    loader.emit(Event::JOIN_REFUSED, None, core::ptr::null());
}

/// Turns off the loaded mod `id`, if there is one, with `reason`.
pub(crate) fn turn_off_mod(id: &str, reason: &str) {
    if let Some(loader) = loader()
        && let Some(index) = loader.mods.iter().position(|m| m.id == id)
    {
        loader.turn_off(index, reason);
    }
}
