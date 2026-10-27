//! Typed mod settings, in `mod-configs/<id>.toml` beside the game.
//!
//! `#[derive(Config)]` writes the file from the type and describes every
//! field in a [`Schema`]: when it takes effect ([`Stage`]), the values it
//! accepts, its documentation and the text key a translation of it uses. The
//! loader reads the schema to answer `enshwick config` on the console and to check values before it
//! writes them; a settings menu builds on the same description.

use serde::{Deserialize, Serialize};

mod reason;
mod schema;
mod table;
mod values;

pub(crate) use reason::reason;
pub use schema::{Field, Group, Kind, Schema, value_at};
pub use table::TableConfig;
pub(crate) use values::changed_keys;
#[doc(hidden)]
pub use values::{__Values, __check_field, __entry};

/// A mod's settings type: parsed from its file, which is written from the
/// type on first start.
///
/// Derive it with `#[derive(Config)]` together with serde's `Deserialize` and
/// `Serialize` and `Default`; doc comments become the file's comments. Field
/// attributes:
///
/// - a stage, one of `#[config(startup)]` (applied when a world loads),
///   `#[config(runtime)]` (the default: applied while the game runs),
///   `#[config(client)]` (each player's own, never sent to or from a server),
///   `#[config(sync)]` (the server's value applies on its joined clients) and
///   `#[config(restart)]` (read once at start; a change waits for the next
///   start of the game);
/// - `#[config(startup, sync)]`, the one pair of stages a field may name:
///   applied when a world loads, and a joined client runs with the server's
///   value, which stays the same while the world runs;
/// - `range = a..=b` (also `a..` or `..=b`) for numbers and `choices = [..]`
///   for values from a list. The console and the Mods screen refuse a value
///   outside them; a file that holds a number outside its range, a list's
///   element too, still reads, the number held to the range with a warning
///   in the mod's log;
/// - `text = "key"`, the text key of the field's translated label in the
///   mod's texts, `config.<field>` by default; its description is the key
///   with `.doc` appended, and for a list each element's label is the key
///   with `.element` appended, `{n}` in it standing for the element's number;
/// - `first = n`, for a list, the number its first element shows with, 1 by
///   default;
/// - `#[config(group)]` on a field whose type is a `Config` of its own: its
///   settings are a table of the file, `[<field>]`, with dotted keys such as
///   `<field>.<key>`, and the Mods screen shows them together. A group's
///   fields take their own stages, ranges and texts.
///
/// `#[config(validate = path)]` on the struct adds a check of its own, and
/// `#[config(text = "key")]` names the text key of the whole set, `config`
/// by default.
///
/// A field takes one stage, or `startup, sync`; any other two fail the
/// build:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
/// struct Settings {
///     /// Both at once.
///     #[config(client, sync)]
///     shared: bool,
/// }
/// ```
///
/// Join values go to every client and into the server's public rules, so a
/// secret that would join them fails the build:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
/// struct Settings {
///     /// A password every client would see.
///     #[config(sync, secret)]
///     password: String,
/// }
/// ```
///
/// `sync` pairs with `startup` alone:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
/// struct Settings {
///     /// Live and from the server at once.
///     #[config(runtime, sync)]
///     rate: u32,
/// }
/// ```
///
/// and a `startup, sync` secret would reach every client too:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
/// struct Settings {
///     /// A password every client would see.
///     #[config(startup, sync, secret)]
///     password: String,
/// }
/// ```
///
/// The file, the schema and the checks name each field as it is written, so
/// a serde attribute that reads another key fails the build:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
/// use enshwick::config::Config;
///
/// /// Altar settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
    /// use enshwick::config::{Config, Stage};
    ///
    /// /// Locker settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    /// struct Settings {
    ///     /// How many lockers one player may own.
    ///     #[config(startup, range = 1..=16)]
    ///     limit: u32,
    /// }
    ///
    /// let schema = Settings::schema();
    /// let limit = schema.field("limit").ok_or("no field")?;
    /// assert_eq!(limit.stage, Stage::Startup);
    /// assert_eq!((limit.min, limit.max), (Some(1.0), Some(16.0)));
    /// assert_eq!(limit.text, "config.limit");
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    fn schema() -> Schema {
        Schema::default()
    }

    /// The values of the fields at `stage`, as an inline TOML table; `None`
    /// when the type has none there.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Config, Stage};
    ///
    /// /// Locker settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    /// struct Settings {
    ///     /// How many lockers one player may own.
    ///     #[config(startup)]
    ///     limit: u32,
    ///     /// The language of replies.
    ///     language: String,
    /// }
    ///
    /// let settings = Settings { limit: 4, language: "uk".into() };
    /// assert_eq!(settings.values(Stage::Startup).as_deref(), Some("{ limit = 4 }"));
    /// assert_eq!(settings.values(Stage::Sync), None);
    /// ```
    fn values(&self, _stage: Stage) -> Option<String> {
        None
    }

    /// Puts `values`, as [`values`](Config::values) writes them for `stage`,
    /// over the fields at `stage`; fields `values` lacks keep their values,
    /// and an error leaves `self` as it was.
    ///
    /// # Errors
    ///
    /// A message for the log when `values` does not parse or a value does
    /// not fit its field.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Config, Stage};
    ///
    /// /// Locker settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    /// struct Settings {
    ///     /// How many lockers one player may own.
    ///     #[config(startup)]
    ///     limit: u32,
    /// }
    ///
    /// let mut settings = Settings { limit: 9 };
    /// settings.merge_values(Stage::Startup, "{ limit = 4 }")?;
    /// assert_eq!(settings.limit, 4);
    /// # Ok::<(), enshwick::config::ConfigError>(())
    /// ```
    fn merge_values(&mut self, _stage: Stage, _values: &str) -> Result<(), ConfigError> {
        Ok(())
    }

    /// Checks parsed values against each field's range and choices and the
    /// struct's own check; an error turns the mod off with the message at
    /// init, and skips a changed file at run time.
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
    /// use enshwick::config::Config;
    ///
    /// /// Comfort settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
    fn validate(&self) -> Result<(), ConfigError> {
        Ok(())
    }

    /// The join values: the fields marked `#[config(sync)]` or
    /// `#[config(startup, sync)]`, as an inline TOML table; `None` when the
    /// type has none.
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
    /// use enshwick::config::Config;
    ///
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
        self.values(Stage::Sync)
    }

    /// Puts the join values of `synced`, as [`synced`](Config::synced) writes
    /// them, over `self`; fields `synced` lacks keep their values, and an
    /// error leaves `self` as it was.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Parse`] when `synced` does not parse or a value does
    /// not fit its field.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::Config;
    ///
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
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
    /// # Ok::<(), enshwick::config::ConfigError>(())
    /// ```
    fn merge_synced(&mut self, synced: &str) -> Result<(), ConfigError> {
        self.merge_values(Stage::Sync, synced)
    }
}

/// Why settings did not read or did not pass their checks; at init it turns
/// the mod off with its message, at run time the changed file is skipped.
///
/// # Examples
///
/// ```
/// use enshwick::config::{ConfigError, Field, Stage};
///
/// let field = Field::new("limit", Stage::Startup, &4).with_range(Some(1.0), Some(16.0));
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
}

impl ConfigError {
    /// An [`Invalid`](Self::Invalid) with `why`, as the derive wraps a
    /// `validate` function's error.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::ConfigError;
    ///
    /// assert_eq!(ConfigError::invalid("first after last").to_string(), "first after last");
    /// ```
    #[must_use]
    pub fn invalid(why: impl core::fmt::Display) -> Self {
        Self::Invalid(why.to_string())
    }
}

impl From<toml::de::Error> for ConfigError {
    /// The key and the type `error` expected, never the value it found.
    fn from(error: toml::de::Error) -> Self {
        Self::Parse(reason(None, &error))
    }
}

/// When a change to a setting takes effect.
///
/// # Examples
///
/// ```
/// use enshwick::config::Stage;
///
/// assert_eq!(Stage::default(), Stage::Runtime);
/// assert!(Stage::Restart.needs_restart());
/// assert!(!Stage::Startup.needs_restart());
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    /// Read when a world loads (or at start, with none); a change takes
    /// effect when the world loads again, and the running values stay until
    /// then.
    Startup,
    /// Applied while the game runs, through the mod's
    /// [`on_config_change`](crate::Load::on_config_change).
    #[default]
    Runtime,
    /// Each player's own: never sent to or taken from a server.
    Client,
    /// The server's value: a joined client runs with it over its own file.
    Sync,
    /// Read once as the game starts, for what only a start can set up, such
    /// as hooks or content: kept in the settings file for every world, and a
    /// change waits for the next start.
    Restart,
}

impl Stage {
    /// Whether a change waits for the next start of the game.
    ///
    /// # Examples
    ///
    /// See [`Stage`].
    #[must_use]
    pub const fn needs_restart(self) -> bool {
        matches!(self, Self::Restart)
    }

    /// Where a setting at this stage lives.
    ///
    /// # Examples
    ///
    /// See [`Scope`].
    #[must_use]
    pub const fn scope(self) -> Scope {
        match self {
            Self::Client | Self::Restart => Scope::User,
            _ => Scope::World,
        }
    }

    /// The stage's name in the schema and in messages, such as `startup`.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick::config::Stage::Client.name(), "client");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Runtime => "runtime",
            Self::Client => "client",
            Self::Sync => "sync",
            Self::Restart => "restart",
        }
    }
}

/// Where a setting lives: with each player, or with the world.
///
/// # Examples
///
/// ```
/// use enshwick::config::{Scope, Stage};
///
/// assert_eq!(Stage::Client.scope(), Scope::User);
/// assert_eq!(Stage::Restart.scope(), Scope::User);
/// assert_eq!(Stage::Startup.scope(), Scope::World);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// The game's own, in the settings file `mod-configs/<id>.toml`, for
    /// every world: the `client` and `restart` stages.
    User,
    /// The world's, kept with it in `mod-data/<id>/worlds/<key>/`; a new
    /// world starts from the settings file. The `startup`, `runtime` and
    /// `sync` stages.
    #[default]
    World,
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

    /// Staged settings.
    #[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize, crate::Config)]
    #[config(text = "lockers.config")]
    struct Staged {
        /// How many, read when the world loads.
        #[config(startup, range = 1..=16)]
        limit: u32,
        /// Workers, read as the game starts.
        #[config(restart)]
        workers: u8,
        /// The mode.
        #[config(choices = ["auto", "on"], text = "lockers.mode")]
        mode: String,
        /// This player's own.
        #[config(client, range = 0.0..)]
        volume: f32,
    }

    impl Default for Staged {
        fn default() -> Self {
            Self {
                limit: 4,
                workers: 2,
                mode: "auto".into(),
                volume: 1.0,
            }
        }
    }

    /// World rules a joined client takes from the server.
    #[derive(Debug, Default, PartialEq, serde::Deserialize, serde::Serialize, crate::Config)]
    struct Rules {
        /// The rule of the world.
        #[config(startup, sync)]
        rule: bool,
        /// Read when the world loads, this side's own.
        #[config(startup)]
        limit: u32,
        /// The server's, live.
        #[config(sync)]
        rate: u32,
    }

    #[test]
    fn a_startup_sync_field_waits_for_a_world_and_joins_the_server_s_values() {
        use super::Stage;

        let rules = Rules {
            rule: true,
            limit: 4,
            rate: 2,
        };
        assert_eq!(
            rules.values(Stage::Startup).as_deref(),
            Some("{ limit = 4, rule = true }")
        );
        assert_eq!(rules.synced().as_deref(), Some("{ rate = 2, rule = true }"));
        let mut joined = Rules::default();
        joined.merge_synced("{ rule = true, rate = 2 }").unwrap();
        assert_eq!((joined.rule, joined.limit, joined.rate), (true, 0, 2));
        let schema = Rules::schema();
        let rule = schema.field("rule").unwrap();
        assert_eq!((rule.stage, rule.synced), (Stage::Startup, true));
        assert!(rule.follows_server());
        assert!(!schema.field("limit").unwrap().follows_server());
        assert!(schema.field("rate").unwrap().follows_server());
        assert_eq!(super::Schema::parse(&schema.to_text()).unwrap(), schema);
        assert!(Rules::template().contains(
            "# Takes effect when the world loads; joined players run with the server's value.\nrule = false"
        ));
    }

    #[test]
    fn the_schema_describes_stages_ranges_choices_and_texts() {
        use super::{Kind, Stage};

        let schema = Staged::schema();
        assert_eq!(schema.doc, "Staged settings.");
        assert_eq!(schema.text, "lockers.config");
        let limit = schema.field("limit").unwrap();
        assert_eq!(
            (limit.stage, limit.kind, limit.min, limit.max),
            (Stage::Startup, Some(Kind::Integer), Some(1.0), Some(16.0))
        );
        assert_eq!(
            (limit.default.as_str(), limit.doc.as_str()),
            ("4", "How many, read when the world loads.")
        );
        assert_eq!(limit.text, "config.limit");
        let mode = schema.field("mode").unwrap();
        assert_eq!(mode.stage, Stage::Runtime);
        assert_eq!(mode.choices, ["\"auto\"", "\"on\""]);
        assert_eq!(mode.text, "lockers.mode");
        let volume = schema.field("volume").unwrap();
        assert_eq!(
            (volume.stage, volume.min, volume.max),
            (Stage::Client, Some(0.0), None)
        );
        assert_eq!(super::Schema::parse(&schema.to_text()).unwrap(), schema);
        let workers = schema.field("workers").unwrap();
        assert_eq!(
            (workers.stage, workers.scope),
            (Stage::Restart, super::Scope::User)
        );
        assert!(Staged::template().contains(
            "# Takes effect when the world loads.
limit = 4"
        ));
        assert!(Staged::template().contains(
            "# Takes effect when the game starts again.
workers = 2"
        ));
    }

    #[test]
    fn values_follow_their_stage_and_validation_uses_the_schema() {
        use super::Stage;

        let mut staged = Staged::default();
        assert_eq!(
            staged.values(Stage::Startup).as_deref(),
            Some("{ limit = 4 }")
        );
        assert_eq!(
            staged.values(Stage::Client).as_deref(),
            Some("{ volume = 1.0 }")
        );
        assert_eq!(
            staged.values(Stage::Restart).as_deref(),
            Some("{ workers = 2 }")
        );
        assert_eq!(staged.values(Stage::Sync), None);
        assert_eq!(staged.synced(), None);
        staged
            .merge_values(Stage::Startup, "{ limit = 9 }")
            .unwrap();
        assert_eq!(staged.limit, 9);
        assert!(staged.validate().is_ok());
        staged.limit = 17;
        assert!(
            staged
                .validate()
                .unwrap_err()
                .to_string()
                .contains("1 to 16")
        );
        staged.limit = 3;
        staged.mode = "off".into();
        assert!(
            staged
                .validate()
                .unwrap_err()
                .to_string()
                .contains("one of")
        );
    }

    #[test]
    fn an_f32_at_its_decimal_bound_passes_and_reads_as_written() {
        /// Ratio.
        #[derive(Debug, serde::Deserialize, serde::Serialize, crate::Config)]
        struct Ratio {
            /// Share.
            #[config(range = 0.0..=0.1, choices = [0.05, 0.1, 1.0])]
            share: f32,
        }
        impl Default for Ratio {
            fn default() -> Self {
                Self { share: 0.1 }
            }
        }
        assert!(Ratio::default().validate().is_ok());
        assert!(Ratio::template().contains("share = 0.1\n"));
        assert_eq!(Ratio::schema().field("share").unwrap().default, "0.1");
        assert!(Ratio { share: 0.11 }.validate().is_err());
        let field = &Ratio::schema().fields[0];
        assert!(field.check(&toml::Value::Integer(1)).is_err());
        let whole =
            super::Field::new("whole", super::Stage::Runtime, &1.0_f64).with_choices(&[0.5, 1.0]);
        assert!(whole.check(&toml::Value::Integer(1)).is_ok());
        assert!(whole.check(&toml::Value::Integer(2)).is_err());
    }

    #[test]
    fn a_field_toml_cannot_hold_still_leaves_the_others_checked() {
        /// Mixed.
        #[derive(Default, serde::Deserialize, serde::Serialize, crate::Config)]
        struct Mixed {
            /// Keyed by number, which TOML cannot write.
            names: std::collections::HashMap<u32, String>,
            /// Checked.
            #[config(range = 1..=16)]
            limit: u32,
            /// Unset.
            #[config(range = 1..=4)]
            spare: Option<u8>,
            /// Beyond TOML's integers.
            #[config(range = 1..)]
            seed: u64,
        }
        let mut mixed = Mixed {
            names: [(1, "one".to_owned())].into(),
            limit: 99,
            seed: 1,
            ..Mixed::default()
        };
        assert!(mixed.validate().unwrap_err().to_string().contains("limit"));
        mixed.limit = 3;
        assert!(mixed.validate().is_ok());
        mixed.seed = u64::MAX;
        assert!(mixed.validate().unwrap_err().to_string().contains("seed"));
    }

    #[test]
    fn the_template_parses_back_whatever_the_names_and_docs() {
        /** Block doc,
        over two lines. */
        #[derive(
            Debug, Default, PartialEq, serde::Deserialize, serde::Serialize, crate::Config,
        )]
        struct Odd {
            /** A raw name,
            documented in a block. */
            r#type: u32,
            #[doc = "First line\nsecond line"]
            size: u8,
        }
        let template = Odd::template();
        let parsed: Odd = toml::from_str(&template).unwrap();
        assert_eq!(parsed, Odd::default());
        assert!(template.contains("#        over two lines."));
        assert!(template.contains("#second line\nsize = 0"));
        assert!(Odd::schema().field("type").is_some());
        assert_eq!(super::__entry("größe", &0), "\"größe\" = 0\n\n");
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

    #[test]
    fn a_held_change_names_its_keys_and_never_a_secret_value() {
        /// Login.
        #[derive(Default, serde::Deserialize, serde::Serialize, crate::Config)]
        struct Login {
            /// The server's password.
            #[config(startup, secret)]
            password: String,
            /// Players at most.
            #[config(startup)]
            slots: u8,
        }
        let before = Login {
            password: "old".to_owned(),
            slots: 4,
        };
        let after = Login {
            password: "hunter2".to_owned(),
            slots: 4,
        };
        let before = before.values(super::Stage::Startup).unwrap_or_default();
        let after = after.values(super::Stage::Startup).unwrap_or_default();
        let line = super::changed_keys(&before, &after);
        assert_eq!(line, "password");
        assert!(!line.contains("hunter2"));
    }

    #[test]
    fn generated_names_leave_the_users_constants_alone() {
        #[expect(non_upper_case_globals, reason = "the names the derive binds")]
        const table: u8 = 0;
        #[expect(non_upper_case_globals, reason = "the names the derive binds")]
        const defaults: u8 = 0;
        /// Names.
        #[derive(Default, serde::Deserialize, serde::Serialize, crate::Config)]
        struct Names {
            /// A field named as a generated local.
            #[config(startup)]
            table: u32,
            /// Another.
            #[config(startup)]
            slots: u32,
        }
        let mut names = Names::default();
        assert!(
            names
                .merge_values(super::Stage::Startup, "{ table = 7, slots = 3 }")
                .is_ok()
        );
        assert_eq!((names.table, names.slots, table, defaults), (7, 3, 0, 0));
    }

    #[test]
    fn a_key_with_an_empty_part_is_refused_by_name() {
        use super::{Field, Group, Schema, Stage};

        for key in ["", ".token", "token.", "auth..token"] {
            let schema = Schema::new("", "config", vec![Field::new(key, Stage::Runtime, "x")]);
            let error = Schema::parse(&schema.to_text()).unwrap_err().to_string();
            assert!(error.contains(&format!("`{key}`")), "{error}");
        }
        let grouped = Schema::new(
            "",
            "config",
            vec![Field::new("auth.token", Stage::Runtime, "x")],
        )
        .with_groups(vec![Group::new("auth..inner", "")]);
        let error = Schema::parse(&grouped.to_text()).unwrap_err().to_string();
        assert!(error.contains("`auth..inner`"), "{error}");
        let fine = Schema::new(
            "",
            "config",
            vec![Field::new("auth.token", Stage::Runtime, "x")],
        );
        assert_eq!(Schema::parse(&fine.to_text()).unwrap(), fine);
    }

    #[test]
    fn a_held_value_is_not_logged() {
        use super::{Field, Schema, Stage};

        let pin = Field::new("pin", Stage::Runtime, &4).with_range(Some(1.0), Some(16.0));
        let schema = Schema::new("", "config", vec![pin]);
        let mut settings: toml::Table = toml::from_str("pin = 4815162342").unwrap();
        let held = schema.clamp(&mut settings);
        assert_eq!(held, ["pin is outside 1 to 16; 16 applies"]);
    }
}
