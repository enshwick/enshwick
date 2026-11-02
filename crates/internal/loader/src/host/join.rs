//! The mods a server's join needs, and the settings it sends a client.

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use enshwick::abi::{Event, Level, ManifestFlags};

use crate::host::loader;
use crate::join::missing::{ADVERTISED_BYTES, Missing, Wanted, listed};
use crate::kept_text::KeptText;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

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

/// On a client in another's world, takes `values`, the settings its server
/// sent for mod `id`, every file seen as one: the mod reads them as the
/// world's the first time, and as a change after.
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
    let first = {
        let mut held = record.server_values.relock();
        if held.as_deref() == Some(values) {
            return;
        }
        held.replace(values.to_owned()).is_none()
    };
    let (line, event) = if first {
        ("the server's settings apply", Event::SETTINGS_REPLACED)
    } else {
        ("the server changed its settings", Event::SETTINGS_CHANGED)
    };
    crate::log::write(
        Level::INFO,
        &record.id,
        &format!("{line}: {}", shown(values)),
    );
    loader.emit(event, Some(index), core::ptr::null());
}

/// On a client leaving another's world or joining one: forgets every value a
/// server sent, and each mod that had some runs on its defaults again.
pub(crate) fn forget_server_values() {
    let Some(loader) = loader() else {
        return;
    };
    for (index, record) in loader.mods.iter().enumerate() {
        if record.server_values.relock().take().is_some() {
            crate::log::write(
                Level::INFO,
                &record.id,
                "the server's settings no longer apply",
            );
            loader.emit(Event::SETTINGS_REPLACED, Some(index), core::ptr::null());
        }
    }
}

/// Settings a server sent, as a log line shows them: on one line, cut
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

/// The most characters of a server's settings a log line shows.
pub(crate) const SHOWN_VALUES: usize = 512;

/// The server's join mods the last refused join found missing.
pub(crate) static JOIN_MISSING: Mutex<Vec<Missing>> = Mutex::new(Vec::new());

/// The loader's own strings the host table has handed out.
static KEPT: Mutex<KeptText> = Mutex::new(KeptText::new(usize::MAX));
/// The server-advertised strings the host table has handed out.
static ADVERTISED: Mutex<KeptText> = Mutex::new(KeptText::new(ADVERTISED_BYTES));

/// `text` kept for the process; the same text is kept once.
pub(crate) fn keep(text: &str) -> &'static str {
    KEPT.relock().keep(text).unwrap_or_default()
}

/// On a client the join check kept out: records `missing`, the server's
/// join mods this client lacks or runs in another version, and raises
/// [`Event::JOIN_REFUSED`].
pub(crate) fn report_join_missing(missing: &[Wanted]) {
    let Some(loader) = loader() else {
        return;
    };
    let (entries, left_out) = listed(&mut ADVERTISED.relock(), missing, |id| {
        keep(
            loader
                .mods
                .iter()
                .find(|m| m.id == id)
                .map_or("", |m| m.file.as_str()),
        )
    });
    if left_out > 0 {
        crate::log!(
            WARN,
            "{left_out} of the server's missing join mods are not listed to mods: \
             the text kept from servers reached {ADVERTISED_BYTES} bytes"
        );
    }
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
        loader.turn_off(ModIndex::new(index), reason);
    }
}
