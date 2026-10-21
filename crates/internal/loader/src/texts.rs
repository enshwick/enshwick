//! The loader's own texts, such as the Mods screen's words and the
//! stand-ins' names: `texts.toml` beside this file, in English and Ukrainian,
//! with `mod-configs/enshwick.texts.toml` over it, read once, the first time
//! a text is needed after the loader is installed.

use std::path::Path;
use std::sync::OnceLock;

use enshwick_loader_api::i18n::{Language, Texts};

/// The texts the loader ships.
const EMBEDDED: &str = include_str!("texts.toml");
/// The file in `mod-configs/` that overrides or adds to them.
const OVERRIDES: &str = "enshwick.texts.toml";

static TEXTS: OnceLock<Texts> = OnceLock::new();

fn texts() -> &'static Texts {
    TEXTS.get_or_init(|| {
        #[cfg(windows)]
        let overrides = crate::host::loader().map(|loader| loader.configs_dir.join(OVERRIDES));
        #[cfg(not(windows))]
        let overrides = None::<std::path::PathBuf>;
        read(overrides.as_deref())
    })
}

/// The language the game shows its texts in.
fn language() -> Language {
    #[cfg(windows)]
    return crate::content::language();
    #[cfg(not(windows))]
    Language::FALLBACK
}

/// The shipped texts with the file at `overrides` over them, when it is
/// there and reads.
fn read(overrides: Option<&Path>) -> Texts {
    let mut texts = Texts::parse(EMBEDDED).unwrap_or_default();
    let Some(path) = overrides else {
        return texts;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return texts;
    };
    match Texts::parse(&text) {
        Ok(overrides) => texts.merge(overrides),
        Err(off) => crate::log!(
            WARN,
            "{}: {off}; the loader's own texts stay as shipped",
            path.display()
        ),
    }
    texts
}

/// The text of `key` in the game's language.
#[cfg(windows)]
pub(crate) fn text(key: &'static str) -> &'static str {
    texts().get(key, language()).unwrap_or(key)
}

/// The text of `key` in the game's language with each `{name}` replaced by
/// its value in `args`.
pub(crate) fn fill(key: &str, args: &[(&str, &str)]) -> String {
    texts().fill(key, language(), args)
}

/// Every language with a text for `key`, and the text.
#[cfg(windows)]
pub(crate) fn translations(key: &'static str) -> impl Iterator<Item = (Language, &'static str)> {
    texts().translations(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keys the loader's code asks for: the first argument of each
    /// `texts::` call that is a literal, and the stand-ins' keys.
    fn keys() -> Vec<String> {
        let source = [include_str!("ui/mods.rs"), include_str!("configure.rs")].concat();
        let mut keys: Vec<String> = source
            .split("texts::")
            .skip(1)
            .filter_map(|call| {
                let argument = call.split_once('(')?.1.trim_start();
                let quoted = argument.strip_prefix('"')?;
                Some(quoted.split_once('"')?.0.to_owned())
            })
            .chain([
                "stand-in.name".to_owned(),
                "stand-in.description".to_owned(),
            ])
            .collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    #[test]
    fn every_key_the_loader_asks_for_has_english_and_ukrainian() {
        let texts = Texts::parse(EMBEDDED).unwrap();
        let keys = keys();
        assert!(keys.len() > 30, "{keys:?}");
        for key in keys {
            for language in [Language::FALLBACK, Language::UK_UA] {
                let found = texts.translations(&key).any(|(other, _)| other == language);
                assert!(found, "{key} has no {}", language.code());
            }
        }
    }

    #[test]
    fn english_and_ukrainian_name_the_same_placeholders() {
        fn placeholders(text: &str) -> Vec<&str> {
            let mut names: Vec<&str> = text
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
                .collect();
            names.sort_unstable();
            names
        }
        let texts = Texts::parse(EMBEDDED).unwrap();
        for (english, ukrainian) in keys().iter().filter_map(|key| {
            Some((
                texts.get(key, Language::FALLBACK)?,
                texts.get(key, Language::UK_UA)?,
            ))
        }) {
            assert_eq!(placeholders(english), placeholders(ukrainian), "{english}");
        }
    }

    #[test]
    fn an_override_file_changes_and_adds_texts_and_a_broken_one_is_ignored() {
        let dir = std::env::temp_dir().join(format!("enshwick-texts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(OVERRIDES);
        std::fs::write(
            &path,
            "[uk]\n\"mods.back\" = \"Повернутися\"\n[de]\n\"mods.back\" = \"Zurück\"\n",
        )
        .unwrap();
        let texts = read(Some(&path));
        assert_eq!(texts.get("mods.back", Language::UK_UA), Some("Повернутися"));
        assert_eq!(texts.get("mods.back", Language::DE_DE), Some("Zurück"));
        assert_eq!(
            texts.get("mods.install", Language::UK_UA),
            Some("Встановити")
        );
        std::fs::write(&path, "[xx]\n").unwrap();
        let texts = read(Some(&path));
        assert_eq!(texts.get("mods.back", Language::UK_UA), Some("Назад"));
        assert_eq!(read(None).get("mods.back", Language::EN_US), Some("Back"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
