//! Numbers in pack files that follow the world's settings: written, or
//! `{ setting = "<key>" }` naming a `[[setting]]` of `pack.toml`.

use serde::Deserialize;

use super::manifest::PackSetting;
use super::reader::Reader;

/// A number as a pack file writes it.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub(super) enum NumberEntry<T> {
    /// The number itself.
    Value(T),
    /// The world's value of a setting of `pack.toml`.
    Setting(SettingEntry),
}

/// `{ setting = "<key>" }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct SettingEntry {
    /// The key of a `[[setting]]` in `pack.toml`.
    pub(super) setting: String,
}

/// A number type a setting may give.
pub(super) trait PackNumber: Copy + PartialEq + core::fmt::Debug {
    /// Its name in a problem.
    const KIND: &'static str;

    /// The number `value` holds, `None` for another type or one out of range.
    fn from_toml(value: &toml::Value) -> Option<Self>;
}

impl PackNumber for f64 {
    const KIND: &'static str = "a number";

    fn from_toml(value: &toml::Value) -> Option<Self> {
        #[expect(clippy::cast_precision_loss, reason = "a setting's integer is small")]
        value
            .as_float()
            .or_else(|| value.as_integer().map(|i| i as Self))
    }
}

impl PackNumber for f32 {
    const KIND: &'static str = "a number";

    fn from_toml(value: &toml::Value) -> Option<Self> {
        #[expect(clippy::cast_possible_truncation, reason = "a chance needs no more")]
        f64::from_toml(value).map(|v| v as Self)
    }
}

impl PackNumber for u32 {
    const KIND: &'static str = "a whole number from 0";

    fn from_toml(value: &toml::Value) -> Option<Self> {
        value.as_integer().and_then(|i| Self::try_from(i).ok())
    }
}

/// A number of a pack, its setting resolved against `pack.toml`.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Number<T> {
    /// Written in the file.
    Fixed(T),
    /// The world's value of the setting `key`; `default` while none came.
    Setting { key: String, default: T },
}

impl<T: PackNumber> Number<T> {
    /// The number in force: the setting's value in `values`, or its default.
    pub(super) fn get(&self, values: &toml::Table) -> T {
        match self {
            Self::Fixed(value) => *value,
            Self::Setting { key, default } => {
                values.get(key).and_then(T::from_toml).unwrap_or(*default)
            }
        }
    }

    /// The number before any world's settings came.
    pub(super) fn default(&self) -> T {
        self.get(&toml::Table::new())
    }

    /// Whether it follows a setting.
    pub(super) const fn follows(&self) -> bool {
        matches!(self, Self::Setting { .. })
    }
}

/// `entry` resolved against `settings`, with a problem at `at` of `file`
/// for a key `pack.toml` does not declare or a default of another type.
pub(super) fn number<T: PackNumber>(
    reader: &mut Reader<'_>,
    file: &str,
    text: &str,
    at: usize,
    entry: &NumberEntry<T>,
    settings: &[PackSetting],
) -> Option<Number<T>> {
    let key = match entry {
        NumberEntry::Value(value) => return Some(Number::Fixed(*value)),
        NumberEntry::Setting(setting) => &setting.setting,
    };
    let Some(declared) = settings.iter().find(|s| &s.key == key) else {
        reader.error(
            file,
            text,
            at,
            format!("setting {key:?} is no [[setting]] of pack.toml"),
        );
        return None;
    };
    let Some(default) = T::from_toml(&declared.default) else {
        reader.error(
            file,
            text,
            at,
            format!("setting {key:?} is not {}", T::KIND),
        );
        return None;
    };
    Some(Number::Setting {
        key: key.clone(),
        default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_s_value_wins_over_its_default_when_it_reads() {
        let chance = Number::Setting {
            key: "chance".to_owned(),
            default: 0.25_f64,
        };
        let mut values = toml::Table::new();
        assert!((chance.get(&values) - 0.25).abs() < f64::EPSILON);
        values.insert("chance".to_owned(), toml::Value::Integer(1));
        assert!((chance.get(&values) - 1.0).abs() < f64::EPSILON);
        values.insert("chance".to_owned(), toml::Value::String("x".to_owned()));
        assert!((chance.get(&values) - 0.25).abs() < f64::EPSILON);
        let count: Number<u32> = Number::Setting {
            key: "count".to_owned(),
            default: 2,
        };
        values.insert("count".to_owned(), toml::Value::Integer(-3));
        assert_eq!(count.get(&values), 2, "a negative count keeps the default");
        assert!(count.follows() && !Number::Fixed(1_u32).follows());
    }
}
