//! Whether a manifest or the lock file changed only in the versions of the
//! workspace's own packages, as a release moves them: such a change compiles
//! the same code under a new version.

use std::collections::BTreeSet;

use toml::{Table, Value};

use super::shared::{DEPENDENCIES, DEPENDENCY_TABLES, LOCKED_PACKAGES, PACKAGE, TARGET, WORKSPACE};

/// The manifest key of a version.
const VERSION: &str = "version";
/// The key of a dependency entry that makes it a workspace package.
const PATH: &str = "path";
/// A lock entry's keys: registry and git entries have a source, members not.
const SOURCE: &str = "source";
const NAME: &str = "name";

/// Whether a package manifest changed only in its version and the versions
/// of its path dependencies; `false` when either side does not read.
pub(super) fn member(before: &str, after: &str) -> bool {
    same(before, after, |manifest| {
        if let Some(Value::Table(package)) = manifest.get_mut(PACKAGE) {
            package.remove(VERSION);
        }
        unversion_paths(manifest);
    })
}

/// Whether the root manifest changed only in `[workspace.package]`'s version
/// and the versions of its `[workspace.dependencies]` path entries; `false`
/// when either side does not read.
pub(super) fn root(before: &str, after: &str) -> bool {
    same(before, after, |manifest| {
        if let Some(Value::Table(workspace)) = manifest.get_mut(WORKSPACE) {
            if let Some(Value::Table(package)) = workspace.get_mut(PACKAGE) {
                package.remove(VERSION);
            }
            if let Some(Value::Table(dependencies)) = workspace.get_mut(DEPENDENCIES) {
                unversion(dependencies);
            }
        }
        unversion_paths(manifest);
    })
}

/// Whether the lock file changed only in the versions of the workspace's own
/// packages, the entries without a source; `false` when either side does not
/// read.
pub(super) fn lock(before: &str, after: &str) -> bool {
    let (Ok(before), Ok(after)) = (
        toml::from_str::<Table>(before),
        toml::from_str::<Table>(after),
    ) else {
        return false;
    };
    let members: BTreeSet<String> = [&before, &after]
        .into_iter()
        .filter_map(|lock| lock.get(LOCKED_PACKAGES)?.as_array())
        .flatten()
        .filter(|entry| entry.get(SOURCE).is_none())
        .filter_map(|entry| entry.get(NAME)?.as_str().map(str::to_owned))
        .collect();
    equal_without(before, after, |lock| {
        let Some(Value::Array(entries)) = lock.get_mut(LOCKED_PACKAGES) else {
            return;
        };
        for entry in entries.iter_mut().filter_map(Value::as_table_mut) {
            if !entry.contains_key(SOURCE) {
                entry.remove(VERSION);
            }
            if let Some(Value::Array(dependencies)) = entry.get_mut(DEPENDENCIES) {
                for dependency in dependencies.iter_mut() {
                    // `name version` names one of several versions; a member has one.
                    let name = dependency.as_str().and_then(|d| d.split(' ').next());
                    if let Some(name) = name.filter(|name| members.contains(*name)) {
                        *dependency = Value::String(name.to_owned());
                    }
                }
            }
        }
    })
}

/// Whether `before` and `after` read as TOML and are equal once `strip` took
/// the versions out of both.
fn same(before: &str, after: &str, strip: impl Fn(&mut Table)) -> bool {
    let (Ok(before), Ok(after)) = (
        toml::from_str::<Table>(before),
        toml::from_str::<Table>(after),
    ) else {
        return false;
    };
    equal_without(before, after, strip)
}

/// Whether `before` and `after` are equal once `strip` took the versions out
/// of both.
fn equal_without(mut before: Table, mut after: Table, strip: impl Fn(&mut Table)) -> bool {
    strip(&mut before);
    strip(&mut after);
    before == after
}

/// Takes the version out of every path dependency of a manifest, at its root
/// and under `[target.<cfg>]`.
fn unversion_paths(manifest: &mut Table) {
    for kind in DEPENDENCY_TABLES {
        if let Some(Value::Table(table)) = manifest.get_mut(*kind) {
            unversion(table);
        }
    }
    if let Some(Value::Table(targets)) = manifest.get_mut(TARGET) {
        for target in targets
            .iter_mut()
            .filter_map(|(_, target)| target.as_table_mut())
        {
            unversion_paths(target);
        }
    }
}

/// Takes the version out of each entry of a dependency table with a path.
fn unversion(dependencies: &mut Table) {
    for entry in dependencies
        .iter_mut()
        .filter_map(|(_, entry)| entry.as_table_mut())
    {
        if entry.contains_key(PATH) {
            entry.remove(VERSION);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_s_own_and_path_dependency_versions_alone_are_a_bump() {
        let before = "[package]\nname = \"a\"\nversion = \"1.0.0-alpha.14\"\n[dependencies]\nb = { path = \"../b\", version = \"=1.0.0-alpha.14\" }\nserde = \"1\"\n[target.'cfg(windows)'.dev-dependencies]\nc = { path = \"../c\", version = \"=1.0.0-alpha.14\" }\n";
        let after = before.replace("alpha.14", "alpha.15");
        assert!(member(before, &after));
        assert!(!member(
            before,
            &after.replace("serde = \"1\"", "serde = \"2\"")
        ));
        assert!(!member(
            before,
            &after.replace("name = \"a\"", "name = \"z\"")
        ));
        assert!(!member(before, "[package"));
    }

    #[test]
    fn a_registry_dependency_s_version_is_no_bump() {
        let before = "[package]\nname = \"a\"\n[dependencies]\nserde = { version = \"1.0.1\" }\n";
        assert!(!member(before, &before.replace("1.0.1", "1.0.2")));
    }

    #[test]
    fn the_root_s_path_entry_and_package_versions_alone_are_a_bump() {
        let before = "[workspace]\nmembers = [\"crates/*\"]\n[workspace.package]\nversion = \"1.0.0-alpha.14\"\nedition = \"2024\"\n[workspace.dependencies]\na = { path = \"crates/a\", version = \"=1.0.0-alpha.14\" }\nserde = { version = \"1.0.1\" }\n";
        assert!(root(before, &before.replace("alpha.14", "alpha.15")));
        assert!(!root(before, &before.replace("1.0.1", "1.0.2")));
        assert!(!root(before, &before.replace("2024", "2021")));
    }

    #[test]
    fn the_lock_s_member_versions_alone_are_a_bump() {
        let before = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0-alpha.14\"\ndependencies = [\"b\", \"syn\"]\n\n[[package]]\nname = \"b\"\nversion = \"1.0.0-alpha.14\"\n\n[[package]]\nname = \"syn\"\nversion = \"2.0.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"aa\"\n";
        assert!(lock(before, &before.replace("alpha.14", "alpha.15")));
        assert!(!lock(before, &before.replace("2.0.0", "2.0.1")));
        assert!(!lock(
            before,
            &before.replace("checksum = \"aa\"", "checksum = \"bb\"")
        ));
    }
}
