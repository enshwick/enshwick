//! The text files a player or an administrator edits by hand: the mod list,
//! its lock, a settings schema and its values, texts and a content pack's
//! manifest.

#![no_main]

use enshwick::config::Schema;
use enshwick::i18n::Texts;
use enshwick::packs::PackManifest;
use enshwick_loader::modlist::{Lock, ModList};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    if let Ok(list) = ModList::parse(text) {
        drop(list.off());
        drop(list.links_off());
        let rendered = list.render();
        assert!(ModList::parse(&rendered).is_ok());
    }
    if let Some(lock) = Lock::parse(text) {
        assert!(Lock::parse(&lock.render()).is_some());
    }
    if let Ok(schema) = Schema::parse(text) {
        drop(schema.to_text());
    }
    drop(Texts::parse(text));
    drop(PackManifest::parse(text));
});
