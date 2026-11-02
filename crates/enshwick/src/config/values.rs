//! Checks and template entries, for the code `#[derive(Config)]` writes.

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

/// `key = value` for a template, from any serializable value.
#[doc(hidden)]
#[must_use]
pub fn __entry<T: Serialize>(key: &str, value: &T) -> String {
    match toml::Value::try_from(value) {
        Ok(value) => format!("{} = {}\n\n", toml_key(key), tidy(value)),
        Err(_) => format!("# {key}: no default could be written\n\n"),
    }
}
