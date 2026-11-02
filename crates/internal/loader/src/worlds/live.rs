//! The settings files of the running world on disk, and what each mod reads.

use std::path::Path;

use enshwick::abi::{Event, Level, Side};

use super::{DEFAULT_FILE, SettingsFile, current, set_current, world_file};
use crate::host::{Loader, ModRecord};
use crate::locks::Relock;

fn files(record: &ModRecord) -> Vec<SettingsFile> {
    record.settings_files.relock().clone()
}

/// The text of `path`; a file that does not read shows as absent.
fn read(path: &Path) -> Option<String> {
    enshwick::files::read_to_string_if_present(path)
        .ok()
        .flatten()
}

/// Writes `text` to `path` atomically.
///
/// # Errors
///
/// When the file cannot be written.
pub(crate) fn write_target(path: &Path, text: &str) -> std::io::Result<()> {
    enshwick::files::write_atomically(path, text)
}

/// What mod `record` reads of its settings file `file` now: the values its
/// world's server sent while this client plays in another's world, the
/// running world's file, or nothing while no world runs.
pub(crate) fn file_text(record: &ModRecord, file: &str) -> String {
    let files = files(record);
    if let Some(view) = record.server_values.relock().clone() {
        let others: Vec<&str> = files
            .iter()
            .map(|f| f.file.as_str())
            .filter(|f| *f != DEFAULT_FILE)
            .collect();
        return super::part(&view, file, &others);
    }
    let Some(key) = current() else {
        return String::new();
    };
    read(&world_file(&record.id, file, &key))
        .or_else(|| {
            files
                .into_iter()
                .find(|f| f.file == file)
                .map(|f| f.template)
        })
        .unwrap_or_default()
}

/// The text of every settings file of `record` seen as one: the running
/// world's, else the defaults.
pub(crate) fn show_text(record: &ModRecord) -> String {
    let files = files(record);
    let key = current();
    let texts: Vec<(&str, String)> = files
        .iter()
        .map(|f| {
            let text = key
                .as_deref()
                .and_then(|key| read(&world_file(&record.id, &f.file, key)))
                .unwrap_or_else(|| f.template.clone());
            (f.file.as_str(), text)
        })
        .collect();
    let parts: Vec<(&str, &str)> = texts.iter().map(|(f, t)| (*f, t.as_str())).collect();
    super::view_text(&parts)
}

/// What a server sends its players of `record`'s settings: every file seen
/// as one, without a secret; `None` for a mod without settings.
pub(crate) fn sent_view(record: &ModRecord) -> Option<String> {
    let schema = super::view_schema(&files(record))?;
    Some(crate::remote::redacted(&schema, &show_text(record)))
}

/// The settings of mod `index` changed while the world runs: the mod is
/// told, and a server's players get the new values.
pub(crate) fn changed(loader: &Loader, index: usize) {
    loader.emit(Event::SETTINGS_CHANGED, Some(index), core::ptr::null());
    let serving = loader.side == Side::SERVER || crate::world::hosting();
    if let Some(record) = loader.mods.get(index).filter(|m| serving && m.joins())
        && let Some(view) = sent_view(record)
    {
        crate::remote::push_sync(&record.id, &view);
    }
}

/// Makes or brings in line every settings file of `record` in the world
/// `key`.
fn prepare(record: &ModRecord, key: &str) {
    let log = |level, line: &str| crate::log::write(level, &record.id, line);
    let who = format!("{} {}", record.id, record.version);
    for file in files(record) {
        let path = world_file(&record.id, &file.file, key);
        let existing = match enshwick::files::read_to_string_if_present(&path) {
            Ok(existing) => existing,
            Err(e) => {
                log(
                    Level::WARN,
                    &format!("{} does not read and stays as it is: {e}", path.display()),
                );
                continue;
            }
        };
        let Some(existing) = existing else {
            match write_target(&path, &file.template) {
                Ok(()) => crate::watch::wrote(&path),
                Err(e) => log(Level::WARN, &format!("{} not written: {e}", path.display())),
            }
            continue;
        };
        let secrets: Vec<&str> = file
            .schema
            .fields
            .iter()
            .filter(|f| f.secret)
            .map(|f| f.key.as_str())
            .collect();
        let synced = match super::sync(&file.template, &existing, &file.file, &who, &secrets) {
            Ok(synced) => synced,
            Err(e) => {
                log(
                    Level::WARN,
                    &format!(
                        "{} is no TOML and stays as it is: {}",
                        path.display(),
                        e.message()
                    ),
                );
                continue;
            }
        };
        let Some(text) = synced.text else {
            continue;
        };
        let backup = super::backup_of(&path);
        if let Err(e) = std::fs::copy(&path, &backup) {
            log(
                Level::WARN,
                &format!(
                    "{} not kept, so the file stays as it is: {e}",
                    backup.display()
                ),
            );
            continue;
        }
        match write_target(&path, &text) {
            Ok(()) => {
                crate::watch::wrote(&path);
                for line in &synced.lines {
                    log(Level::INFO, line);
                }
            }
            Err(e) => log(Level::WARN, &format!("{} not written: {e}", path.display())),
        }
    }
}

/// A world loaded: every mod's settings files are made or brought in line,
/// and every mod reads them.
pub(crate) fn load(loader: &Loader) {
    let Some(key) = crate::saves::world_key(loader).map(str::to_owned) else {
        crate::log!(
            WARN,
            "world settings: the world has no key yet; mods run on their defaults"
        );
        return;
    };
    for record in &loader.mods {
        prepare(record, &key);
    }
    crate::log!(INFO, "world settings: world {key}");
    set_current(Some(key));
    crate::recipes::world_changed(loader);
    loader.emit(Event::SETTINGS_REPLACED, None, core::ptr::null());
}

/// The world stopped: every mod runs on its defaults again.
pub(crate) fn unload(loader: &Loader) {
    if current().is_none() {
        return;
    }
    set_current(None);
    crate::recipes::world_changed(loader);
    loader.emit(Event::SETTINGS_REPLACED, None, core::ptr::null());
}
