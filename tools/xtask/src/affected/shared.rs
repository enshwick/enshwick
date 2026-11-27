//! The packages a change to a shared build file reaches: the lock file's
//! changed entries and the root manifest's changed inherited keys.

use std::collections::BTreeSet;

use toml::{Table, Value};

use super::{Package, Texts};
use crate::cargo_files::{self, DEPENDENCIES, Locked};
pub(super) use crate::cargo_files::{LOCK, MANIFEST};

/// The manifest table of workspace settings, and the key that inherits one.
pub(super) const WORKSPACE: &str = "workspace";
/// The manifest table of package fields.
pub(super) const PACKAGE: &str = "package";
/// The `[workspace]` tables members take keys from with `key.workspace = true`.
const INHERITED: &[&str] = &[PACKAGE, DEPENDENCIES];
/// The manifest table of build profiles.
const PROFILE: &str = "profile";
/// The profiles the local check builds with: clippy, doc and `test` under
/// `dev`, the tests under `test`; a change to them can change a test's
/// outcome (overflow checks, debug assertions, panic strategy), so it stays
/// wide.
const CHECKED_PROFILES: &[&str] = &["dev", "test"];

/// The workspace packages a change of the lock file reaches: those whose
/// locked entries changed and those that depend on such an entry, directly
/// or not; `None` when either side does not read.
pub(super) fn locked(texts: &dyn Texts, packages: &[Package]) -> Option<BTreeSet<String>> {
    let before = lock(&texts.before(LOCK)?)?;
    let after = lock(&texts.after(LOCK)?)?;
    let changed = before
        .iter()
        .filter(|entry| !after.contains(entry))
        .chain(after.iter().filter(|entry| !before.contains(entry)))
        .map(key)
        .collect();
    Some(reach(changed, before.iter().chain(&after), packages))
}

/// The workspace packages among `reached` and the entries that depend on
/// one of them, directly or not.
fn reach<'a>(
    mut reached: BTreeSet<Key>,
    entries: impl Iterator<Item = &'a Locked>,
    packages: &[Package],
) -> BTreeSet<String> {
    let graph: Vec<(Key, Vec<&str>)> = entries
        .map(|entry| (key(entry), entry.dependencies().collect()))
        .collect();
    loop {
        let known = reached.len();
        for (key, deps) in &graph {
            if !reached.contains(key)
                && deps
                    .iter()
                    .any(|dep| reached.iter().any(|at| names(dep, at)))
            {
                reached.insert(key.clone());
            }
        }
        if reached.len() == known {
            break;
        }
    }
    reached
        .into_iter()
        .filter(|(_, _, source)| source.is_none())
        .map(|(name, _, _)| name)
        .filter(|name| packages.iter().any(|package| &package.name == name))
        .collect()
}

/// A locked package: its name, version and source (none for a workspace
/// member).
type Key = (String, String, Option<String>);

fn key(entry: &Locked) -> Key {
    (
        entry.name().unwrap_or_default().to_owned(),
        entry.version().unwrap_or_default().to_owned(),
        entry.source().map(str::to_owned),
    )
}

/// The entries of a lock file; `None` when it does not read.
fn lock(text: &str) -> Option<Vec<Locked>> {
    cargo_files::locked(text).ok()
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
/// entry, and, since features unify across the workspace, every package the
/// lock file builds on a changed dependency's crate; `None` when another part
/// changed (lints, profiles, patches, members, metadata), which every package
/// builds with, or a file does not read. With `local`, a profile the local
/// check never builds with (release, bench, a custom one) reaches nothing.
pub(super) fn inherited(
    texts: &dyn Texts,
    packages: &[Package],
    local: bool,
) -> Option<BTreeSet<String>> {
    let mut before = toml::from_str::<Table>(&texts.before(MANIFEST)?).ok()?;
    let mut after = toml::from_str::<Table>(&texts.after(MANIFEST)?).ok()?;
    if local {
        keep_checked_profiles(&mut before);
        keep_checked_profiles(&mut after);
    }
    let mut changed = Vec::new();
    let mut crates = BTreeSet::new();
    for &section in INHERITED {
        let (old, new) = (take(&mut before, section), take(&mut after, section));
        for key in old.keys().chain(new.keys()) {
            let (was, is) = (old.get(key), new.get(key));
            if was == is {
                continue;
            }
            if section == DEPENDENCIES {
                crates.extend(
                    [was, is]
                        .into_iter()
                        .flatten()
                        .map(|entry| cargo_files::package(key, entry).to_owned()),
                );
            }
            changed.push((section, key.clone()));
        }
    }
    if before != after {
        return None;
    }
    if changed.is_empty() {
        return Some(BTreeSet::new());
    }
    let entries = match texts.after(LOCK) {
        Some(text) => lock(&text)?,
        None => Vec::new(),
    };
    let built_on = entries
        .iter()
        .map(key)
        .filter(|(name, _, _)| crates.contains(name))
        .collect();
    let mut reached = reach(built_on, entries.iter(), packages);
    reached.extend(
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
            .map(|package| package.name.clone()),
    );
    Some(reached)
}

/// Drops from a manifest every `[profile.<name>]` but [`CHECKED_PROFILES`],
/// and `[profile]` itself when none is left.
fn keep_checked_profiles(manifest: &mut Table) {
    let Some(Value::Table(profiles)) = manifest.get_mut(PROFILE) else {
        return;
    };
    profiles.retain(|name, _| CHECKED_PROFILES.contains(&name));
    if profiles.is_empty() {
        manifest.remove(PROFILE);
    }
}

/// Removes `[workspace.<key>]` from a manifest and returns it as a table,
/// empty when it is missing or no table.
fn take(manifest: &mut Table, key: &str) -> Table {
    match manifest
        .get_mut(WORKSPACE)
        .and_then(Value::as_table_mut)
        .and_then(|workspace| workspace.remove(key))
    {
        Some(Value::Table(table)) => table,
        _ => Table::new(),
    }
}

/// Whether a member's manifest inherits `key` of `[workspace.<section>]`.
fn takes(manifest: &Table, section: &str, key: &str) -> bool {
    let inherits = |entry: &Value| entry.get(WORKSPACE).and_then(Value::as_bool) == Some(true);
    if section != DEPENDENCIES {
        return manifest
            .get(section)
            .and_then(|table| table.get(key))
            .is_some_and(inherits);
    }
    cargo_files::dependencies(manifest).any(|dep| dep.key == key && inherits(dep.entry))
}
