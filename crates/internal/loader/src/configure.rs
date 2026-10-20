//! `/config <mod> [key] [value]`: administrators read and change the
//! settings of mods, checked against the schema each mod hands the loader.
//!
//! The answer and the file edit are pure logic here; the chat module runs
//! the command, writes the file and raises the mod's settings event.

use enshwick_loader_api::config::{Field, Kind, Schema, Stage};
use toml_edit::DocumentMut;

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
pub const HELP: &str = "/config: mods with settings; /config <mod>: its settings; /config <mod> <key>: one setting; /config <mod> <key> <value>: change it (administrators)";

/// The value of `key` in the settings file `text`, as TOML; the field's
/// default when the file lacks it.
fn current(text: &str, field: &Field) -> String {
    text.parse::<toml::Table>()
        .ok()
        .and_then(|table| table.get(&field.key).map(ToString::to_string))
        .unwrap_or_else(|| field.default.clone())
}

/// How a setting's stage reads in a list.
fn stage_note(stage: Stage) -> &'static str {
    match stage {
        Stage::Startup => "restart needed",
        Stage::Client => "each player's own",
        Stage::Sync => "server's value for joined players",
        _ => "applied live",
    }
}

/// `/config <mod>`: one line per setting.
#[must_use]
pub fn list(id: &str, schema: &Schema, text: &str) -> String {
    let mut lines = vec![format!("{id} settings (/config {id} <key> for one):")];
    lines.extend(schema.fields.iter().map(|field| {
        format!(
            "{} = {} ({}; {})",
            field.key,
            current(text, field),
            field.accepts(),
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
    let mut lines = vec![format!(
        "{id} {} = {} (default {})",
        field.key,
        current(text, field),
        if field.default.is_empty() {
            "none"
        } else {
            &field.default
        }
    )];
    if !field.doc.is_empty() {
        lines.push(field.doc.clone());
    }
    lines.push(format!(
        "Takes {}; {} ({}).",
        field.accepts(),
        stage_note(field.stage),
        field.stage.name()
    ));
    Ok(lines.join("\n"))
}

fn field<'a>(id: &str, schema: &'a Schema, key: &str) -> Result<&'a Field, String> {
    schema.field(key).ok_or_else(|| {
        let keys: Vec<&str> = schema.fields.iter().map(|f| f.key.as_str()).collect();
        format!("{id} has no setting {key}; it has {}.", keys.join(", "))
    })
}

/// `value` as typed into the chat, as a TOML value for `field`: TOML, or
/// for a text setting the words as they are.
fn typed(field: &Field, value: &str) -> Result<toml_edit::Value, String> {
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
        (Err(_), _) => {
            return Err(format!(
                "{value} is no value for {}: it takes {}.",
                field.key,
                field.accepts()
            ));
        }
    };
    let checked: toml::Table = format!("v = {parsed}")
        .parse()
        .map_err(|_| format!("{value} is no value for {}.", field.key))?;
    if let Some(value) = checked.get("v") {
        field
            .check(value)
            .map_err(|why| format!("Not changed: {why}."))?;
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
    let mut new = typed(field, value)?;
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|_| format!("Not changed: mod-configs/{id}.toml is not valid TOML."))?;
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
    })
}

/// The reply to a change of `id`'s `key`, on a game server or not.
#[must_use]
pub fn changed(id: &str, key: &str, change: &Change) -> String {
    let then = match change.stage {
        Stage::Startup => "restart needed: it takes effect at the next start",
        Stage::Sync => "applied now; joined players get it when they join again",
        Stage::Client => "applied now, on this game only",
        _ => "applied now",
    };
    format!("{id} {key} = {}: saved; {then}.", change.value)
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
            text.contains("limit = 4 (1 to 16; restart needed)"),
            "{text}"
        );
        assert!(text.contains("language = \"en\" (one of \"en\", \"uk\"; applied live)"));
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
            "safes limit = 6: saved; restart needed: it takes effect at the next start."
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
        assert!(set("safes", &schema(), "limit = ", "limit", "2").is_err());
    }
}
