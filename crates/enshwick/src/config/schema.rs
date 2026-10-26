//! A settings type's schema: each field's key, stage, kind and default, as
//! the loader and the settings screen read it.

use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, Scope, Stage};

/// The TOML type of a setting's value.
///
/// # Examples
///
/// ```
/// use enshwick::config::Kind;
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
/// use enshwick::config::{Field, Stage};
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
    /// A `startup` setting a joined client takes from the server, as it
    /// would a `sync` one: `#[config(startup, sync)]`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub synced: bool,
    /// The number a list's first element shows with on the Mods screen,
    /// such as 0 for a list by Flame level: `#[config(first = 0)]`.
    #[serde(default = "first_element", skip_serializing_if = "is_first_element")]
    pub first: u32,
}

/// The number a list's first element shows with unless the field says.
const fn first_element() -> u32 {
    1
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde passes a reference"
)]
const fn is_first_element(first: &u32) -> bool {
    *first == 1
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
            first: first_element(),
            ..Self::default()
        }
    }

    /// The field numbering a list's elements from `first` on the Mods
    /// screen.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Stage};
    ///
    /// let per_level = Field::new("per_level", Stage::Startup, &[2, 4]).with_first(0);
    /// assert_eq!(per_level.first, 0);
    /// ```
    #[must_use]
    pub const fn with_first(mut self, first: u32) -> Self {
        self.first = first;
        self
    }

    /// The field inside the group `group`: its key and its default text key
    /// prefixed with the group's.
    #[doc(hidden)]
    #[must_use]
    pub fn __in_group(mut self, group: &str) -> Self {
        let inner = format!("config.{}", self.key);
        self.key = format!("{group}.{}", self.key);
        if self.text == inner {
            self.text = format!("config.{}", self.key);
        }
        self
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
    /// use enshwick::config::{Field, Stage};
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
    /// use enshwick::config::{Field, Stage};
    ///
    /// let field = Field::new("limit", Stage::Startup, &4).with_doc("How many.", "lockers.limit");
    /// assert_eq!((field.doc.as_str(), field.text.as_str()), ("How many.", "lockers.limit"));
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
    /// use enshwick::config::{Field, Stage};
    ///
    /// let field = Field::new("password", Stage::Runtime, "").with_secret(true);
    /// assert!(field.secret);
    /// ```
    #[must_use]
    pub const fn with_secret(mut self, secret: bool) -> Self {
        self.secret = secret;
        self
    }

    /// The field, at [`Stage::Startup`], also taken from the server by a
    /// joined client, or not.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Stage};
    ///
    /// let field = Field::new("rules", Stage::Startup, &true).with_synced(true);
    /// assert!(field.follows_server());
    /// ```
    #[must_use]
    pub const fn with_synced(mut self, synced: bool) -> Self {
        self.synced = synced;
        self
    }

    /// Whether a joined client runs with the server's value of the field:
    /// a `sync` field, or a `startup` one marked `sync` too.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Stage};
    ///
    /// assert!(Field::new("radius", Stage::Sync, &8).follows_server());
    /// assert!(!Field::new("limit", Stage::Startup, &4).follows_server());
    /// ```
    #[must_use]
    pub fn follows_server(&self) -> bool {
        self.stage == Stage::Sync || self.synced
    }

    /// Checks `value` against the field's type, range and choices; the range
    /// applies to every element of a list.
    ///
    /// # Errors
    ///
    /// [`ConfigError::Value`] naming the field and what it accepts.
    ///
    /// # Examples
    ///
    /// See [`ConfigError`].
    pub fn check(&self, value: &toml::Value) -> Result<(), ConfigError> {
        let value = &tidy(value.clone());
        if let Some(kind) = self.kind
            && !kind.accepts(Kind::of(value))
        {
            return Err(self.refused(format!("takes {}", kind_name(kind))));
        }
        if let toml::Value::Array(items) = value {
            let element = self.element_kind();
            for item in items {
                if element.is_some_and(|kind| !kind.accepts(Kind::of(item))) {
                    return Err(self.refused(format!(
                        "takes a list of {}",
                        element.map_or("values", kind_name)
                    )));
                }
                self.check_bounds(item)?;
            }
        }
        let number = self.check_bounds(value)?;
        let chosen = |choice: &String| {
            *choice == value.to_string()
                || number.is_some_and(|n| {
                    choice
                        .parse::<f64>()
                        .is_ok_and(|c| c.to_bits() == n.to_bits())
                })
        };
        if !self.choices.is_empty() && !self.choices.iter().any(chosen) {
            return Err(self.refused(format!("is {}", self.accepts())));
        }
        Ok(())
    }

    /// Holds the number `value`, named `key`, to the range; the log line
    /// when it was outside.
    fn clamp_one(&self, value: &mut toml::Value, key: &str) -> Option<String> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "bounds are compared as f64; a setting near 2^53 is no setting"
        )]
        let n = match value {
            toml::Value::Integer(n) => *n as f64,
            toml::Value::Float(n) => *n,
            _ => return None,
        };
        let bound = match (self.min, self.max) {
            (Some(min), _) if n < min => min,
            (_, Some(max)) if n > max => max,
            _ => return None,
        };
        let was = value.to_string();
        *value = match value {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a whole-number range's ends are whole numbers"
            )]
            toml::Value::Integer(_) => toml::Value::Integer(bound.round() as i64),
            _ => toml::Value::Float(bound),
        };
        Some(format!(
            "{key} = {was} is outside {}; {value} applies",
            self.accepts()
        ))
    }

    /// The kind of each element of a list setting, from its default.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Kind, Stage};
    ///
    /// let per_level = Field::new("per_level", Stage::Startup, &[2, 4, 8]);
    /// assert_eq!(per_level.element_kind(), Some(Kind::Integer));
    /// ```
    #[must_use]
    pub fn element_kind(&self) -> Option<Kind> {
        let table: toml::Table = toml::from_str(&format!("v = {}", self.default)).ok()?;
        match table.get("v")? {
            toml::Value::Array(items) => items.first().map(Kind::of),
            _ => None,
        }
    }

    /// Checks a number against the range; the number, `None` for a value
    /// that is none.
    fn check_bounds(&self, value: &toml::Value) -> Result<Option<f64>, ConfigError> {
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
            return Err(self.refused(format!("is {}", self.accepts())));
        }
        Ok(number)
    }

    fn refused(&self, need: String) -> ConfigError {
        ConfigError::Value {
            key: self.key.clone(),
            need,
        }
    }

    /// What the field accepts, for messages: `1 to 16`, `at least 0`,
    /// `one of "auto", "on", "off"` or its type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Stage};
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

/// A mod's settings described for the loader, the console and a settings menu:
/// what [`Config::schema`] returns, and what a mod hands the loader as text.
///
/// # Examples
///
/// ```
/// use enshwick::config::{Field, Schema, Stage};
///
/// let schema = Schema::new("Locker settings.", "config", vec![Field::new("limit", Stage::Startup, &4)]);
/// assert_eq!(Schema::parse(&schema.to_text())?, schema);
/// # Ok::<(), enshwick::config::ConfigError>(())
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    /// What the settings are for, in English: the struct's doc comment.
    pub doc: String,
    /// The key of the set's translated title in the mod's texts.
    pub text: String,
    /// Every field, in file order; a field of a group has a dotted key,
    /// `<group>.<key>`.
    #[serde(default, rename = "field")]
    pub fields: Vec<Field>,
    /// The groups, each a table in the settings file, in file order.
    #[serde(default, rename = "group", skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<Group>,
}

/// A group of settings: a table of the settings file, such as `[comfort]`,
/// which the Mods screen shows together.
///
/// # Examples
///
/// See [`Config`].
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// The table's key; a dotted path for a group inside a group.
    pub key: String,
    /// Its documentation, in English: the field's doc comment.
    pub doc: String,
    /// The key of its translated title in the mod's texts.
    pub text: String,
}

impl Group {
    /// The group `key`, documented by `doc`, translated under
    /// `config.<key>`.
    ///
    /// # Examples
    ///
    /// ```
    /// let group = enshwick::config::Group::new("comfort", "Comfort rules.");
    /// assert_eq!(group.text, "config.comfort");
    /// ```
    #[must_use]
    pub fn new(key: &str, doc: &str) -> Self {
        Self {
            key: key.to_owned(),
            doc: doc.trim().to_owned(),
            text: format!("config.{key}"),
        }
    }

    /// The group inside the group `outer`.
    #[doc(hidden)]
    #[must_use]
    pub fn __in_group(mut self, outer: &str) -> Self {
        let inner = format!("config.{}", self.key);
        self.key = format!("{outer}.{}", self.key);
        if self.text == inner {
            self.text = format!("config.{}", self.key);
        }
        self
    }
}

/// The value at the dotted `key` of `settings`, to change.
fn value_at_mut<'a>(settings: &'a mut toml::Table, key: &str) -> Option<&'a mut toml::Value> {
    let mut parts = key.split('.');
    let mut value = settings.get_mut(parts.next()?)?;
    for part in parts {
        value = value.as_table_mut()?.get_mut(part)?;
    }
    Some(value)
}

/// The value at `key` of `settings`, a dotted key reaching into tables:
/// `comfort.multiplier` is `multiplier` in `[comfort]`.
///
/// # Examples
///
/// ```
/// let settings: toml::Table = toml::from_str("limit = 4\n[comfort]\nmultiplier = 2.0\n")?;
/// let at = |key| enshwick::config::value_at(&settings, key).cloned();
/// assert_eq!(at("comfort.multiplier"), Some(toml::Value::Float(2.0)));
/// assert_eq!(at("limit"), Some(toml::Value::Integer(4)));
/// assert_eq!(at("comfort.missing"), None);
/// # Ok::<(), toml::de::Error>(())
/// ```
#[must_use]
pub fn value_at<'a>(settings: &'a toml::Table, key: &str) -> Option<&'a toml::Value> {
    let mut parts = key.split('.');
    let mut value = settings.get(parts.next()?)?;
    for part in parts {
        value = value.as_table()?.get(part)?;
    }
    Some(value)
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
            groups: Vec::new(),
        }
    }

    /// The schema with `groups`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Group, Schema, Stage};
    ///
    /// let multiplier = Field::new("comfort.multiplier", Stage::Runtime, &1.0);
    /// let schema = Schema::new("", "config", vec![multiplier])
    ///     .with_groups(vec![Group::new("comfort", "Comfort rules.")]);
    /// assert_eq!(schema.groups[0].key, "comfort");
    /// ```
    #[must_use]
    pub fn with_groups(mut self, groups: Vec<Group>) -> Self {
        self.groups = groups;
        self
    }

    /// Holds every number of `settings` that lies outside its field's range,
    /// list elements too, to the nearest end of the range; returns a line
    /// per value held, for the log. A file a player wrote keeps reading.
    #[must_use]
    pub(crate) fn clamp(&self, settings: &mut toml::Table) -> Vec<String> {
        let mut held = Vec::new();
        for field in &self.fields {
            if field.min.is_none() && field.max.is_none() {
                continue;
            }
            let Some(value) = value_at_mut(settings, &field.key) else {
                continue;
            };
            if let toml::Value::Array(items) = value {
                for (i, item) in items.iter_mut().enumerate() {
                    if let Some(line) = field.clamp_one(item, &format!("{}[{i}]", field.key)) {
                        held.push(line);
                    }
                }
            } else if let Some(line) = field.clamp_one(value, &field.key) {
                held.push(line);
            }
        }
        held
    }

    /// The group a field with key `key` belongs to, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Group, Schema};
    ///
    /// let schema = Schema::new("", "config", vec![])
    ///     .with_groups(vec![Group::new("comfort", "")]);
    /// assert_eq!(schema.group_of("comfort.multiplier").map(|g| g.key.as_str()), Some("comfort"));
    /// assert!(schema.group_of("limit").is_none());
    /// ```
    #[must_use]
    pub fn group_of(&self, key: &str) -> Option<&Group> {
        let (group, _) = key.rsplit_once('.')?;
        self.groups.iter().find(|g| g.key == group)
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
    /// [`ConfigError::Parse`] when `text` is not one.
    ///
    /// # Examples
    ///
    /// See [`Schema`].
    pub fn parse(text: &str) -> Result<Self, ConfigError> {
        toml::from_str(text)
            .map_err(|e| ConfigError::Parse(format!("settings schema: {}", e.message())))
    }

    /// Checks every field of `settings` that has a value against its
    /// [`Field::check`].
    ///
    /// # Errors
    ///
    /// The first field's [`ConfigError::Value`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::config::{Field, Schema, Stage};
    ///
    /// let limit = Field::new("limit", Stage::Startup, &4).with_range(Some(1.0), Some(16.0));
    /// let schema = Schema::new("", "config", vec![limit]);
    /// assert!(schema.check(&toml::from_str("limit = 20")?).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn check(&self, settings: &toml::Table) -> Result<(), ConfigError> {
        self.fields.iter().try_for_each(|field| {
            value_at(settings, &field.key).map_or(Ok(()), |value| field.check(value))
        })
    }
}

/// `value` with each float an `f32` field gave, such as 0.10000000149011612,
/// back at the decimal written for it, 0.1, so that bounds and texts read as
/// the author wrote them.
pub(super) fn tidy(value: toml::Value) -> toml::Value {
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
pub(super) fn toml_key(key: &str) -> String {
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
