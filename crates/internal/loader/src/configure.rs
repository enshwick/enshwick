//! `/config <mod> [key] [value]`: administrators read and change the
//! settings of mods, checked against the schema each mod hands the loader.
//!
//! The answer and the file edit are pure logic here; the chat module runs
//! the command, writes the file and raises the mod's settings event.

use enshwick_loader_api::config::{Field, Kind, Schema, Stage};
use toml_edit::DocumentMut;

use crate::texts;

#[cfg(windows)]
mod apply;
pub mod rows;
#[cfg(windows)]
pub(crate) use apply::apply;

/// The command's name.
pub const COMMAND: &str = "config";

/// What `/config` was asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request<'a> {
    /// `/config`: the mods with settings.
    Mods,
    /// `/config <mod>`: every setting of a mod.
    List(&'a str),
    /// `/config <mod> <key>`: one setting.
    Get(&'a str, &'a str),
    /// `/config <mod> <key> <value>`: change one setting.
    Set(&'a str, &'a str, &'a str),
}

/// Reads the arguments of `/config`.
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

/// The help line of `/config`.
#[must_use]
pub fn help() -> String {
    texts::fill("config.help", &[])
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
        .and_then(|table| table.get(&field.key).map(ToString::to_string))
        .unwrap_or_else(|| field.default.clone())
}

/// How a setting's stage reads in a list.
fn stage_note(stage: Stage) -> String {
    let key = match stage {
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

/// `/config <mod>`: one line per setting.
#[must_use]
pub fn list(id: &str, schema: &Schema, text: &str) -> String {
    let mut lines = vec![texts::fill("config.list", &[("id", id)])];
    lines.extend(schema.fields.iter().map(|field| {
        format!(
            "{} = {} ({}; {})",
            field.key,
            current(text, field),
            accepts(field),
            stage_note(field.stage)
        )
    }));
    lines.join("\n")
}

/// `/config <mod> <key>`: the setting with its documentation.
///
/// # Errors
///
/// A reply when the mod has no such setting.
pub fn get(id: &str, schema: &Schema, text: &str, key: &str) -> Result<String, String> {
    let field = field(id, schema, key)?;
    let value = current(text, field);
    let args = [
        ("id", id),
        ("key", field.key.as_str()),
        ("value", value.as_str()),
        ("default", field.default.as_str()),
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
            ("stage", &stage_note(field.stage)),
            ("name", field.stage.name()),
        ],
    ));
    Ok(lines.join("\n"))
}

fn field<'a>(id: &str, schema: &'a Schema, key: &str) -> Result<&'a Field, String> {
    schema.field(key).ok_or_else(|| {
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
    let mut new = typed(field, value)?;
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
        texts::fill("refused.toml", &[("id", id), ("error", e.message())])
    })?;
    match doc.get_mut(key).and_then(toml_edit::Item::as_value_mut) {
        Some(old) => {
            *new.decor_mut() = old.decor().clone();
            *old = new;
        }
        None => {
            doc.insert(key, toml_edit::Item::Value(new));
        }
    }
    let value = doc
        .get(key)
        .and_then(toml_edit::Item::as_value)
        .map(|v| v.clone().decorated("", "").to_string())
        .unwrap_or_default();
    Ok(Change {
        text: doc.to_string(),
        stage: field.stage,
        value,
        destination: crate::worlds::Destination::Global,
    })
}

/// The reply to a change of `id`'s `key`, on a game server or not.
#[must_use]
pub fn changed(id: &str, key: &str, change: &Change) -> String {
    use crate::worlds::Destination;
    let args = [("id", id), ("key", key), ("value", change.value.as_str())];
    match (&change.destination, change.stage) {
        (Destination::Next, _) => texts::fill("change.next", &args),
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
                    .with_doc("How many Safes a player may own.", ""),
                Field::new("radius", Stage::Runtime, &8.5),
                Field::new("language", Stage::Runtime, "en").with_choices(&["en", "uk"]),
                Field::new("note", Stage::Client, "hi"),
                Field::new("token", Stage::Runtime, "").with_secret(true),
            ],
        )
    }

    const FILE: &str =
        "# Safe settings.\n\n# How many.\nlimit = 4 # four\n\nradius = 8.5\n\nlanguage = \"en\"\n";

    #[test]
    fn arguments_read_as_requests() {
        assert_eq!(parse(""), Request::Mods);
        assert_eq!(parse(" safes "), Request::List("safes"));
        assert_eq!(parse("safes limit"), Request::Get("safes", "limit"));
        assert_eq!(
            parse("safes note hello there"),
            Request::Set("safes", "note", "hello there")
        );
    }

    #[test]
    fn lists_and_gets_show_values_ranges_and_stages() {
        let text = list("safes", &schema(), FILE);
        assert!(
            text.contains("limit = 4 (1 to 16; when the world loads)"),
            "{text}"
        );
        assert!(
            text.contains("language = \"en\" (one of \"en\", \"uk\"; the world's, applied live)")
        );
        assert!(text.contains("note = \"hi\" (text in quotes; each player's own)"));
        let one = get("safes", &schema(), FILE, "limit").unwrap();
        assert!(one.starts_with("safes limit = 4 (default 4)\nHow many Safes"));
        assert!(
            get("safes", &schema(), FILE, "nope")
                .unwrap_err()
                .contains("it has limit")
        );
    }

    #[test]
    fn a_secret_is_never_shown_or_changed_by_a_reply() {
        let file = format!("{FILE}token = \"hunter2\"\n");
        let all = list("safes", &schema(), &file);
        let one = get("safes", &schema(), &file, "token").unwrap();
        assert!(
            !all.contains("hunter2") && all.contains("token = (secret)"),
            "{all}"
        );
        assert!(
            !one.contains("hunter2") && one.contains("(secret)"),
            "{one}"
        );
        assert_eq!(
            set("safes", &schema(), &file, "token", "\"x\"").unwrap_err(),
            "token is changed in the server's file only."
        );
    }

    #[test]
    fn a_change_is_checked_and_keeps_the_comments() {
        let change = set("safes", &schema(), FILE, "limit", "6").unwrap();
        assert_eq!(change.stage, Stage::Startup);
        assert!(
            change.text.contains("# How many.\nlimit = 6 # four\n"),
            "{}",
            change.text
        );
        assert_eq!(
            changed("safes", "limit", &change),
            "safes limit = 6: saved for this world; it takes effect when the world loads again."
        );
        assert!(
            set("safes", &schema(), FILE, "limit", "40")
                .unwrap_err()
                .contains("1 to 16")
        );
        assert!(set("safes", &schema(), FILE, "limit", "many").is_err());
        assert!(
            set("safes", &schema(), FILE, "language", "de")
                .unwrap_err()
                .contains("one of")
        );
        let radius = set("safes", &schema(), FILE, "radius", "10").unwrap();
        assert!(radius.text.contains("radius = 10.0"));
        let words = set("safes", &schema(), FILE, "note", "hello there").unwrap();
        assert!(
            words.text.ends_with("note = \"hello there\"\n"),
            "{}",
            words.text
        );
        let broken = set("safes", &schema(), "limit = ", "limit", "2").unwrap_err();
        assert!(
            broken.starts_with("Not changed: mod-configs/safes.toml is not valid TOML: ")
                && broken.len() > 55,
            "{broken}"
        );
        assert_eq!(
            set("safes", &schema(), FILE, "limit", "40").unwrap_err(),
            "Not changed: limit takes 1 to 16."
        );
    }
}
