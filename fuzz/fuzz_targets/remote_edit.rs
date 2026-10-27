//! A remote settings edit from an administrator's game, applied to a mod's
//! settings file the way the server applies it.
//!
//! Fields: the settings file, the key, the value, join values.

#![no_main]

use enshwick::config::{Field, Schema, Stage};
use enshwick_fuzz::text_fields;
use enshwick_loader::{configure, remote};
use libfuzzer_sys::fuzz_target;

fn schema() -> Schema {
    Schema::new(
        "Fuzzed settings.",
        "config",
        vec![
            Field::new("limit", Stage::Startup, &4).with_range(Some(1.0), Some(16.0)),
            Field::new("radius", Stage::Runtime, &8.5),
            Field::new("language", Stage::Runtime, "en").with_choices(&["en", "uk"]),
            Field::new("note", Stage::Client, "hi"),
            Field::new("token", Stage::Runtime, "").with_secret(true),
            Field::new("group.flag", Stage::Runtime, &true),
            Field::new("group.list", Stage::Runtime, &[1, 2, 3]),
        ],
    )
}

fuzz_target!(|data: &[u8]| {
    let Some([text, key, value, values]) = text_fields::<4>(data) else {
        return;
    };
    let schema = schema();
    drop(configure::list("fuzz", &schema, text));
    drop(configure::get("fuzz", &schema, text, key));
    if configure::set("fuzz", &schema, text, key, value).is_ok() {
        assert!(!schema.field(key).is_some_and(|f| f.secret));
    }
    let hidden = remote::redacted(&schema, text);
    if let Ok(table) = hidden.parse::<toml::Table>() {
        assert!(!table.contains_key("token"));
    }
    drop(remote::values_text(values));
});
