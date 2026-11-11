//! Mod settings per world.
//!
//! Each settings file a mod declares lives beside the save, in
//! `<stem>-mod-settings/<id>/<file>.toml`, and nowhere else. As a world
//! loads, the loader writes a missing file from the mod's template, or
//! brings an existing one in line with the mod's schema: keys the mod added
//! are written at their defaults with their comments, keys it dropped are
//! removed, and the comments the loader wrote are refreshed. Values and the
//! lines a person wrote stay. Then every mod reads its settings. With no
//! world, mods run on their defaults; a client in another's world runs on
//! the values that world's server sent.
//!
//! The console, the Mods screen and the server's administrators see a mod's
//! files as one: `settings.toml` at the top, every other file a table named
//! after it.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::config::{Group, Schema};
use toml_edit::{DocumentMut, Item, Table};

use enshwick_base::Relock;

/// The file of a settings type that names none.
pub const DEFAULT_FILE: &str = "settings";

/// One settings file a mod declared: its name without `.toml`, its schema
/// and the text written from its defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsFile {
    /// The name, such as `settings`.
    pub file: String,
    /// What each key is.
    pub schema: Schema,
    /// The file written from the defaults, with the doc comments.
    pub template: String,
}

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

/// Where the settings file `file` of mod `id` in the world `key` lives.
#[must_use]
pub fn world_file(id: &str, file: &str, key: &str) -> PathBuf {
    crate::world_folders::settings_of(key)
        .join(id)
        .join(format!("{file}.toml"))
}

/// Where the copy of a world's settings file is kept before the loader
/// changes it: `<file>.toml.bak` beside it.
#[must_use]
pub fn backup_of(path: &std::path::Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".bak");
    path.with_file_name(name)
}

/// The value at the dotted `key` of `doc`: `comfort.multiplier` is
/// `multiplier` in `[comfort]`.
#[must_use]
pub fn value_at<'a>(doc: &'a Table, key: &str) -> Option<&'a toml_edit::Value> {
    let (path, leaf) = key.rsplit_once('.').unwrap_or(("", key));
    let mut table = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        table = table.get(part)?.as_table()?;
    }
    table.get(leaf)?.as_value()
}

/// Sets the dotted `key` of `doc` to `value`, the old value's comments kept
/// and missing tables made; `false` when a part of the key is a value.
pub fn set_at(doc: &mut Table, key: &str, mut value: toml_edit::Value) -> bool {
    let (path, leaf) = key.rsplit_once('.').unwrap_or(("", key));
    let mut table = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let item = table
            .entry(part)
            .or_insert_with(|| Item::Table(Table::new()));
        let Some(next) = item.as_table_mut() else {
            return false;
        };
        table = next;
    }
    match table.get_mut(leaf).and_then(Item::as_value_mut) {
        Some(old) => {
            *value.decor_mut() = old.decor().clone();
            *old = value;
        }
        None => {
            table.insert(leaf, Item::Value(value));
        }
    }
    true
}

/// Removes the dotted `key` of `doc`, if it is there, through standard and
/// inline tables alike.
pub fn remove_at(doc: &mut Table, key: &str) {
    let (path, leaf) = key.rsplit_once('.').unwrap_or(("", key));
    let mut table: &mut dyn toml_edit::TableLike = doc;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let Some(next) = table.get_mut(part).and_then(Item::as_table_like_mut) else {
            return;
        };
        table = next;
    }
    let _removed = table.remove(leaf);
}

/// What [`sync`] made of a world's settings file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// The file's new text; `None` when it stays as it is.
    pub text: Option<String>,
    /// A line per key added or removed, for the mod's log.
    pub lines: Vec<String>,
}

/// Brings `existing`, a world's settings file named `file`, in line with
/// `template`, the mod's file from its defaults.
///
/// Keys the template has and the file lacks are added at their defaults
/// with their comments, keys the file has and the template lacks are
/// removed, and the comment lines right above each key (or table) are the
/// template's. Values and every other comment stay. `who` names the mod and
/// its version for the lines; a key in `secrets` is named without its
/// default.
///
/// # Errors
///
/// When `existing` or `template` is no TOML; the file stays as it is.
pub fn sync(
    template: &str,
    existing: &str,
    file: &str,
    who: &str,
    secrets: &[&str],
) -> Result<Synced, toml_edit::TomlError> {
    let template: DocumentMut = template.parse()?;
    let mut doc: DocumentMut = existing.parse()?;
    let mut lines = Vec::new();
    let names = Names { file, who, secrets };
    sync_table(
        template.as_table(),
        doc.as_table_mut(),
        "",
        &names,
        &mut lines,
    );
    let text = doc.to_string();
    Ok(Synced {
        text: (text != existing).then_some(text),
        lines,
    })
}

/// How [`sync`] names what it changed.
struct Names<'a> {
    file: &'a str,
    who: &'a str,
    secrets: &'a [&'a str],
}

impl Names<'_> {
    fn added(&self, key: &str, value: &toml_edit::Value) -> String {
        if self.secrets.contains(&key) {
            format!("{}.toml: added {key}", self.file)
        } else {
            let value = value.clone().decorated("", "");
            format!("{}.toml: added {key} = {value}", self.file)
        }
    }

    fn removed(&self, key: &str) -> String {
        format!(
            "{}.toml: removed {key}, which {} does not have",
            self.file, self.who
        )
    }
}

/// [`sync`] for one table, its keys under `prefix`.
fn sync_table(
    template: &Table,
    doc: &mut Table,
    prefix: &str,
    names: &Names<'_>,
    lines: &mut Vec<String>,
) {
    let gone: Vec<String> = doc
        .iter()
        .filter(|(key, item)| {
            template
                .get(key)
                .is_none_or(|kept| kept.is_table() != item.is_table())
        })
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in gone {
        if let Some(item) = doc.remove(&key) {
            leaves(&item, &format!("{prefix}{key}"), &mut |path, _| {
                lines.push(names.removed(path));
            });
        }
    }
    for (key, item) in template {
        let path = format!("{prefix}{key}");
        let Some(kept) = template.key(key) else {
            continue;
        };
        if !doc.contains_key(key) {
            leaves(item, &path, &mut |path, value| {
                lines.push(names.added(path, value));
            });
            doc.insert_formatted(kept, item.clone());
            continue;
        }
        if let (Some(inner), Some(own)) = (
            item.as_table(),
            doc.get_mut(key).and_then(Item::as_table_mut),
        ) {
            refresh(own.decor_mut(), inner.decor());
            sync_table(inner, own, &format!("{path}."), names, lines);
        } else if let Some(mut own) = doc.key_mut(key) {
            refresh(own.leaf_decor_mut(), kept.leaf_decor());
        }
    }
}

/// Calls `found` with the dotted path and value of every value in `item`.
fn leaves(item: &Item, path: &str, found: &mut dyn FnMut(&str, &toml_edit::Value)) {
    match item {
        Item::Value(value) => found(path, value),
        Item::Table(table) => {
            for (key, inner) in table {
                leaves(inner, &format!("{path}.{key}"), found);
            }
        }
        _ => {}
    }
}

/// Puts the comment lines right above a key in `own`'s prefix to those of
/// `template`, the lines before them kept.
fn refresh(own: &mut toml_edit::Decor, template: &toml_edit::Decor) {
    let prefix = |decor: &toml_edit::Decor| {
        decor
            .prefix()
            .and_then(toml_edit::RawString::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let (mine, theirs) = (prefix(own), prefix(template));
    let (kept, _) = comment_block(&mine);
    let (_, block) = comment_block(&theirs);
    let refreshed = format!("{kept}{block}");
    if refreshed != mine {
        own.set_prefix(refreshed);
    }
}

/// `prefix` cut before the comment lines that end it: the lines before them
/// and the block itself.
fn comment_block(prefix: &str) -> (&str, &str) {
    let mut start = prefix.len();
    for line in prefix.split_inclusive('\n').rev() {
        if !line.trim_start().starts_with('#') {
            break;
        }
        start -= line.len();
    }
    prefix.split_at(start)
}

/// A mod's settings files seen as one schema: `settings.toml`'s fields at
/// the top, every other file a group named after it; `None` without files.
#[must_use]
pub fn view_schema(files: &[SettingsFile]) -> Option<Schema> {
    let mut view = files.iter().find(|f| f.file == DEFAULT_FILE).map_or_else(
        || Schema::new("", "config", Vec::new()),
        |f| f.schema.clone(),
    );
    for file in files.iter().filter(|f| f.file != DEFAULT_FILE) {
        let prefixed = |key: &str| format!("{}.{key}", file.file);
        let mut group = Group::new(&file.file, &file.schema.doc);
        file.schema.text.clone_into(&mut group.text);
        view.groups.push(group);
        view.groups
            .extend(file.schema.groups.iter().cloned().map(|mut g| {
                g.key = prefixed(&g.key);
                g
            }));
        view.fields
            .extend(file.schema.fields.iter().cloned().map(|mut f| {
                f.key = prefixed(&f.key);
                f
            }));
    }
    (!files.is_empty()).then_some(view)
}

/// The texts of a mod's settings files, `(file, text)`, seen as one, as
/// [`view_schema`] sees their schemas; a text that is no TOML shows empty.
#[must_use]
pub fn view_text(parts: &[(&str, &str)]) -> String {
    let parse = |text: &str| text.parse::<DocumentMut>().unwrap_or_default();
    let mut view = parts
        .iter()
        .find(|(file, _)| *file == DEFAULT_FILE)
        .map_or_else(DocumentMut::new, |(_, text)| parse(text));
    for (file, text) in parts.iter().filter(|(file, _)| *file != DEFAULT_FILE) {
        let mut table = parse(text).as_table().clone();
        table.set_implicit(false);
        view.insert(file, Item::Table(table));
    }
    view.to_string()
}

/// The file a key of the view names and its key there: `altars.per_level`
/// is `per_level` of `altars.toml` when the mod has that file.
#[must_use]
pub fn split_key<'a>(key: &'a str, files: &[SettingsFile]) -> (&'a str, &'a str) {
    key.split_once('.')
        .filter(|(file, _)| {
            files
                .iter()
                .any(|f| f.file != DEFAULT_FILE && f.file == *file)
        })
        .unwrap_or((DEFAULT_FILE, key))
}

/// The file a key of the view lives in, as a person finds it in the
/// world's settings folder: `<id>/<file>.toml`.
#[must_use]
pub fn shown_file(id: &str, files: &[SettingsFile], key: &str) -> String {
    format!("{id}/{}.toml", split_key(key, files).0)
}

/// The text of `file` out of `view`, a view's text as [`view_text`] writes
/// it, the mod's other files named in `others`.
#[must_use]
pub fn part(view: &str, file: &str, others: &[&str]) -> String {
    let Ok(mut table) = view.parse::<toml::Table>() else {
        return String::new();
    };
    let part = if file == DEFAULT_FILE {
        table.retain(|key, _| !others.contains(&key));
        table
    } else {
        match table.remove(file) {
            Some(toml::Value::Table(inner)) => inner,
            _ => toml::Table::new(),
        }
    };
    toml::to_string(&part).unwrap_or_default()
}

#[cfg(windows)]
pub(crate) use live::{changed, file_text, load, sent_view, show_text, unload, write_target};

#[cfg(windows)]
mod live;

#[cfg(test)]
mod tests;
