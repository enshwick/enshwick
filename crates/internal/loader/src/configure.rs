//! `enshwick config <mod> [key] [value]` on the server's console reads and
//! changes the settings of mods, checked against the schema each mod hands
//! the loader.
//!
//! The answer and the file edit are pure logic here; the console runs the
//! line, writes the file and raises the mod's settings event.

use enshwick::config::{Field, Kind, Schema, Stage};
use toml_edit::DocumentMut;

use crate::texts;

#[cfg(windows)]
mod apply;
pub mod rows;
#[cfg(windows)]
pub(crate) use apply::apply;

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

/// The stage as the field's attribute names it, such as `startup, sync`.
fn stage_name(field: &Field) -> &'static str {
    if field.synced {
        "startup, sync"
    } else {
        field.stage.name()
    }
}

/// How a setting's stage reads in a list.
fn stage_note(field: &Field) -> String {
    let key = match field.stage {
        Stage::Startup if field.synced => "stage.startup-sync",
        Stage::Startup => "stage.startup",
        Stage::Restart => "stage.restart",
        Stage::Client => "stage.client",
        Stage::Sync => "stage.sync",
        _ => "stage.runtime",
    };
    texts::fill(key, &[])
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

/// A mod's reason for refusing its settings, in the reader's language.
///
/// The reason's key names a text of the mod's `texts`, filled from its
/// tab-separated `name=value` pairs; a reason no text has shows as the mod
/// gave it.
#[must_use]
pub fn mod_reason(texts: Option<&enshwick::i18n::Texts>, why: &str) -> String {
    let mut parts = why.split('\t');
    let key = parts.next().unwrap_or_default();
    let args: Vec<(&str, &str)> = parts.filter_map(|pair| pair.split_once('=')).collect();
    match texts {
        Some(texts) if texts.get(key, texts::language()).is_some() => {
            texts.fill(key, texts::language(), &args)
        }
        _ => why.replace('\t', " "),
    }
}

/// A mod's settings: one line per setting.
#[must_use]
pub fn list(id: &str, schema: &Schema, text: &str) -> String {
    let mut lines = vec![texts::fill("config.list", &[("id", id)])];
    lines.extend(schema.fields.iter().map(|field| {
        format!(
            "{} = {} ({}; {})",
            field.key,
            current(text, field),
            accepts(field),
            stage_note(field)
        )
    }));
    lines.join("\n")
}

/// One setting with its documentation.
///
/// # Errors
///
/// A reply when the mod has no such setting.
pub fn get(id: &str, schema: &Schema, text: &str, key: &str) -> Result<String, String> {
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
    lines.push(texts::fill(
        "config.takes",
        &[
            ("accepts", &accepts(field)),
            ("stage", &stage_note(field)),
            ("name", stage_name(field)),
        ],
    ));
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

fn field<'a>(id: &str, schema: &'a Schema, key: &str) -> Result<&'a Field, String> {
    schema.field(element(key).0).ok_or_else(|| {
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        texts::fill(
            "config.no-key",
            &[("id", id), ("key", key), ("keys", &keys.join(", "))],
        )
    })
}

/// `value` as typed into the chat, as a TOML value for `field`: TOML, or
/// for a text setting the words as they are.
fn typed(field: &Field, value: &str) -> Result<toml_edit::Value, String> {
    let no_value = || {
        texts::fill(
            "config.bad-value",
            &[
                ("value", value),
                ("key", &field.key),
                ("accepts", &accepts(field)),
            ],
        )
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
        return Err(refused(&texts::fill(
            "refused.value",
            &[("key", &field.key), ("accepts", &accepts(field))],
        )));
    }
    Ok(parsed)
}

/// What a change did, for the reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The settings file with the new value, comments kept.
    pub text: String,
    /// The setting's stage.
    pub stage: Stage,
    /// A `startup` setting joined players take from the server.
    pub synced: bool,
    /// The value as written.
    pub value: String,
    /// Where it was written.
    pub destination: crate::worlds::Destination,
}

/// Sets `key` to `value` in the settings file `text`, checked against
/// `schema`; the file keeps its comments and layout.
///
/// # Errors
///
/// A reply when the mod has no such setting, the value does not fit it or
/// the file is not TOML.
pub fn set(
    id: &str,
    schema: &Schema,
    text: &str,
    key: &str,
    value: &str,
) -> Result<Change, String> {
    let field = field(id, schema, key)?;
    if field.secret {
        return Err(texts::fill("remote.secret", &[("key", key)]));
    }
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        texts::fill("refused.toml", &[("id", id), ("error", e.message())])
    })?;
    let value = if let (base, Some(index)) = element(key) {
        set_element(&mut doc, field, base, index, value)?
    } else {
        let new = typed(field, value)?;
        if !crate::worlds::set_at(doc.as_table_mut(), key, new) {
            return Err(texts::fill("refused.toml", &[("id", id), ("error", key)]));
        }
        crate::worlds::value_at(doc.as_table(), key)
            .map(|v| v.clone().decorated("", "").to_string())
            .unwrap_or_default()
    };
    Ok(Change {
        text: doc.to_string(),
        stage: field.stage,
        synced: field.synced,
        value,
        destination: crate::worlds::Destination::Global,
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
) -> Result<String, String> {
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

/// The reply when a list has no element `index`.
fn no_element(field: &Field, index: usize) -> String {
    texts::fill(
        "config.no-element",
        &[("key", &field.key), ("index", &index.to_string())],
    )
}

/// The reply to a change of `id`'s `key`, on a game server or not.
#[must_use]
pub fn changed(id: &str, key: &str, change: &Change) -> String {
    use crate::worlds::Destination;
    let args = [("id", id), ("key", key), ("value", change.value.as_str())];
    match (&change.destination, change.stage) {
        (Destination::Next, _) => texts::fill("change.next", &args),
        (_, Stage::Startup) if change.synced => texts::fill("change.startup-sync", &args),
        (_, Stage::Startup) => texts::fill("change.startup", &args),
        (_, Stage::Restart) => texts::fill("change.restart", &args),
        (_, Stage::Sync) => texts::fill("change.sync", &args),
        (_, Stage::Client) => texts::fill("change.client", &args),
        _ => texts::fill("change.runtime", &args),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        Schema::new(
            "Safe settings.",
            "config",
            vec![
                Field::new("limit", Stage::Startup, &4)
                    .with_range(Some(1.0), Some(16.0))
                    .with_doc("How many stalls a player may own.", ""),
                Field::new("radius", Stage::Runtime, &8.5),
                Field::new("language", Stage::Runtime, "en").with_choices(&["en", "uk"]),
                Field::new("note", Stage::Client, "hi"),
                Field::new("token", Stage::Runtime, "").with_secret(true),
                Field::new("shared", Stage::Runtime, "letmein").with_secret(true),
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
            "[en]\n\"config.too-few\" = \"Too few stalls.\"\n[uk]\n\"config.too-few\" = \"Замало ятків.\"\n",
        )
        .unwrap();
        let uk = Some(enshwick::i18n::Language::UK_UA);
        assert_eq!(
            mod_reason(Some(&texts), "config.too-few"),
            "Too few stalls."
        );
        assert_eq!(
            texts::for_reader(uk, || mod_reason(Some(&texts), "config.too-few")),
            "Замало ятків."
        );
        assert_eq!(
            mod_reason(Some(&texts), "limit is 1 to 16"),
            "limit is 1 to 16"
        );
        assert_eq!(mod_reason(None, "config.too-few"), "config.too-few");
        let filled = enshwick::i18n::Texts::parse(
            "[en]\n\"config.min\" = \"At least {min} stalls.\"\n[uk]\n\"config.min\" = \"Щонайменше {min} ятки.\"\n",
        )
        .unwrap();
        let why = enshwick::config::ConfigError::text("config.min", &[("min", "2")]);
        let sent = "config.min\tmin=2";
        assert_eq!(why.to_string(), "config.min (min = 2)");
        assert_eq!(mod_reason(Some(&filled), sent), "At least 2 stalls.");
        assert_eq!(
            texts::for_reader(uk, || mod_reason(Some(&filled), sent)),
            "Щонайменше 2 ятки."
        );
        assert_eq!(mod_reason(None, sent), "config.min min=2");
    }

    #[test]
    fn lists_and_gets_show_values_ranges_and_stages() {
        let text = list("you.trading-post", &schema(), FILE);
        assert!(
            text.contains("limit = 4 (1 to 16; when the world loads)"),
            "{text}"
        );
        assert!(
            text.contains("language = \"en\" (one of \"en\", \"uk\"; the world's, applied live)")
        );
        assert!(text.contains("note = \"hi\" (text in quotes; each player's own)"));
        let one = get("you.trading-post", &schema(), FILE, "limit").unwrap();
        assert!(one.starts_with("you.trading-post limit = 4 (default 4)\nHow many stalls"));
        assert!(
            get("you.trading-post", &schema(), FILE, "nope")
                .unwrap_err()
                .contains("it has limit")
        );
    }

    #[test]
    fn a_startup_sync_setting_says_joined_players_get_it_when_the_world_loads() {
        let schema = Schema::new(
            "",
            "config",
            vec![Field::new("rule", Stage::Startup, &true).with_synced(true)],
        );
        let text = list("rules", &schema, "rule = true\n");
        assert!(
            text.contains(
                "rule = true (true or false; when the world loads, joined players get it)"
            ),
            "{text}"
        );
        let one = get("rules", &schema, "rule = true\n", "rule").unwrap();
        assert!(one.contains("(startup, sync)"), "{one}");
        let change = set("rules", &schema, "rule = true\n", "rule", "false").unwrap();
        assert!(change.synced);
        assert!(
            changed("rules", "rule", &change).contains("joined players get it then"),
            "{}",
            changed("rules", "rule", &change)
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
            set("you.trading-post", &schema(), &file, "token", "\"x\"").unwrap_err(),
            "token is changed in the server's file only."
        );
    }

    #[test]
    fn a_change_is_checked_and_keeps_the_comments() {
        let change = set("you.trading-post", &schema(), FILE, "limit", "6").unwrap();
        assert_eq!(change.stage, Stage::Startup);
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
                .contains("1 to 16")
        );
        assert!(set("you.trading-post", &schema(), FILE, "limit", "many").is_err());
        assert!(
            set("you.trading-post", &schema(), FILE, "language", "de")
                .unwrap_err()
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
        let broken = set("you.trading-post", &schema(), "limit = ", "limit", "2").unwrap_err();
        assert!(
            broken
                .starts_with("Not changed: mod-configs/you.trading-post.toml is not valid TOML: ")
                && broken.len() > 55,
            "{broken}"
        );
        assert_eq!(
            set("you.trading-post", &schema(), FILE, "limit", "40").unwrap_err(),
            "Not changed: limit takes 1 to 16."
        );
    }

    #[test]
    fn a_grouped_setting_is_set_in_its_table() {
        let schema = Schema::new(
            "",
            "config",
            vec![Field::new("comfort.multiplier", Stage::Runtime, &1.0)],
        );
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
            vec![
                Field::new("per_level", Stage::Startup, &[2, 4, 8])
                    .with_range(Some(1.0), Some(32.0)),
            ],
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
