//! The updater's words in the player's language: `texts.toml`, with a
//! server owner's `mod-configs/updater.texts.toml` over it. The log stays in
//! English.

use std::sync::OnceLock;

use enshwick_loader_api::i18n::{Language, Texts};

/// The texts the updater ships.
pub(crate) const EMBEDDED: &str = include_str!("texts.toml");

static TEXTS: OnceLock<Texts> = OnceLock::new();

/// Keeps the texts `load.texts` merged; the embedded ones until then.
pub(crate) fn set(texts: Texts) {
    let _first = TEXTS.set(texts);
}

fn texts() -> &'static Texts {
    TEXTS.get_or_init(|| Texts::parse(EMBEDDED).unwrap_or_default())
}

/// The text of `key` in the player's language, `args` filled in.
pub(crate) fn text(key: &str, args: &[(&str, &str)]) -> String {
    in_language(key, enshwick_loader_api::i18n::language(), args)
}

/// The text of `key` in `language`, `args` filled in.
pub(crate) fn in_language(key: &str, language: Language, args: &[(&str, &str)]) -> String {
    texts().fill(key, language, args)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    type Tables = BTreeMap<String, BTreeMap<String, String>>;

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|part| part.split_once('}').map(|(name, _)| name))
            .collect()
    }

    #[test]
    fn every_language_has_every_text_with_the_same_placeholders() {
        let tables: Tables = toml::from_str(EMBEDDED).unwrap();
        let english = &tables["en"];
        assert!(Texts::parse(EMBEDDED).is_ok());
        for (code, table) in &tables {
            assert_eq!(
                table.keys().collect::<Vec<_>>(),
                english.keys().collect::<Vec<_>>(),
                "{code} has other keys than en"
            );
            for (key, text) in table {
                assert_eq!(
                    placeholders(text),
                    placeholders(&english[key]),
                    "{code} {key}"
                );
            }
        }
    }

    #[test]
    fn a_text_reads_in_the_language_asked() {
        assert_eq!(
            in_language("line.missing", Language::UK_UA, &[]),
            "не встановлено"
        );
        assert_eq!(
            in_language(
                "deleted",
                Language::DE_DE,
                &[("file", "a.dll"), ("name", "B")]
            ),
            "a.dll deleted: B replaces it"
        );
    }
}
