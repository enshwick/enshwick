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

    /// Checks parsed values; an error turns the mod off with the message at
    /// init, and skips a changed file at run time.
    ///
    /// # Errors
    ///
    /// A message for the log when a value is out of range.
    ///
    /// # Examples
    ///
    /// The default accepts every value; the derive takes a checking function:
    ///
    /// ```
    /// use enshwick_api::config::Config;
    ///
    /// /// Comfort settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
    /// #[config(validate = Settings::check)]
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
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    /// The join values: the fields marked `#[config(sync)]`, as an inline
    /// TOML table; `None` when the type has none.
    ///
    /// A join value is one the server and its players must agree on, such as
    /// a table the client's interface also reads. The server advertises these
    /// values, and a client that joins it runs with them over its own file:
    /// its [`on_config_change`](crate::Load::on_config_change) handler gets
    /// the merged settings.
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
    ///     #[config(sync)]
    ///     altars: Vec<u8>,
    ///     /// Log every placed altar; this side's own business.
    ///     verbose: bool,
    /// }
    ///
    /// let settings = Settings { altars: vec![2, 4], verbose: true };
    /// assert_eq!(settings.synced().as_deref(), Some("{ altars = [2, 4] }"));
    /// ```
    fn synced(&self) -> Option<String> {
        None
    }

    /// Puts the join values of `synced`, as [`synced`](Config::synced) writes
    /// them, over `self`; fields `synced` lacks keep their values, and an
    /// error leaves `self` as it was.
    ///
    /// # Errors
    ///
    /// A message for the log when `synced` does not parse or a value does not
    /// fit its field.
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
    ///     #[config(sync)]
    ///     altars: Vec<u8>,
    ///     /// Log every placed altar; this side's own business.
    ///     verbose: bool,
    /// }
    ///
    /// let mut settings = Settings { altars: vec![1], verbose: true };
    /// settings.merge_synced("{ altars = [2, 4] }")?;
    /// assert_eq!((settings.altars, settings.verbose), (vec![2, 4], true));
    /// # Ok::<(), String>(())
    /// ```
    fn merge_synced(&mut self, _synced: &str) -> Result<(), String> {
        Ok(())
    }
}

/// The join values of a settings value, built by `#[derive(Config)]`.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct __Synced(toml::Table);

impl __Synced {
    /// Adds `key`; a value TOML cannot hold, such as `None`, is left out.
    pub fn put<T: serde::Serialize>(&mut self, key: &str, value: &T) {
        if let Ok(value) = toml::Value::try_from(value) {
            self.0.insert(key.to_owned(), value);
        }
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
    /// A message when `text` is not one.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut document: toml::Table = toml::from_str(&format!("synced = {text}"))
            .map_err(|e| format!("join values: {}", e.message()))?;
        match document.remove("synced") {
            Some(toml::Value::Table(table)) if document.is_empty() => Ok(Self(table)),
            _ => Err("join values: not an inline table".to_owned()),
        }
    }

    /// The value of `key` as `T`, `None` when it is absent.
    ///
    /// # Errors
    ///
    /// A message when the value does not fit `T`.
    pub fn take<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, String> {
        self.0
            .get(key)
            .map(|value| {
                value
                    .clone()
                    .try_into()
                    .map_err(|e: toml::de::Error| format!("join value {key}: {}", e.message()))
            })
            .transpose()
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

#[cfg(test)]
mod tests {
    use super::Config;

    /// Test settings.
    #[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize, crate::Config)]
    #[serde(default)]
    struct Settings {
        /// A join value.
        #[config(sync)]
        table: Vec<u16>,
        /// Another join value.
        #[config(sync)]
        name: String,
        /// This side's own.
        local: u32,
    }

    fn settings() -> Settings {
        Settings {
            table: vec![2, 4, 8],
            name: "Base \"one\"".into(),
            local: 7,
        }
    }

    #[test]
    fn only_join_values_are_written_sorted_and_inline() {
        assert_eq!(
            settings().synced().as_deref(),
            Some("{ name = 'Base \"one\"', table = [2, 4, 8] }")
        );
    }

    #[test]
    fn join_values_merge_over_the_rest_and_round_trip() {
        let synced = settings().synced().unwrap();
        let mut local = Settings {
            local: 3,
            ..Settings::default()
        };
        local.merge_synced(&synced).unwrap();
        assert_eq!(
            local,
            Settings {
                local: 3,
                ..settings()
            }
        );
        local.merge_synced("{ table = [1] }").unwrap();
        assert_eq!(local.table, [1]);
        assert_eq!(local.name, "Base \"one\"");
    }

    #[test]
    fn a_bad_value_leaves_the_settings_as_they_were() {
        let mut local = settings();
        assert!(local.merge_synced("{ name = 'x', table = 'no' }").is_err());
        assert!(local.merge_synced("table = [1]").is_err());
        assert!(local.merge_synced("{ table = [1] }\nname = 'x'").is_err());
        assert_eq!(local, settings());
    }

    #[test]
    fn types_without_join_values_write_none() {
        /// Plain.
        #[derive(Default, serde::Deserialize, serde::Serialize, crate::Config)]
        struct Plain {
            /// A value.
            value: u8,
        }
        let mut plain = Plain { value: 1 };
        assert_eq!(plain.synced(), None);
        assert!(plain.merge_synced("{ value = 2 }").is_ok());
        assert_eq!(plain.value, 1);
    }
}
