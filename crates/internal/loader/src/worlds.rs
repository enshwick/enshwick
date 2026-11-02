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
//! A change to a world setting while no world runs goes, on a game client
//! as from the main menu, into the template: the defaults of worlds made
//! from then on. On a dedicated server, which runs one world, it waits in
//! `mod-data/<id>/worlds/next.toml` and goes into the world as it loads.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::config::{Schema, Scope, Stage};
use toml_edit::DocumentMut;

use crate::locks::Relock;

/// The world this process runs.
struct Current {
    /// Its key, while one runs.
    key: Mutex<Option<String>>,
    /// Whether one runs, for readers in every frame.
    running: AtomicBool,
}

static CURRENT: Current = Current {
    key: Mutex::new(None),
    running: AtomicBool::new(false),
};

/// The key of the world this process runs, while one runs.
#[must_use]
pub fn current() -> Option<String> {
    CURRENT.key.relock().clone()
}

/// Whether this process runs a world, without a lock.
#[must_use]
pub fn running() -> bool {
    CURRENT.running.load(Ordering::Acquire)
}

/// Sets the world this process runs, or none.
pub fn set_current(key: Option<String>) {
    let mut current = CURRENT.key.relock();
    CURRENT.running.store(key.is_some(), Ordering::Release);
    *current = key;
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
    /// The settings file, for each player's own settings.
    Global,
    /// The settings of the world with this key.
    World(String),
    /// The changes waiting for the next world, on a dedicated server.
    Next,
    /// The settings file, for a world setting while a game client runs no
    /// world: the template new worlds start from.
    NewWorlds,
}

/// Where a change of a setting at `stage` goes while the world `current`
/// runs, or none, on a dedicated `server` or a game client.
#[must_use]
pub fn destination(stage: Stage, current: Option<&str>, server: bool) -> Destination {
    match (stage.scope(), current) {
        (Scope::World, Some(key)) => Destination::World(key.to_owned()),
        (Scope::World, None) if server => Destination::Next,
        (Scope::World, None) => Destination::NewWorlds,
        _ => Destination::Global,
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

/// The value at the dotted `key` of `doc`: `comfort.multiplier` is
/// `multiplier` in `[comfort]`.
#[must_use]
pub fn value_at<'a>(doc: &'a toml_edit::Table, key: &str) -> Option<&'a toml_edit::Value> {
    let (path, leaf) = key.rsplit_once('.').map_or(("", key), |(p, l)| (p, l));
    let mut table = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        table = table.get(part)?.as_table()?;
    }
    table.get(leaf)?.as_value()
}

/// Sets the dotted `key` of `doc` to `value`, the old value's comments kept
/// and missing tables made; `false` when a part of the key is a value.
pub fn set_at(doc: &mut toml_edit::Table, key: &str, mut value: toml_edit::Value) -> bool {
    let (path, leaf) = key.rsplit_once('.').map_or(("", key), |(p, l)| (p, l));
    let mut table = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let item = table
            .entry(part)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        let Some(next) = item.as_table_mut() else {
            return false;
        };
        table = next;
    }
    match table.get_mut(leaf).and_then(toml_edit::Item::as_value_mut) {
        Some(old) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        None => {
            table.insert(leaf, toml_edit::Item::Value(value));
        }
    }
    true
}

/// Removes the dotted `key` of `doc`, if it is there, through standard and
/// inline tables alike.
pub fn remove_at(doc: &mut toml_edit::Table, key: &str) {
    let (path, leaf) = key.rsplit_once('.').map_or(("", key), |(p, l)| (p, l));
    let mut table: &mut dyn toml_edit::TableLike = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let Some(next) = table
            .get_mut(part)
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            return;
        };
        table = next;
    }
    let _removed = table.remove(leaf);
}

/// Every value of `table` with its dotted key, tables walked in order.
fn leaves(table: &toml_edit::Table, prefix: &str, found: &mut Vec<(String, toml_edit::Value)>) {
    for (key, item) in table {
        let path = format!("{prefix}{key}");
        if let Some(inner) = item.as_table() {
            leaves(inner, &format!("{path}."), found);
        } else if let Some(value) = item.as_value() {
            found.push((path, value.clone()));
        }
    }
}

/// `base` with every key of `over` set from it, `base`'s comments kept; keys
/// in `only` alone when it is given. Keys inside tables count by their
/// dotted path. A text that is no TOML leaves `base`.
#[must_use]
pub fn overlay(base: &str, over: &str, only: Option<&[&str]>) -> String {
    let (Ok(mut doc), Ok(over)) = (base.parse::<DocumentMut>(), over.parse::<DocumentMut>()) else {
        return base.to_owned();
    };
    let mut found = Vec::new();
    leaves(over.as_table(), "", &mut found);
    for (key, value) in found {
        if only.is_some_and(|keys| !keys.contains(&key.as_str())) {
            continue;
        }
        let _set = set_at(doc.as_table_mut(), &key, value);
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

/// What [`prepare`] makes of the world's settings file at `path`: `None`
/// when nothing needs writing.
///
/// # Errors
///
/// When the file is there and does not read, text that is not UTF-8
/// included: it stays as it is, never replaced by the template.
pub fn world_text(
    template: &str,
    path: &Path,
    next: Option<&str>,
) -> std::io::Result<Option<String>> {
    let existing = enshwick::files::read_to_string_if_present(path)?;
    Ok(prepare(template, existing.as_deref(), next))
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
    let mut found = Vec::new();
    leaves(template.as_table(), "", &mut found);
    let mut added = false;
    for (key, value) in found {
        if value_at(doc.as_table(), &key).is_none() {
            added |= set_at(doc.as_table_mut(), &key, value);
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

    use enshwick::abi::Event;
    use enshwick::config::Schema;

    use super::{Destination, compose, current, next_file, set_current, world_file, world_text};
    use crate::host::{Loader, ModRecord};
    use crate::locks::Relock;

    fn schema(record: &ModRecord) -> Option<Schema> {
        record.schema.relock().clone()
    }

    fn global_path(loader: &Loader, record: &ModRecord) -> PathBuf {
        loader.configs_dir.join(format!("{}.toml", record.id))
    }

    /// The text of `path` for showing or composing; a file that does not
    /// read shows as absent, as nothing is written from it.
    fn read(path: &std::path::Path) -> Option<String> {
        enshwick::files::read_to_string_if_present(path)
            .ok()
            .flatten()
    }

    fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
        enshwick::files::write_atomically(path, text)
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

    /// The text shown for the settings of `record` on this process: the
    /// running world's, else the template with the changes waiting for the
    /// next world, with the settings file's `user` settings over either.
    pub(crate) fn show_text(loader: &Loader, record: &ModRecord) -> String {
        let global = read(&global_path(loader, record)).unwrap_or_default();
        let shown = if let Some(key) = current() {
            read(&world_file(&loader.data_dir, &record.id, &key)).unwrap_or_else(|| global.clone())
        } else {
            match read(&next_file(&loader.data_dir, &record.id)) {
                Some(next) => super::overlay(&global, &next, None),
                None => global.clone(),
            }
        };
        match schema(record) {
            Some(schema) => compose(&shown, &global, &schema),
            None => shown,
        }
    }

    /// The file a change at `destination` goes to and its text now, empty
    /// while no file is there.
    ///
    /// # Errors
    ///
    /// The file and why it does not read; a change must not replace it.
    pub(crate) fn target_text(
        loader: &Loader,
        record: &ModRecord,
        destination: &Destination,
    ) -> Result<(PathBuf, String), (PathBuf, std::io::Error)> {
        let path = match destination {
            Destination::Global | Destination::NewWorlds => global_path(loader, record),
            Destination::World(key) => world_file(&loader.data_dir, &record.id, key),
            Destination::Next => next_file(&loader.data_dir, &record.id),
        };
        match enshwick::files::read_to_string_if_present(&path) {
            Ok(text) => Ok((path, text.unwrap_or_default())),
            Err(error) => Err((path, error)),
        }
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
            let text = match world_text(&global, &world_path, next.as_deref()) {
                Ok(text) => text,
                Err(e) => {
                    crate::log::write(
                        enshwick::abi::Level::WARN,
                        &record.id,
                        &format!(
                            "world settings {} do not read and stay as they are: {e}",
                            world_path.display()
                        ),
                    );
                    continue;
                }
            };
            if let Some(text) = text {
                if let Err(e) = write(&world_path, &text) {
                    crate::log::write(
                        enshwick::abi::Level::WARN,
                        &record.id,
                        &format!("world settings not written: {e}"),
                    );
                    continue;
                }
                crate::watch::wrote(&world_path);
            }
            if next.is_some() {
                let _gone = std::fs::remove_file(&next_path);
                crate::log::write(
                    enshwick::abi::Level::INFO,
                    &record.id,
                    &format!(
                        "world settings: the changes waiting in {} went into world {key}",
                        next_path.display()
                    ),
                );
            }
        }
        crate::log!(INFO, "world settings: world {key}");
        set_current(Some(key));
        crate::recipes::world_changed(loader);
        loader.emit(Event::SETTINGS_REPLACED, None, core::ptr::null());
    }

    /// The world stopped: every mod reads its settings file again.
    pub(crate) fn unload(loader: &Loader) {
        if current().is_none() {
            return;
        }
        set_current(None);
        crate::recipes::world_changed(loader);
        loader.emit(Event::SETTINGS_REPLACED, None, core::ptr::null());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enshwick::config::Field;

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
    fn a_world_settings_file_that_does_not_read_is_never_replaced() {
        let dir = std::env::temp_dir().join(format!("enshwick-world-read-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let template = "limit = 4\n";
        let path = dir.join("settings.toml");
        assert_eq!(
            world_text(template, &path, None).unwrap().as_deref(),
            Some(template),
            "a new world starts from the template"
        );
        std::fs::write(&path, b"limit = 9\n\xff\n").unwrap();
        assert!(world_text(template, &path, None).is_err(), "not UTF-8");
        let _gone = std::fs::remove_dir_all(&dir);
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
    fn a_restart_setting_left_in_a_world_file_shows_the_settings_files_value() {
        let schema = Schema::new(
            "",
            "config",
            vec![
                Field::new("limit", Stage::Startup, &4),
                Field::new("workers", Stage::Restart, &0),
            ],
        );
        let world = "limit = 9
workers = 2
";
        let global = "limit = 4
workers = 6
";
        assert_eq!(
            compose(world, global, &schema),
            "limit = 9
workers = 6
"
        );
    }

    #[test]
    fn changes_go_by_scope_and_world() {
        for server in [false, true] {
            assert_eq!(
                destination(Stage::Client, Some("w"), server),
                Destination::Global
            );
            assert_eq!(
                destination(Stage::Restart, Some("w"), server),
                Destination::Global
            );
            assert_eq!(
                destination(Stage::Client, None, server),
                Destination::Global
            );
            assert_eq!(
                destination(Stage::Runtime, Some("w"), server),
                Destination::World("w".into())
            );
        }
        assert_eq!(
            destination(Stage::Sync, None, false),
            Destination::NewWorlds,
            "a client with no world sets the defaults of new worlds"
        );
        assert_eq!(
            destination(Stage::Sync, None, true),
            Destination::Next,
            "a server's change waits for the world it runs"
        );
        assert_eq!(client_keys(&schema()), ["labels"]);
        let file = world_file(Path::new("d"), "you.trading-post", "w-1");
        assert!(file.ends_with("you.trading-post/worlds/w-1/settings.toml"));
        assert!(
            next_file(Path::new("d"), "you.trading-post")
                .ends_with("you.trading-post/worlds/next.toml")
        );
    }

    #[test]
    fn grouped_settings_overlay_and_fill_in_by_their_dotted_keys() {
        let template = "limit = 4\n\n# Comfort.\n[comfort]\nmultiplier = 1.0\nshown = true\n";
        let world = "limit = 6\n\n[comfort]\nmultiplier = 2.0\n";
        let filled = with_new_keys(world, template);
        assert_eq!(
            filled,
            "limit = 6\n\n[comfort]\nmultiplier = 2.0\nshown = true\n"
        );
        let global = "[comfort]\nmultiplier = 9.0\nshown = false\n";
        let composed = overlay(&filled, global, Some(&["comfort.shown"]));
        assert_eq!(
            composed,
            "limit = 6\n\n[comfort]\nmultiplier = 2.0\nshown = false\n"
        );
        let mut doc: DocumentMut = "a = 1\n".parse().unwrap();
        assert!(set_at(doc.as_table_mut(), "comfort.multiplier", 3.into()));
        assert!(!set_at(doc.as_table_mut(), "a.b", 3.into()), "a is a value");
        assert_eq!(
            value_at(doc.as_table(), "comfort.multiplier")
                .map(ToString::to_string)
                .as_deref(),
            Some("3")
        );
        remove_at(doc.as_table_mut(), "comfort.multiplier");
        assert!(value_at(doc.as_table(), "comfort.multiplier").is_none());
    }
}
