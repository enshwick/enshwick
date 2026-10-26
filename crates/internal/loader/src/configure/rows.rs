//! The rows of a mod's settings page, built from the schema the mod handed
//! the loader and the settings file: which game widget shows each setting,
//! its value, whether the player may change it, and the TOML a change writes.

use enshwick::config::{Field, Kind, Schema, Stage};
use enshwick::i18n::{Language, Texts};

/// The most stops a slider row gets; longer whole-number ranges slide
/// freely and round.
pub const MAX_STOPS: u32 = 32;

/// How a setting shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Widget {
    /// A toggle row.
    Toggle(bool),
    /// A slider row over `min..=max`; `steps` stops, 0 for a free slider;
    /// `whole` for a whole-number setting.
    Slider {
        /// The value.
        value: f32,
        /// The least value.
        min: f32,
        /// The greatest value.
        max: f32,
        /// Stops, both ends included; 0 slides freely.
        steps: u32,
        /// The setting takes whole numbers.
        whole: bool,
    },
    /// A dropdown row over the setting's choices, as players read them.
    Choice {
        /// The choices, without TOML quotes.
        options: Vec<String>,
        /// The choice the file holds; past the end when it holds none of
        /// them.
        index: usize,
    },
    /// A line with the value, for a setting no row edits: text, lists,
    /// tables and numbers without a range.
    Text(String),
}

/// Why a row takes no changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lock {
    /// The player may change it.
    Open,
    /// A joined server sets it; the row shows the server's value.
    Server,
    /// No row edits it; the file does.
    File,
}

/// One row of a settings page.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The key in the settings file.
    pub key: String,
    /// The label players read: the key in words.
    pub label: String,
    /// The setting's documentation, in English.
    pub doc: String,
    /// How it shows, with its value.
    pub widget: Widget,
    /// Whether it takes changes.
    pub lock: Lock,
    /// When a change takes effect.
    pub stage: Stage,
}

/// `max_altars` as `Max altars`.
#[must_use]
pub fn key_words(key: &str) -> String {
    let spaced = key.replace(['_', '-'], " ");
    let mut chars = spaced.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// A choice as players read it: a TOML string without its quotes.
fn plain(choice: &str) -> String {
    choice
        .parse::<toml::Value>()
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| {
            format!("v = {choice}")
                .parse::<toml::Table>()
                .ok()
                .and_then(|t| t.get("v").and_then(|v| v.as_str().map(str::to_owned)))
                .unwrap_or_else(|| choice.to_owned())
        })
}

#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "settings in rows are small numbers shown by a float slider"
)]
fn number(value: &toml::Value) -> Option<f32> {
    match value {
        toml::Value::Integer(n) => Some(*n as f32),
        toml::Value::Float(n) => Some(*n as f32),
        _ => None,
    }
}

/// A mod's texts in the player's language, for its rows' words.
#[derive(Clone, Copy, Debug)]
pub struct Words<'a> {
    /// The mod's texts.
    pub texts: &'a Texts,
    /// The player's language.
    pub language: Language,
}

impl Words<'_> {
    fn get(&self, key: &str) -> Option<String> {
        self.texts.get(key, self.language).map(str::to_owned)
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a whole-number range of at most MAX_STOPS values"
)]
fn widget(field: &Field, value: &toml::Value, words: Option<Words<'_>>) -> Widget {
    if !field.choices.is_empty() {
        let shown = value.to_string();
        return Widget::Choice {
            options: field
                .choices
                .iter()
                .map(|c| {
                    let choice = plain(c);
                    words
                        .and_then(|w| w.get(&format!("{}.{choice}", field.text)))
                        .unwrap_or(choice)
                })
                .collect(),
            index: field
                .choices
                .iter()
                .position(|c| *c == shown)
                .unwrap_or(field.choices.len()),
        };
    }
    match (field.kind, field.min, field.max) {
        (Some(Kind::Bool), ..) => Widget::Toggle(value.as_bool().unwrap_or(false)),
        (Some(kind @ (Kind::Integer | Kind::Float)), Some(min), Some(max)) if max > min => {
            let whole = kind == Kind::Integer;
            let count = max - min + 1.0;
            let steps = if whole && count <= f64::from(MAX_STOPS) {
                count as u32
            } else {
                0
            };
            #[expect(clippy::cast_possible_truncation, reason = "a slider shows f32")]
            let (min, max) = (min as f32, max as f32);
            Widget::Slider {
                value: number(value).unwrap_or(min).clamp(min, max),
                min,
                max,
                steps,
                whole,
            }
        }
        _ => Widget::Text(value.to_string()),
    }
}

/// The rows of `schema` with the values of the settings file `file`, and
/// a joined server's join values `server` over them, labelled with the
/// mod's `words` where it has them: `<text>` for a row's label, `<text>.doc`
/// for its description, `<text>.<choice>` for a choice.
#[must_use]
pub fn rows(
    schema: &Schema,
    file: &str,
    server: Option<&toml::Table>,
    words: Option<Words<'_>>,
) -> Vec<Row> {
    let table: toml::Table = file.parse().unwrap_or_default();
    schema
        .fields
        .iter()
        .map(|field| {
            let from_server =
                field.follows_server() && server.is_some_and(|s| s.contains_key(&field.key));
            let value = server
                .filter(|_| from_server)
                .and_then(|s| s.get(&field.key))
                .or_else(|| table.get(&field.key))
                .cloned()
                .or_else(|| default(field))
                .unwrap_or_else(|| toml::Value::String(String::new()));
            let widget = widget(field, &value, words);
            let lock = if from_server {
                Lock::Server
            } else if matches!(widget, Widget::Text(_)) {
                Lock::File
            } else {
                Lock::Open
            };
            Row {
                key: field.key.clone(),
                label: words
                    .and_then(|w| w.get(&field.text))
                    .unwrap_or_else(|| key_words(&field.key)),
                doc: words
                    .and_then(|w| w.get(&format!("{}.doc", field.text)))
                    .unwrap_or_else(|| field.doc.clone()),
                widget,
                lock,
                stage: field.stage,
            }
        })
        .collect()
}

fn default(field: &Field) -> Option<toml::Value> {
    format!("v = {}", field.default)
        .parse::<toml::Table>()
        .ok()?
        .remove("v")
}

/// The TOML a toggle writes.
#[must_use]
pub fn toggled(on: bool) -> String {
    on.to_string()
}

/// The TOML a slider writes: a whole number for a whole-number setting.
#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "a whole-number setting's slider value, rounded"
)]
pub fn slid(value: f32, whole: bool) -> String {
    if whole {
        (f64::from(value).round() as i64).to_string()
    } else {
        // Two decimals, as the slider shows them.
        let rounded = (f64::from(value) * 100.0).round() / 100.0;
        let text = rounded.to_string();
        if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        }
    }
}

/// The TOML a dropdown writes: the choice as the schema holds it.
#[must_use]
pub fn picked(field: &Field, index: usize) -> Option<String> {
    field.choices.get(index).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn schema() -> Schema {
        Schema::new(
            "",
            "config",
            vec![
                Field::new("enabled", Stage::Runtime, &true),
                Field::new("max_altars", Stage::Startup, &4)
                    .with_range(Some(1.0), Some(20.0))
                    .with_doc("Altars per level.", ""),
                Field::new("radius", Stage::Sync, &8.0).with_range(Some(0.0), Some(5.0)),
                Field::new("mode", Stage::Runtime, "auto").with_choices(&["auto", "on"]),
                Field::new("note", Stage::Client, "hi"),
                Field::new("count", Stage::Runtime, &500).with_range(Some(0.0), Some(1000.0)),
            ],
        )
    }

    #[test]
    fn settings_show_as_rows_with_their_values() {
        let file = "enabled = false\nmax_altars = 6\nradius = 2.5\nmode = \"on\"\n";
        let rows = rows(&schema(), file, None, None);
        assert_eq!(rows[0].widget, Widget::Toggle(false));
        assert_eq!(rows[1].label, "Max altars");
        assert_eq!(rows[1].stage, Stage::Startup);
        assert_eq!(
            rows[1].widget,
            Widget::Slider {
                value: 6.0,
                min: 1.0,
                max: 20.0,
                steps: 20,
                whole: true
            }
        );
        assert!(matches!(
            rows[2].widget,
            Widget::Slider {
                steps: 0,
                whole: false,
                ..
            }
        ));
        assert_eq!(
            rows[3].widget,
            Widget::Choice {
                options: vec!["auto".into(), "on".into()],
                index: 1
            }
        );
        assert_eq!(rows[4].widget, Widget::Text("\"hi\"".into()));
        assert_eq!(rows[4].lock, Lock::File);
        assert!(
            matches!(rows[5].widget, Widget::Slider { steps: 0, whole: true, value, .. } if (value - 500.0).abs() < 1e-3)
        );
        assert!(rows.iter().take(4).all(|r| r.lock == Lock::Open));
    }

    #[test]
    fn a_joined_servers_values_show_locked() {
        let server: toml::Table = "radius = 4.0".parse().unwrap();
        let rows = rows(&schema(), "radius = 1.0", Some(&server), None);
        assert_eq!(rows[2].lock, Lock::Server);
        assert!(
            matches!(rows[2].widget, Widget::Slider { value, .. } if (value - 4.0).abs() < 1e-3)
        );
        assert_eq!(rows[0].lock, Lock::Open);
    }

    #[test]
    fn a_joined_servers_startup_sync_value_shows_locked() {
        let schema = Schema::new(
            "",
            "config",
            vec![
                Field::new("rule", Stage::Startup, &true).with_synced(true),
                Field::new("limit", Stage::Startup, &4),
            ],
        );
        let server: toml::Table = "rule = false\nlimit = 9".parse().unwrap();
        let rows = rows(&schema, "rule = true\nlimit = 2", Some(&server), None);
        assert_eq!(
            (rows[0].lock, &rows[0].widget),
            (Lock::Server, &Widget::Toggle(false))
        );
        assert_ne!(
            rows[1].lock,
            Lock::Server,
            "a startup value stays this side's"
        );
    }

    #[test]
    fn a_mods_texts_label_its_rows_in_the_players_language() {
        let texts = Texts::parse(
            "[en]\n\"config.max_altars\" = \"Altars\"\n[uk]\n\"config.max_altars\" = \"Вівтарі\"\n\"config.max_altars.doc\" = \"На рівень.\"\n\"config.mode.on\" = \"Увімк.\"\n",
        )
        .unwrap();
        let words = Words {
            texts: &texts,
            language: Language::UK_UA,
        };
        let rows = rows(&schema(), "", None, Some(words));
        assert_eq!(rows[1].label, "Вівтарі");
        assert_eq!(rows[1].doc, "На рівень.");
        assert_eq!(rows[0].label, "Enabled");
        assert!(
            matches!(&rows[3].widget, Widget::Choice { options, .. } if options[1] == "Увімк.")
        );
        let german = rows_in(&texts, Language::DE_DE);
        assert_eq!(german[1].label, "Altars");
    }

    fn rows_in(texts: &Texts, language: Language) -> Vec<Row> {
        rows(&schema(), "", None, Some(Words { texts, language }))
    }

    #[test]
    fn changes_write_toml_the_schema_accepts() {
        let schema = schema();
        assert_eq!(toggled(true), "true");
        assert_eq!(slid(6.4, true), "6");
        assert_eq!(slid(2.0, false), "2.0");
        assert_eq!(slid(2.345, false), "2.35");
        assert_eq!(
            picked(schema.field("mode").unwrap(), 1).as_deref(),
            Some("\"on\"")
        );
        assert_eq!(picked(schema.field("mode").unwrap(), 5), None);
        let v: toml::Table = format!("v = {}", toggled(true)).parse().unwrap();
        assert!(schema.field("enabled").unwrap().check(&v["v"]).is_ok());
        assert_eq!(key_words("max_altars"), "Max altars");
        assert_eq!(key_words(""), "");
    }
}
