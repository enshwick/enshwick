//! The server's game settings: the difficulty and gameplay values its
//! configuration or the host's menu chose, read and changed while the world
//! runs.
//!
//! The server keeps one live `GameSettings` and every system reads it when
//! it runs, so a change takes effect on the next step. A change goes the
//! game's own way, as its admin command does: the loader copies the new
//! values in and raises `GameSettingsChangedEvent`, which clients copy into
//! their own world, and it also marks the settings for the server's next
//! info message, which every client takes too. The game validates no
//! change, so the loader refuses day and night lengths of zero or less,
//! which the clock divides by. The game writes no change to disk: the
//! configuration's values return with the next start.
//!
//! Fields mods keep are the loader's to keep: by reflected name, with the
//! world, in `mod-data/enshwick/worlds/<key>/game-settings.toml`. When a
//! world loads, or at the end of its first step where the load does not run
//! on the game thread, the loader takes the values it loaded with, then
//! writes the kept fields over them; a forget puts the loaded values back.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{Event, Status};

use crate::host::Loader;
use crate::locks::Relock;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    "server.game_settings",
    "server.game_settings_preset",
    "server.game_settings_resend",
];

/// The kept fields' file in a world's folder.
const FILE: &str = "game-settings.toml";

/// The preset the game names custom settings with.
const CUSTOM: u8 = 4;

/// Where the server keeps its settings, from the build table and
/// reflection.
#[derive(Debug)]
struct Layout {
    settings: usize,
    preset: usize,
    resend: usize,
    size: usize,
    day_length: usize,
    night_length: usize,
    event: u32,
    event_size: usize,
    event_settings: usize,
    /// Every field by name: where it starts and how many bytes it spans,
    /// up to the next field.
    fields: Vec<(String, usize, usize)>,
}

/// The kept fields of the running world.
#[derive(Debug, Default)]
struct Kept {
    key: Option<String>,
    fields: BTreeMap<String, Vec<u8>>,
    /// The settings the world loaded with.
    loaded: Option<Vec<u8>>,
}

static KEPT: Mutex<Kept> = Mutex::new(Kept {
    key: None,
    fields: BTreeMap::new(),
    loaded: None,
});
/// A world loaded and its first step has not ended yet.
static PENDING: AtomicBool = AtomicBool::new(false);

/// The kept fields' file, by field name and the bytes as hex.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
struct KeptFile {
    kept: BTreeMap<String, String>,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let offset = |ty: &str, field: &str| {
                reflection
                    .field_offset(ty, field)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let size_of = |ty: &str| {
                reflection
                    .type_info(ty)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())
            };
            let settings = "keen::ecs::GameSettings";
            let event = "keen::ecs::GameSettingsChangedEvent";
            let event_info = reflection.type_info(event).ok()?;
            let info = reflection.type_info(settings).ok()?;
            let total = usize::try_from(info.size()).ok()?;
            Some(Layout {
                fields: spans(
                    info.fields()
                        .iter()
                        .filter_map(|f| Some((f.name(), usize::try_from(f.offset()).ok()?))),
                    total,
                ),
                settings: loader.offset("server.game_settings")?,
                preset: loader.offset("server.game_settings_preset")?,
                resend: loader.offset("server.game_settings_resend")?,
                size: size_of(settings)?,
                day_length: offset(settings, "dayTimeDuration")?,
                night_length: offset(settings, "nightTimeDuration")?,
                event: crate::world::type_hash(event_info.va())?,
                event_size: usize::try_from(event_info.size()).ok()?,
                event_settings: offset(event, "newSettings")?,
            })
        })
        .as_ref()
}

/// The live settings, on the game thread.
fn live(loader: &Loader) -> Result<(&'static Layout, usize, usize), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let server = crate::world::server()?;
    // SAFETY: `server` comes from `crate::world::server`, so this runs on the
    // game thread that alone writes it; the table's `server.game_settings`
    // offset holds the pointer to its settings, set from its start.
    let settings: usize = unsafe { crate::memory::read(server, layout.settings) };
    if settings == 0 {
        return Err(Status::MISSING);
    }
    Ok((layout, server, settings))
}

/// Copies the live settings into `out`, which is their reflected size, and
/// returns the preset.
pub(crate) fn read(loader: &Loader, out: &mut [u8]) -> Result<u8, Status> {
    let (layout, server, settings) = live(loader)?;
    if out.len() != layout.size {
        return Err(Status::BAD_ARGS);
    }
    for (at, byte) in out.iter_mut().enumerate() {
        // SAFETY: `settings` is non-zero (checked by `live`) and the live
        // settings span their reflected size, `out.len()` (`out.len() !=
        // layout.size` returns first), so `at` is inside them; on the game
        // thread (checked by `live`) that alone writes them.
        *byte = unsafe { crate::memory::read(settings, at) };
    }
    // SAFETY: `server` comes from `live` on the game thread that alone writes
    // it; the table's `server.game_settings_preset` offset holds its preset
    // byte.
    Ok(unsafe { crate::memory::read(server, layout.preset) })
}

/// Replaces the live settings with `bytes`, their whole reflected layout,
/// and tells the clients.
pub(crate) fn write(loader: &Loader, bytes: &[u8]) -> Result<(), Status> {
    let (layout, server, settings) = live(loader)?;
    if bytes.len() != layout.size {
        return Err(Status::BAD_ARGS);
    }
    let length = |at: usize| {
        bytes
            .get(at..at + 8)
            .and_then(|b| b.try_into().ok())
            .map_or(0, i64::from_le_bytes)
    };
    if length(layout.day_length) <= 0 || length(layout.night_length) <= 0 {
        return Err(Status::BAD_ARGS);
    }
    for (at, &byte) in bytes.iter().enumerate() {
        // SAFETY: `settings` is non-zero (checked by `live`) and the live
        // settings span their reflected size, `bytes.len()` (`bytes.len() !=
        // layout.size` returns first), so `at` is inside them; `live` returns
        // only on the game thread at a quiet moment, where no system reads
        // them, as the game's admin command writes them; the day and night
        // lengths the clock divides by are positive (the length checks return
        // first).
        unsafe { crate::memory::write(settings, at, byte) };
    }
    // SAFETY: `server` comes from `live`, on the game thread at a quiet
    // moment; the table's `server.game_settings_preset` offset holds its
    // preset byte, and `CUSTOM` is the game's preset for custom settings.
    unsafe { crate::memory::write(server, layout.preset, CUSTOM) };
    // SAFETY: `server` comes from `live`, on the game thread at a quiet
    // moment; the table's `server.game_settings_resend` offset holds its
    // byte flag for the settings in its next info message.
    unsafe { crate::memory::write(server, layout.resend, 1u8) };
    let mut event = vec![0u8; layout.event_size];
    if let Some(out) = event.get_mut(layout.event_settings..layout.event_settings + bytes.len()) {
        out.copy_from_slice(bytes);
        if let Err(status) = crate::emits::push(loader, layout.event, &event) {
            crate::log!(
                DEBUG,
                "game settings changed; the event was refused ({}), the info message carries them",
                status.0
            );
        }
    }
    Ok(())
}

/// Each field's span, up to the next field or the end, from names and
/// offsets in any order.
fn spans<'a>(
    fields: impl Iterator<Item = (&'a str, usize)>,
    size: usize,
) -> Vec<(String, usize, usize)> {
    let mut fields: Vec<(String, usize)> = fields
        .filter(|&(_, at)| at < size)
        .map(|(name, at)| (name.to_owned(), at))
        .collect();
    fields.sort_by_key(|&(_, at)| at);
    let ends: Vec<usize> = fields
        .iter()
        .skip(1)
        .map(|&(_, at)| at)
        .chain(core::iter::once(size))
        .collect();
    fields
        .into_iter()
        .zip(ends)
        .map(|((name, at), end)| (name, at, end - at))
        .collect()
}

/// The fields of `layout` that a set byte of `mask` falls in, with their
/// bytes from `data`.
fn masked(layout: &Layout, data: &[u8], mask: &[u8]) -> Vec<(String, Vec<u8>)> {
    layout
        .fields
        .iter()
        .filter(|&&(_, at, len)| {
            mask.get(at..at + len)
                .is_some_and(|m| m.iter().any(|&b| b != 0))
        })
        .filter_map(|(name, at, len)| Some((name.clone(), data.get(*at..at + len)?.to_vec())))
        .collect()
}

/// `fields` written over `bytes`, skipping names or sizes this build does
/// not have; how many were written.
fn overlay(layout: &Layout, bytes: &mut [u8], fields: &BTreeMap<String, Vec<u8>>) -> usize {
    let mut applied = 0;
    for (name, at, len) in &layout.fields {
        if let Some(value) = fields.get(name).filter(|v| v.len() == *len)
            && let Some(out) = bytes.get_mut(*at..at + len)
        {
            out.copy_from_slice(value);
            applied += 1;
        }
    }
    applied
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|&b| [DIGITS[usize::from(b >> 4)], DIGITS[usize::from(b & 0xf)]])
        .map(char::from)
        .collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// Reads the kept fields of world `key`; none for a missing or broken file.
fn read_kept(loader: &Loader, key: &str) -> BTreeMap<String, Vec<u8>> {
    let path = file(loader, key);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    match toml::from_str::<KeptFile>(&text) {
        Ok(kept) => kept
            .kept
            .into_iter()
            .filter_map(|(name, value)| Some((name, unhex(&value)?)))
            .collect(),
        Err(error) => {
            crate::log!(WARN, "game settings: {} not read: {error}", path.display());
            BTreeMap::new()
        }
    }
}

/// Writes the kept fields of world `key`, removing the file when none are.
fn write_kept(loader: &Loader, key: &str, fields: &BTreeMap<String, Vec<u8>>) {
    let path = file(loader, key);
    let written = if fields.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        let kept = KeptFile {
            kept: fields.iter().map(|(n, v)| (n.clone(), hex(v))).collect(),
        };
        let text = format!(
            "# Game settings mods keep for this world, applied each time it loads:\n\
             # each field by its reflected name, its bytes as hex.\n{}",
            toml::to_string(&kept).unwrap_or_default()
        );
        path.parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| {
                let fresh = path.with_extension("tmp");
                std::fs::write(&fresh, text)?;
                std::fs::rename(&fresh, &path)
            })
    };
    if let Err(error) = written {
        crate::log!(
            WARN,
            "game settings: {} not written: {error}",
            path.display()
        );
    }
}

/// Follows the server's world: reads the kept fields when one loads, for
/// the end of its first step, and forgets them when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    match event {
        Event::WORLD_LOADED => {
            let key = crate::saves::world_key(loader).map(str::to_owned);
            let fields = key
                .as_deref()
                .map(|key| read_kept(loader, key))
                .unwrap_or_default();
            *KEPT.relock() = Kept {
                key,
                fields,
                loaded: None,
            };
            PENDING.store(true, Ordering::Release);
            settle(loader);
        }
        Event::SHUTDOWN_STARTED => {
            PENDING.store(false, Ordering::Release);
            *KEPT.relock() = Kept::default();
        }
        _ => {}
    }
}

/// At the end of a loaded world's first step: takes the values it loaded
/// with and writes the kept fields over them.
pub(crate) fn after_step(loader: &Loader) {
    if !PENDING.load(Ordering::Acquire) {
        return;
    }
    settle(loader);
}

#[cold]
fn settle(loader: &Loader) {
    let Some(layout) = layout(loader) else {
        PENDING.store(false, Ordering::Release);
        return;
    };
    let mut bytes = vec![0u8; layout.size];
    if read(loader, &mut bytes).is_err() {
        return;
    }
    PENDING.store(false, Ordering::Release);
    let mut kept = KEPT.relock();
    kept.loaded = Some(bytes.clone());
    if kept.fields.is_empty() {
        return;
    }
    let applied = overlay(layout, &mut bytes, &kept.fields);
    match write(loader, &bytes) {
        Ok(()) => crate::log!(INFO, "game settings: {applied} kept fields applied"),
        Err(status) => crate::log!(WARN, "game settings: kept fields refused ({})", status.0),
    }
}

/// Writes `data` as [`write`] does and keeps the fields `mask` marks with
/// the running world.
pub(crate) fn keep(loader: &Loader, data: &[u8], mask: &[u8]) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    if mask.len() != data.len() {
        return Err(Status::BAD_ARGS);
    }
    let mut kept = KEPT.relock();
    let key = kept.key.clone().ok_or(Status::MISSING)?;
    write(loader, data)?;
    let fields = masked(layout, data, mask);
    if fields.is_empty() {
        return Ok(());
    }
    kept.fields.extend(fields);
    write_kept(loader, &key, &kept.fields);
    Ok(())
}

/// Forgets the running world's kept fields and puts back the values it
/// loaded with.
pub(crate) fn forget(loader: &Loader) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let mut kept = KEPT.relock();
    let key = kept.key.clone().ok_or(Status::MISSING)?;
    if kept.fields.is_empty() {
        return Ok(());
    }
    if let Some(loaded) = kept.loaded.clone() {
        let mut bytes = vec![0u8; layout.size];
        read(loader, &mut bytes)?;
        let back: BTreeMap<String, Vec<u8>> = layout
            .fields
            .iter()
            .filter(|(name, ..)| kept.fields.contains_key(name))
            .filter_map(|(name, at, len)| Some((name.clone(), loaded.get(*at..at + len)?.to_vec())))
            .collect();
        overlay(layout, &mut bytes, &back);
        write(loader, &bytes)?;
    }
    kept.fields.clear();
    write_kept(loader, &key, &kept.fields);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            settings: 0,
            preset: 0,
            resend: 0,
            size: 12,
            day_length: 0,
            night_length: 0,
            event: 0,
            event_size: 0,
            event_settings: 0,
            fields: spans([("b", 4), ("a", 0), ("c", 9)].into_iter(), 12),
        }
    }

    #[test]
    fn fields_span_to_the_next_one() {
        assert_eq!(
            layout().fields,
            vec![("a".into(), 0, 4), ("b".into(), 4, 5), ("c".into(), 9, 3)]
        );
    }

    #[test]
    fn a_mask_keeps_whole_fields_that_lay_back_over_other_values() {
        let layout = layout();
        let data: Vec<u8> = (0..12).collect();
        let mut mask = vec![0u8; 12];
        mask[5] = 0xff;
        let fields: BTreeMap<String, Vec<u8>> = masked(&layout, &data, &mask).into_iter().collect();
        assert_eq!(fields.get("b"), Some(&vec![4, 5, 6, 7, 8]));
        assert_eq!(fields.len(), 1);
        let mut other = vec![0u8; 12];
        assert_eq!(overlay(&layout, &mut other, &fields), 1);
        assert_eq!(other, vec![0, 0, 0, 0, 4, 5, 6, 7, 8, 0, 0, 0]);
    }

    #[test]
    fn hex_round_trips_and_refuses_odd_text() {
        assert_eq!(unhex(&hex(&[0, 0x80, 0x3f])), Some(vec![0, 0x80, 0x3f]));
        assert_eq!(unhex("abc"), None);
        assert_eq!(unhex("zz"), None);
    }
}
