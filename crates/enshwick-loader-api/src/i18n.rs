//! Translations: a mod ships its texts per language in one TOML file, a
//! server owner may add to them or change them, and the game shows them for
//! the player's language.
//!
//! ```toml
//! [en]
//! "safe.name" = "Safe"
//!
//! [uk]
//! "safe.name" = "Сейф"
//! ```
//!
//! `en` is the fallback for every language without a text of its own. A file
//! `mod-configs/<id>.texts.toml` in the same form overrides or adds texts.

use std::collections::{BTreeMap, HashMap};

use crate::content::Id;
use crate::{Load, Off};

/// A game language, as the game numbers them.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::i18n::Language;
///
/// assert_eq!(Language::from_code("uk"), Some(Language::UK_UA));
/// assert_eq!(Language::UK_UA.code(), "uk");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language(pub u32);

macro_rules! languages {
    ($($name:ident = $id:literal, $code:literal, $doc:literal;)*) => {
        impl Language {
            $(#[doc = $doc] pub const $name: Self = Self($id);)*

            /// Every language the game knows, the fallback first.
            pub const ALL: &[Self] = &[$(Self::$name),*];

            /// The language's code in text files, such as `uk` or `pt-br`.
            ///
            /// # Examples
            ///
            /// See [`Language`].
            #[must_use]
            pub const fn code(self) -> &'static str {
                match self.0 {
                    $($id => $code,)*
                    _ => "",
                }
            }

            /// The language with code `code`, ignoring case.
            ///
            /// # Examples
            ///
            /// See [`Language`].
            #[must_use]
            pub fn from_code(code: &str) -> Option<Self> {
                let code = code.to_ascii_lowercase();
                match code.as_str() {
                    $($code => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

languages! {
    FALLBACK = 0, "en", "English, and every language without a text of its own.";
    EN_US = 1, "en-us", "English (United States).";
    EN_GB = 2, "en-gb", "English (United Kingdom).";
    DE_DE = 3, "de", "German.";
    NL_NL = 4, "nl", "Dutch.";
    FR_FR = 5, "fr", "French.";
    IT_IT = 6, "it", "Italian.";
    ES_ES = 7, "es", "Spanish (Spain).";
    ES_MX = 8, "es-mx", "Spanish (Mexico).";
    RU_RU = 9, "ru", "Russian.";
    PL_PL = 10, "pl", "Polish.";
    JA_JP = 11, "ja", "Japanese.";
    ID_ID = 12, "id", "Indonesian.";
    KO_KR = 13, "ko", "Korean.";
    MS_MY = 14, "ms", "Malay.";
    PT_PT = 15, "pt", "Portuguese (Portugal).";
    PT_BR = 16, "pt-br", "Portuguese (Brazil).";
    TH_TH = 17, "th", "Thai.";
    TR_TR = 18, "tr", "Turkish.";
    VI_VN = 19, "vi", "Vietnamese.";
    ZH_CN = 20, "zh-cn", "Chinese (Simplified).";
    ZH_TW = 21, "zh-tw", "Chinese (Traditional).";
    FR_CA = 22, "fr-ca", "French (Canada).";
    ES_419 = 23, "es-419", "Spanish (Latin America).";
    BG_BG = 24, "bg", "Bulgarian.";
    CS_CZ = 25, "cs", "Czech.";
    DA_DK = 26, "da", "Danish.";
    FI_FI = 27, "fi", "Finnish.";
    EL_GR = 28, "el", "Greek.";
    HU_HU = 29, "hu", "Hungarian.";
    NO_NO = 30, "no", "Norwegian.";
    NB_NO = 31, "nb", "Norwegian (Bokmål).";
    RO_RO = 32, "ro", "Romanian.";
    SV_SE = 33, "sv", "Swedish.";
    UK_UA = 34, "uk", "Ukrainian.";
}

/// A mod's texts by language and key.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::i18n::{Language, Texts};
///
/// let texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n")?;
/// assert_eq!(texts.get("greeting", Language::UK_UA), Some("Привіт"));
/// assert_eq!(texts.get("greeting", Language::DE_DE), Some("Hello"));
/// # Ok::<(), enshwick_loader_api::Off>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Texts {
    by_language: HashMap<Language, BTreeMap<String, String>>,
}

impl Texts {
    /// Texts from TOML: a table per language code, keys to texts.
    ///
    /// # Errors
    ///
    /// [`Off`] for malformed TOML, an unknown language code or a value that
    /// is not text.
    ///
    /// # Examples
    ///
    /// See [`Texts`].
    pub fn parse(toml: &str) -> Result<Self, Off> {
        let tables: BTreeMap<String, BTreeMap<String, String>> =
            toml::from_str(toml).map_err(|e| Off::new(format!("texts: {}", e.message())))?;
        let mut texts = Self::default();
        for (code, entries) in tables {
            let language = Language::from_code(&code)
                .ok_or_else(|| Off::new(format!("texts: unknown language {code:?}")))?;
            texts
                .by_language
                .entry(language)
                .or_default()
                .extend(entries);
        }
        Ok(texts)
    }

    /// Adds `other`'s texts over these.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::i18n::{Language, Texts};
    ///
    /// let mut texts = Texts::parse("[en]\na = \"A\"\n")?;
    /// texts.merge(Texts::parse("[en]\na = \"B\"\n")?);
    /// assert_eq!(texts.get("a", Language::FALLBACK), Some("B"));
    /// # Ok::<(), enshwick_loader_api::Off>(())
    /// ```
    pub fn merge(&mut self, other: Self) {
        for (language, entries) in other.by_language {
            self.by_language
                .entry(language)
                .or_default()
                .extend(entries);
        }
    }

    /// The text of `key` in `language`, else in English.
    ///
    /// # Examples
    ///
    /// See [`Texts`].
    #[must_use]
    pub fn get(&self, key: &str, language: Language) -> Option<&str> {
        let find = |language| {
            self.by_language
                .get(&language)
                .and_then(|entries| entries.get(key))
                .map(String::as_str)
        };
        find(language).or_else(|| find(Language::FALLBACK))
    }

    /// The texts as TOML, a table per language code, as [`Texts::parse`]
    /// reads them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::i18n::Texts;
    ///
    /// let texts = Texts::parse("[uk]\ngreeting = \"Привіт\"\n")?;
    /// assert_eq!(Texts::parse(&texts.to_toml())?, texts);
    /// # Ok::<(), enshwick_loader_api::Off>(())
    /// ```
    #[must_use]
    pub fn to_toml(&self) -> String {
        let tables: BTreeMap<&str, &BTreeMap<String, String>> = self
            .by_language
            .iter()
            .map(|(language, entries)| (language.code(), entries))
            .collect();
        toml::to_string(&tables).unwrap_or_default()
    }

    /// Every language with a text for `key`, and the text, in no set order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::i18n::Texts;
    ///
    /// let texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n")?;
    /// let mut all: Vec<&str> = texts.translations("greeting").map(|(_, text)| text).collect();
    /// all.sort_unstable();
    /// assert_eq!(all, ["Hello", "Привіт"]);
    /// # Ok::<(), enshwick_loader_api::Off>(())
    /// ```
    pub fn translations<'a>(
        &'a self,
        key: &'a str,
    ) -> impl Iterator<Item = (Language, &'a str)> + 'a {
        self.by_language
            .iter()
            .filter_map(move |(&language, entries)| {
                entries.get(key).map(|t| (language, t.as_str()))
            })
    }
}

impl Load<'_> {
    /// The mod's texts: `embedded`, usually `include_str!("texts.toml")`,
    /// with `mod-configs/<id>.texts.toml` over it when the server has one.
    /// The loader keeps them too, to label the mod's settings on the Mods
    /// screen: `config.<field>` (or the field's `text` key) for a row,
    /// `<key>.doc` for its description and `<key>.<choice>` for a choice.
    ///
    /// # Errors
    ///
    /// [`Off`] when either file is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let texts = load.texts("[en]\n\"safe.name\" = \"Safe\"\n")?;
    ///     let _ = texts;
    ///     Ok(())
    /// }
    /// ```
    pub fn texts(&mut self, embedded: &str) -> Result<Texts, Off> {
        let mut texts = Texts::parse(embedded)?;
        let data = self.data_dir()?;
        // `<game>/mod-data/<id>` beside `<game>/mod-configs/`.
        let overrides = data
            .file_name()
            .zip(data.parent().and_then(std::path::Path::parent))
            .map(|(id, game)| {
                game.join("mod-configs")
                    .join(format!("{}.texts.toml", id.to_string_lossy()))
            });
        if let Some(path) = overrides
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            texts.merge(
                Texts::parse(&text)
                    .map_err(|off| Off::new(format!("{}: {off}", path.display())))?,
            );
        }
        let api = self.host().api();
        if api
            .header
            .covers(core::mem::offset_of!(crate::abi::HostApi, texts) + size_of::<usize>())
        {
            let toml = texts.to_toml();
            let _kept = (api.texts)(self.host().handle(), crate::abi::Str::new(&toml));
        }
        Ok(texts)
    }

    /// Gives each game tag the translations of its key, for every language
    /// that has one: `(key, tag)` pairs, such as an item's name tag, each
    /// tag the mod's own content id.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old, refuses a tag's id (see
    /// [`Load::declare`]) or a key has no text at all.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Id;
    ///
    /// const SAFE_NAME: Id = Id::new("safes:safe.name");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let texts = load.texts("[en]\n\"safe.name\" = \"Safe\"\n[uk]\n\"safe.name\" = \"Сейф\"\n")?;
    ///     load.add_texts(&texts, &[("safe.name", SAFE_NAME)])
    /// }
    /// ```
    pub fn add_texts(&mut self, texts: &Texts, tags: &[(&str, Id)]) -> Result<(), Off> {
        for &(key, tag) in tags {
            let tag = self.declare(tag)?;
            let mut any = false;
            for (language, text) in texts.translations(key) {
                self.add_text(tag, language.0, text)?;
                any = true;
            }
            if !any {
                return Err(Off::new(format!("texts: no text for {key:?}")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_round_trips_through_its_code() {
        for &language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::ALL.len(), 35);
        assert_eq!(Language::from_code("PT-BR"), Some(Language::PT_BR));
        assert_eq!(Language::from_code("xx"), None);
    }

    #[test]
    fn unknown_languages_and_non_text_values_are_errors() {
        assert!(Texts::parse("[xx]\na = \"b\"\n").is_err());
        assert!(Texts::parse("[en]\na = 1\n").is_err());
    }
}
