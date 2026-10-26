//! The fake's migrations: the steps a mod declared, run on text the way the
//! loader runs them on files.

use std::collections::HashMap;

use super::{FakeHost, Installed, table};
use crate::abi::Str;
use crate::migrations::MigrationError;

/// A migration step as the fake keeps it: kind, from, to.
pub(crate) type FakeStep = (u32, String, String);

/// The migrations a mod declared, as the fake keeps them: id and steps as
/// `(kind, from, to)`, and the mod's map.
#[derive(Clone, Debug)]
pub(crate) struct FakeMigrations {
    pub(crate) list: Vec<(String, Vec<FakeStep>)>,
    pub(crate) map: crate::abi::MigrateFn,
    pub(crate) user: usize,
}

impl FakeMigrations {
    /// Runs the map of step `step` of migration `migration` on `input`.
    fn call(&self, migration: usize, step: usize, input: &str) -> Result<String, String> {
        let (Ok(migration), Ok(step)) = (u32::try_from(migration), u32::try_from(step)) else {
            return Err("too many steps".to_owned());
        };
        let mut out = Str::EMPTY;
        // SAFETY: the mod's map, with its pointer and a text valid for the call.
        let status = unsafe {
            (self.map)(
                core::ptr::with_exposed_provenance_mut(self.user),
                migration,
                step,
                Str::new(input),
                &raw mut out,
            )
        };
        // SAFETY: the text stays valid until the next call.
        let text = unsafe { out.as_str() }.unwrap_or_default().to_owned();
        if status.is_ok() { Ok(text) } else { Err(text) }
    }
}

impl FakeHost {
    /// The settings file another mod `id` keeps, which
    /// [`take_setting`](crate::migrations::Migration::take_setting) steps
    /// read in [`migrate_settings`](Installed::migrate_settings).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::migrations::Migrations;
    ///
    /// let loader = enshwick::test::FakeHost::new()
    ///     .other_settings("me.comfort", "multiplier = 2.0\n")
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn other_settings(mut self, id: &str, text: &str) -> Self {
        self.other_settings.insert(id.to_owned(), text.to_owned());
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The ids of the migrations the mod declared, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.migrations().is_empty());
    /// ```
    #[must_use]
    pub fn migrations(&self) -> Vec<String> {
        table::with(Vec::new(), |s| {
            s.migrations
                .as_ref()
                .map(|m| m.list.iter().map(|(id, _)| id.clone()).collect())
                .unwrap_or_default()
        })
    }

    /// Runs the settings steps of every migration the mod declared on the
    /// settings text `old`, as the loader does on a file an older version
    /// wrote; the new text, without comments.
    ///
    /// # Errors
    ///
    /// Why a step failed: the text is no TOML, or a step's map refused.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.migrate_settings("a = 1\n"), Ok("a = 1\n".to_owned()));
    /// ```
    pub fn migrate_settings(&self, old: &str) -> Result<String, MigrationError> {
        use crate::abi::MigrationStep as S;
        let migrations = table::with(None, |s| s.migrations.clone());
        let others = table::with(HashMap::new(), |s| s.host.other_settings.clone());
        let mut table: toml::Table = toml::from_str(old)?;
        let list = migrations.as_ref().map_or(&[][..], |m| &m.list[..]);
        for (index, (_, steps)) in list.iter().enumerate() {
            for (step, (kind, from, to)) in steps.iter().enumerate() {
                match *kind {
                    S::TAKE_SETTING => take(&mut table, from, to, &others)?,
                    S::RENAME_SETTING if !table.contains_key(to) => {
                        if let Some(value) = table.remove(from) {
                            table.insert(to.clone(), value);
                        }
                    }
                    S::REMOVE_SETTING => {
                        table.remove(from);
                    }
                    S::MAP_SETTING => {
                        let (Some(value), Some(map)) = (table.get(from), migrations.as_ref())
                        else {
                            continue;
                        };
                        let text = crate::migrations::write_value(value)?;
                        let new = map
                            .call(index, step, &text)
                            .map_err(|e| MigrationError::new(format!("setting {from}: {e}")))?;
                        table.insert(from.clone(), crate::migrations::parse_value(&new)?);
                    }
                    _ => {}
                }
            }
        }
        toml::to_string(&table).map_err(|e| MigrationError::new(e.to_string()))
    }

    /// Runs the store steps of every migration the mod declared on the
    /// world store `name` holding `old`, following renames; the name and
    /// text it ends with.
    ///
    /// # Errors
    ///
    /// Why a step's map refused.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// let migrated = loader.migrate_store("owners", "count = 1\n");
    /// assert_eq!(migrated, Ok(("owners".to_owned(), "count = 1\n".to_owned())));
    /// ```
    pub fn migrate_store(&self, name: &str, old: &str) -> Result<(String, String), MigrationError> {
        use crate::abi::MigrationStep as S;
        let migrations = table::with(None, |s| s.migrations.clone());
        let (mut name, mut text) = (name.to_owned(), old.to_owned());
        let list = migrations.as_ref().map_or(&[][..], |m| &m.list[..]);
        for (index, (_, steps)) in list.iter().enumerate() {
            for (step, (kind, from, to)) in steps.iter().enumerate() {
                match *kind {
                    S::RENAME_STORE if *from == name => name.clone_from(to),
                    S::MAP_STORE if *from == name => {
                        let Some(map) = migrations.as_ref() else {
                            continue;
                        };
                        text = map
                            .call(index, step, &text)
                            .map_err(|e| MigrationError::new(format!("store {from}: {e}")))?;
                    }
                    _ => {}
                }
            }
        }
        Ok((name, text))
    }
}

/// Sets the dotted key `to` of `table` from `from`, `<mod id>:<key>`, in the
/// settings of `others`, as the loader's take step does.
fn take(
    table: &mut toml::Table,
    from: &str,
    to: &str,
    others: &HashMap<String, String>,
) -> Result<(), MigrationError> {
    let (id, key) = from
        .split_once(':')
        .ok_or_else(|| MigrationError::new(format!("setting {from}: not <mod id>:<key>")))?;
    let Some(text) = others.get(id) else {
        return Ok(());
    };
    let other: toml::Table = toml::from_str(text)?;
    let mut parts = key.split('.');
    let mut value = parts.next().and_then(|first| other.get(first));
    for part in parts {
        value = value.and_then(|v| v.get(part));
    }
    let Some(value) = value.cloned() else {
        return Ok(());
    };
    let (path, leaf) = to.rsplit_once('.').map_or(("", to), |(p, l)| (p, l));
    let mut at = table;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        at = at
            .entry(part)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| MigrationError::new(format!("setting {to}: {part} is not a table")))?;
    }
    at.insert(leaf.to_owned(), value);
    Ok(())
}
