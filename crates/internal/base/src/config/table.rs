//! Settings whose fields are known only at run time, such as a data pack's
//! or a script's: a TOML table read against a [`Schema`].

use core::ffi::c_void;
use core::fmt::Write as _;
use std::sync::Arc;

use crate::abi::{Event, Level, Status, Str};
use crate::config::{ConfigError, Schema, reason};
use crate::host::Host;
use crate::{BaseLoad, Off};

/// The file of every settings table, as of a type without its own file.
const FILE: &str = "settings";

/// One mod's settings described by a [`Schema`] rather than a type: declared
/// with [`Load::table_config`], followed with [`TableConfig::on_change`].
///
/// The world's file, its reading and the Mods screen work as they do for a
/// [`Config`](crate::config::Config) type; its values follow the file while
/// the world runs.
///
/// # Examples
///
/// ```
/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
/// use enshwick::config::{Field, Schema};
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let limit = Field::new("limit", &4).with_doc("How many.", "");
///     let (settings, defaults) = load.table_config(Schema::new("", "config", vec![limit]))?;
///     assert_eq!(defaults["limit"].as_integer(), Some(4));
///     settings.on_change(load, |values| enshwick::info!("limit {}", values["limit"]))
/// }
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct TableConfig {
    host: Host,
    schema: Arc<Schema>,
}

impl Schema {
    /// The settings file a world starts with: each field's documentation as
    /// comments over its default.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_base as enshwick;
    /// use enshwick::config::{Field, Schema};
    ///
    /// let limit = Field::new("limit", &4).with_doc("How many.", "");
    /// let text = Schema::new("Lockers.", "config", vec![limit]).template();
    /// assert!(text.contains("# How many.\nlimit = 4\n"));
    /// ```
    #[must_use]
    pub fn template(&self) -> String {
        let mut text = String::new();
        comment(&mut text, &self.doc);
        for field in &self.fields {
            text.push('\n');
            comment(&mut text, &field.doc);
            if !field.default.is_empty() {
                let key = super::schema::toml_key(&field.key);
                let _written = writeln!(text, "{key} = {}", field.default);
            }
        }
        text
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

    /// `text` read against the schema as [`read`](crate::config::read)
    /// reads a type: what does not read takes its default, with a line
    /// through `log`.
    fn read_table(&self, text: &str, log: &dyn Fn(&str)) -> toml::Table {
        let mut table = if text.trim().is_empty() {
            toml::Table::new()
        } else {
            toml::from_str(text).unwrap_or_else(|e| {
                log(&format!(
                    "{FILE}.toml: {}; the defaults apply",
                    reason(Some(text), &e)
                ));
                toml::Table::new()
            })
        };
        for line in self.settle(&mut table) {
            log(&format!("{FILE}.toml: {line}"));
        }
        self.fill_defaults(&mut table);
        table
    }
}

impl BaseLoad {
    /// As `Load::table_config`.
    #[doc(hidden)]
    pub fn table_config(&mut self, schema: Schema) -> Result<(TableConfig, toml::Table), Off> {
        refuse_unsupported(&schema)?;
        let host = self.host();
        host.config_schema(FILE, &schema.to_text(), &schema.template())?;
        let schema = Arc::new(schema);
        let user = Arc::into_raw(Arc::clone(&schema))
            .cast_mut()
            .cast::<c_void>();
        let checked = (host.api().config_check)(host.handle(), Str::new(FILE), check_table, user);
        if !checked.is_ok() {
            // SAFETY: the loader refused the check, so it keeps no copy of
            // `user`, which `Arc::into_raw` made just above.
            drop(unsafe { Arc::from_raw(user.cast_const().cast::<Schema>()) });
        }
        let defaults = schema.read_table("", &|_| {});
        Ok((TableConfig { host, schema }, defaults))
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

    /// Calls `handler` with the world's values as each world loads, the
    /// defaults as it stops, and the file's again each time it changes while
    /// the world runs; the thread is not fixed. What does not read takes
    /// its default, with a line in the log.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader cannot tell the mod of settings.
    ///
    /// # Examples
    ///
    /// See [`TableConfig`].
    pub fn on_change<F>(&self, load: &mut impl crate::Loading, handler: F) -> Result<(), Off>
    where
        F: Fn(toml::Table) + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        for event in [Event::SETTINGS_REPLACED, Event::SETTINGS_CHANGED] {
            let (settings, handler) = (self.clone(), Arc::clone(&handler));
            load.base_mut().on_raw_event(event, move |_| {
                match settings.host.settings_text(FILE) {
                    Ok(text) => handler(
                        settings
                            .schema
                            .read_table(&text, &|line| settings.host.log(Level::WARN, line)),
                    ),
                    Err(off) => settings
                        .host
                        .log(Level::WARN, &format!("settings not read: {off}")),
                }
            })?;
        }
        Ok(())
    }
}

/// Refuses what [`Load::table_config`] does not take: groups and dotted
/// or empty keys.
fn refuse_unsupported(schema: &Schema) -> Result<(), Off> {
    if !schema.groups.is_empty() {
        return Err(Off::new("settings: a schema read as a table has no groups"));
    }
    for field in &schema.fields {
        let key = &field.key;
        if key.is_empty() {
            return Err(Off::new("settings: a field has an empty key"));
        }
        if key.contains('.') {
            return Err(Off::new(format!(
                "settings: {key} is a dotted key; a table has top-level keys"
            )));
        }
    }
    Ok(())
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
                .map_err(|e| ConfigError::Parse(reason(Some(text), &e)))
                .and_then(|table| schema.check(&table));
            let Err(reason) = checked else {
                return Status::OK;
            };
            crate::load::CHECK_WHY.with_borrow_mut(|kept| {
                *kept = reason.to_reason();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Field, Group};

    #[test]
    fn groups_and_dotted_or_empty_keys_are_refused() {
        let grouped = Schema::new("", "config", vec![Field::new("a", &1)])
            .with_groups(vec![Group::new("g", "")]);
        assert!(refuse_unsupported(&grouped).is_err());
        let dotted = Schema::new("", "config", vec![Field::new("g.a", &1)]);
        assert!(refuse_unsupported(&dotted).is_err());
        let empty = Schema::new("", "config", vec![Field::new("", &1)]);
        assert!(refuse_unsupported(&empty).is_err());
        let fine = Schema::new("", "config", vec![Field::new("a", &1).with_secret(true)]);
        assert!(refuse_unsupported(&fine).is_ok());
    }

    #[test]
    fn a_table_reads_its_file_with_every_missing_or_bad_value_at_its_default() {
        let schema = Schema::new(
            "",
            "config",
            vec![
                Field::new("limit", &4).with_range(Some(1.0), Some(16.0)),
                Field::new("name", "x"),
            ],
        );
        let lines = std::cell::RefCell::new(Vec::new());
        let table = schema.read_table("limit = 20\nname = 3\n", &|line| {
            lines.borrow_mut().push(line.to_owned());
        });
        assert_eq!(table["limit"].as_integer(), Some(16));
        assert_eq!(table["name"].as_str(), Some("x"));
        assert_eq!(lines.borrow().len(), 2);
        assert_eq!(
            schema.read_table("", &|_| {})["limit"].as_integer(),
            Some(4)
        );
    }
}
