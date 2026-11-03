//! The mods a server's join needs, the client's mods a world does not run,
//! and the settings a server sends a client.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::{Event, Level};

use crate::host::loader;
use crate::join::Difference;
use crate::join::missing::{ADVERTISED_BYTES, Missing, Wanted, listed};
use crate::kept_text::KeptText;
use crate::locks::Relock;
use crate::mod_index::ModIndex;

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
    /// The difference of the last refusal or kick, until the menu shows it.
    shown: Mutex<Option<Difference>>,
    /// Set while `shown` holds one, so the menu's frame takes no lock.
    pending: AtomicBool,
}

static JOINS: Joins = Joins {
    missing: Mutex::new(Vec::new()),
    off_for_world: Mutex::new(Vec::new()),
    shown: Mutex::new(None),
    pending: AtomicBool::new(false),
};

/// The running mods of side both as the join check counts them,
/// `(id, version)`: read each time, since a mod may turn off while the game
/// runs.
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

/// The mods of side both this process loaded but turned off until the game
/// restarts, `(id, version)`: a world that runs one cannot be joined before.
pub(crate) fn join_mods_off() -> Vec<(String, String)> {
    let Some(loader) = loader() else {
        return Vec::new();
    };
    let for_world = JOINS.off_for_world.relock();
    loader
        .mods
        .iter()
        .enumerate()
        .filter(|(index, m)| {
            m.joins_when_on()
                && m.off.load(Ordering::Acquire)
                && !for_world.contains(&ModIndex::new(*index))
        })
        .map(|(_, m)| (m.id.clone(), m.join_version()))
        .collect()
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
    JOINS.off_for_world.relock().push(ModIndex::new(index));
    loader.turn_off(ModIndex::new(index), OFF_FOR_WORLD);
}

/// Turns the mods off for the last world back on, before the client joins
/// or hosts another world, which decides again.
pub(crate) fn back_on() {
    let indices = core::mem::take(&mut *JOINS.off_for_world.relock());
    let Some(loader) = loader() else {
        return;
    };
    for index in indices {
        loader.turn_back_on(index);
    }
}

/// On a client a server refused or kicked: keeps `difference` for the
/// mismatch screen, which the menu shows next.
pub(crate) fn show_difference(difference: Difference) {
    *JOINS.shown.relock() = Some(difference);
    JOINS.pending.store(true, Ordering::Release);
}

/// The difference the menu shows next, taken: `None` when there is none.
pub(crate) fn take_difference() -> Option<Difference> {
    if !JOINS.pending.swap(false, Ordering::AcqRel) {
        return None;
    }
    JOINS.shown.relock().take()
}

/// Whether a difference waits for the menu, without a lock.
pub(crate) fn difference_waits() -> bool {
    JOINS.pending.load(Ordering::Acquire)
}

/// On a client whose hello the server answered: it plays there, so a list
/// the server sent before is forgotten.
pub(crate) fn admitted() {
    JOINS.pending.store(false, Ordering::Release);
    *JOINS.shown.relock() = None;
}

/// On a client the server is about to kick: `list` is the server's mods of
/// side both, `complete` when whole. The difference to the client's mods
/// waits for the menu.
pub(crate) fn kicked_for_mods(list: &str, complete: bool) {
    let ours = crate::advertise::mod_list(&join_mods());
    let off = crate::advertise::mod_list(&join_mods_off());
    let difference = crate::join::difference(list, &ours, &off, complete);
    crate::log!(
        WARN,
        "the server does not let this game play: {} of its mods missing, {} in another version; the menu lists them ({})",
        difference.missing.len(),
        difference.mismatch.len(),
        crate::errors::JOIN_MISSING
    );
    if difference.refuses() {
        show_difference(difference);
    }
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

/// The server's mods the last refused join found missing.
pub(crate) fn join_missing() -> Vec<Missing> {
    JOINS.missing.relock().clone()
}

/// The loader's own strings the host table has handed out.
static KEPT: Mutex<KeptText> = Mutex::new(KeptText::new(usize::MAX));
/// The server-advertised strings the host table has handed out.
static ADVERTISED: Mutex<KeptText> = Mutex::new(KeptText::new(ADVERTISED_BYTES));

/// `text` kept for the process; the same text is kept once.
pub(crate) fn keep(text: &str) -> &'static str {
    KEPT.relock().keep(text).unwrap_or_default()
}

/// On a client the join check kept out: records `missing`, the server's
/// mods of side both this client lacks or runs in another version, and
/// raises [`Event::JOIN_REFUSED`].
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
            "{left_out} of the server's missing mods are not listed to mods: \
             the text kept from servers reached {ADVERTISED_BYTES} bytes"
        );
    }
    *JOINS.missing.relock() = entries;
    loader.emit(Event::JOIN_REFUSED, None, core::ptr::null());
}
