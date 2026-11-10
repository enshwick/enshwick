//! Typed mod settings, one file per settings type in each world's
//! `<stem>-mod-settings/<id>/` beside its save.
//!
//! [`#[enshwick::config]`](macro@crate::config) describes every field in a
//! [`Schema`]: the values it accepts, its documentation and the text key a
//! translation of it uses. As a world loads, the loader writes the file from
//! the type's [`template`](Config::template), or brings it in line with the
//! type, and the mod gets the values through
//! [`Load::on_settings`](crate::Load::on_settings).

mod read;
mod reason;
mod schema;
mod table;
mod values;

pub(crate) use read::read;
pub(crate) use reason::reason;
pub use schema::{Field, Group, Kind, Schema, value_at};
pub use table::TableConfig;
#[doc(hidden)]
pub use values::{__check_field, __entry};

/// A mod's settings type: one file of each world's settings, written from
/// the type as the world first loads.
///
/// Declare the struct with [`#[enshwick::config]`](macro@crate::config) and
/// derive `Default`: every field has its default, and a key the file lacks
/// reads as it. Doc comments become the file's comments. Struct arguments:
///
/// - `file = "name"`, the file `<stem>-mod-settings/<id>/name.toml`,
///   `settings.toml` by default; a mod with several settings types names a
///   file for each, and two types of one crate with one file fail the build;
/// - `group`, for a type that is a `#[config(group)]` field of another and
///   writes no file of its own;
/// - `validate = path`, a check of its own: when it refuses, the mod keeps
///   the values it had, the defaults as a world loads;
/// - `text = "key"`, the text key of the whole set, `config` by default.
///
/// Field attributes:
///
/// - `range = a..=b` (also `a..` or `..=b`) for numbers and `choices = [..]`
///   for values from a list. The console and the Mods screen refuse a value
///   outside them; a file that holds a number outside its range, a list's
///   element too, still reads, the number held to the range, and a value
///   outside its choices reads as the default, each with a line in the
///   mod's log;
/// - `text = "key"`, the text key of the field's translated label in the
///   mod's texts, `config.<field>` by default; its description is the key
///   with `.doc` appended, and for a list each element's label is the key
///   with `.element` appended, `{n}` in it standing for the element's number;
/// - `first = n`, for a list, the number its first element shows with, 1 by
///   default;
/// - `secret`, for a value such as a password: never shown, sent to a
///   client, changed in game or logged;
/// - `#[config(group)]` on a field whose type is declared
///   `#[enshwick::config(group)]`: its settings are a table of the file,
///   `[<field>]`, with dotted keys such as `<field>.<key>`, and the Mods
///   screen shows them together. A group's fields take their own ranges and
///   texts.
///
/// Two settings types of one crate that write one file fail the build, the
/// error naming the file and pointing at both types:
///
/// ```compile_fail,E0428
/// # extern crate enshwick_core as enshwick;
/// /// Altars.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Altars {
///     /// Altars per player.
///     altars: u8,
/// }
///
/// /// Zones.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Zones {
///     /// Zone size.
///     size: u8,
/// }
/// ```
///
/// The derive alone, without `#[serde(deny_unknown_fields)]`, fails the
/// build, so a type is declared through the attribute:
///
/// ```compile_fail
/// # extern crate enshwick_core as enshwick;
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
/// struct Settings {
///     /// A misspelt key would pass unread.
///     limit: u32,
/// }
/// ```
///
/// A file name is lowercase letters, digits, `-` and `_`:
///
/// ```compile_fail
/// # extern crate enshwick_core as enshwick;
/// /// Settings.
/// #[enshwick::config(file = "Altars.toml")]
/// #[derive(Default)]
/// struct Settings {
///     /// Altars.
///     altars: u8,
/// }
/// ```
///
/// The file, the schema and the checks name each field as it is written, so
/// a serde attribute that reads another key fails the build:
///
/// ```compile_fail
/// # extern crate enshwick_core as enshwick;
/// /// Settings.
/// #[enshwick::config]
/// #[derive(Default)]
/// struct Settings {
///     /// Renamed.
///     #[serde(rename = "max-count")]
///     limit: u32,
/// }
/// ```
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::config::Config;
///
/// /// Altar settings.
/// #[enshwick::config(file = "altars")]
/// #[derive(Default)]
/// struct Settings {
///     /// Altars allowed at each Flame level.
///     altars: Vec<u8>,
/// }
///
/// assert_eq!(Settings::FILE, "altars");
/// assert!(Settings::template().contains("# Altars allowed at each Flame level."));
/// ```
pub trait Config: serde::de::DeserializeOwned + Default {
    /// The file's name in the world's settings folder of the mod, without
    /// `.toml`.
    const FILE: &'static str = "settings";

    /// The commented file written from the defaults.
    fn template() -> String;

    /// The template's entries without its heading, its tables under
    /// `path`, for a group of another `Config`.
    #[doc(hidden)]
    #[must_use]
    fn __template_body(path: &str) -> String {
        let _ = path;
        Self::template()
    }

    /// What every field is, for the loader, the console and a settings menu.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::config::Config;
    ///
    /// /// Locker settings.
    /// #[enshwick::config]
    /// #[derive(Default)]
    /// struct Settings {
    ///     /// How many lockers one player may own.
    ///     #[config(range = 1..=16)]
    ///     limit: u32,
    /// }
    ///
    /// let schema = Settings::schema();
    /// let limit = schema.field("limit").ok_or("no field")?;
    /// assert_eq!((limit.min, limit.max), (Some(1.0), Some(16.0)));
    /// assert_eq!(limit.text, "config.limit");
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    fn schema() -> Schema {
        Schema::default()
    }

    /// Checks parsed values against each field's range and choices and the
    /// struct's own check; the console and the Mods screen refuse a change
    /// it refuses, and a world's file it refuses leaves the mod on the
    /// values it had.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Value`] for a value out of its field's range or
    /// choices, [`ConfigError::Invalid`] when the struct's own check refuses.
    ///
    /// # Examples
    ///
    /// The default accepts every value; the derive takes a checking function
    /// whose error is any `Display`:
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::config::Config;
    ///
    /// /// Comfort settings.
    /// #[enshwick::config(validate = Settings::check)]
    /// #[derive(Default)]
    /// struct Settings {
    ///     /// Comfort per piece of furniture, times this factor.
    ///     multiplier: f32,
    /// }
    ///
    /// impl Settings {
    ///     fn check(&self) -> Result<(), String> {
    ///         (self.multiplier >= 0.0).then_some(()).ok_or_else(|| "a negative multiplier".into())
    ///     }
    /// }
    ///
    /// assert!(Settings { multiplier: -1.0 }.validate().is_err());
    /// ```
    fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

/// Why settings did not read or did not pass their checks: a change is
/// refused with its message, and a world's file leaves the mod on the values
/// it had.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::config::{ConfigError, Field};
///
/// let field = Field::new("limit", &4).with_range(Some(1.0), Some(16.0));
/// let refused = field.check(&toml::Value::Integer(20)).unwrap_err();
/// assert_eq!(refused, ConfigError::Value { key: "limit".into(), need: "is 1 to 16".into() });
/// assert_eq!(refused.to_string(), "limit is 1 to 16");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// The text is not settings TOML, or a value does not fit its field's
    /// type.
    #[error("{0}")]
    Parse(String),
    /// A value lies outside what its field accepts.
    #[error("{key} {need}")]
    Value {
        /// The field's key.
        key: String,
        /// What the field needs, such as `is 1 to 16`.
        need: String,
    },
    /// The settings type's own check refused the values.
    #[error("{0}")]
    Invalid(String),
    /// The settings type's own check refused the values with a text of the
    /// mod's texts, which the player reads in their language.
    #[error("{}", crate::i18n::reason::show(key, args))]
    Text {
        /// The key of the text in the mod's texts.
        key: String,
        /// The values of the text's `{name}` placeholders, by name.
        args: Vec<(String, String)>,
    },
}

impl ConfigError {
    /// An [`Invalid`](Self::Invalid) with `why`, a reason in one language.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::config::ConfigError;
    ///
    /// assert_eq!(ConfigError::invalid("first after last").to_string(), "first after last");
    /// ```
    #[must_use]
    pub fn invalid(why: impl core::fmt::Display) -> Self {
        Self::Invalid(why.to_string())
    }

    /// A [`Text`](Self::Text) refusal: the text of `key` in the mod's texts
    /// with each `{name}` filled from `args`, shown to each player in their
    /// language. A `validate` function returns it to explain a refusal in
    /// every language the mod translates.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::config::ConfigError;
    ///
    /// let refused = ConfigError::text("config.too-few", &[("min", "2")]);
    /// assert_eq!(refused.to_string(), "config.too-few (min = 2)");
    /// ```
    #[must_use]
    pub fn text(key: &str, args: &[(&str, &str)]) -> Self {
        Self::Text {
            key: key.to_owned(),
            args: args
                .iter()
                .map(|&(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
        }
    }

    /// The reason as the loader reads it from a settings check: a text
    /// refusal as its key and `name=value` pairs, tab-separated, any other
    /// as its message.
    pub(crate) fn to_reason(&self) -> String {
        match self {
            Self::Text { key, args } => crate::i18n::reason::encode(key, args),
            _ => self.to_string(),
        }
    }
}

impl From<String> for ConfigError {
    /// An [`Invalid`](ConfigError::Invalid), as a `validate` function's
    /// `String` error becomes.
    fn from(why: String) -> Self {
        Self::Invalid(why)
    }
}

impl From<&str> for ConfigError {
    /// An [`Invalid`](ConfigError::Invalid), as a `validate` function's
    /// `&str` error becomes.
    fn from(why: &str) -> Self {
        Self::Invalid(why.to_owned())
    }
}

impl From<toml::de::Error> for ConfigError {
    /// The key and the type `error` expected, never the value it found.
    fn from(error: toml::de::Error) -> Self {
        Self::Parse(reason(None, &error))
    }
}

/// Whether `name` may name a settings file: lowercase ASCII letters,
/// digits, `-` and `_`, as `#[enshwick::config(file = ...)]` takes it.
#[doc(hidden)]
#[must_use]
pub fn __file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// A settings type declared `#[enshwick::config(group)]`, a table of another
/// settings type's file.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
    message = "`{Self}` is a `#[config(group)]` field, so declare it `#[enshwick::config(group)]`",
    label = "declared without `group`"
)]
pub trait __Group: Config {}

/// Fails the build unless `T` is declared as a group.
#[doc(hidden)]
pub const fn __group<T: __Group>() {}

#[cfg(test)]
mod tests;
