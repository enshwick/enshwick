//! A remote settings edit from an administrator's game, applied to a mod's
//! settings file the way the server applies it.
//!
//! Fields: the settings file, the key, the value, and a world's file the
//! first field serves as the template of.

#![no_main]

use enshwick::config::{Field, Schema, value_at};
use enshwick_fuzz::text_fields;
use enshwick_loader::{configure, remote, worlds};
use libfuzzer_sys::fuzz_target;

fn schema() -> Schema {
    Schema::new(
        "Fuzzed settings.",
        "config",
        vec![
            Field::new("limit", &4).with_range(Some(1.0), Some(16.0)),
            Field::new("radius", &8.5),
            Field::new("language", "en").with_choices(&["en", "uk"]),
            Field::new("note", "hi"),
            Field::new("token", "").with_secret(true),
            Field::new("group.flag", &true),
            Field::new("group.list", &[1, 2, 3]),
            Field::new("group.secret", "").with_secret(true),
            Field::new("group.inner.key", "").with_secret(true),
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
        for field in schema.fields.iter().filter(|f| f.secret) {
            assert!(value_at(&table, &field.key).is_none(), "{}", field.key);
        }
    }
    // A world's file brought in line with a template keeps no key the
    // template lacks, and a second pass changes nothing.
    if let Ok(synced) = worlds::sync(text, values, "settings", "fuzz 1.0.0", &[]) {
        let after = synced.text.unwrap_or_else(|| values.to_owned());
        let again = worlds::sync(text, &after, "settings", "fuzz 1.0.0", &[]).unwrap();
        assert_eq!(again.text, None, "{after}");
    }
});
