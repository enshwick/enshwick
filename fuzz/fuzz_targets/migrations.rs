//! A mod's settings migrations run on a settings file a player edited, with
//! another mod's file to take settings from, and the record of what ran.
//!
//! Fields: the settings file, the other mod's file, the steps one per line as
//! `<kind> <from> <to>`, the value a map step gives, the record.

#![no_main]

use enshwick_fuzz::text_fields;
use enshwick_loader::migrations::{Migration, Record, Step, apply_settings};
use libfuzzer_sys::fuzz_target;

/// Steps in one migration, at most.
const MAX_STEPS: usize = 16;

fn steps(text: &str) -> Vec<Step> {
    text.lines()
        .take(MAX_STEPS)
        .map(|line| {
            let mut words = line.splitn(3, ' ');
            let kind = words.next().and_then(|k| k.parse().ok()).unwrap_or(0);
            Step {
                kind,
                from: words.next().unwrap_or_default().to_owned(),
                to: words.next().unwrap_or_default().to_owned(),
                from_guid: [0; 16],
                to_guid: [0; 16],
            }
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let Some([text, other, steps_text, mapped, record]) = text_fields::<5>(data) else {
        return;
    };
    let migration = Migration {
        id: "fuzz".to_owned(),
        steps: steps(steps_text),
    };
    let map = |_: usize, _: usize, _: &str| Ok(mapped.to_owned());
    let source = |_: &str| Ok(Some(other.to_owned()));
    if let Ok(migrated) = apply_settings(text, 0, &migration, &map, &source) {
        assert!(
            migrated.parse::<toml_edit::DocumentMut>().is_ok(),
            "{migrated}"
        );
    }
    if let Ok(read) = Record::parse(record) {
        let written = toml::to_string(&read);
        assert!(written.is_ok());
        if let Ok(written) = written {
            // The loader writes a comment above the record, as here.
            assert_eq!(Record::parse(&format!("# ran\n{written}")).ok(), Some(read));
        }
    }
});
