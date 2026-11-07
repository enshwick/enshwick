//! The loader's own texts, such as the Mods screen's words and the
//! stand-ins' names: a file per language in `texts/` beside this file, read
//! once, the first time a text is needed.

use std::cell::Cell;
use std::sync::OnceLock;

use enshwick::i18n::{Language, Texts};

/// The texts the loader ships, a file per language.
const EMBEDDED: &[&str] = include!(concat!(env!("OUT_DIR"), "/texts.rs"));

static TEXTS: OnceLock<Texts> = OnceLock::new();

fn texts() -> &'static Texts {
    TEXTS.get_or_init(shipped)
}

thread_local! {
    /// The language of the player the texts filled on this thread are for,
    /// while [`for_reader`] runs.
    static READER: Cell<Option<Language>> = const { Cell::new(None) };
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
/// else the game's; a dedicated server has none, so its console answers in
/// English.
pub(crate) fn language() -> Language {
    if let Some(reader) = reader() {
        return reader;
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

/// A reason given as a text of the loader's, filled in the game's language;
/// `None` when its key names no text here.
pub(crate) fn reason(why: &str) -> Option<String> {
    texts().reason(why, language())
}

/// Every language with a text for `key`, and the text.
#[cfg(windows)]
pub(crate) fn translations(key: &'static str) -> impl Iterator<Item = (Language, &'static str)> {
    texts().translations(key)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

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
        // A built-in stat kind's name is `<kind>.name`; its first part is
        // a kind, not a namespace of texts the code asks for.
        let stat_names: Vec<String> = enshwick::stats::builtin::ALL
            .iter()
            .filter_map(|(id, _)| enshwick::stats::name_key(&id.to_string()))
            .collect();
        let namespaces: Vec<String> = fallback
            .as_table()
            .unwrap()
            .keys()
            .filter(|key| !stat_names.contains(key))
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
                && !["toml", "txt", "log", "json", "dll", "lock"].contains(parts.last().unwrap())
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

    /// Apostrophes other than `'`, the one the game's Ukrainian texts use:
    /// 2,074 times, U+2019 261 times and the others never (game texts of
    /// build 1269579). The game font draws U+02BC as a box (client 26430625).
    const OTHER_APOSTROPHES: [char; 9] = [
        '\u{2018}', '\u{2019}', '\u{201B}', '\u{02B9}', '\u{02BB}', '\u{02BC}', '\u{02BD}',
        '\u{2032}', '\u{FF07}',
    ];

    /// The keys of `file`'s Ukrainian texts with an apostrophe other than
    /// `'`; other languages keep their own typography.
    fn other_apostrophes(file: &str) -> Vec<String> {
        let table: toml::Table = file.parse().unwrap();
        table
            .get(Language::UK_UA.code())
            .and_then(toml::Value::as_table)
            .into_iter()
            .flatten()
            .filter(|(_, text)| {
                text.as_str()
                    .is_some_and(|text| text.contains(OTHER_APOSTROPHES))
            })
            .map(|(key, _)| key.clone())
            .collect()
    }

    #[test]
    fn ukrainian_texts_use_the_apostrophe_the_game_font_draws() {
        for file in EMBEDDED {
            assert_eq!(other_apostrophes(file), Vec::<String>::new());
        }
    }

    #[test]
    fn an_apostrophe_the_game_font_lacks_is_found_in_ukrainian_texts_only() {
        let code = Language::UK_UA.code();
        let file = format!(
            "[{code}]\n\"a\" = \"здоров'я\"\n\"b\" = \"ім\u{2019}я\"\n\"c\" = \"ім\\u02BCя\"\n"
        );
        assert_eq!(other_apostrophes(&file), ["b", "c"]);
        assert_eq!(
            other_apostrophes("[fr]\n\"a\" = \"l\u{2019}arbre\"\n"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn every_built_in_stat_kind_has_a_name_in_every_language() {
        let texts = shipped();
        for (id, _) in enshwick::stats::builtin::ALL {
            let key = enshwick::stats::name_key(&id.to_string()).unwrap();
            let named: Vec<Language> = texts
                .translations(&key)
                .map(|(language, _)| language)
                .collect();
            for &language in Language::OFFERED {
                assert!(named.contains(&language), "{key} in {}", language.code());
            }
        }
    }

    #[test]
    fn the_developer_tools_warning_is_upper_case_in_every_language() {
        let texts = shipped();
        for &language in Language::OFFERED {
            let warning = texts.get("developer.on", language).unwrap();
            assert_eq!(warning, warning.to_uppercase(), "{}", language.code());
        }
    }

    #[test]
    fn a_developer_mod_s_english_off_reason_is_the_api_s() {
        assert_eq!(
            shipped().get("mods.developer.off", Language::EN_US),
            Some(enshwick::mods::DEVELOPER_TOOL_OFF)
        );
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
}
