//! Mod settings per world.
//!
//! A mod's settings file, `mod-configs/<id>.toml`, holds each player's own
//! settings (the `client` stage) and is the template every world starts
//! from. The other settings live with the world, in the mod's data folder:
//! `mod-data/<id>/worlds/<key>/settings.toml`, made from the template when a
//! world first loads, never inside the game's save. While a world runs in
//! this process, a mod reads that world's settings with its own `client`
//! settings over them; with no world, the template.
//!
//! Changes to world settings made before any world runs, as on the screen
//! that creates or starts a world, wait in `mod-data/<id>/worlds/next.toml`
//! and go into the next world that loads.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use enshwick_loader_api::config::{Schema, Scope, Stage};
use toml_edit::DocumentMut;

/// The key of the world this process runs, while one runs.
static CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// The key of the world this process runs, while one runs.
#[must_use]
pub fn current() -> Option<String> {
    CURRENT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Sets the world this process runs, or none.
pub fn set_current(key: Option<String>) {
    *CURRENT.lock().unwrap_or_else(PoisonError::into_inner) = key;
}

/// Where a world's settings of mod `id` live.
#[must_use]
pub fn world_file(data_dir: &Path, id: &str, key: &str) -> PathBuf {
    data_dir
        .join(id)
        .join("worlds")
        .join(key)
        .join("settings.toml")
}

/// Where changes for the next world wait.
#[must_use]
pub fn next_file(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join(id).join("worlds").join("next.toml")
}

/// Where a change of a setting at `stage` goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Destination {
    /// The settings file: each player's own settings, and the template.
    Global,
    /// The settings of the world with this key.
    World(String),
    /// The changes waiting for the next world.
    Next,
}

/// Where a change of a setting at `stage` goes while the world `current`
/// runs, or none.
#[must_use]
pub fn destination(stage: Stage, current: Option<&str>) -> Destination {
    match (stage, current) {
        (stage, _) if stage.scope() == Scope::User => Destination::Global,
        (_, Some(key)) => Destination::World(key.to_owned()),
        (_, None) => Destination::Next,
    }
}

/// The keys of `schema`'s settings kept in the settings file for every
/// world: each player's own, and the ones read as the game starts.
#[must_use]
pub fn client_keys(schema: &Schema) -> Vec<&str> {
    schema
        .fields
        .iter()
        .filter(|f| f.scope == Scope::User)
        .map(|f| f.key.as_str())
        .collect()
}

/// `base` with every key of `over` set from it, `base`'s comments kept; keys
/// in `only` alone when it is given. A text that is no TOML leaves `base`.
#[must_use]
pub fn overlay(base: &str, over: &str, only: Option<&[&str]>) -> String {
    let (Ok(mut doc), Ok(over)) = (base.parse::<DocumentMut>(), over.parse::<DocumentMut>()) else {
        return base.to_owned();
    };
    for (key, item) in over.iter() {
        if only.is_some_and(|keys| !keys.contains(&key)) {
            continue;
        }
        let Some(value) = item.as_value() else {
            continue;
        };
        match doc.get_mut(key).and_then(toml_edit::Item::as_value_mut) {
            Some(old) => {
                let mut new = value.clone();
                *new.decor_mut() = old.decor().clone();
                *old = new;
            }
            None => {
                doc.insert(key, toml_edit::Item::Value(value.clone()));
            }
        }
    }
    doc.to_string()
}

/// What a mod reads: the world's settings `world` with the player's own
/// `client` settings from the settings file `global` over them.
#[must_use]
pub fn compose(world: &str, global: &str, schema: &Schema) -> String {
    overlay(world, global, Some(&client_keys(schema)))
}

/// The settings a world starts with, or a world's settings after the changes
/// waiting for the next world: `None` when nothing needs writing.
#[must_use]
pub fn prepare(template: &str, existing: Option<&str>, next: Option<&str>) -> Option<String> {
    match (existing, next) {
        (Some(world), None) => {
            let filled = with_new_keys(world, template);
            (filled != world).then_some(filled)
        }
        (Some(world), Some(next)) => Some(overlay(&with_new_keys(world, template), next, None)),
        (None, Some(next)) => Some(overlay(template, next, None)),
        (None, None) => Some(template.to_owned()),
    }
}

/// `world` with the keys `template` has and it lacks, as a newer version of
/// the mod adds them, at their template values.
#[must_use]
pub fn with_new_keys(world: &str, template: &str) -> String {
    let (Ok(mut doc), Ok(template)) = (
        world.parse::<DocumentMut>(),
        template.parse::<DocumentMut>(),
    ) else {
        return world.to_owned();
    };
    let mut added = false;
    for (key, item) in template.iter() {
        if doc.get(key).is_none()
            && let Some(value) = item.as_value()
        {
            doc.insert(key, toml_edit::Item::Value(value.clone()));
            added = true;
        }
    }
    if added {
        doc.to_string()
    } else {
        world.to_owned()
    }
}

#[cfg(windows)]
pub(crate) use live::{effective, load, show_text, target_text, unload, write_target};

#[cfg(windows)]
mod live {
    use std::path::PathBuf;

    use enshwick_loader_api::abi::Event;
    use enshwick_loader_api::config::Schema;

    use super::{Destination, compose, current, next_file, prepare, set_current, world_file};
    use crate::host::{Loader, ModRecord};

    fn schema(record: &ModRecord) -> Option<Schema> {
        record.schema.lock().ok().and_then(|s| s.clone())
    }

    fn global_path(loader: &Loader, record: &ModRecord) -> PathBuf {
        loader.configs_dir.join(format!("{}.toml", record.id))
    }

    fn read(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    /// Writes `text` to `path` through a temporary file and a rename.
    fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let staged = path.with_extension("toml.new");
        std::fs::write(&staged, text)?;
        std::fs::rename(&staged, path)
    }

    /// What mod `record` reads now: the running world's settings with its
    /// own `client` settings over them, else the settings file `global`.
    pub(crate) fn effective(loader: &Loader, record: &ModRecord, global: &str) -> String {
        let (Some(key), Some(schema)) = (current(), schema(record)) else {
            return global.to_owned();
        };
        match read(&world_file(&loader.data_dir, &record.id, &key)) {
            Some(world) => compose(&world, global, &schema),
            None => global.to_owned(),
        }
    }

    /// The text shown for the world settings of `record` on this process:
    /// the running world's, else the template with the changes waiting for
    /// the next world.
    pub(crate) fn show_text(loader: &Loader, record: &ModRecord) -> String {
        let global = read(&global_path(loader, record)).unwrap_or_default();
        if let Some(key) = current() {
            return read(&world_file(&loader.data_dir, &record.id, &key)).unwrap_or(global);
        }
        match read(&next_file(&loader.data_dir, &record.id)) {
            Some(next) => super::overlay(&global, &next, None),
            None => global,
        }
    }

    /// The file a change at `destination` goes to and its text now.
    pub(crate) fn target_text(
        loader: &Loader,
        record: &ModRecord,
        destination: &Destination,
    ) -> (PathBuf, String) {
        let path = match destination {
            Destination::Global => global_path(loader, record),
            Destination::World(key) => world_file(&loader.data_dir, &record.id, key),
            Destination::Next => next_file(&loader.data_dir, &record.id),
        };
        let text = read(&path).unwrap_or_default();
        (path, text)
    }

    /// Writes a change's new text to its file.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub(crate) fn write_target(path: &std::path::Path, text: &str) -> std::io::Result<()> {
        write(path, text)
    }

    /// A world loaded: its settings files are made or updated for every mod
    /// that describes its settings, and every mod reads them, the settings
    /// that wait for a start too.
    pub(crate) fn load(loader: &Loader) {
        let Some(key) = crate::saves::world_key(loader).map(str::to_owned) else {
            crate::log!(
                WARN,
                "world settings: the world has no key yet; the settings files apply"
            );
            return;
        };
        crate::migrations::world_loaded(loader, &key);
        for record in loader.mods.iter().filter(|m| schema(m).is_some()) {
            let global = read(&global_path(loader, record)).unwrap_or_default();
            let world_path = world_file(&loader.data_dir, &record.id, &key);
            let next_path = next_file(&loader.data_dir, &record.id);
            let next = read(&next_path);
            if let Some(text) = prepare(&global, read(&world_path).as_deref(), next.as_deref()) {
                if let Err(e) = write(&world_path, &text) {
                    crate::log::write(
                        enshwick_loader_api::abi::Level::WARN,
                        &record.id,
                        &format!("world settings not written: {e}"),
                    );
                    continue;
                }
                crate::watch::wrote(&world_path);
            }
            if next.is_some() {
                let _gone = std::fs::remove_file(&next_path);
            }
        }
        crate::log!(INFO, "world settings: world {key}");
        set_current(Some(key));
        crate::recipes::world_changed(loader);
        loader.emit(Event::WORLD_SETTINGS, None, core::ptr::null());
    }

    /// The world stopped: every mod reads its settings file again.
    pub(crate) fn unload(loader: &Loader) {
        if current().is_none() {
            return;
        }
        set_current(None);
        crate::recipes::world_changed(loader);
        loader.emit(Event::WORLD_SETTINGS, None, core::ptr::null());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick_loader_api::config::Field;

    fn schema() -> Schema {
        Schema::new(
            "",
            "config",
            vec![
                Field::new("limit", Stage::Startup, &4),
                Field::new("labels", Stage::Client, &true),
            ],
        )
    }

    #[test]
    fn a_world_starts_from_the_template_and_takes_waiting_changes() {
        let template = "# Limit.\nlimit = 4\nlabels = true\n";
        assert_eq!(prepare(template, None, None).as_deref(), Some(template));
        assert_eq!(
            prepare(template, Some("limit = 9\nlabels = true\n"), None),
            None
        );
        let started = prepare(template, None, Some("limit = 6\n")).unwrap();
        assert!(started.contains("# Limit.\nlimit = 6"), "{started}");
        let later = prepare(
            template,
            Some("limit = 9 # mine\nlabels = true\n"),
            Some("limit = 2\n"),
        )
        .unwrap();
        assert_eq!(later, "limit = 2 # mine\nlabels = true\n");
        let newer = prepare(template, Some("limit = 9\n"), None).unwrap();
        assert_eq!(newer, "limit = 9\nlabels = true\n");
    }

    #[test]
    fn a_mod_reads_its_world_with_the_players_own_settings_over_it() {
        let world = "limit = 9\nlabels = true\n";
        let global = "limit = 4\nlabels = false\n";
        assert_eq!(
            compose(world, global, &schema()),
            "limit = 9\nlabels = false\n"
        );
        assert_eq!(overlay("a = 1\n", "not toml [", None), "a = 1\n");
    }

    #[test]
    fn changes_go_by_scope_and_world() {
        assert_eq!(destination(Stage::Client, Some("w")), Destination::Global);
        assert_eq!(destination(Stage::Restart, Some("w")), Destination::Global);
        assert_eq!(
            destination(Stage::Runtime, Some("w")),
            Destination::World("w".into())
        );
        assert_eq!(destination(Stage::Sync, None), Destination::Next);
        assert_eq!(client_keys(&schema()), ["labels"]);
        let file = world_file(Path::new("d"), "safes", "w-1");
        assert!(file.ends_with("safes/worlds/w-1/settings.toml"));
        assert!(next_file(Path::new("d"), "safes").ends_with("safes/worlds/next.toml"));
    }
}
