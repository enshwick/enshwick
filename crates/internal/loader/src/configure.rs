//! `enshwick config <mod> [key] [value]` on the server's console reads and
//! changes the settings of mods, checked against the schema each mod hands
//! the loader.
//!
//! The answer and the file edit are pure logic here; the console runs the
//! line, writes the file and raises the mod's settings event.

use enshwick::config::{Field, Kind, Schema};
use toml_edit::DocumentMut;

use crate::texts;

#[cfg(windows)]
mod apply;
pub mod rows;
#[cfg(windows)]
pub(crate) use apply::{ApplySettingError, apply};

/// What `enshwick config` was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request<'a> {
    /// No mod: the mods with settings.
    Mods,
    /// A mod: every setting of it.
    List(&'a str),
    /// A mod and a key: one setting.
    Get(&'a str, &'a str),
    /// A mod, a key and a value: change one setting.
    Set(&'a str, &'a str, &'a str),
}

/// Reads the words after `enshwick config`.
#[must_use]
pub fn parse(args: &str) -> Request<'_> {
    let args = args.trim();
    let mut parts = args.splitn(3, char::is_whitespace);
    match (parts.next(), parts.next(), parts.next()) {
        (None | Some(""), _, _) => Request::Mods,
        (Some(id), None, _) => Request::List(id),
        (Some(id), Some(key), None) => Request::Get(id, key),
        (Some(id), Some(key), Some(value)) => Request::Set(id, key, value.trim()),
    }
}

/// The value of `key` in the settings file `text`, as TOML; the field's
/// default when the file lacks it, and a mark for a secret, which a reply
/// never shows.
fn current(text: &str, field: &Field) -> String {
    if field.secret {
        return texts::fill("config.secret", &[]);
    }
    text.parse::<toml::Table>()
        .ok()
        .and_then(|table| enshwick::config::value_at(&table, &field.key).map(ToString::to_string))
        .unwrap_or_else(|| field.default.clone())
}

/// What `field` accepts, in the game's language: `1 to 16`, `at least 0`,
/// `one of "auto", "on", "off"` or its type.
#[must_use]
pub fn accepts(field: &Field) -> String {
    if !field.choices.is_empty() {
        return texts::fill("accepts.one-of", &[("choices", &field.choices.join(", "))]);
    }
    match (field.min, field.max) {
        (Some(min), Some(max)) => texts::fill(
            "accepts.range",
            &[("min", &min.to_string()), ("max", &max.to_string())],
        ),
        (Some(min), None) => texts::fill("accepts.min", &[("min", &min.to_string())]),
        (None, Some(max)) => texts::fill("accepts.max", &[("max", &max.to_string())]),
        (None, None) => {
            let key = match field.kind {
                Some(Kind::Bool) => "kind.bool",
                Some(Kind::Integer) => "kind.integer",
                Some(Kind::Float) => "kind.float",
                Some(Kind::String) => "kind.string",
                Some(Kind::Array) => "kind.array",
                Some(Kind::Table) => "kind.table",
                Some(Kind::Datetime) => "kind.datetime",
                _ => "accepts.any",
            };
            texts::fill(key, &[])
        }
    }
}

/// A refusal of a change for `why`, in the game's language.
#[must_use]
pub fn refused(why: &str) -> String {
    texts::fill("refused", &[("why", why)])
}

/// A mod's reason, such as for refusing its settings or turning a feature
/// off, in the reader's language.
///
/// A reason the mod gave as a text of its `texts` is filled in that
/// language, as is one the API gives in the loader's texts, such as for a
/// schema section that does not read; a reason no text has shows as the
/// mod gave it.
#[must_use]
pub fn mod_reason(texts: Option<&enshwick::i18n::Texts>, why: &str) -> String {
    texts
        .and_then(|texts| texts.reason(why, texts::language()))
        .or_else(|| from_api(why).then(|| texts::reason(why)).flatten())
        .unwrap_or_else(|| why.replace('\t', " "))
}

/// The reasons the API gives in the loader's texts, each key with the
/// arguments `enshwick::__private` encodes for a section that does not read.
const API_REASONS: [(&str, &[&str]); 3] = [
    ("mods.off.section.newer", &["format", "reads"]),
    ("mods.off.section.older", &["format", "reads"]),
    ("mods.off.section.damaged", &["byte"]),
];

/// Whether `why` is one of [`API_REASONS`] with exactly its arguments, so a
/// mod's bare key never reads as the loader's text.
fn from_api(why: &str) -> bool {
    let mut parts = why.split('\t');
    let key = parts.next().unwrap_or_default();
    let names = parts.map(|pair| pair.split_once('=').map_or(pair, |(name, _)| name));
    API_REASONS
        .iter()
        .any(|&(known, args)| known == key && names.clone().eq(args.iter().copied()))
}

/// A mod's settings: one line per setting.
#[must_use]
pub fn list(id: &str, schema: &Schema, text: &str) -> String {
    let mut lines = vec![texts::fill("config.list", &[("id", id)])];
    lines.extend(schema.fields.iter().map(|field| {
        format!(
            "{} = {} ({})",
            field.key,
            current(text, field),
            accepts(field)
        )
    }));
    lines.join("\n")
}

/// One setting with its documentation.
///
/// # Errors
///
/// [`FindSettingError`] when the mod has no such setting.
pub fn get(id: &str, schema: &Schema, text: &str, key: &str) -> Result<String, FindSettingError> {
    let field = field(id, schema, key)?;
    let value = current(text, field);
    // A secret's default is as secret as its value.
    let default = if field.secret && !field.default.is_empty() {
        texts::fill("config.secret", &[])
    } else {
        field.default.clone()
    };
    let args = [
        ("id", id),
        ("key", field.key.as_str()),
        ("value", value.as_str()),
        ("default", default.as_str()),
    ];
    let mut lines = vec![if field.default.is_empty() {
        texts::fill("config.get.no-default", &args)
    } else {
        texts::fill("config.get", &args)
    }];
    if !field.doc.is_empty() {
        lines.push(field.doc.clone());
    }
    lines.push(texts::fill("config.takes", &[("accepts", &accepts(field))]));
    Ok(lines.join("\n"))
}

/// The setting a key names and the element of a list it names, if any:
/// `per_level[2]` is element 2 of `per_level`.
#[must_use]
pub fn element(key: &str) -> (&str, Option<usize>) {
    key.strip_suffix(']')
        .and_then(|rest| rest.rsplit_once('['))
        .and_then(|(base, index)| Some((base, Some(index.parse().ok()?))))
        .unwrap_or((key, None))
}

fn field<'a>(id: &str, schema: &'a Schema, key: &str) -> Result<&'a Field, FindSettingError> {
    schema
        .field(element(key).0)
        .ok_or_else(|| FindSettingError {
            id: id.to_owned(),
            key: key.to_owned(),
            keys: schema.fields.iter().map(|f| f.key.clone()).collect(),
        })
}

/// `value` as typed into the chat, as a TOML value for `field`: TOML, or
/// for a text setting the words as they are.
fn typed(field: &Field, value: &str) -> Result<toml_edit::Value, ChangeSettingError> {
    let no_value = || ChangeSettingError::BadValue {
        field: Box::new(field.clone()),
        value: value.to_owned(),
    };
    let parsed = value.parse::<toml_edit::Value>();
    let parsed = match (parsed, field.kind) {
        (Ok(toml_edit::Value::Integer(n)), Some(Kind::Float)) => {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a whole number typed for a number setting"
            )]
            let float = *n.value() as f64;
            toml_edit::Value::from(float)
        }
        (Ok(parsed), _) => parsed,
        (Err(_), Some(Kind::String)) => toml_edit::Value::from(value),
        (Err(_), _) => return Err(no_value()),
    };
    let checked: toml::Table = format!("v = {parsed}").parse().map_err(|_| no_value())?;
    if let Some(value) = checked.get("v")
        && field.check(value).is_err()
    {
        return Err(ChangeSettingError::Refused {
            field: Box::new(field.clone()),
        });
    }
    Ok(parsed)
}

/// What a change did, for the reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The settings file with the new value, comments kept.
    pub text: String,
    /// The value as written.
    pub value: String,
}

/// Sets `key` to `value` in the settings file `text`, checked against
/// `schema`; the file keeps its comments and layout.
///
/// # Errors
///
/// [`ChangeSettingError`] when the mod has no such setting, the value does
/// not fit it or the file is not TOML.
pub fn set(
    id: &str,
    schema: &Schema,
    text: &str,
    key: &str,
    value: &str,
) -> Result<Change, ChangeSettingError> {
    let field = field(id, schema, key)?;
    if field.secret {
        return Err(ChangeSettingError::Secret {
            key: key.to_owned(),
        });
    }
    let mut doc: DocumentMut = text.parse().map_err(|source| ChangeSettingError::Toml {
        id: id.to_owned(),
        source,
    })?;
    let value = if let (base, Some(index)) = element(key) {
        set_element(&mut doc, field, base, index, value)?
    } else {
        let new = typed(field, value)?;
        if !crate::worlds::set_at(doc.as_table_mut(), key, new) {
            return Err(ChangeSettingError::Unwritable {
                id: id.to_owned(),
                key: key.to_owned(),
            });
        }
        crate::worlds::value_at(doc.as_table(), key)
            .map(|v| v.clone().decorated("", "").to_string())
            .unwrap_or_default()
    };
    Ok(Change {
        text: doc.to_string(),
        value,
    })
}

/// Sets element `index` of the list `base` in `doc` to `value`, checked as
/// an element of `field`; the element as written.
fn set_element(
    doc: &mut DocumentMut,
    field: &Field,
    base: &str,
    index: usize,
    value: &str,
) -> Result<String, ChangeSettingError> {
    let mut element = field.clone();
    element.kind = field.element_kind();
    let mut new = typed(&element, value)?;
    let current = crate::worlds::value_at(doc.as_table(), base)
        .cloned()
        .or_else(|| field.default.parse::<toml_edit::Value>().ok());
    let Some(toml_edit::Value::Array(mut items)) = current else {
        return Err(no_element(field, index));
    };
    let Some(old) = items.get_mut(index) else {
        return Err(no_element(field, index));
    };
    *new.decor_mut() = old.decor().clone();
    *old = new;
    let written = old.clone().decorated("", "").to_string();
    if !crate::worlds::set_at(doc.as_table_mut(), base, toml_edit::Value::Array(items)) {
        return Err(no_element(field, index));
    }
    Ok(written)
}

/// The refusal when a list has no element `index`.
fn no_element(field: &Field, index: usize) -> ChangeSettingError {
    ChangeSettingError::NoElement {
        key: field.key.clone(),
        index,
    }
}

/// Why a mod has no setting of a key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{id} has no setting {key}")]
pub struct FindSettingError {
    id: String,
    key: String,
    /// The keys the mod has.
    keys: Vec<String>,
}

impl FindSettingError {
    /// The reply in the game's language, which names the keys the mod has.
    #[must_use]
    pub fn reply(&self) -> String {
        texts::fill(
            "config.no-key",
            &[
                ("id", &self.id),
                ("key", &self.key),
                ("keys", &self.keys.join(", ")),
            ],
        )
    }
}

/// Why a setting was not changed.
#[derive(Clone, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChangeSettingError {
    /// The mod has no such setting.
    #[error(transparent)]
    Find(#[from] FindSettingError),
    /// The setting is secret: only the server's file changes it.
    #[error("{key} is secret, changed in the server's file only")]
    Secret {
        /// The setting's key.
        key: String,
    },
    /// The settings file is no TOML.
    #[error("the settings file of {id} is no TOML: {source}")]
    Toml {
        /// The mod's id.
        id: String,
        /// What the parser found.
        #[source]
        source: toml_edit::TomlError,
    },
    /// The key cannot be written into the file's tables.
    #[error("{key} does not fit the tables of the settings file of {id}")]
    Unwritable {
        /// The mod's id.
        id: String,
        /// The setting's key.
        key: String,
    },
    /// The typed words are no value of the setting.
    #[error("{value:?} is no value of {}", field.key)]
    BadValue {
        /// The setting.
        field: Box<Field>,
        /// The words typed.
        value: String,
    },
    /// The value lies outside what the setting accepts.
    #[error("{} does not accept the value", field.key)]
    Refused {
        /// The setting.
        field: Box<Field>,
    },
    /// The list has no such element.
    #[error("{key} has no element {index}")]
    NoElement {
        /// The list's key.
        key: String,
        /// The element asked for.
        index: usize,
    },
}

impl ChangeSettingError {
    /// The reply in the game's language.
    #[must_use]
    pub fn reply(&self) -> String {
        match self {
            Self::Find(error) => error.reply(),
            Self::Secret { key } => texts::fill("remote.secret", &[("key", key)]),
            Self::Toml { id, source } => {
                texts::fill("refused.toml", &[("file", id), ("error", source.message())])
            }
            Self::Unwritable { id, key } => {
                texts::fill("refused.toml", &[("file", id), ("error", key)])
            }
            Self::BadValue { field, value } => texts::fill(
                "config.bad-value",
                &[
                    ("value", value),
                    ("key", &field.key),
                    ("accepts", &accepts(field)),
                ],
            ),
            Self::Refused { field } => refused(&texts::fill(
                "refused.value",
                &[("key", &field.key), ("accepts", &accepts(field))],
            )),
            Self::NoElement { key, index } => texts::fill(
                "config.no-element",
                &[("key", key), ("index", &index.to_string())],
            ),
        }
    }
}

/// The reply to a change of `id`'s `key`: saved for the world, applied when
/// it loads again.
#[must_use]
pub fn changed(id: &str, key: &str, change: &Change) -> String {
    texts::fill(
        "change.saved",
        &[("id", id), ("key", key), ("value", change.value.as_str())],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        Schema::new(
            "Safe settings.",
            "config",
            vec![
                Field::new("limit", &4)
                    .with_range(Some(1.0), Some(16.0))
                    .with_doc("How many stalls a player may own.", ""),
                Field::new("radius", &8.5),
                Field::new("language", "en").with_choices(&["en", "uk"]),
                Field::new("note", "hi"),
                Field::new("token", "").with_secret(true),
                Field::new("shared", "letmein").with_secret(true),
            ],
        )
    }

    const FILE: &str =
        "# Safe settings.\n\n# How many.\nlimit = 4 # four\n\nradius = 8.5\n\nlanguage = \"en\"\n";

    #[test]
    fn arguments_read_as_requests() {
        assert_eq!(parse(""), Request::Mods);
        assert_eq!(
            parse(" you.trading-post "),
            Request::List("you.trading-post")
        );
        assert_eq!(
            parse("you.trading-post limit"),
            Request::Get("you.trading-post", "limit")
        );
        assert_eq!(
            parse("you.trading-post note hello there"),
            Request::Set("you.trading-post", "note", "hello there")
        );
    }

    #[test]
    fn a_mod_s_reason_that_names_a_text_reads_in_the_reader_s_language() {
        let texts = enshwick::i18n::Texts::parse(
            "[en]\n\"stalls.too-few\" = \"Too few stalls.\"\n[uk]\n\"stalls.too-few\" = \"Замало ятків.\"\n",
        )
        .unwrap();
        let uk = Some(enshwick::i18n::Language::UK_UA);
        assert_eq!(
            mod_reason(Some(&texts), "stalls.too-few"),
            "Too few stalls."
        );
        assert_eq!(
            texts::for_reader(uk, || mod_reason(Some(&texts), "stalls.too-few")),
            "Замало ятків."
        );
        assert_eq!(
            mod_reason(Some(&texts), "limit is 1 to 16"),
            "limit is 1 to 16"
        );
        assert_eq!(mod_reason(None, "stalls.too-few"), "stalls.too-few");
        let filled = enshwick::i18n::Texts::parse(
            "[en]\n\"stalls.min\" = \"At least {min} stalls.\"\n[uk]\n\"stalls.min\" = \"Щонайменше {min} ятки.\"\n",
        )
        .unwrap();
        let why = enshwick::config::ConfigError::text("stalls.min", &[("min", "2")]);
        let sent = "stalls.min\tmin=2";
        assert_eq!(why.to_string(), "stalls.min (min = 2)");
        assert_eq!(mod_reason(Some(&filled), sent), "At least 2 stalls.");
        assert_eq!(
            texts::for_reader(uk, || mod_reason(Some(&filled), sent)),
            "Щонайменше 2 ятки."
        );
        assert_eq!(mod_reason(None, sent), "stalls.min min=2");
    }

    #[test]
    fn a_section_that_does_not_read_is_told_in_the_loader_s_texts() {
        let uk = Some(enshwick::i18n::Language::UK_UA);
        let newer = "mods.off.section.newer\tformat=99\treads=3";
        assert_eq!(
            mod_reason(None, newer),
            "its declarations are format 99, and this Enshwick reads 3: update Enshwick"
        );
        assert_eq!(
            texts::for_reader(uk, || mod_reason(None, newer)),
            "його оголошення мають формат 99, а цей Enshwick читає 3: оновіть Enshwick"
        );
        let older = "mods.off.section.older\tformat=1\treads=3";
        assert_eq!(
            mod_reason(None, older),
            "its declarations are format 1, and this Enshwick reads 3: rebuild it with this Enshwick"
        );
        let damaged = "mods.off.section.damaged\tbyte=26";
        assert_eq!(
            mod_reason(None, damaged),
            "its declarations are damaged at byte 26: rebuild it with this Enshwick"
        );
        assert_eq!(
            texts::for_reader(uk, || mod_reason(None, damaged)),
            "його оголошення пошкоджено на байті 26: зберіть його заново з цим Enshwick"
        );
        assert_eq!(mod_reason(None, "mods.back"), "mods.back");
    }

    #[test]
    fn a_mod_s_bare_section_key_shows_as_the_mod_gave_it() {
        for given in [
            "mods.off.section.newer",
            "mods.off.section.damaged\tfoo=1",
            "mods.off.section.older\tformat=1",
        ] {
            assert_eq!(mod_reason(None, given), given.replace('\t', " "));
        }
    }

    #[test]
    fn lists_and_gets_show_values_and_ranges() {
        let text = list("you.trading-post", &schema(), FILE);
        assert!(text.contains("limit = 4 (1 to 16)"), "{text}");
        assert!(text.contains("language = \"en\" (one of \"en\", \"uk\")"));
        assert!(text.contains("note = \"hi\" (text in quotes)"));
        let one = get("you.trading-post", &schema(), FILE, "limit").unwrap();
        assert!(one.starts_with("you.trading-post limit = 4 (default 4)\nHow many stalls"));
        assert!(
            get("you.trading-post", &schema(), FILE, "nope")
                .unwrap_err()
                .reply()
                .contains("it has limit")
        );
    }

    #[test]
    fn a_secret_is_never_shown_or_changed_by_a_reply() {
        let file = format!("{FILE}token = \"hunter2\"\n");
        let all = list("you.trading-post", &schema(), &file);
        let one = get("you.trading-post", &schema(), &file, "token").unwrap();
        assert!(
            !all.contains("hunter2") && all.contains("token = (secret)"),
            "{all}"
        );
        assert!(
            !one.contains("hunter2") && one.contains("(secret)"),
            "{one}"
        );
        let shared = get("you.trading-post", &schema(), &file, "shared").unwrap();
        assert!(!shared.contains("letmein"), "{shared}");
        assert_eq!(
            set("you.trading-post", &schema(), &file, "token", "\"x\"")
                .unwrap_err()
                .reply(),
            "token is changed in the server's file only."
        );
    }

    #[test]
    fn a_change_is_checked_and_keeps_the_comments() {
        let change = set("you.trading-post", &schema(), FILE, "limit", "6").unwrap();
        assert!(
            change.text.contains("# How many.\nlimit = 6 # four\n"),
            "{}",
            change.text
        );
        assert_eq!(
            changed("you.trading-post", "limit", &change),
            "you.trading-post limit = 6: saved for this world; it takes effect when the world loads again."
        );
        assert!(
            set("you.trading-post", &schema(), FILE, "limit", "40")
                .unwrap_err()
                .reply()
                .contains("1 to 16")
        );
        assert!(set("you.trading-post", &schema(), FILE, "limit", "many").is_err());
        assert!(
            set("you.trading-post", &schema(), FILE, "language", "de")
                .unwrap_err()
                .reply()
                .contains("one of")
        );
        let radius = set("you.trading-post", &schema(), FILE, "radius", "10").unwrap();
        assert!(radius.text.contains("radius = 10.0"));
        let words = set("you.trading-post", &schema(), FILE, "note", "hello there").unwrap();
        assert!(
            words.text.ends_with("note = \"hello there\"\n"),
            "{}",
            words.text
        );
        let broken = set("you.trading-post", &schema(), "limit = ", "limit", "2")
            .unwrap_err()
            .reply();
        assert!(
            broken.starts_with("Not changed: you.trading-post is not valid TOML: ")
                && broken.len() > 55,
            "{broken}"
        );
        assert_eq!(
            set("you.trading-post", &schema(), FILE, "limit", "40")
                .unwrap_err()
                .reply(),
            "Not changed: limit takes 1 to 16."
        );
    }

    #[test]
    fn a_grouped_setting_is_set_in_its_table() {
        let schema = Schema::new("", "config", vec![Field::new("comfort.multiplier", &1.0)]);
        let change = set(
            "you.rates",
            &schema,
            "limit = 4\n",
            "comfort.multiplier",
            "2.5",
        )
        .unwrap();
        assert_eq!(change.text, "limit = 4\n\n[comfort]\nmultiplier = 2.5\n");
        assert_eq!(change.value, "2.5");
    }

    #[test]
    fn an_element_of_a_list_is_set_in_place_and_checked_by_the_range() {
        let schema = Schema::new(
            "",
            "config",
            vec![Field::new("per_level", &[2, 4, 8]).with_range(Some(1.0), Some(32.0))],
        );
        let file = "# Altars.\nper_level = [2, 4, 8]\n";
        let change = set("you.altars", &schema, file, "per_level[1]", "12").unwrap();
        assert_eq!(change.text, "# Altars.\nper_level = [2, 12, 8]\n");
        assert_eq!(change.value, "12");
        assert!(set("you.altars", &schema, file, "per_level[1]", "40").is_err());
        assert!(set("you.altars", &schema, file, "per_level[3]", "4").is_err());
        let fresh = set("you.altars", &schema, "", "per_level[0]", "3").unwrap();
        assert_eq!(fresh.text, "per_level = [3, 4, 8]\n");
        assert_eq!(element("per_level[2]"), ("per_level", Some(2)));
        assert_eq!(element("per_level"), ("per_level", None));
        assert_eq!(element("odd[x]"), ("odd[x]", None));
    }
}
