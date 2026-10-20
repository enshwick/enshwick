//! Typed mod settings, in `mod-configs/<id>.toml` beside the game.

/// A mod's settings type: parsed from its file, which is written from the
/// type on first start.
///
/// Derive it with `#[derive(Config)]` together with serde's `Deserialize` and
/// `Serialize` and `Default`; doc comments become the file's comments.
///
/// # Examples
///
/// ```
/// use enshwick_api::config::Config;
///
/// /// Altar settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
/// struct Settings {
///     /// Altars allowed at each Flame level.
///     altars: Vec<u8>,
/// }
///
/// assert!(Settings::template().contains("# Altars allowed at each Flame level."));
/// ```
pub trait Config: serde::de::DeserializeOwned + Default {
    /// The commented file written when the mod's settings are missing.
    fn template() -> String;

    /// Checks parsed values; an error turns the mod off with the message.
    ///
    /// # Errors
    ///
    /// A message for the log when a value is out of range.
    ///
    /// # Examples
    ///
    /// The default accepts every value; override it to check ranges.
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}

/// `key = value` for a template, from any serializable value.
#[doc(hidden)]
#[must_use]
pub fn __entry<T: serde::Serialize>(key: &str, value: &T) -> String {
    match toml::Value::try_from(value) {
        Ok(value) => format!("{key} = {value}\n\n"),
        Err(_) => format!("# {key}: no default could be written\n\n"),
    }
}
