//! The loader's own texts, such as the Mods screen's words and the
//! stand-ins' names: a file per language in `texts/` beside this file, with
//! `mod-configs/enshwick.texts.toml` over them, read once, the first time a
//! text is needed after the loader is installed.

use std::path::Path;
use std::sync::OnceLock;

use enshwick::i18n::{Language, Texts};

/// The texts the loader ships, a file per language.
const EMBEDDED: &[&str] = include!(concat!(env!("OUT_DIR"), "/texts.rs"));
/// The file in `mod-configs/` that overrides or adds to them.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "overrides are read only in the game")
)]
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

/// The texts the loader ships, every language's file merged; a file that
/// does not read is left out, which a test keeps from shipping.
fn shipped() -> Texts {
    let mut texts = Texts::default();
    for file in EMBEDDED {
        if let Ok(language) = Texts::parse(file) {
            texts.merge(language);
        }
    }
    texts
}

/// The shipped texts with the file at `overrides` over them, when it is
/// there and reads.
fn read(overrides: Option<&Path>) -> Texts {
    let mut texts = shipped();
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
    use std::path::PathBuf;

    use super::*;

    /// Every source file of the loader but this one.
    fn sources() -> Vec<PathBuf> {
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        let this = Path::new(file!()).file_name().unwrap();
        let mut files = Vec::new();
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name() != Some(this)
                {
                    files.push(path);
                }
            }
        }
        files
    }

    /// The keys the loader's code asks for: the first argument of each
    /// `texts::` call that is a literal, every literal shaped like a key in
    /// one of the fallback texts' namespaces, such as a key in a table of
    /// keys, and the stand-ins' keys.
    fn keys() -> Vec<String> {
        let source: String = sources()
            .iter()
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect();
        let fallback = EMBEDDED
            .iter()
            .map(|file| file.parse::<toml::Table>().unwrap())
            .find_map(|mut table| table.remove(Language::FALLBACK.code()))
            .unwrap();
        let namespaces: Vec<String> = fallback
            .as_table()
            .unwrap()
            .keys()
            .filter_map(|key| key.split_once('.').map(|(first, _)| first.to_owned()))
            .collect();
        let key_shaped = |literal: &&str| {
            let parts: Vec<&str> = literal.split('.').collect();
            parts.len() > 1
                && parts.iter().all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                })
                && namespaces.iter().any(|n| n == parts[0])
                // Ids of services, messages and engine names share these
                // namespaces; their texts are found by the calls alone.
                && !["enshwick", "world"].contains(&parts[0])
                && !["toml", "txt", "log", "json", "dll"].contains(parts.last().unwrap())
        };
        let mut keys: Vec<String> = source
            .split("texts::")
            .skip(1)
            .filter_map(|call| {
                let argument = call.split_once('(')?.1.trim_start();
                let quoted = argument.strip_prefix('"')?;
                Some(quoted.split_once('"')?.0.to_owned())
            })
            .chain(
                source
                    .split('"')
                    .skip(1)
                    .filter(|between| !between.contains(['\n', ' ']))
                    .filter(key_shaped)
                    .map(str::to_owned),
            )
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
    fn every_shipped_file_holds_one_language_of_its_own() {
        let mut codes: Vec<String> = EMBEDDED
            .iter()
            .map(|file| {
                assert!(
                    Texts::parse(file).is_ok(),
                    "{}",
                    &file[..file.len().min(40)]
                );
                let table: toml::Table = file.parse().unwrap();
                assert_eq!(table.len(), 1, "{:?}", table.keys().collect::<Vec<_>>());
                table.keys().next().unwrap().clone()
            })
            .collect();
        let shipped = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), shipped, "{codes:?}");
        assert!(codes.iter().any(|code| code == Language::FALLBACK.code()));
    }

    #[test]
    fn every_key_the_loader_asks_for_has_a_fallback_text() {
        let texts = shipped();
        let keys = keys();
        assert!(keys.len() > 30, "{keys:?}");
        let missing: Vec<&String> = keys
            .iter()
            .filter(|key| texts.get(key, Language::FALLBACK).is_none())
            .collect();
        assert_eq!(missing, Vec::<&String>::new());
    }

    #[test]
    fn every_text_exists_in_all_languages() {
        assert_eq!(shipped().untranslated(), []);
    }

    #[test]
    fn every_language_names_the_fallback_s_placeholders() {
        fn placeholders(text: &str) -> Vec<&str> {
            let mut names: Vec<&str> = text
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split_once('}').map(|(name, _)| name))
                .collect();
            names.sort_unstable();
            names
        }
        let texts = shipped();
        let table = EMBEDDED
            .iter()
            .map(|file| file.parse::<toml::Table>().unwrap())
            .find(|table| table.contains_key(Language::FALLBACK.code()))
            .unwrap();
        let fallback = table[Language::FALLBACK.code()].as_table().unwrap();
        for key in fallback.keys() {
            let expected = placeholders(texts.get(key, Language::FALLBACK).unwrap());
            for (language, text) in texts.translations(key) {
                assert_eq!(placeholders(text), expected, "{key} in {}", language.code());
            }
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
        assert_eq!(texts.get("mods.title", Language::UK_UA), Some("Моди"));
        std::fs::write(&path, "[xx]\n").unwrap();
        let texts = read(Some(&path));
        assert_eq!(texts.get("mods.back", Language::UK_UA), Some("Назад"));
        assert_eq!(read(None).get("mods.back", Language::EN_US), Some("Back"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
