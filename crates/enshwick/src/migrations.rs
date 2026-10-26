//! Migrations: what a new version of a mod changes in what older versions
//! left behind, run once per world and recorded.
//!
//! A migration has an id and steps: settings renamed, removed or changed,
//! world stores renamed or changed, content renamed. The mod declares every
//! migration it ever made, oldest first, with [`Load::migrate`]. The loader
//! runs the settings steps on the settings file at start, and every step on
//! a world's files the first time the world loads with the migration known,
//! before the mod reads the world's settings or opens its stores; it
//! records each migration that ran in `mod-data/enshwick/migrations.toml`,
//! so none runs twice. A world the mod has never seen starts current: its
//! files come from the mod as it is now. Before it changes anything the
//! loader logs what it will do, and with `migrations_dry_run = true` in
//! `mod-configs/enshwick.toml` it only logs.
//!
//! [`Load::migrate`]: crate::Load::migrate

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::abi::{self, MigrationStep, Status, Str};
use crate::content::Guid;
use crate::{Load, Off};

/// Why a migration's map cannot change a value; the loader logs it and
/// leaves the world's files as they were.
///
/// A `&str` or `String` turns into one with `?` or `into`.
///
/// # Examples
///
/// ```
/// use enshwick::migrations::MigrationError;
///
/// fn double(old: toml::Value) -> Result<toml::Value, MigrationError> {
///     let limit = old.as_integer().ok_or("limit is a number")?;
///     Ok(toml::Value::Integer(limit * 2))
/// }
///
/// assert_eq!(double(toml::Value::Boolean(true)).unwrap_err().to_string(), "limit is a number");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MigrationError(String);

impl MigrationError {
    /// A refusal with `why`.
    ///
    /// # Examples
    ///
    /// ```
    /// let refused = enshwick::migrations::MigrationError::new("no owners table");
    /// assert_eq!(refused.to_string(), "no owners table");
    /// ```
    #[must_use]
    pub fn new(why: impl Into<String>) -> Self {
        Self(why.into())
    }
}

impl From<&str> for MigrationError {
    fn from(why: &str) -> Self {
        Self(why.to_owned())
    }
}

impl From<String> for MigrationError {
    fn from(why: String) -> Self {
        Self(why)
    }
}

impl From<toml::de::Error> for MigrationError {
    fn from(error: toml::de::Error) -> Self {
        Self(error.message().to_owned())
    }
}

/// A change of a value: a setting's TOML value, or a store's whole table.
type Map = Box<dyn Fn(&str) -> Result<String, MigrationError> + Send + Sync>;

/// One step, kept on the mod's side.
struct Step {
    kind: u32,
    from: String,
    to: String,
    from_guid: Guid,
    to_guid: Guid,
    map: Option<Map>,
}

impl core::fmt::Debug for Step {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Step")
            .field("kind", &self.kind)
            .field("from", &self.from)
            .field("to", &self.to)
            .finish_non_exhaustive()
    }
}

/// One migration of a mod: an id and the steps it takes, in order.
///
/// # Examples
///
/// See [`Migrations`].
#[derive(Debug)]
pub struct Migration {
    id: String,
    steps: Vec<Step>,
}

impl Migration {
    fn push(mut self, kind: u32, from: &str, to: &str, map: Option<Map>) -> Self {
        self.steps.push(Step {
            kind,
            from: from.to_owned(),
            to: to.to_owned(),
            from_guid: Guid([0; 16]),
            to_guid: Guid([0; 16]),
            map,
        });
        self
    }

    /// Renames the setting `from` to `to`, its value and comments kept; a
    /// file without `from` stays as it is.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn rename_setting(self, from: &str, to: &str) -> Self {
        self.push(MigrationStep::RENAME_SETTING, from, to, None)
    }

    /// Removes the setting `key`.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn remove_setting(self, key: &str) -> Self {
        self.push(MigrationStep::REMOVE_SETTING, key, "", None)
    }

    /// Changes the value of the setting `key` with `map`, which takes the
    /// old value and returns the new one or why it cannot; a file without
    /// `key` stays as it is.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn map_setting(
        self,
        key: &str,
        map: impl Fn(toml::Value) -> Result<toml::Value, MigrationError> + Send + Sync + 'static,
    ) -> Self {
        let map: Map = Box::new(move |text| {
            let value = parse_value(text)?;
            write_value(&map(value)?)
        });
        self.push(MigrationStep::MAP_SETTING, key, "", Some(map))
    }

    /// Renames the world store `from` to `to`: the file of
    /// [`Load::world_store`](crate::Load::world_store) moves.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn rename_store(self, from: &str, to: &str) -> Self {
        self.push(MigrationStep::RENAME_STORE, from, to, None)
    }

    /// Changes the world store `name` with `map`, which edits its table in
    /// place or says why it cannot; a world without the store stays as it
    /// is.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn map_store(
        self,
        name: &str,
        map: impl Fn(&mut toml::Table) -> Result<(), MigrationError> + Send + Sync + 'static,
    ) -> Self {
        let map: Map = Box::new(move |text| {
            let mut table: toml::Table = toml::from_str(text)?;
            map(&mut table)?;
            toml::to_string(&table).map_err(|e| MigrationError(e.to_string()))
        });
        self.push(MigrationStep::MAP_STORE, name, "", Some(map))
    }

    /// Reads content saved under `from`, such as a placed object of an item
    /// whose GUID changed, as the content now under `to`. An item the mod
    /// adds under `to` is also served under `from`, so stacks saved with the
    /// old item stay.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn rename_content(mut self, from: Guid, to: Guid) -> Self {
        self.steps.push(Step {
            kind: MigrationStep::RENAME_CONTENT,
            from: from.to_string(),
            to: to.to_string(),
            from_guid: from,
            to_guid: to,
            map: None,
        });
        self
    }
}

/// Every migration a mod ever made, oldest first.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::migrations::Migrations;
///
/// const OLD_SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// const SAFE: Id = Id::new("safes:safe");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.migrate(
///         Migrations::new()
///             .add("2", |m| {
///                 m.rename_setting("cost", "cost_multiplier")
///                     .rename_content(OLD_SAFE, SAFE.guid())
///             })
///             .add("3", |m| {
///                 m.map_setting("limit", |old| {
///                     let limit = old.as_integer().ok_or("limit is a number")?;
///                     Ok(toml::Value::Integer(limit * 2))
///                 })
///                 .map_store("owners", |table| {
///                     table.remove("legacy");
///                     Ok(())
///                 })
///             }),
///     )
/// }
/// ```
#[derive(Debug, Default)]
pub struct Migrations {
    list: Vec<Migration>,
}

impl Migrations {
    /// No migrations yet.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the migration `id`, after every migration added so far, with
    /// the steps `build` gives it. An id is never reused: the record keeps
    /// it for every world.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    #[must_use]
    pub fn add(mut self, id: &str, build: impl FnOnce(Migration) -> Migration) -> Self {
        self.list.push(build(Migration {
            id: id.to_owned(),
            steps: Vec::new(),
        }));
        self
    }

    /// The ids of the migrations, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let migrations = enshwick::migrations::Migrations::new()
    ///     .add("2", |m| m.remove_setting("old"))
    ///     .add("3", |m| m.rename_store("a", "b"));
    /// assert_eq!(migrations.ids(), ["2", "3"]);
    /// ```
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.list.iter().map(|m| m.id.as_str()).collect()
    }

    /// Checks the declaration: ids present and unique, names present,
    /// no content renamed to itself.
    fn check(&self) -> Result<(), String> {
        for (index, migration) in self.list.iter().enumerate() {
            if migration.id.is_empty() || migration.id.chars().any(char::is_control) {
                return Err(format!("migration {index}: an id is plain text"));
            }
            if self.list[..index].iter().any(|m| m.id == migration.id) {
                return Err(format!("migration {}: the id is used twice", migration.id));
            }
            for step in &migration.steps {
                let renames = matches!(
                    step.kind,
                    MigrationStep::RENAME_SETTING | MigrationStep::RENAME_STORE
                );
                if step.from.is_empty() || (renames && step.to.is_empty()) {
                    return Err(format!("migration {}: a step names nothing", migration.id));
                }
                if step.kind == MigrationStep::RENAME_CONTENT && step.from_guid == step.to_guid {
                    return Err(format!(
                        "migration {}: content {} renamed to itself",
                        migration.id, step.from
                    ));
                }
            }
        }
        Ok(())
    }
}

/// A TOML value from its text, as the loader hands a setting over.
pub(crate) fn parse_value(text: &str) -> Result<toml::Value, MigrationError> {
    let mut table: toml::Table = toml::from_str(&format!("v = {text}"))?;
    table.remove("v").ok_or_else(|| "no value".into())
}

/// A TOML value as text, as the loader takes a setting back.
pub(crate) fn write_value(value: &toml::Value) -> Result<String, MigrationError> {
    // `Display` writes a value inline, tables and arrays of tables too.
    let text = value.to_string();
    parse_value(&text)?;
    Ok(text)
}

/// The mod's migrations, kept for the process: the loader calls their maps.
struct Kept {
    list: Vec<Migration>,
}

static KEPT: OnceLock<Kept> = OnceLock::new();

thread_local! {
    /// The text a map handed the loader last on this thread; it stays until
    /// the thread's next map, so a map on another thread never moves it.
    static OUT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// The GUIDs content the mod adds under `guid` was saved under before, by
/// its migrations.
pub(crate) fn old_guids(guid: Guid) -> Vec<Guid> {
    KEPT.get()
        .map_or_else(Vec::new, |kept| renamed_to(&kept.list, guid))
}

/// Every GUID a chain of renames in `list` leads from to `guid`.
fn renamed_to(list: &[Migration], guid: Guid) -> Vec<Guid> {
    let mut found: Vec<Guid> = Vec::new();
    let mut at = 0;
    let mut wanted = guid;
    loop {
        for (from, to) in renames(list) {
            if to == wanted && from != guid && !found.contains(&from) {
                found.push(from);
            }
        }
        let Some(&next) = found.get(at) else {
            return found;
        };
        wanted = next;
        at += 1;
    }
}

/// The content renames of `list`, old GUID to new, in order.
fn renames(list: &[Migration]) -> impl Iterator<Item = (Guid, Guid)> + '_ {
    list.iter()
        .flat_map(|m| &m.steps)
        .filter(|s| s.kind == MigrationStep::RENAME_CONTENT)
        .map(|s| (s.from_guid, s.to_guid))
}

/// The game id content saved as `id` has now: the id of the content the
/// mod's migrations renamed `id`'s GUID to, else `id` itself. An item
/// renamed with [`Migration::rename_content`] stays in old stacks under its
/// old id, so compare item ids through this.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, Id};
/// use enshwick::migrations::{Migrations, current_id};
///
/// const OLD_SAFE: Guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
/// const SAFE: Id = Id::new("safes:safe");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     load.migrate(Migrations::new().add("2", |m| m.rename_content(OLD_SAFE, SAFE.guid())))?;
///     assert_eq!(current_id(OLD_SAFE.id()), SAFE.id());
///     assert_eq!(current_id(SAFE.id()), SAFE.id());
///     Ok(())
/// }
/// ```
#[must_use]
pub fn current_id(id: u32) -> u32 {
    KEPT.get().map_or(id, |kept| renamed_from(&kept.list, id))
}

/// The id at the end of the chain of renames in `list` that starts at
/// `id`: as many hops as there are renames, so a cycle ends.
fn renamed_from(list: &[Migration], id: u32) -> u32 {
    let mut current = id;
    for _ in renames(list) {
        match renames(list).find(|(from, _)| from.id() == current) {
            Some((_, to)) if to.id() != current && to.id() != id => current = to.id(),
            _ => break,
        }
    }
    current
}

/// Runs a step's map for the loader.
unsafe extern "system" fn run_map(
    _user: *mut core::ffi::c_void,
    migration: u32,
    step: u32,
    input: Str,
    out: *mut Str,
) -> Status {
    let Some(kept) = KEPT.get() else {
        return Status::MISSING;
    };
    let map = kept
        .list
        .get(migration as usize)
        .and_then(|m| m.steps.get(step as usize))
        .and_then(|s| s.map.as_ref());
    let Some(map) = map else {
        return Status::MISSING;
    };
    // SAFETY: the loader's text is valid for the call.
    let Some(input) = (unsafe { input.as_str() }) else {
        return Status::BAD_ARGS;
    };
    let (status, text) = crate::host::guard(
        "panic in a migration",
        || (Status::FAULTED, "the migration panicked".to_owned()),
        || match map(input) {
            Ok(text) => (Status::OK, text),
            Err(reason) => (Status::REFUSED, reason.0),
        },
    );
    OUT.with_borrow_mut(|held| {
        *held = text;
        if !out.is_null() {
            // SAFETY: the loader's out pointer; the text stays in this
            // thread's `OUT` until the thread's next call.
            unsafe { out.write(Str::new(held)) };
        }
    });
    status
}

impl Load<'_> {
    /// Hands the loader every migration the mod ever made, oldest first;
    /// call it before [`config`](Self::config), so the settings file is
    /// current when the mod reads it.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed declaration, a second call, or a migration
    /// that fails on the settings file.
    ///
    /// # Examples
    ///
    /// See [`Migrations`].
    pub fn migrate(&mut self, migrations: Migrations) -> Result<(), Off> {
        migrations
            .check()
            .map_err(|reason| Off::new(format!("migrations: {reason}")))?;
        if KEPT
            .set(Kept {
                list: migrations.list,
            })
            .is_err()
        {
            return Err(Off::new("migrations: declared twice"));
        }
        let Some(kept) = KEPT.get() else {
            return Err(Off::new("migrations: not kept"));
        };
        let steps: Vec<Vec<MigrationStep>> = kept
            .list
            .iter()
            .map(|m| {
                m.steps
                    .iter()
                    .map(|s| MigrationStep {
                        kind: s.kind,
                        from: Str::new(&s.from),
                        to: Str::new(&s.to),
                        from_guid: s.from_guid.0,
                        to_guid: s.to_guid.0,
                    })
                    .collect()
            })
            .collect();
        let list: Vec<abi::Migration> = kept
            .list
            .iter()
            .zip(&steps)
            .map(|(m, steps)| abi::Migration {
                id: Str::new(&m.id),
                steps: steps.as_ptr(),
                count: u32::try_from(steps.len()).unwrap_or(u32::MAX),
            })
            .collect();
        let set = abi::MigrationSet {
            header: abi::Header::of::<abi::MigrationSet>(),
            migrations: list.as_ptr(),
            count: u32::try_from(list.len()).unwrap_or(u32::MAX),
            map: run_map,
            user: core::ptr::null_mut(),
        };
        let host = self.host();
        let status = (host.api().migrate)(host.handle(), &raw const set);
        crate::load::ok(status, (), || {
            "migrations: refused; the log says why".to_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KEPT, Kept, Migrations, parse_value, renamed_from, renamed_to, run_map, write_value,
    };
    use crate::abi::{Status, Str};
    use crate::content::Guid;

    /// Runs the first step's map on `input` and returns the text it handed
    /// over, still in place.
    fn map(input: &str) -> Str {
        let mut out = Str::new("");
        // SAFETY: `input` lives for the call, `out` is writable.
        let status = unsafe { run_map(core::ptr::null_mut(), 0, 0, Str::new(input), &raw mut out) };
        assert_eq!(status, Status::OK);
        out
    }

    #[test]
    fn a_map_on_another_thread_leaves_this_threads_text_alone() {
        let list = Migrations::new()
            .add("1", |m| {
                m.map_setting("n", |v| {
                    Ok(toml::Value::Integer(v.as_integer().unwrap_or(0) + 1))
                })
            })
            .list;
        assert!(KEPT.set(Kept { list }).is_ok());
        let here = map("1");
        let there = std::thread::spawn(|| {
            // SAFETY: the text stays until this thread's next map.
            unsafe { map("41").as_str() }.map(str::to_owned)
        })
        .join()
        .unwrap();
        assert_eq!(there.as_deref(), Some("42"));
        // SAFETY: this thread ran no map since.
        assert_eq!(unsafe { here.as_str() }, Some("2"));
    }

    #[test]
    fn values_round_trip_as_text() {
        for text in [
            "5",
            "true",
            "\"text\"",
            "[1, 2]",
            "1.5",
            "{ a = 1 }",
            "[{ a = 1 }, { b = \"x\" }]",
        ] {
            let value = parse_value(text).unwrap();
            assert_eq!(write_value(&value).unwrap(), text);
        }
        assert!(parse_value("not toml [").is_err());
    }

    #[test]
    fn renames_follow_their_chain_both_ways() {
        let (a, b, c) = (Guid::named("t:a"), Guid::named("t:b"), Guid::named("t:c"));
        let list = Migrations::new()
            .add("2", |m| m.rename_content(a, b))
            .add("3", |m| m.rename_content(b, c))
            .list;
        assert_eq!(renamed_from(&list, a.id()), c.id());
        assert_eq!(renamed_from(&list, b.id()), c.id());
        assert_eq!(renamed_from(&list, c.id()), c.id());
        assert_eq!(renamed_to(&list, c), [b, a]);
        let cycle = Migrations::new()
            .add("2", |m| m.rename_content(a, b))
            .add("3", |m| m.rename_content(b, a))
            .list;
        assert_eq!(renamed_from(&cycle, a.id()), b.id());
        assert_eq!(renamed_to(&cycle, a), [b]);
    }

    #[test]
    fn a_declaration_is_checked() {
        assert!(
            Migrations::new()
                .add("2", |m| m.rename_setting("a", "b"))
                .check()
                .is_ok()
        );
        assert!(
            Migrations::new()
                .add("2", |m| m)
                .add("2", |m| m)
                .check()
                .is_err()
        );
        assert!(Migrations::new().add("", |m| m).check().is_err());
        assert!(
            Migrations::new()
                .add("2", |m| m.rename_setting("a", ""))
                .check()
                .is_err()
        );
        let guid = Guid::parse("8a3f6e21-5c4b-4d7e-9f10-2b7c5e6a9d31");
        assert!(
            Migrations::new()
                .add("2", |m| m.rename_content(guid, guid))
                .check()
                .is_err()
        );
    }
}
