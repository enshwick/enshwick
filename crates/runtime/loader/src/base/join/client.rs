//! The mods a server's join needs, the client's mods a world does not run,
//! and the settings a server sends a client.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_core::abi::{Event, Level};

use super::missing::{ADVERTISED_BYTES, Missing, Wanted, listed};
use super::{Difference, Row};
use crate::base::wire::Drops;
use crate::engine::kept_text::{KeptText, keep};
use crate::engine::loader::{ModRecord, loader};
use crate::engine::mod_index::ModIndex;
use enshwick_base::Relock;

/// Why a mod is off while the client plays in a world that does not run it.
const OFF_FOR_WORLD: &str = "off for this world: it does not run this mod";
/// Why a pack is off once a world did not run it: its data went with it.
const OFF_UNTIL_RESTART: &str =
    "the world you joined does not run it; restart the game to use it again";

/// What the client knows of the joins it tried: the mods a refused join
/// found missing, as mods read them, the mods it turned off for the world it
/// plays in, and the difference the mismatch screen shows next.
struct Joins {
    missing: Mutex<Vec<Missing>>,
    /// Indices of the mods off for the world the client plays in.
    off_for_world: Mutex<Vec<ModIndex>>,
    /// The rows of the last refusal's or kick's difference, built once,
    /// until the menu shows them.
    shown: Mutex<Option<Vec<Row>>>,
    /// Lists of mods a server sent before a kick, counted for the log.
    kicks: Drops,
    /// Set while `shown` holds one, so the menu's frame takes no lock.
    pending: AtomicBool,
    /// The server-advertised strings the host table has handed out, kept
    /// for the process since mods may hold them.
    advertised: Mutex<KeptText>,
    /// The hash of a data pack's files by its id, named by the packs when
    /// they seal.
    pack_hash: OnceLock<fn(&str) -> Option<String>>,
}

static STATE: Joins = Joins {
    missing: Mutex::new(Vec::new()),
    off_for_world: Mutex::new(Vec::new()),
    shown: Mutex::new(None),
    kicks: Drops::new(),
    pending: AtomicBool::new(false),
    advertised: Mutex::new(KeptText::new(ADVERTISED_BYTES)),
    pack_hash: OnceLock::new(),
};

/// Names `hash`, which gives a data pack's files hash by its id, for the
/// join versions; the first one named stays.
pub(crate) fn on_pack_hash(hash: fn(&str) -> Option<String>) {
    let _first = STATE.pack_hash.set(hash);
}

/// The version of `record`'s mod the join check compares: a pack's carries
/// the hash of its files, so other files of one version differ.
pub(crate) fn join_version(record: &ModRecord) -> String {
    let hash = STATE
        .pack_hash
        .get()
        .filter(|_| record.pack)
        .and_then(|hash| hash(&record.id));
    match hash {
        Some(hash) => format!("{}+{hash}", record.version),
        None => record.version.clone(),
    }
}

/// The running mods of side both as the join check counts them,
/// `(id, version)`: read each time, since a mod may turn off while the game
/// runs.
pub(crate) fn join_mods() -> Vec<(String, String)> {
    loader().map_or_else(Vec::new, |loader| {
        loader
            .mods
            .iter()
            .filter(|m| m.joins())
            .map(|m| (m.id.clone(), join_version(m)))
            .collect()
    })
}

/// The mods of side both this process loaded but turned off until the game
/// restarts, `(id, version)`: a world that runs one cannot be joined before.
pub(crate) fn join_mods_off() -> Vec<(String, String)> {
    let Some(loader) = loader() else {
        return Vec::new();
    };
    let for_world = STATE.off_for_world.relock();
    loader
        .mods
        .iter()
        .enumerate()
        .filter(|(index, m)| {
            m.joins_when_on()
                && m.off.load(Ordering::Acquire)
                && !for_world.contains(&ModIndex::new(*index))
        })
        .map(|(_, m)| (m.id.clone(), join_version(m)))
        .collect()
}

/// Whether mod `index` is off for the world the client plays in, as its
/// server or host does not run it.
pub(crate) fn off_for_world(index: ModIndex) -> bool {
    STATE.off_for_world.relock().contains(&index)
}

/// Turns off the client's mod `id` for the world it joins, which does not
/// run it: until it plays in a world that does. A pack stays off until the
/// game restarts, since its data went when it turned off.
pub(crate) fn turn_off_for_world(id: &str) {
    let Some(loader) = loader() else {
        return;
    };
    let Some(index) = loader.mods.iter().position(|m| m.id == id) else {
        return;
    };
    let record = &loader.mods[index];
    if record.off.load(Ordering::Acquire) {
        return;
    }
    if record.pack {
        loader.turn_off(ModIndex::new(index), OFF_UNTIL_RESTART);
        return;
    }
    STATE.off_for_world.relock().push(ModIndex::new(index));
    loader.turn_off(ModIndex::new(index), OFF_FOR_WORLD);
}

/// Turns the mods off for the last world back on, before the client joins
/// or hosts another world, which decides again.
pub(crate) fn back_on() {
    let indices = core::mem::take(&mut *STATE.off_for_world.relock());
    let Some(loader) = loader() else {
        return;
    };
    for index in indices {
        loader.turn_back_on(index);
    }
}

/// On a client a server refused or kicked: keeps the rows of `difference`
/// for the mismatch screen, which the menu shows next; built here once, not
/// in each frame.
pub(crate) fn show_difference(difference: &Difference) {
    *STATE.shown.relock() = Some(difference.rows());
    STATE.pending.store(true, Ordering::Release);
}

/// The rows the menu shows next, taken: `None` when there are none.
pub(crate) fn take_difference() -> Option<Vec<Row>> {
    if !STATE.pending.swap(false, Ordering::AcqRel) {
        return None;
    }
    STATE.shown.relock().take()
}

/// Whether a difference waits for the menu, without a lock.
pub(crate) fn difference_waits() -> bool {
    STATE.pending.load(Ordering::Acquire)
}

/// On a client whose hello the server answered: it plays there, so a list
/// the server sent before is forgotten.
pub(crate) fn admitted() {
    STATE.pending.store(false, Ordering::Release);
    *STATE.shown.relock() = None;
}

/// On a client the server is about to kick: `list` is the server's mods of
/// side both, `complete` when whole. The difference to the client's mods
/// waits for the menu.
pub(crate) fn kicked_for_mods(list: &str, complete: bool) {
    let ours = crate::base::advertise::mod_list(&join_mods());
    let off = crate::base::advertise::mod_list(&join_mods_off());
    let difference = crate::base::join::difference(list, &ours, &off, complete);
    // A server may send the list with every hello it refuses: the first and
    // every 100th are logged.
    if STATE.kicks.add(1).is_some() {
        crate::log!(
            WARN,
            "the server does not let this game play: {} of its mods missing, {} in another version; the menu lists them ({})",
            difference.missing.len(),
            difference.mismatch.len(),
            crate::engine::errors::JOIN_MISSING
        );
    }
    if difference.refuses() {
        show_difference(&difference);
    }
}

/// On a client in another's world, takes `values`, the settings its server
/// sent for mod `id`, every file seen as one: the mod reads them as the
/// world's the first time, and as a change after.
pub(crate) fn apply_server_value(id: &str, values: &str) {
    // A hosting client's own server: its file's values are the server's.
    let Some(loader) = loader().filter(|_| !crate::engine::world::hosting()) else {
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
    let Some(settings) = crate::base::worlds::settings_of(record) else {
        return;
    };
    let first = {
        let mut held = settings.server_values.relock();
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
    crate::engine::log::write(
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
        let taken = crate::base::worlds::settings_of(record)
            .and_then(|settings| settings.server_values.relock().take());
        if taken.is_some() {
            crate::engine::log::write(
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

/// The server's mods the last refused join found missing.
pub(crate) fn join_missing() -> Vec<Missing> {
    STATE.missing.relock().clone()
}

/// On a client the join check kept out: records `missing`, the server's
/// mods of side both this client lacks or runs in another version, and
/// raises [`Event::JOIN_REFUSED`].
pub(crate) fn report_join_missing(missing: &[Wanted]) {
    let Some(loader) = loader() else {
        return;
    };
    let (entries, left_out) = listed(&mut STATE.advertised.relock(), missing, |id| {
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
            "{left_out} of the server's missing mods are not listed to mods: \
             the text kept from servers reached {ADVERTISED_BYTES} bytes"
        );
    }
    *STATE.missing.relock() = entries;
    loader.emit(Event::JOIN_REFUSED, None, core::ptr::null());
}

#[cfg(test)]
mod tests;
