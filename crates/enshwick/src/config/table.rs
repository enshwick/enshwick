//! Settings whose fields are known only at run time, such as a data pack's
//! or a script's: a TOML table checked against a [`Schema`].

use core::ffi::c_void;
use core::fmt::Write as _;
use std::sync::{Arc, Mutex};

use crate::abi::{Event, Level, Status, Str};
use crate::config::{ConfigError, Schema, Stage};
use crate::host::Host;
use crate::locks::Relock;
use crate::{Load, Off};

/// The values in force at the stages that wait: `startup` until the next
/// world, `restart` until the next start.
#[derive(Debug, Default)]
struct Kept {
    startup: Option<toml::Table>,
    restart: Option<toml::Table>,
}

/// One mod's settings described by a [`Schema`] rather than a type: read
/// with [`Load::table_config`], followed with [`TableConfig::on_change`].
///
/// The file, its stages, the Mods screen and the join values work as they
/// do for a [`Config`](crate::config::Config) type.
///
/// # Examples
///
/// ```
/// use enshwick::config::{Field, Schema, Stage};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let limit = Field::new("limit", Stage::Runtime, &4).with_doc("How many.", "");
///     let (settings, values) = load.table_config(Schema::new("", "config", vec![limit]))?;
///     assert_eq!(values["limit"].as_integer(), Some(4));
///     settings.on_change(load, |values| enshwick::info!("limit {}", values["limit"]))
/// }
/// ```
#[derive(Clone, Debug)]
pub struct TableConfig {
    host: Host,
    schema: Arc<Schema>,
    kept: Arc<Mutex<Kept>>,
}

impl Schema {
    /// The settings file a player sees first: each field's documentation as
    /// comments over its default, and a note for one that waits.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Schema, Stage};
    ///
    /// let limit = Field::new("limit", Stage::Startup, &4).with_doc("How many.", "");
    /// let text = Schema::new("Lockers.", "config", vec![limit]).template();
    /// assert!(text.contains("# How many.\n# Takes effect when the world loads.\nlimit = 4\n"));
    /// ```
    #[must_use]
    pub fn template(&self) -> String {
        let mut text = String::new();
        comment(&mut text, &self.doc);
        for field in &self.fields {
            text.push('\n');
            comment(&mut text, &field.doc);
            text.push_str(match (field.stage, field.synced) {
                (_, true) | (Stage::Sync, _) => {
                    "# The server's value applies to the players who join it.\n"
                }
                (Stage::Startup, _) => "# Takes effect when the world loads.\n",
                (Stage::Restart, _) => "# Takes effect when the game starts again.\n",
                _ => "",
            });
            if !field.default.is_empty() {
                let key = super::schema::toml_key(&field.key);
                let _written = writeln!(text, "{key} = {}", field.default);
            }
        }
        text
    }

    /// The fields of `table` that `select` picks, as an inline table; `None`
    /// when it picks none.
    fn values_where(
        &self,
        table: &toml::Table,
        select: impl Fn(&super::Field) -> bool,
    ) -> Option<toml::Table> {
        let picked: toml::Table = self
            .fields
            .iter()
            .filter(|f| select(f))
            .filter_map(|f| table.get(&f.key).map(|v| (f.key.clone(), v.clone())))
            .collect();
        (!picked.is_empty()).then_some(picked)
    }

    /// Every field `table` lacks, at its default.
    fn fill_defaults(&self, table: &mut toml::Table) {
        for field in &self.fields {
            if table.contains_key(&field.key) || field.default.is_empty() {
                continue;
            }
            let parsed: Result<toml::Table, _> = toml::from_str(&format!("v = {}", field.default));
            if let Some(value) = parsed.ok().and_then(|mut t| t.remove("v")) {
                table.insert(field.key.clone(), value);
            }
        }
    }
}

impl Load<'_> {
    /// The mod's settings described by `schema`: `mod-configs/<id>.toml`,
    /// written from [`Schema::template`] when missing, its values checked
    /// against the schema and every missing one at its default. The loader
    /// gets the schema for the Mods screen and the console.
    ///
    /// # Errors
    ///
    /// [`Off`] when the file does not parse or a value is refused.
    ///
    /// # Examples
    ///
    /// See [`TableConfig`].
    pub fn table_config(&mut self, schema: Schema) -> Result<(TableConfig, toml::Table), Off> {
        let host = self.host();
        host.config_schema(&schema.to_text());
        let schema = Arc::new(schema);
        let user = Arc::into_raw(Arc::clone(&schema))
            .cast_mut()
            .cast::<c_void>();
        let _checked = (host.api().config_check)(host.handle(), check_table, user);
        let settings = TableConfig {
            host,
            schema,
            kept: Arc::new(Mutex::new(Kept::default())),
        };
        let values = settings.read(|kept, table| {
            kept.startup = settings
                .schema
                .values_where(table, |f| f.stage == Stage::Startup);
            kept.restart = settings
                .schema
                .values_where(table, |f| f.stage == Stage::Restart);
        })?;
        Ok((settings, values))
    }
}

impl TableConfig {
    /// The schema the settings follow.
    ///
    /// # Examples
    ///
    /// See [`TableConfig`].
    #[must_use]
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Calls `handler` with the values in force each time the file changes,
    /// after the console or the Mods screen sets one, and as a world loads
    /// or stops, as [`Load::on_config_change`] does for a type: `startup`
    /// values change only as a world loads or stops, `restart` values never.
    /// A file that no longer reads or checks is logged and skipped.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot watch settings.
    ///
    /// # Examples
    ///
    /// See [`TableConfig`].
    pub fn on_change<F>(&self, load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(toml::Table) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        let on_world = Arc::clone(&handler);
        let world = self.clone();
        load.on_raw_event(Event::SETTINGS_REPLACED, move |_| {
            let read = world.read(|kept, table| {
                world.hold(kept.restart.as_ref(), table, Stage::Restart);
                kept.startup = world
                    .schema
                    .values_where(table, |f| f.stage == Stage::Startup);
            });
            match read {
                Ok(values) => {
                    on_world(values);
                    world.host.log(Level::INFO, "the world's settings apply");
                }
                Err(off) => world.host.log(
                    Level::WARN,
                    &format!("the world's settings are skipped: {off}"),
                ),
            }
        })?;
        let running = self.clone();
        load.on_raw_event(Event::SETTINGS_CHANGED, move |_| {
            let read = running.read(|kept, table| {
                running.hold(kept.restart.as_ref(), table, Stage::Restart);
                running.hold(kept.startup.as_ref(), table, Stage::Startup);
            });
            match read {
                Ok(values) => {
                    handler(values);
                    running.host.log(Level::INFO, "settings reloaded");
                }
                Err(off) => running.host.log(
                    Level::WARN,
                    &format!("settings change skipped, the running values stay: {off}"),
                ),
            }
        })
    }

    /// Reads the file, lets `stages` put the values in force back, and puts
    /// the server's join values over them while this client is joined.
    fn read(&self, stages: impl FnOnce(&mut Kept, &mut toml::Table)) -> Result<toml::Table, Off> {
        let text = self.host.settings_text(&self.schema.template())?;
        let mut table: toml::Table =
            toml::from_str(&text).map_err(|e| Off::new(format!("settings: {}", e.message())))?;
        for held in self.schema.clamp(&mut table) {
            self.host.log(Level::WARN, &format!("settings: {held}"));
        }
        self.schema.fill_defaults(&mut table);
        self.schema
            .check(&table)
            .map_err(|why| Off::new(format!("settings: {why}")))?;
        stages(&mut self.kept.relock(), &mut table);
        let local = self
            .schema
            .values_where(&table, super::Field::follows_server)
            .map(|t| inline(&t))
            .unwrap_or_default();
        let Some(server) = self.host.sync(&local) else {
            return Ok(table);
        };
        match self.with_server(&mut table, &server) {
            Ok(()) => self.host.log(
                Level::INFO,
                &format!("the server's join values apply over the settings file: {server}"),
            ),
            Err(why) => self.host.log(
                Level::WARN,
                &format!("the server's join values are refused, the file's apply: {why}"),
            ),
        }
        Ok(table)
    }

    /// Puts `kept`, the values in force at `stage`, back over `table`,
    /// noting a change that waits.
    fn hold(&self, kept: Option<&toml::Table>, table: &mut toml::Table, stage: Stage) {
        let Some(kept) = kept else {
            return;
        };
        let now = self.schema.values_where(table, |f| f.stage == stage);
        if now.as_ref() == Some(kept) {
            return;
        }
        let waits = if stage == Stage::Restart {
            "the game starts again"
        } else {
            "the world loads"
        };
        let changed: Vec<&str> = kept
            .iter()
            .filter(|(key, value)| now.as_ref().and_then(|n| n.get(*key)) != Some(*value))
            .map(|(key, _)| key.as_str())
            .collect();
        self.host.log(
            Level::INFO,
            &format!(
                "settings that take effect when {waits} changed: {}",
                changed.join(", ")
            ),
        );
        for (key, value) in kept {
            table.insert(key.clone(), value.clone());
        }
    }

    /// Puts the join values `server` over `table` and checks them; an error
    /// leaves `table` as it was.
    fn with_server(&self, table: &mut toml::Table, server: &str) -> Result<(), ConfigError> {
        let parsed: toml::Table = toml::from_str(&format!("v = {server}"))
            .map_err(|e| ConfigError::Parse(e.message().to_owned()))?;
        let Some(toml::Value::Table(values)) = parsed.get("v") else {
            return Err(ConfigError::Parse(
                "the join values are no table".to_owned(),
            ));
        };
        let mut merged = table.clone();
        for (key, value) in values {
            if self
                .schema
                .field(key)
                .is_some_and(super::Field::follows_server)
            {
                merged.insert(key.clone(), value.clone());
            }
        }
        self.schema.check(&merged)?;
        *table = merged;
        Ok(())
    }
}

/// Appends `doc` to `text` as comment lines.
fn comment(text: &mut String, doc: &str) {
    for line in doc.lines() {
        if line.trim().is_empty() {
            text.push_str("#\n");
        } else {
            let _written = writeln!(text, "# {line}");
        }
    }
}

/// `table` as an inline TOML table, as join values cross to the loader.
fn inline(table: &toml::Table) -> String {
    let entries: Vec<String> = table
        .iter()
        .map(|(key, value)| format!("{} = {value}", super::schema::toml_key(key)))
        .collect();
    format!("{{ {} }}", entries.join(", "))
}

/// Checks a settings text against the schema at `user`, for the loader's
/// check before it writes a change.
unsafe extern "system" fn check_table(text: Str, why: *mut Str, user: *mut c_void) -> Status {
    crate::host::guard(
        "panic in the settings check",
        || Status::REFUSED,
        || {
            // SAFETY: `user` is the `Arc<Schema>` `table_config` leaked for
            // the process; it is never freed.
            let Some(schema) = (unsafe { user.cast::<Schema>().as_ref() }) else {
                return Status::OK;
            };
            // SAFETY: the loader passes the text for the call.
            let text = unsafe { text.as_str() }.unwrap_or("");
            let checked = toml::from_str::<toml::Table>(text)
                .map_err(ConfigError::from)
                .and_then(|table| schema.check(&table));
            let Err(reason) = checked else {
                return Status::OK;
            };
            crate::load::CHECK_WHY.with_borrow_mut(|kept| {
                *kept = reason.to_string();
                if !why.is_null() {
                    // SAFETY: the loader's out pointer; the reason stays in
                    // this thread's `CHECK_WHY` until its next check.
                    unsafe { why.write(Str::new(kept)) };
                }
            });
            Status::REFUSED
        },
    )
}
