//! Settings values as TOML text, for the code `#[derive(Config)]` writes.

use serde::Serialize;

use crate::config::{ConfigError, Schema};

use super::schema::{tidy, toml_key};

/// Checks the serialized fields of `settings` against `schema`, for the
/// derive's `validate`.
#[doc(hidden)]
pub fn __check_field<T: Serialize>(
    schema: &Schema,
    key: &str,
    value: &T,
) -> Result<(), ConfigError> {
    let Some(field) = schema.field(key) else {
        return Ok(());
    };
    // In a table, so a `None` is left out rather than refused.
    let mut one = std::collections::BTreeMap::new();
    one.insert(key, value);
    match toml::Value::try_from(one) {
        Ok(toml::Value::Table(table)) => table.get(key).map_or(Ok(()), |v| field.check(v)),
        _ if field.min.is_none() && field.max.is_none() && field.choices.is_empty() => Ok(()),
        _ => Err(ConfigError::Value {
            key: key.to_owned(),
            need: "cannot be checked: TOML cannot hold its value".to_owned(),
        }),
    }
}

/// The values of a settings value at one stage, built by `#[derive(Config)]`.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct __Values(toml::Table);

impl __Values {
    /// Adds `key`; a value TOML cannot hold, such as `None`, is left out.
    pub fn put<T: Serialize>(&mut self, key: &str, value: &T) {
        if let Ok(value) = toml::Value::try_from(value) {
            self.0.insert(key.to_owned(), value);
        }
    }

    /// Adds the values `text` of the group `key`, an inline table as
    /// [`finish`](Self::finish) writes it.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] when `text` is not one.
    pub fn put_group(&mut self, key: &str, text: &str) -> Result<(), ConfigError> {
        let group = Self::parse(text)?;
        self.0.insert(key.to_owned(), toml::Value::Table(group.0));
        Ok(())
    }

    /// The values of the group `key` as an inline table, `None` when it is
    /// absent.
    #[must_use]
    pub fn group(&self, key: &str) -> Option<String> {
        self.0
            .get(key)
            .and_then(toml::Value::as_table)
            .map(|table| toml::Value::Table(table.clone()).to_string())
    }

    /// The values as an inline table, keys sorted.
    #[must_use]
    pub fn finish(self) -> String {
        toml::Value::Table(self.0).to_string()
    }

    /// Reads an inline table.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] when `text` is not one.
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        let document = format!("synced = {text}");
        let mut document: toml::Table = toml::from_str(&document).map_err(|e| {
            ConfigError::Parse(format!("values: {}", super::reason(Some(&document), &e)))
        })?;
        match document.remove("synced") {
            Some(toml::Value::Table(table)) if document.is_empty() => Ok(Self(table)),
            _ => Err(ConfigError::Parse("values: not an inline table".to_owned())),
        }
    }

    /// The value of `key` as `T`, `None` when it is absent.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] when the value does not fit `T`.
    pub fn take<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, ConfigError> {
        self.0
            .get(key)
            .map(|value| {
                value.clone().try_into().map_err(|e: toml::de::Error| {
                    ConfigError::Parse(format!("value {key}: {}", super::reason(None, &e)))
                })
            })
            .transpose()
    }
}

/// The keys whose values differ between two inline tables of
/// [`__Values`], sorted and joined, for a log line that shows no values: a
/// secret field may be among them.
pub(crate) fn changed_keys(before: &str, after: &str) -> String {
    let table = |text| {
        __Values::parse(text)
            .map(|values| values.0)
            .unwrap_or_default()
    };
    let (before, after) = (table(before), table(after));
    let mut keys: Vec<&str> = before
        .keys()
        .chain(after.keys())
        .filter(|key| before.get(*key) != after.get(*key))
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.join(", ")
}

/// `key = value` for a template, from any serializable value.
#[doc(hidden)]
#[must_use]
pub fn __entry<T: Serialize>(key: &str, value: &T) -> String {
    match toml::Value::try_from(value) {
        Ok(value) => format!("{} = {}\n\n", toml_key(key), tidy(value)),
        Err(_) => format!("# {key}: no default could be written\n\n"),
    }
}
