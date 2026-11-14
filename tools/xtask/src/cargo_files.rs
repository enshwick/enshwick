//! Reading Cargo's own files: the lock file's entries and a manifest's
//! dependency entries, for every check that needs either.

use toml::{Table, Value};

/// The lock file, from the repository root.
pub(crate) const LOCK: &str = "Cargo.lock";
/// The manifest table of normal dependencies, and a lock entry's key of its
/// dependencies.
pub(crate) const DEPENDENCIES: &str = "dependencies";
/// The manifest table of test, example and benchmark dependencies.
pub(crate) const DEV_DEPENDENCIES: &str = "dev-dependencies";
/// The manifest table of platform-specific dependency tables.
pub(crate) const TARGET: &str = "target";
/// The tables of a manifest that list dependencies, at its root or under
/// `[target.<cfg>]`.
pub(crate) const DEPENDENCY_TABLES: &[&str] =
    &[DEPENDENCIES, DEV_DEPENDENCIES, "build-dependencies"];
/// The lock file's array of locked packages.
pub(crate) const LOCKED_PACKAGES: &str = "package";
/// A lock entry's key of its crate's name.
pub(crate) const NAME: &str = "name";
/// A lock entry's or a manifest's key of a version.
pub(crate) const VERSION: &str = "version";
/// A lock entry's key of where its crate comes from.
pub(crate) const SOURCE: &str = "source";
/// The key of a dependency entry that names its crate when the entry's own
/// key does not.
const RENAMED: &str = "package";

/// One `[[package]]` of a lock file, whole.
#[derive(Debug, PartialEq)]
pub(crate) struct Locked(Table);

impl Locked {
    fn field(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(Value::as_str)
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.field(NAME)
    }

    pub(crate) fn version(&self) -> Option<&str> {
        self.field(VERSION)
    }

    /// Where it comes from: none for a workspace member.
    pub(crate) fn source(&self) -> Option<&str> {
        self.field(SOURCE)
    }

    /// Its `dependencies`, each `name`, `name version` or
    /// `name version (source)`.
    pub(crate) fn dependencies(&self) -> impl Iterator<Item = &str> {
        self.0
            .get(DEPENDENCIES)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
    }
}

/// The entries of a lock file.
///
/// # Errors
///
/// When the text is no TOML or holds no array of package tables: a
/// workspace's lock always locks its own members.
pub(crate) fn locked(text: &str) -> Result<Vec<Locked>, String> {
    let mut lock: Table = toml::from_str(text).map_err(|e| format!("{LOCK}: {e}"))?;
    match lock.remove(LOCKED_PACKAGES) {
        None => Err(format!("{LOCK}: no [[{LOCKED_PACKAGES}]]")),
        Some(Value::Array(entries)) => entries
            .into_iter()
            .map(|entry| match entry {
                Value::Table(table) => Ok(Locked(table)),
                _ => Err(format!("{LOCK}: a [[{LOCKED_PACKAGES}]] is no table")),
            })
            .collect(),
        Some(_) => Err(format!("{LOCK}: `{LOCKED_PACKAGES}` is no array of tables")),
    }
}

/// One entry of a manifest's dependency table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dependency<'a> {
    /// The table it sits in, one of [`DEPENDENCY_TABLES`].
    pub(crate) kind: &'static str,
    /// Its key, the name the code uses.
    pub(crate) key: &'a str,
    /// Its value: a version string or a table.
    pub(crate) entry: &'a Value,
}

impl<'a> Dependency<'a> {
    /// The crate it names: its `package` when renamed, else its key.
    pub(crate) fn package(&self) -> &'a str {
        package(self.key, self.entry)
    }
}

/// The crate the dependency entry `key = entry` names: its `package` when
/// renamed, else its key.
pub(crate) fn package<'a>(key: &'a str, entry: &'a Value) -> &'a str {
    entry.get(RENAMED).and_then(Value::as_str).unwrap_or(key)
}

/// Every dependency entry of a manifest, in each of [`DEPENDENCY_TABLES`] at
/// its root and then under each `[target.<cfg>]`.
pub(crate) fn dependencies(manifest: &Table) -> impl Iterator<Item = Dependency<'_>> {
    let targets = manifest
        .get(TARGET)
        .and_then(Value::as_table)
        .into_iter()
        .flat_map(Table::values)
        .filter_map(Value::as_table);
    std::iter::once(manifest)
        .chain(targets)
        .flat_map(|root| {
            DEPENDENCY_TABLES
                .iter()
                .filter_map(move |&kind| Some((kind, root.get(kind)?.as_table()?)))
        })
        .flat_map(|(kind, table)| {
            table
                .iter()
                .map(move |(key, entry)| Dependency { kind, key, entry })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_walk_finds_every_table_at_the_root_and_under_each_target() {
        let manifest: Table = toml::from_str(
            "[dependencies]\na = \"1\"\nb = { package = \"bee\", version = \"1\" }\n\
             [dev-dependencies]\nc = \"1\"\n[build-dependencies]\nd = \"1\"\n\
             [target.'cfg(windows)'.dependencies]\ne = \"1\"\n\
             [target.'cfg(unix)'.dev-dependencies]\nf = \"1\"\n[features]\ng = []\n",
        )
        .expect("the manifest reads");
        // Sorted: toml's `preserve_order` feature changes the targets' order.
        let mut found: Vec<(&str, &str, &str)> = dependencies(&manifest)
            .map(|dep| (dep.kind, dep.key, dep.package()))
            .collect();
        found.sort_unstable_by_key(|&(_, key, _)| key);
        assert_eq!(
            found,
            [
                (DEPENDENCIES, "a", "a"),
                (DEPENDENCIES, "b", "bee"),
                (DEV_DEPENDENCIES, "c", "c"),
                ("build-dependencies", "d", "d"),
                (DEPENDENCIES, "e", "e"),
                (DEV_DEPENDENCIES, "f", "f"),
            ]
        );
    }

    #[test]
    fn a_lock_entry_reads_as_its_name_version_source_and_dependencies() {
        let entries = locked(
            "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\ndependencies = [\"syn 2.0.0\"]\n\n\
             [[package]]\nname = \"syn\"\nversion = \"2.0.0\"\nsource = \"registry+x\"\n",
        )
        .expect("the lock reads");
        let [a, syn] = &entries[..] else {
            panic!("two entries, not {entries:?}");
        };
        assert_eq!(
            (a.name(), a.version(), a.source()),
            (Some("a"), Some("1.0.0"), None)
        );
        assert_eq!(a.dependencies().collect::<Vec<_>>(), ["syn 2.0.0"]);
        assert_eq!(syn.source(), Some("registry+x"));
        assert_eq!(syn.dependencies().count(), 0);
    }

    #[test]
    fn a_lock_without_packages_fails_like_a_broken_one() {
        assert!(locked("version = 4\n").is_err());
        assert!(locked("[[package]").is_err());
        assert!(locked("package = 1\n").is_err());
        assert!(locked("package = [1]\n").is_err());
    }
}
