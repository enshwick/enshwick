//! Typed mod settings, in `mod-configs/<id>.toml` beside the game.
//!
//! `#[derive(Config)]` writes the file from the type and describes every
//! field in a [`Schema`]: when it takes effect ([`Stage`]), the values it
//! accepts, its documentation and the text key a translation of it uses. The
//! loader reads the schema to answer `/config` and to check values before it
//! writes them; a settings menu builds on the same description.

use serde::{Deserialize, Serialize};

/// A mod's settings type: parsed from its file, which is written from the
/// type on first start.
///
/// Derive it with `#[derive(Config)]` together with serde's `Deserialize` and
/// `Serialize` and `Default`; doc comments become the file's comments. Field
/// attributes:
///
/// - a stage, one of `#[config(startup)]` (read once at start; a change waits
///   for a restart), `#[config(runtime)]` (the default: applied while the
///   game runs), `#[config(client)]` (each player's own, never sent to or from
///   a server) and `#[config(sync)]` (the server's value applies on its
///   joined clients);
/// - `range = a..=b` (also `a..` or `..=b`) for numbers and `choices = [..]`
///   for values from a list, both checked whenever the file is read and
///   before `/config` writes a value;
/// - `text = "key"`, the text key of the field's translated label in the
///   mod's texts, `config.<field>` by default; its description is the key
///   with `.doc` appended.
///
/// `#[config(validate = path)]` on the struct adds a check of its own, and
/// `#[config(text = "key")]` names the text key of the whole set, `config`
/// by default.
///
/// A field takes one stage; two fail the build:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
/// struct Settings {
///     /// Both at once.
///     #[config(client, sync)]
///     shared: bool,
/// }
/// ```
///
/// The file, the schema and the checks name each field as it is written, so
/// a serde attribute that reads another key fails the build:
///
/// ```compile_fail
/// /// Settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
/// use enshwick_loader_api::config::Config;
///
/// /// Altar settings.
/// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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

    /// What every field is, for the loader, `/config` and a settings menu.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Config, Stage};
    ///
    /// /// Safe settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    /// struct Settings {
    ///     /// How many Safes one player may own.
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
    /// use enshwick_loader_api::config::{Config, Stage};
    ///
    /// /// Safe settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    /// struct Settings {
    ///     /// How many Safes one player may own.
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
    /// use enshwick_loader_api::config::{Config, Stage};
    ///
    /// /// Safe settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
    /// struct Settings {
    ///     /// How many Safes one player may own.
    ///     #[config(startup)]
    ///     limit: u32,
    /// }
    ///
    /// let mut settings = Settings { limit: 9 };
    /// settings.merge_values(Stage::Startup, "{ limit = 4 }")?;
    /// assert_eq!(settings.limit, 4);
    /// # Ok::<(), String>(())
    /// ```
    fn merge_values(&mut self, _stage: Stage, _values: &str) -> Result<(), String> {
        Ok(())
    }

    /// Checks parsed values against each field's range and choices and the
    /// struct's own check; an error turns the mod off with the message at
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
    /// use enshwick_loader_api::config::Config;
    ///
    /// /// Comfort settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
    /// use enshwick_loader_api::config::Config;
    ///
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
    /// A message for the log when `synced` does not parse or a value does not
    /// fit its field.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::Config;
    ///
    /// /// Altar settings.
    /// #[derive(Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
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
    fn merge_synced(&mut self, synced: &str) -> Result<(), String> {
        self.merge_values(Stage::Sync, synced)
    }
}

/// When a change to a setting takes effect.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::config::Stage;
///
/// assert_eq!(Stage::default(), Stage::Runtime);
/// assert!(Stage::Startup.needs_restart());
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

impl Stage {
    /// Whether a change waits for a restart.
    ///
    /// # Examples
    ///
    /// See [`Stage`].
    #[must_use]
    pub const fn needs_restart(self) -> bool {
        matches!(self, Self::Startup)
    }

    /// Where a setting at this stage lives.
    ///
    /// # Examples
    ///
    /// See [`Scope`].
    #[must_use]
    pub const fn scope(self) -> Scope {
        match self {
            Self::Client => Scope::User,
            _ => Scope::World,
        }
    }

    /// The stage's name in the schema and in messages, such as `startup`.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick_loader_api::config::Stage::Client.name(), "client");
    /// ```
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::Runtime => "runtime",
            Self::Client => "client",
            Self::Sync => "sync",
        }
    }
}

/// Where a setting lives: with each player, or with the world.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::config::{Scope, Stage};
///
/// assert_eq!(Stage::Client.scope(), Scope::User);
/// assert_eq!(Stage::Startup.scope(), Scope::World);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Each player's own, in the settings file `mod-configs/<id>.toml`, for
    /// every world: the `client` stage.
    User,
    /// The world's, kept with it in `mod-data/<id>/worlds/<key>/`; a new
    /// world starts from the settings file. Every stage but `client`.
    #[default]
    World,
}

/// The TOML type of a setting's value.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::config::Kind;
///
/// assert_eq!(Kind::of(&toml::Value::Integer(3)), Kind::Integer);
/// ```
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `true` or `false`.
    Bool,
    /// A whole number.
    Integer,
    /// A number with a fraction; whole numbers are accepted too.
    Float,
    /// Text.
    String,
    /// A list.
    Array,
    /// A table.
    Table,
    /// A date or time.
    Datetime,
}

impl Kind {
    /// The kind of `value`.
    ///
    /// # Examples
    ///
    /// See [`Kind`].
    #[must_use]
    pub const fn of(value: &toml::Value) -> Self {
        match value {
            toml::Value::Boolean(_) => Self::Bool,
            toml::Value::Integer(_) => Self::Integer,
            toml::Value::Float(_) => Self::Float,
            toml::Value::String(_) => Self::String,
            toml::Value::Array(_) => Self::Array,
            toml::Value::Table(_) => Self::Table,
            toml::Value::Datetime(_) => Self::Datetime,
        }
    }

    /// Whether a value of `kind` fits a setting of this kind.
    const fn accepts(self, kind: Self) -> bool {
        matches!(
            (self, kind),
            (Self::Bool, Self::Bool)
                | (Self::Integer, Self::Integer)
                | (Self::Float, Self::Float | Self::Integer)
                | (Self::String, Self::String)
                | (Self::Array, Self::Array)
                | (Self::Table, Self::Table)
                | (Self::Datetime, Self::Datetime)
        )
    }
}

/// One setting, as [`Config::schema`] describes it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::config::{Field, Stage};
///
/// let field = Field::new("radius", Stage::Runtime, &8.0_f32).with_range(Some(1.0), Some(50.0));
/// assert!(field.check(&toml::Value::Float(10.0)).is_ok());
/// assert!(field.check(&toml::Value::Float(80.0)).is_err());
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// The key in the settings file.
    pub key: String,
    /// When a change takes effect.
    pub stage: Stage,
    /// Where it lives, as its stage says.
    #[serde(default)]
    pub scope: Scope,
    /// The type of its value; `None` when the default has none TOML can
    /// write, such as `None`.
    pub kind: Option<Kind>,
    /// The default value, as TOML; empty when it has none TOML can write.
    pub default: String,
    /// The smallest number it accepts.
    pub min: Option<f64>,
    /// The largest number it accepts.
    pub max: Option<f64>,
    /// The values it accepts, as TOML; empty for any.
    pub choices: Vec<String>,
    /// Its documentation, in English: the field's doc comment.
    pub doc: String,
    /// The key of its translated label in the mod's texts; the description
    /// is under this key with `.doc` appended.
    pub text: String,
    /// A secret, such as a password: never sent to a client, and changed in
    /// the file only.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub secret: bool,
}

impl Field {
    /// A field at `stage` whose default is `default`, documented by nothing
    /// yet and translated under `config.<key>`.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn new<T: Serialize + ?Sized>(key: &str, stage: Stage, default: &T) -> Self {
        let value = toml::Value::try_from(default).ok().map(tidy);
        Self {
            key: key.to_owned(),
            stage,
            scope: stage.scope(),
            kind: value.as_ref().map(Kind::of),
            default: value.map(|v| v.to_string()).unwrap_or_default(),
            text: format!("config.{key}"),
            ..Self::default()
        }
    }

    /// The field accepting numbers from `min` to `max`, each `None` for no
    /// bound.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub const fn with_range(mut self, min: Option<f64>, max: Option<f64>) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// The field accepting only `choices`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Field, Stage};
    ///
    /// let field = Field::new("mode", Stage::Runtime, "auto").with_choices(&["auto", "on", "off"]);
    /// assert!(field.check(&toml::Value::String("on".into())).is_ok());
    /// assert!(field.check(&toml::Value::String("maybe".into())).is_err());
    /// ```
    #[must_use]
    pub fn with_choices<T: Serialize>(mut self, choices: &[T]) -> Self {
        self.choices = choices
            .iter()
            .filter_map(|choice| toml::Value::try_from(choice).ok())
            .map(|value| tidy(value).to_string())
            .collect();
        self
    }

    /// The field documented by `doc` and translated under `text`; an empty
    /// `text` keeps `config.<key>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Field, Stage};
    ///
    /// let field = Field::new("limit", Stage::Startup, &4).with_doc("How many.", "safes.limit");
    /// assert_eq!((field.doc.as_str(), field.text.as_str()), ("How many.", "safes.limit"));
    /// ```
    #[must_use]
    pub fn with_doc(mut self, doc: &str, text: &str) -> Self {
        doc.trim().clone_into(&mut self.doc);
        if !text.is_empty() {
            text.clone_into(&mut self.text);
        }
        self
    }

    /// The field marked secret, or not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Field, Stage};
    ///
    /// let field = Field::new("password", Stage::Runtime, "").with_secret(true);
    /// assert!(field.secret);
    /// ```
    #[must_use]
    pub const fn with_secret(mut self, secret: bool) -> Self {
        self.secret = secret;
        self
    }

    /// Checks `value` against the field's type, range and choices.
    ///
    /// # Errors
    ///
    /// A message naming the field and what it accepts.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    pub fn check(&self, value: &toml::Value) -> Result<(), String> {
        let value = &tidy(value.clone());
        if let Some(kind) = self.kind
            && !kind.accepts(Kind::of(value))
        {
            return Err(format!("{} takes {}", self.key, kind_name(kind)));
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "bounds are compared as f64; a setting near 2^53 is no setting"
        )]
        let number = match value {
            toml::Value::Integer(n) => Some(*n as f64),
            toml::Value::Float(n) => Some(*n),
            _ => None,
        };
        if let Some(n) = number
            && !(self.min.is_none_or(|min| n >= min) && self.max.is_none_or(|max| n <= max))
        {
            return Err(format!("{} is {}", self.key, self.accepts()));
        }
        let chosen = |choice: &String| {
            *choice == value.to_string()
                || number.is_some_and(|n| {
                    choice
                        .parse::<f64>()
                        .is_ok_and(|c| c.to_bits() == n.to_bits())
                })
        };
        if !self.choices.is_empty() && !self.choices.iter().any(chosen) {
            return Err(format!("{} is {}", self.key, self.accepts()));
        }
        Ok(())
    }

    /// What the field accepts, for messages: `1 to 16`, `at least 0`,
    /// `one of "auto", "on", "off"` or its type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Field, Stage};
    ///
    /// let field = Field::new("limit", Stage::Startup, &4).with_range(Some(1.0), Some(16.0));
    /// assert_eq!(field.accepts(), "1 to 16");
    /// ```
    #[must_use]
    pub fn accepts(&self) -> String {
        if !self.choices.is_empty() {
            return format!("one of {}", self.choices.join(", "));
        }
        match (self.min, self.max) {
            (Some(min), Some(max)) => format!("{min} to {max}"),
            (Some(min), None) => format!("at least {min}"),
            (None, Some(max)) => format!("at most {max}"),
            (None, None) => self.kind.map_or("any value", kind_name).to_owned(),
        }
    }
}

/// How messages name a kind.
const fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Bool => "true or false",
        Kind::Integer => "a whole number",
        Kind::Float => "a number",
        Kind::String => "text in quotes",
        Kind::Array => "a list in [ ]",
        Kind::Table => "a table in { }",
        Kind::Datetime => "a date or time",
    }
}

/// A mod's settings described for the loader, `/config` and a settings menu:
/// what [`Config::schema`] returns, and what a mod hands the loader as text.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::config::{Field, Schema, Stage};
///
/// let schema = Schema::new("Safe settings.", "config", vec![Field::new("limit", Stage::Startup, &4)]);
/// assert_eq!(Schema::parse(&schema.to_text())?, schema);
/// # Ok::<(), String>(())
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    /// What the settings are for, in English: the struct's doc comment.
    pub doc: String,
    /// The key of the set's translated title in the mod's texts.
    pub text: String,
    /// Every field, in file order.
    #[serde(default, rename = "field")]
    pub fields: Vec<Field>,
}

impl Schema {
    /// A schema documented by `doc`, translated under `text`, with `fields`.
    ///
    /// # Examples
    ///
    /// See [`Schema`].
    #[must_use]
    pub fn new(doc: &str, text: &str, fields: Vec<Field>) -> Self {
        Self {
            doc: doc.trim().to_owned(),
            text: text.to_owned(),
            fields,
        }
    }

    /// The field with key `key`.
    ///
    /// # Examples
    ///
    /// See [`Config::schema`].
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// The schema as TOML text, as it crosses to the loader.
    ///
    /// # Examples
    ///
    /// See [`Schema`].
    #[must_use]
    pub fn to_text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    /// Reads a schema written by [`to_text`](Self::to_text).
    ///
    /// # Errors
    ///
    /// A message when `text` is not one.
    ///
    /// # Examples
    ///
    /// See [`Schema`].
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("settings schema: {}", e.message()))
    }

    /// Checks every field of `settings` that has a value against its
    /// [`Field::check`].
    ///
    /// # Errors
    ///
    /// The first field's message.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::config::{Field, Schema, Stage};
    ///
    /// let limit = Field::new("limit", Stage::Startup, &4).with_range(Some(1.0), Some(16.0));
    /// let schema = Schema::new("", "config", vec![limit]);
    /// assert!(schema.check(&toml::from_str("limit = 20").map_err(|e| e.to_string())?).is_err());
    /// # Ok::<(), String>(())
    /// ```
    pub fn check(&self, settings: &toml::Table) -> Result<(), String> {
        self.fields.iter().try_for_each(|field| {
            settings
                .get(&field.key)
                .map_or(Ok(()), |value| field.check(value))
        })
    }
}

/// Checks the serialized fields of `settings` against `schema`, for the
/// derive's `validate`.
#[doc(hidden)]
pub fn __check_field<T: Serialize>(schema: &Schema, key: &str, value: &T) -> Result<(), String> {
    let Some(field) = schema.field(key) else {
        return Ok(());
    };
    // In a table, so a `None` is left out rather than refused.
    let mut one = std::collections::BTreeMap::new();
    one.insert(key, value);
    match toml::Value::try_from(one) {
        Ok(toml::Value::Table(table)) => table.get(key).map_or(Ok(()), |v| field.check(v)),
        _ if field.min.is_none() && field.max.is_none() && field.choices.is_empty() => Ok(()),
        _ => Err(format!(
            "{key} cannot be checked: TOML cannot hold its value"
        )),
    }
}

/// `value` with each float an `f32` field gave, such as 0.10000000149011612,
/// back at the decimal written for it, 0.1, so that bounds and texts read as
/// the author wrote them.
fn tidy(value: toml::Value) -> toml::Value {
    match value {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "only a float an f32 holds exactly is replaced"
        )]
        toml::Value::Float(n) if f64::from(n as f32).to_bits() == n.to_bits() => {
            toml::Value::Float((n as f32).to_string().parse().unwrap_or(n))
        }
        toml::Value::Array(items) => toml::Value::Array(items.into_iter().map(tidy).collect()),
        toml::Value::Table(table) => {
            toml::Value::Table(table.into_iter().map(|(k, v)| (k, tidy(v))).collect())
        }
        other => other,
    }
}

/// `key` as a TOML key: bare when it may be, quoted otherwise.
fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if bare {
        key.to_owned()
    } else {
        toml::Value::String(key.to_owned()).to_string()
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
            .map_err(|e| format!("values: {}", e.message()))?;
        match document.remove("synced") {
            Some(toml::Value::Table(table)) if document.is_empty() => Ok(Self(table)),
            _ => Err("values: not an inline table".to_owned()),
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
                    .map_err(|e: toml::de::Error| format!("value {key}: {}", e.message()))
            })
            .transpose()
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
    #[config(text = "safes.config")]
    struct Staged {
        /// How many, read at start.
        #[config(startup, range = 1..=16)]
        limit: u32,
        /// The mode.
        #[config(choices = ["auto", "on"], text = "safes.mode")]
        mode: String,
        /// This player's own.
        #[config(client, range = 0.0..)]
        volume: f32,
    }

    impl Default for Staged {
        fn default() -> Self {
            Self {
                limit: 4,
                mode: "auto".into(),
                volume: 1.0,
            }
        }
    }

    #[test]
    fn the_schema_describes_stages_ranges_choices_and_texts() {
        use super::{Kind, Stage};

        let schema = Staged::schema();
        assert_eq!(schema.doc, "Staged settings.");
        assert_eq!(schema.text, "safes.config");
        let limit = schema.field("limit").unwrap();
        assert_eq!(
            (limit.stage, limit.kind, limit.min, limit.max),
            (Stage::Startup, Some(Kind::Integer), Some(1.0), Some(16.0))
        );
        assert_eq!(
            (limit.default.as_str(), limit.doc.as_str()),
            ("4", "How many, read at start.")
        );
        assert_eq!(limit.text, "config.limit");
        let mode = schema.field("mode").unwrap();
        assert_eq!(mode.stage, Stage::Runtime);
        assert_eq!(mode.choices, ["\"auto\"", "\"on\""]);
        assert_eq!(mode.text, "safes.mode");
        let volume = schema.field("volume").unwrap();
        assert_eq!(
            (volume.stage, volume.min, volume.max),
            (Stage::Client, Some(0.0), None)
        );
        assert_eq!(super::Schema::parse(&schema.to_text()).unwrap(), schema);
        assert!(Staged::template().contains(
            "# Takes effect when the world loads.
limit = 4"
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
        assert_eq!(staged.values(Stage::Sync), None);
        assert_eq!(staged.synced(), None);
        staged
            .merge_values(Stage::Startup, "{ limit = 9 }")
            .unwrap();
        assert_eq!(staged.limit, 9);
        assert!(staged.validate().is_ok());
        staged.limit = 17;
        assert!(staged.validate().unwrap_err().contains("1 to 16"));
        staged.limit = 3;
        staged.mode = "off".into();
        assert!(staged.validate().unwrap_err().contains("one of"));
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
        assert!(mixed.validate().unwrap_err().contains("limit"));
        mixed.limit = 3;
        assert!(mixed.validate().is_ok());
        mixed.seed = u64::MAX;
        assert!(mixed.validate().unwrap_err().contains("seed"));
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
}
