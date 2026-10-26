//! Settings and migrations: the settings text, synced values and migration steps.

use std::collections::HashMap;

use super::FakeHost;
use super::Installed;
use super::table;
use crate::abi::Level;
use crate::abi::Str;
use crate::migrations::MigrationError;

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

    /// The text of the mod's settings file; without it the mod reads its own
    /// defaults, as on first start.
    ///
    /// # Examples
    ///
    /// See [`crate`].
    #[must_use]
    pub fn settings(mut self, text: &str) -> Self {
        self.settings = Some(text.to_owned());
        self
    }

    /// The path `Load::data_dir` returns; `mod-data/test` by default. The
    /// fake creates nothing there.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().data_dir("target/mod-data/bonus").install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn data_dir(mut self, path: &str) -> Self {
        path.clone_into(&mut self.data_dir);
        self
    }

    /// The most detailed level the loader keeps for the mod's lines.
    ///
    /// # Examples
    ///
    /// See [`FakeHost`].
    #[must_use]
    pub fn log_level(mut self, level: Level) -> Self {
        self.log_level = level;
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// Replaces the text of the mod's settings file, as a player's edit
    /// would; raise [`events::SettingsChanged`](crate::events::SettingsChanged)
    /// after it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::events;
    ///
    /// let loader = enshwick::test::FakeHost::new().settings("limit = 7\n").install();
    /// loader.set_settings("limit = 9\n");
    /// assert_eq!(loader.raise::<events::SettingsChanged>(&()), 0);
    /// ```
    pub fn set_settings(&self, text: &str) {
        table::with((), |s| s.host.settings = Some(text.to_owned()));
    }

    /// Sets the join values of the server this client joined, which the mod
    /// gets over its file's on its next read, or `None` for none, as after
    /// joining a server without them; raise
    /// [`events::SettingsChanged`](crate::events::SettingsChanged)
    /// after it, as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_server_synced(Some("{ altars = [2, 4] }"));
    /// assert!(loader.reported_synced().is_empty());
    /// ```
    pub fn set_server_synced(&self, synced: Option<&str>) {
        table::with((), |s| s.host.server_synced = synced.map(str::to_owned));
    }

    /// The join values the mod reported each time it read its settings, as
    /// its file holds them.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_server_synced`].
    #[must_use]
    pub fn reported_synced(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.synced.clone())
    }

    /// The defaults the mod passed each time it read its settings.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.settings_defaults().is_empty());
    /// ```
    #[must_use]
    pub fn settings_defaults(&self) -> Vec<String> {
        table::with(Vec::new(), |s| s.defaults.clone())
    }

    /// The settings schema the mod handed the loader, which
    /// [`Load::config`](crate::Load::config) does; `None` before.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.config_schema().is_none());
    /// ```
    #[must_use]
    pub fn config_schema(&self) -> Option<crate::config::Schema> {
        table::with(None, |s| s.config_schema.clone())
            .and_then(|text| crate::config::Schema::parse(&text).ok())
    }

    /// Runs the mod's check of a new settings text, as the loader does
    /// before it writes a change: `Ok` when the mod accepts `text`, else
    /// its reason; `Ok` for a mod without a check.
    ///
    /// # Errors
    ///
    /// [`SettingsRefused`] with the mod's reason for refusing `text`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.check_settings("x = 1"), Ok(()));
    /// ```
    pub fn check_settings(&self, text: &str) -> Result<(), SettingsRefused> {
        let Some((callback, user)) = table::with(None, |s| s.config_check) else {
            return Ok(());
        };
        let mut why = Str::EMPTY;
        // SAFETY: the mod's check with its own pointer; the text lives for
        // the call.
        let status = unsafe {
            callback(
                Str::new(text),
                &raw mut why,
                core::ptr::with_exposed_provenance_mut(user),
            )
        };
        if status.is_ok() {
            return Ok(());
        }
        // SAFETY: the reason stays valid until the mod's next call.
        let reason = unsafe { why.as_str() }.unwrap_or("refused").to_owned();
        Err(SettingsRefused { reason })
    }

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
                    S::RENAME_SETTING => {
                        if crate::config::value_at(&table, to).is_none()
                            && let Some(value) = remove_path(&mut table, from)
                        {
                            set_path(&mut table, to, value)?;
                        }
                    }
                    S::REMOVE_SETTING => {
                        let _gone = remove_path(&mut table, from);
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

/// A mod's check refused a settings text, as [`Installed::check_settings`]
/// reports it.
///
/// # Examples
///
/// See [`Installed::check_settings`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct SettingsRefused {
    /// The mod's reason.
    pub reason: String,
}

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
    fn call(&self, migration: usize, step: usize, input: &str) -> Result<String, MigrationError> {
        let (Ok(migration), Ok(step)) = (u32::try_from(migration), u32::try_from(step)) else {
            return Err(MigrationError::new("too many steps"));
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
        if status.is_ok() {
            Ok(text)
        } else {
            Err(MigrationError::new(text))
        }
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
    set_path(table, to, value)
}

/// Sets the dotted key `key` of `table` to `value`, making missing tables.
fn set_path(table: &mut toml::Table, key: &str, value: toml::Value) -> Result<(), MigrationError> {
    let (path, leaf) = key.rsplit_once('.').map_or(("", key), |(p, l)| (p, l));
    let mut at = table;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        at = at
            .entry(part)
            .or_insert_with(|| toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or_else(|| MigrationError::new(format!("setting {key}: {part} is not a table")))?;
    }
    at.insert(leaf.to_owned(), value);
    Ok(())
}

/// Takes the dotted key `key` out of `table`.
fn remove_path(table: &mut toml::Table, key: &str) -> Option<toml::Value> {
    let (path, leaf) = key.rsplit_once('.').map_or(("", key), |(p, l)| (p, l));
    let mut at = table;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        at = at.get_mut(part)?.as_table_mut()?;
    }
    at.remove(leaf)
}
