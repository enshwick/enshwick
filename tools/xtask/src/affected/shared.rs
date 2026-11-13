//! The packages a change to a shared build file reaches: the lock file's
//! changed entries and the root manifest's changed inherited keys.

use std::collections::BTreeSet;

use toml::{Table, Value};

use super::{Package, Texts};

/// The workspace manifest.
pub(super) const MANIFEST: &str = "Cargo.toml";
/// The lock file.
pub(super) const LOCK: &str = "Cargo.lock";
/// The `[workspace]` tables members take keys from with `key.workspace = true`.
const INHERITED: &[&str] = &["package", "dependencies"];
/// The `[workspace]` keys whose change reaches no package by itself: a new
/// member's own files are in the change, a removed one has nothing to check.
const MEMBERSHIP: &[&str] = &["members", "exclude"];
/// The tables of a manifest that list dependencies, at its root or under
/// `[target.<cfg>]`.
const DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

/// The workspace packages a change of the lock file reaches: those whose
/// locked entries changed and those that depend on such an entry, directly
/// or not; `None` when either side does not read.
pub(super) fn locked(texts: &dyn Texts, packages: &[Package]) -> Option<BTreeSet<String>> {
    let before = lock(&texts.before(LOCK)?)?;
    let after = lock(&texts.after(LOCK)?)?;
    let mut reached: BTreeSet<Key> = before
        .iter()
        .filter(|entry| !after.contains(entry))
        .chain(after.iter().filter(|entry| !before.contains(entry)))
        .map(Entry::key)
        .collect();
    loop {
        let known = reached.len();
        for entries in [&before, &after] {
            for entry in entries {
                if !reached.contains(&entry.key())
                    && entry
                        .dependencies()
                        .any(|dep| reached.iter().any(|key| names(dep, key)))
                {
                    reached.insert(entry.key());
                }
            }
        }
        if reached.len() == known {
            break;
        }
    }
    Some(
        reached
            .into_iter()
            .filter(|(_, _, source)| source.is_none())
            .map(|(name, _, _)| name)
            .filter(|name| packages.iter().any(|package| &package.name == name))
            .collect(),
    )
}

/// A locked package: its name, version and source (none for a workspace
/// member).
type Key = (String, String, Option<String>);

/// One `[[package]]` of a lock file, whole.
#[derive(PartialEq)]
struct Entry(Table);

impl Entry {
    fn key(&self) -> Key {
        let field = |name| self.0.get(name).and_then(Value::as_str).map(str::to_owned);
        (
            field("name").unwrap_or_default(),
            field("version").unwrap_or_default(),
            field("source"),
        )
    }

    /// Its `dependencies`, each `name`, `name version` or
    /// `name version (source)`.
    fn dependencies(&self) -> impl Iterator<Item = &str> {
        self.0
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }
}

/// The entries of a lock file; `None` when it does not read.
fn lock(text: &str) -> Option<Vec<Entry>> {
    toml::from_str::<Table>(text)
        .ok()?
        .get("package")?
        .as_array()?
        .iter()
        .map(|entry| entry.as_table().cloned().map(Entry))
        .collect()
}

/// Whether the dependency `dep` of a lock entry names the entry `key`.
fn names(dep: &str, (name, version, source): &Key) -> bool {
    let mut words = dep.splitn(3, ' ');
    words.next() == Some(name.as_str())
        && words.next().is_none_or(|v| v == version)
        && words.next().is_none_or(|s| {
            Some(s.trim_start_matches('(').trim_end_matches(')')) == source.as_deref()
        })
}

/// The workspace packages a change of the root manifest reaches: those that
/// inherit a changed `[workspace.package]` field or `[workspace.dependencies]`
/// entry; `None` when another part changed (lints, profiles, patches,
/// metadata), which every package builds with, or a side does not read.
pub(super) fn inherited(texts: &dyn Texts, packages: &[Package]) -> Option<BTreeSet<String>> {
    let mut before = toml::from_str::<Table>(&texts.before(MANIFEST)?).ok()?;
    let mut after = toml::from_str::<Table>(&texts.after(MANIFEST)?).ok()?;
    let mut changed = Vec::new();
    for &section in INHERITED {
        let (old, new) = (take(&mut before, section), take(&mut after, section));
        changed.extend(
            old.keys()
                .chain(new.keys())
                .filter(|key| old.get(*key) != new.get(*key))
                .map(|key| (section, key.clone())),
        );
    }
    for &key in MEMBERSHIP {
        take(&mut before, key);
        take(&mut after, key);
    }
    if before != after {
        return None;
    }
    Some(
        packages
            .iter()
            .filter(|package| {
                // A manifest that does not read fails its build: check it.
                texts
                    .after(&format!("{}/{MANIFEST}", package.dir))
                    .and_then(|text| toml::from_str::<Table>(&text).ok())
                    .is_none_or(|manifest| {
                        changed
                            .iter()
                            .any(|(section, key)| takes(&manifest, section, key))
                    })
            })
            .map(|package| package.name.clone())
            .collect(),
    )
}

/// Removes `[workspace.<key>]` from a manifest and returns it as a table,
/// empty when it is missing or no table.
fn take(manifest: &mut Table, key: &str) -> Table {
    match manifest
        .get_mut("workspace")
        .and_then(Value::as_table_mut)
        .and_then(|workspace| workspace.remove(key))
    {
        Some(Value::Table(table)) => table,
        _ => Table::new(),
    }
}

/// Whether a member's manifest inherits `key` of `[workspace.<section>]`.
fn takes(manifest: &Table, section: &str, key: &str) -> bool {
    let inherits = |table: Option<&Value>| {
        table
            .and_then(|table| table.get(key))
            .and_then(|entry| entry.get("workspace"))
            .and_then(Value::as_bool)
            == Some(true)
    };
    if section != "dependencies" {
        return inherits(manifest.get(section));
    }
    let targets = manifest
        .get("target")
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(Table::values)
        .filter_map(Value::as_table);
    std::iter::once(manifest).chain(targets).any(|root| {
        DEPENDENCY_TABLES
            .iter()
            .any(|kind| inherits(root.get(*kind)))
    })
}
