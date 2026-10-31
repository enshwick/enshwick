//! Translations: a mod ships its texts per language in `src/texts.toml`, a
//! server owner may add to them or change them, and the game shows them for
//! the player's language. [`texts!`](crate::texts) names the keys.
//!
//! ```toml
//! [en]
//! "locker.name" = "Locker"
//!
//! [uk]
//! "locker.name" = "Шафка"
//! ```
//!
//! `en` is the fallback for every language without a text of its own. A file
//! `mod-configs/<id>.texts.toml` in the same form overrides or adds texts.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;

use crate::content::Id;
use crate::locks::Relock;
use crate::{Host, Load, Off};

pub(crate) mod reason;

/// The language the player reads the game in, for a mod's own texts in its
/// dialogs, windows and answers.
///
/// On a server, the language of the player the loader asks the mod about,
/// such as in an action or a settings check, as that player's game said
/// when it joined; the fallback elsewhere on a server, and on a client
/// before its first text.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::{Texts, language};
///
/// let texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n")?;
/// let greeting = texts.fill("greeting", language(), &[]);
/// assert!(!greeting.is_empty());
/// # Ok::<(), enshwick::Off>(())
/// ```
#[must_use]
pub fn language() -> Language {
    crate::host().map_or(Language::FALLBACK, |host| host.language())
}

impl Host {
    /// The language the player reads the game in; see [`language`].
    ///
    /// # Examples
    ///
    /// ```
    /// if let Some(host) = enshwick::host() {
    ///     enshwick::info!("texts in {}", host.language().code());
    /// }
    /// ```
    #[must_use]
    pub fn language(&self) -> Language {
        let language = Language((self.api().language)(self.handle()));
        if language.code().is_empty() {
            Language::FALLBACK
        } else {
            language
        }
    }
}

/// The game's own text of tag `id` in the language the player reads the
/// game in, such as an item's or a survivor's name; a text missing there
/// comes in English, as the game shows it.
///
/// `id` is a text tag's game id, [`Guid::fnv1a`](crate::content::Guid::fnv1a) of
/// the tag an item's or a template's data names. The text is read from the
/// game's archive and stays valid for the process. `None` for a tag the
/// game lacks, and always on a server, whose archive holds no texts.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::game_text;
///
/// if let Some(text) = game_text(0x8f21_95d5) {
///     enshwick::info!("the game calls it {text}");
/// }
/// ```
#[must_use]
pub fn game_text(id: u32) -> Option<&'static str> {
    game_text_in(id, language())
}

/// The game's own text of tag `id` in `language`, as [`game_text`] reads
/// it; [`Language::FALLBACK`] asks for English.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::{Language, game_text_in};
///
/// let name = game_text_in(0x8f21_95d5, Language::UK_UA);
/// assert!(name.is_none() || name.is_some_and(|n| !n.is_empty()));
/// ```
#[must_use]
pub fn game_text_in(id: u32, language: Language) -> Option<&'static str> {
    crate::host()?.game_text(id, language)
}

impl Host {
    /// The game's own text of tag `id` in `language`; see [`game_text_in`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// if let Some(host) = enshwick::host() {
    ///     let _ = host.game_text(0x8f21_95d5, Language::DE_DE);
    /// }
    /// ```
    #[must_use]
    pub fn game_text(&self, id: u32, language: Language) -> Option<&'static str> {
        let api = self.api();
        let mut out = crate::abi::Str::EMPTY;
        if !(api.game_text)(self.handle(), id, language.0, &raw mut out).is_ok() {
            return None;
        }
        // SAFETY: the loader keeps the text for the life of the process.
        unsafe { out.as_str() }
    }
}

/// A game language, as the game numbers them.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::Language;
///
/// assert_eq!(Language::from_code("uk"), Some(Language::UK_UA));
/// assert_eq!(Language::UK_UA.code(), "uk");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Language(pub u32);

macro_rules! languages {
    ($($name:ident = $id:literal, $code:literal, $parent:ident, $doc:literal;)*) => {
        impl Language {
            $(#[doc = $doc] pub const $name: Self = Self($id);)*

            /// Every language the game knows, the fallback first.
            pub const ALL: &[Self] = &[$(Self::$name),*];

            /// The language a text missing in this one is looked up in
            /// next: the main variety of a regional language, else the
            /// fallback.
            const fn parent(self) -> Self {
                match self.0 {
                    $($id => Self::$parent,)*
                    _ => Self::FALLBACK,
                }
            }

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

// Each language with the one its missing texts come from next.
languages! {
    FALLBACK = 0, "en", FALLBACK, "English, and every language without a text of its own.";
    EN_US = 1, "en-us", FALLBACK, "English (United States).";
    EN_GB = 2, "en-gb", FALLBACK, "English (United Kingdom).";
    DE_DE = 3, "de", FALLBACK, "German.";
    NL_NL = 4, "nl", FALLBACK, "Dutch.";
    FR_FR = 5, "fr", FALLBACK, "French.";
    IT_IT = 6, "it", FALLBACK, "Italian.";
    ES_ES = 7, "es", FALLBACK, "Spanish (Spain).";
    ES_MX = 8, "es-mx", ES_419, "Spanish (Mexico).";
    RU_RU = 9, "ru", FALLBACK, "Russian.";
    PL_PL = 10, "pl", FALLBACK, "Polish.";
    JA_JP = 11, "ja", FALLBACK, "Japanese.";
    ID_ID = 12, "id", FALLBACK, "Indonesian.";
    KO_KR = 13, "ko", FALLBACK, "Korean.";
    MS_MY = 14, "ms", FALLBACK, "Malay.";
    PT_PT = 15, "pt", FALLBACK, "Portuguese (Portugal).";
    PT_BR = 16, "pt-br", PT_PT, "Portuguese (Brazil).";
    TH_TH = 17, "th", FALLBACK, "Thai.";
    TR_TR = 18, "tr", FALLBACK, "Turkish.";
    VI_VN = 19, "vi", FALLBACK, "Vietnamese.";
    ZH_CN = 20, "zh-cn", FALLBACK, "Chinese (Simplified).";
    ZH_TW = 21, "zh-tw", FALLBACK, "Chinese (Traditional).";
    FR_CA = 22, "fr-ca", FR_FR, "French (Canada).";
    ES_419 = 23, "es-419", ES_ES, "Spanish (Latin America).";
    BG_BG = 24, "bg", FALLBACK, "Bulgarian.";
    CS_CZ = 25, "cs", FALLBACK, "Czech.";
    DA_DK = 26, "da", FALLBACK, "Danish.";
    FI_FI = 27, "fi", FALLBACK, "Finnish.";
    EL_GR = 28, "el", FALLBACK, "Greek.";
    HU_HU = 29, "hu", FALLBACK, "Hungarian.";
    NO_NO = 30, "no", FALLBACK, "Norwegian.";
    NB_NO = 31, "nb", NO_NO, "Norwegian (Bokmål).";
    RO_RO = 32, "ro", FALLBACK, "Romanian.";
    SV_SE = 33, "sv", FALLBACK, "Swedish.";
    UK_UA = 34, "uk", FALLBACK, "Ukrainian.";
}

impl Language {
    /// The languages the game's settings offer a player, the set a mod
    /// translates into: English as [`Language::FALLBACK`], which the game's
    /// English (`en-us`) reads, then the others in the menu's order (client
    /// build 426).
    pub const OFFERED: &[Self] = &[
        Self::FALLBACK,
        Self::DE_DE,
        Self::FR_FR,
        Self::IT_IT,
        Self::ES_ES,
        Self::RU_RU,
        Self::PL_PL,
        Self::JA_JP,
        Self::KO_KR,
        Self::PT_BR,
        Self::TH_TH,
        Self::TR_TR,
        Self::ZH_CN,
        Self::ZH_TW,
        Self::UK_UA,
    ];

    /// The languages every key of a mod's texts needs a text in:
    /// [`Language::FALLBACK`] and Ukrainian. [`texts!`](crate::texts) and
    /// [`declare_mod!`](crate::declare_mod) refuse to build a mod whose
    /// texts lack one; an empty text counts as none.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// assert!(Language::REQUIRED.iter().all(|l| Language::OFFERED.contains(l)));
    /// ```
    pub const REQUIRED: &[Self] = &[Self::FALLBACK, Self::UK_UA];

    /// The languages a text for this one is looked up in, in order: this
    /// language, the main variety of a regional one (`es-mx`, `es-419`,
    /// `es`), and [`Language::FALLBACK`] last.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// let order: Vec<Language> = Language::PT_BR.lookup_order().collect();
    /// assert_eq!(order, [Language::PT_BR, Language::PT_PT, Language::FALLBACK]);
    /// ```
    pub fn lookup_order(self) -> impl Iterator<Item = Self> {
        core::iter::successors(Some(self), |language| {
            (*language != Self::FALLBACK).then(|| language.parent())
        })
    }
}

// Every lookup ends in the fallback, so every key needs a text there.
const _: () = assert!(Language::REQUIRED[0].0 == Language::FALLBACK.0);

/// The key of the mod's own name in its texts: the Mods screen, the join
/// dialog and the status show it in the player's language, and the
/// manifest's `name` where the texts lack it.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::{Language, MOD_NAME, Texts};
///
/// let texts = Texts::parse("[en]\n\"mod.name\" = \"Better Altars\"\n[uk]\n\"mod.name\" = \"Кращі олтарі\"\n")?;
/// assert_eq!(texts.get(MOD_NAME, Language::UK_UA), Some("Кращі олтарі"));
/// # Ok::<(), enshwick::Off>(())
/// ```
pub const MOD_NAME: &str = "mod.name";

/// The key of the sentence on what the mod does in its texts, shown as
/// [`MOD_NAME`] is, the manifest's `description` where the texts lack it.
///
/// # Examples
///
/// ```
/// assert_eq!(enshwick::i18n::MOD_DESCRIPTION, "mod.description");
/// ```
pub const MOD_DESCRIPTION: &str = "mod.description";

/// A mod's texts by language and key.
///
/// # Examples
///
/// ```
/// use enshwick::i18n::{Language, Texts};
///
/// let texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n")?;
/// assert_eq!(texts.get("greeting", Language::UK_UA), Some("Привіт"));
/// assert_eq!(texts.get("greeting", Language::DE_DE), Some("Hello"));
/// # Ok::<(), enshwick::Off>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Texts {
    by_language: FxHashMap<Language, BTreeMap<String, String>>,
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
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// let mut texts = Texts::parse("[en]\na = \"A\"\n")?;
    /// texts.merge(Texts::parse("[en]\na = \"B\"\n")?);
    /// assert_eq!(texts.get("a", Language::FALLBACK), Some("B"));
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    pub fn merge(&mut self, other: Self) {
        for (language, entries) in other.by_language {
            self.by_language
                .entry(language)
                .or_default()
                .extend(entries);
        }
    }

    /// The text of `key` in `language`, else in the next language of its
    /// [`lookup_order`](Language::lookup_order), English last.
    ///
    /// # Examples
    ///
    /// See [`Texts`].
    #[must_use]
    pub fn get(&self, key: &str, language: Language) -> Option<&str> {
        language.lookup_order().find_map(|language| {
            self.by_language
                .get(&language)
                .and_then(|entries| entries.get(key))
                .map(String::as_str)
        })
    }

    /// The text of `key` in `language` with each `{name}` in it replaced by
    /// the value `args` pairs with `name`; the key itself when no language
    /// has a text for it, so a missing text shows rather than vanishes.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// let texts = Texts::parse("[en]\nupdate = \"{name}: {from} to {to}\"\n")?;
    /// let line = texts.fill("update", Language::UK_UA, &[("name", "Lanterns"), ("from", "0.1.0"), ("to", "0.2.0")]);
    /// assert_eq!(line, "Lanterns: 0.1.0 to 0.2.0");
    /// assert_eq!(texts.fill("missing", Language::FALLBACK, &[]), "missing");
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn fill(&self, key: &str, language: Language, args: &[(&str, &str)]) -> String {
        let text = self.get(key, language).unwrap_or(key);
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let value = after.find('}').and_then(|close| {
                let name = &after[..close];
                args.iter()
                    .find(|(arg, _)| *arg == name)
                    .map(|(_, value)| (*value, close))
            });
            if let Some((value, close)) = value {
                out.push_str(value);
                rest = &after[close + 1..];
            } else {
                out.push('{');
                rest = after;
            }
        }
        out.push_str(rest);
        out
    }

    /// A reason a mod gave as a text, such as [`Off::text`], filled in
    /// `language`; `None` when its key names no text here.
    #[doc(hidden)]
    #[must_use]
    pub fn reason(&self, reason: &str, language: Language) -> Option<String> {
        let (key, args) = reason::decode(reason);
        self.get(key, language)?;
        Some(self.fill(key, language, &args))
    }

    /// The texts as TOML, a table per language code, as [`Texts::parse`]
    /// reads them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Texts;
    ///
    /// let texts = Texts::parse("[uk]\ngreeting = \"Привіт\"\n")?;
    /// assert_eq!(Texts::parse(&texts.to_toml())?, texts);
    /// # Ok::<(), enshwick::Off>(())
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
    /// use enshwick::i18n::Texts;
    ///
    /// let texts = Texts::parse("[en]\ngreeting = \"Hello\"\n[uk]\ngreeting = \"Привіт\"\n")?;
    /// let mut all: Vec<&str> = texts.translations("greeting").map(|(_, text)| text).collect();
    /// all.sort_unstable();
    /// assert_eq!(all, ["Hello", "Привіт"]);
    /// # Ok::<(), enshwick::Off>(())
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

    /// Every language with texts of its own, in the order of
    /// [`Language::ALL`]: the set a test of the texts iterates, so a new
    /// language is a new table in the file and no change to the test.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// let texts = Texts::parse("[uk]\na = \"А\"\n[en]\na = \"A\"\n")?;
    /// let languages: Vec<Language> = texts.languages().collect();
    /// assert_eq!(languages, [Language::FALLBACK, Language::UK_UA]);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    pub fn languages(&self) -> impl Iterator<Item = Language> + '_ {
        Language::ALL
            .iter()
            .copied()
            .filter(|language| self.by_language.contains_key(language))
    }

    /// Each of `languages` paired with each key some language has a text
    /// for and it lacks, in the order of `languages`, then by key; a text of
    /// its main variety counts (`pt` for `pt-br`), English only for
    /// [`Language::FALLBACK`] itself. A mod's test checks its texts against
    /// [`Language::OFFERED`] with it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// let texts = Texts::parse("[en]\na = \"A\"\n[uk]\na = \"А\"\n")?;
    /// assert_eq!(texts.missing_in(&[Language::UK_UA, Language::DE_DE]), [(Language::DE_DE, "a")]);
    /// // In a mod's test: assert_eq!(texts.missing_in(Language::OFFERED), []);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn missing_in(&self, languages: &[Language]) -> Vec<(Language, &str)> {
        let keys: std::collections::BTreeSet<&str> = self
            .by_language
            .values()
            .flat_map(|entries| entries.keys().map(String::as_str))
            .collect();
        let has = |language: Language, key: &str| {
            language
                .lookup_order()
                .take_while(|l| *l != Language::FALLBACK || language == Language::FALLBACK)
                .any(|l| {
                    self.by_language
                        .get(&l)
                        .is_some_and(|e| e.contains_key(key))
                })
        };
        languages
            .iter()
            .flat_map(|&language| {
                keys.iter()
                    .filter(move |key| !has(language, key))
                    .map(move |&key| (language, key))
            })
            .collect()
    }

    /// Each language paired with each key another language has a text for
    /// and it lacks, sorted by language as [`languages`](Self::languages)
    /// gives them, then by key; empty when every language has every text.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::{Language, Texts};
    ///
    /// let texts = Texts::parse("[en]\na = \"A\"\nb = \"B\"\n[uk]\na = \"А\"\n")?;
    /// assert_eq!(texts.untranslated(), [(Language::UK_UA, "b")]);
    /// # Ok::<(), enshwick::Off>(())
    /// ```
    #[must_use]
    pub fn untranslated(&self) -> Vec<(Language, &str)> {
        let keys: std::collections::BTreeSet<&str> = self
            .by_language
            .values()
            .flat_map(|entries| entries.keys().map(String::as_str))
            .collect();
        self.languages()
            .flat_map(|language| {
                let entries = &self.by_language[&language];
                keys.iter()
                    .filter(|key| !entries.contains_key(**key))
                    .map(move |&key| (language, key))
            })
            .collect()
    }
}

/// The texts each mod, or each data pack in the loader's copy of the
/// API, handed the loader so far.
static HANDED: crate::fresh::Fresh<std::sync::Mutex<Vec<(crate::abi::ModHandle, Texts)>>> =
    crate::fresh::Fresh::new();

impl Load<'_> {
    /// The mod's texts: its `src/texts.toml`, which
    /// [`declare_mod!`](crate::declare_mod) embeds and checks, with
    /// `mod-configs/<id>.texts.toml` over it when the server has one. The
    /// loader keeps them too, to label the mod's settings on the Mods
    /// screen: `config.<field>` (or the field's `text` key) for a row,
    /// `<key>.doc` for its description and `<key>.<choice>` for a choice.
    /// [`texts!`](crate::texts) names the keys.
    ///
    /// # Errors
    ///
    /// [`Off`] when the crate has no `src/texts.toml` or the server's file
    /// is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::i18n::Language;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts()?;
    ///     enshwick::info!("{}", texts.fill("safe.name", Language::FALLBACK, &[]));
    ///     Ok(())
    /// }
    /// ```
    pub fn texts(&mut self) -> Result<Texts, Off> {
        let embedded = self
            .embedded_texts()
            .ok_or_else(|| Off::new("texts: the mod's crate has no src/texts.toml"))?;
        self.texts_from(embedded)
    }

    /// Texts from `toml` as [`Load::texts`] hands them over, such as a data
    /// pack's; each call adds to what the loader keeps.
    ///
    /// # Errors
    ///
    /// [`Off`] when `toml` or the server's file is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts_from("[en]\n\"locker.name\" = \"Locker\"\n")?;
    ///     let _ = texts;
    ///     Ok(())
    /// }
    /// ```
    pub fn texts_from(&mut self, toml: &str) -> Result<Texts, Off> {
        let mut texts = Texts::parse(toml)?;
        let data = self.data_dir()?;
        // `<game>/mod-data/<id>` beside `<game>/mod-configs/`.
        let overrides = data
            .file_name()
            .zip(data.parent().and_then(std::path::Path::parent))
            .map(|(id, game)| {
                game.join("mod-configs")
                    .join(format!("{}.texts.toml", id.to_string_lossy()))
            });
        if let Some(path) = overrides {
            match std::fs::read_to_string(&path) {
                Ok(text) => texts.merge(
                    Texts::parse(&text)
                        .map_err(|off| Off::new(format!("{}: {off}", path.display())))?,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                // Such as a file saved as UTF-16 or in a code page.
                Err(error) => return Err(Off::new(format!("{}: {error}", path.display()))),
            }
        }
        let api = self.host().api();
        // The loader keeps the last texts it was given, so it gets all
        // of this mod's, a pack's added to the mod's own.
        let me = self.host().handle();
        let mut handed = HANDED.get_or_init(std::sync::Mutex::default).relock();
        let all = if let Some((_, kept)) = handed.iter_mut().find(|(owner, _)| *owner == me) {
            kept.merge(texts.clone());
            kept.to_toml()
        } else {
            handed.push((me, texts.clone()));
            texts.to_toml()
        };
        let _kept = (api.texts)(me, crate::abi::Str::new(&all));

        Ok(texts)
    }

    /// Gives each game tag the translations of its key, for every language
    /// that has one: `(key, tag)` pairs, such as an item's name tag, each
    /// tag the mod's own content id.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses a tag's id (see [`Load::declare`]) or
    /// a key has no text at all.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, id};
    ///
    /// const LOCKER_NAME: Id = id!("locker.name");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts_from("[en]\n\"locker.name\" = \"Locker\"\n[uk]\n\"locker.name\" = \"Шафка\"\n")?;
    ///     load.add_texts(&texts, &[("locker.name", LOCKER_NAME)])
    /// }
    /// ```
    pub fn add_texts(&mut self, texts: &Texts, tags: &[(&str, Id)]) -> Result<(), Off> {
        for &(key, tag) in tags {
            let tag = self.declare(tag)?;
            let mut any = false;
            for (language, text) in texts.translations(key) {
                self.add_text(tag, language, text)?;
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
    fn a_text_reason_fills_in_the_language_asked_and_plain_words_name_no_text() {
        let texts = Texts::parse(
            "[en]\n\"glow.off\" = \"No glow on {build}.\"\n[uk]\n\"glow.off\" = \"Без сяйва на {build}.\"\n",
        )
        .unwrap();
        let sent = reason::encode("glow.off", &[("build", "426")]);
        assert_eq!(
            texts.reason(&sent, Language::FALLBACK).as_deref(),
            Some("No glow on 426.")
        );
        assert_eq!(
            texts.reason(&sent, Language::UK_UA).as_deref(),
            Some("Без сяйва на 426.")
        );
        assert_eq!(texts.reason("glow is missing", Language::UK_UA), None);
    }

    #[test]
    fn every_language_round_trips_through_its_code() {
        for &language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
        assert_eq!(Language::from_code("PT-BR"), Some(Language::PT_BR));
        assert_eq!(Language::from_code("xx"), None);
    }

    #[test]
    fn a_regional_language_falls_back_to_its_main_variety_before_english() {
        let texts = Texts::parse(
            "[en]\na = \"A\"\nb = \"B\"\n[es]\na = \"Ae\"\nb = \"Be\"\n[es-419]\na = \"Al\"\n[pt]\na = \"Ap\"\n",
        )
        .unwrap();
        assert_eq!(texts.get("a", Language::ES_MX), Some("Al"));
        assert_eq!(texts.get("b", Language::ES_MX), Some("Be"));
        assert_eq!(texts.get("a", Language::PT_BR), Some("Ap"));
        assert_eq!(texts.get("b", Language::PT_BR), Some("B"));
        assert_eq!(texts.get("a", Language::ZH_TW), Some("A"));
        assert_eq!(texts.get("a", Language(99)), Some("A"));
    }

    #[test]
    fn missing_texts_are_named_per_language_and_a_main_variety_counts() {
        let texts = Texts::parse(
            "[en]\na = \"A\"\nb = \"B\"\n[pt]\na = \"Ap\"\nb = \"Bp\"\n[de]\na = \"Ad\"\n",
        )
        .unwrap();
        let wanted = [
            Language::FALLBACK,
            Language::PT_BR,
            Language::DE_DE,
            Language::UK_UA,
        ];
        assert_eq!(
            texts.missing_in(&wanted),
            [
                (Language::DE_DE, "b"),
                (Language::UK_UA, "a"),
                (Language::UK_UA, "b")
            ]
        );
    }

    #[test]
    fn every_offered_language_has_a_code_and_english_is_the_fallback() {
        assert_eq!(Language::OFFERED.first(), Some(&Language::FALLBACK));
        assert!(!Language::OFFERED.contains(&Language::EN_US));
        for language in Language::OFFERED {
            assert!(!language.code().is_empty());
        }
    }

    #[test]
    fn every_language_s_lookup_order_ends_at_the_fallback_once() {
        for &language in Language::ALL.iter().chain(&[Language(99)]) {
            let order: Vec<Language> = language.lookup_order().collect();
            assert_eq!(order.first(), Some(&language));
            assert_eq!(order.last(), Some(&Language::FALLBACK));
            assert_eq!(
                order.iter().filter(|l| **l == Language::FALLBACK).count(),
                1
            );
        }
    }

    #[test]
    fn placeholders_take_their_values_and_others_stay() {
        let texts =
            Texts::parse("[en]\na = \"{x} and {y} and {unknown} and {\"\n[uk]\na = \"{y}, {x}\"\n")
                .unwrap();
        let args = [("x", "1"), ("y", "{x}")];
        assert_eq!(
            texts.fill("a", Language::FALLBACK, &args),
            "1 and {x} and {unknown} and {"
        );
        assert_eq!(texts.fill("a", Language::UK_UA, &args), "{x}, 1");
    }

    #[test]
    fn unknown_languages_and_non_text_values_are_errors() {
        assert!(Texts::parse("[xx]\na = \"b\"\n").is_err());
        assert!(Texts::parse("[en]\na = 1\n").is_err());
    }
}
