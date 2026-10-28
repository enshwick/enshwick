//! The loader's own texts, such as the Mods screen's words and the
//! stand-ins' names: a file per language in `texts/` beside this file, with
//! `mod-configs/enshwick.texts.toml` over them, read once, the first time a
//! text is needed after the loader is installed.

use std::cell::Cell;
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

thread_local! {
    /// The language of the player the texts filled on this thread are for,
    /// while [`for_reader`] runs.
    static READER: Cell<Option<Language>> = const { Cell::new(None) };
}

/// The language the loader's own setting `language` names, once
/// [`configure`] read it.
static OWN: OnceLock<Language> = OnceLock::new();

/// Takes the loader's setting `language` on a dedicated `server`, which has
/// no game language of its own: the code of the language texts are in where
/// no player's game says its own, empty for English. A client's texts follow
/// its game.
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "the setting is read only in the game")
)]
pub(crate) fn configure(code: &str, server: bool) {
    if !server {
        return;
    }
    match own_language(code) {
        Ok(Some(language)) => {
            let _first = OWN.set(language);
        }
        Ok(None) => {}
        Err(code) => crate::log!(
            WARN,
            "language = \"{code}\" in the loader's settings is no language the game knows; texts follow the game's"
        ),
    }
}

/// The language a setting's `code` names: `None` for an empty code, the
/// code back for one no language has.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "the setting is read only in the game")
)]
fn own_language(code: &str) -> Result<Option<Language>, &str> {
    let code = code.trim();
    if code.is_empty() {
        return Ok(None);
    }
    Language::from_code(code).map(Some).ok_or(code)
}

/// Runs `f` with every text it fills in `language`, for an answer to one
/// player; `None` keeps the language already in force.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "players are answered only in the game")
)]
pub(crate) fn for_reader<R>(language: Option<Language>, f: impl FnOnce() -> R) -> R {
    scoped(language.or(READER.get()), f)
}

/// Runs `f` with the reader `reader`, then puts the one before back, also
/// when `f` panics.
fn scoped<R>(reader: Option<Language>, f: impl FnOnce() -> R) -> R {
    /// Puts the reader before back.
    struct Restore(Option<Language>);
    impl Drop for Restore {
        fn drop(&mut self) {
            READER.set(self.0);
        }
    }
    let _restore = Restore(READER.replace(reader));
    f()
}

/// Runs `f` for no player in particular, such as an event every mod hears,
/// inside an answer to one player: texts take the loader's own language.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "players are answered only in the game")
)]
pub(crate) fn for_nobody<R>(f: impl FnOnce() -> R) -> R {
    scoped(None, f)
}

/// Runs `f` with every text it fills in the language the game of player
/// `key` reads, as its hello said; the loader's own for a game that said
/// none.
#[cfg(windows)]
pub(crate) fn for_player<R>(key: u64, f: impl FnOnce() -> R) -> R {
    for_reader(crate::net::language_of(key), f)
}

/// The language of the player the texts on this thread are for, inside
/// [`for_reader`]; `None` outside.
pub(crate) fn reader() -> Option<Language> {
    READER.get()
}

/// The language texts are filled in: the reader's inside [`for_reader`],
/// else the loader's setting, else the game's.
pub(crate) fn language() -> Language {
    if let Some(reader) = reader() {
        return reader;
    }
    if let Some(&own) = OWN.get() {
        return own;
    }
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
    fn every_language_the_game_offers_has_every_text() {
        assert_eq!(shipped().missing_in(Language::OFFERED), []);
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
    fn texts_take_the_reader_s_language_inside_its_scope_only() {
        assert_eq!(fill("mods.back", &[]), "Back");
        let inside = for_reader(Some(Language::UK_UA), || {
            let nested = for_reader(None, || fill("mods.back", &[]));
            (fill("mods.back", &[]), nested)
        });
        assert_eq!(inside, ("Назад".to_owned(), "Назад".to_owned()));
        let everyone = for_reader(Some(Language::UK_UA), || {
            (
                for_nobody(|| fill("mods.back", &[])),
                fill("mods.back", &[]),
            )
        });
        assert_eq!(everyone, ("Back".to_owned(), "Назад".to_owned()));
        assert_eq!(fill("mods.back", &[]), "Back");
        let panicked = std::panic::catch_unwind(|| {
            for_reader(Some(Language::UK_UA), || panic!("in an answer"));
        });
        assert!(panicked.is_err());
        assert_eq!(fill("mods.back", &[]), "Back");
    }

    #[test]
    fn the_own_language_setting_reads_a_code_or_nothing() {
        assert_eq!(own_language(""), Ok(None));
        assert_eq!(own_language(" UK "), Ok(Some(Language::UK_UA)));
        assert_eq!(own_language("pt-br"), Ok(Some(Language::PT_BR)));
        assert!(own_language("klingon").is_err());
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
