//! The text files a player or an administrator edits by hand: a world's
//! record of its mods, a settings schema and its values, texts and a content
//! pack's manifest.

#![no_main]

use enshwick::config::Schema;
use enshwick::i18n::Texts;
use enshwick::packs::PackManifest;
use enshwick_loader::base::record::Record;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if let Some(record) = Record::parse(text) {
        assert!(Record::parse(&record.render()).is_some());
    }
    if let Ok(schema) = Schema::parse(text) {
        drop(schema.to_text());
    }
    drop(Texts::parse(text));
    drop(PackManifest::parse(text));
});
