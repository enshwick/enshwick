//! Translations as a script sees them: the namespace `i18n` and its texts
//! as the object `Texts`. A language crosses as its code, and a pair of a
//! language and a key or text as the list `{ code, text }`.

use super::{Language, Texts};
use crate::script::{Bound, Class, FromScript as _, Function, Object, Value};
use crate::{Load, Off};
use enshwick_values::content::Id;

/// Each of `pairs` with its language as its code.
fn coded<'a>(pairs: impl IntoIterator<Item = (Language, &'a str)>) -> Vec<(String, String)> {
    pairs
        .into_iter()
        .map(|(language, text)| (language.code().to_owned(), text.to_owned()))
        .collect()
}

impl Object for Texts {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Texts",
        "A mod's texts by language and key, with the lookup and fill the game's own texts use.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_TEXTS];
}

#[crate::script(texts)]
impl Texts {
    /// Adds `other`'s texts over these, as [`Texts::merge`] does, and
    /// returns the texts merged: a script's object stays as it was.
    #[script(name = "merge")]
    pub(crate) fn merged(mut self, other: Self) -> Self {
        self.merge(other);
        self
    }

    /// The text of `key` in `language`, else in its next language, English
    /// last, as [`Texts::get`] looks it up.
    #[script(name = "get")]
    pub(crate) fn get_script(&self, key: &str, language: Language) -> Option<String> {
        self.get(key, language).map(str::to_owned)
    }

    /// The text of `key` in `language` with each `{name}` replaced by the
    /// value `args` pairs with it, as [`Texts::fill`] fills it; `args` is a
    /// list of `{ name, value }`.
    #[script(name = "fill")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the pairs over as its own copy"
    )]
    pub(crate) fn fill_script(
        &self,
        key: &str,
        language: Language,
        args: Vec<(String, String)>,
    ) -> String {
        let args: Vec<(&str, &str)> = args
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        self.fill(key, language, &args)
    }

    /// The texts as TOML, a table per language code, as
    /// [`Texts::to_toml`] writes them.
    #[script(name = "to_toml")]
    pub(crate) fn to_toml_script(&self) -> String {
        self.to_toml()
    }

    /// Every language with a text for `key`, each as `{ code, text }`, as
    /// [`Texts::translations`] lists them.
    #[script(name = "translations")]
    pub(crate) fn translations_script(&self, key: &str) -> Vec<(String, String)> {
        coded(self.translations(key))
    }

    /// Every language with texts of its own, as [`Texts::languages`] lists
    /// them.
    #[script(name = "languages")]
    pub(crate) fn languages_script(&self) -> Vec<Language> {
        self.languages().collect()
    }

    /// Each of `languages` with each key it lacks, as `{ code, key }`, as
    /// [`Texts::missing_in`] pairs them.
    #[script(name = "missing_in")]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "a script hands the list over as its own copy"
    )]
    pub(crate) fn missing_in_script(&self, languages: Vec<Language>) -> Vec<(String, String)> {
        coded(self.missing_in(&languages))
    }

    /// Each language with each key another language has and it lacks, as
    /// `{ code, key }`, as [`Texts::untranslated`] pairs them.
    #[script(name = "untranslated")]
    pub(crate) fn untranslated_script(&self) -> Vec<(String, String)> {
        coded(self.untranslated())
    }
}

/// The free functions of [`i18n`](super) as a script's namespace `i18n`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct I18nFns;

#[crate::script(i18n_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl I18nFns {
    /// The language the player reads the game in, as
    /// [`language`](super::language) gives it.
    #[script]
    #[must_use]
    pub fn language(self) -> Language {
        super::language()
    }

    /// The game's own text of tag `id` in the player's language, as
    /// [`game_text`](super::game_text) reads it; nil on a server.
    #[script]
    #[must_use]
    pub fn game_text(self, id: u32) -> Option<String> {
        super::game_text(id).map(str::to_owned)
    }

    /// The game's own text of tag `id` in `language`, as
    /// [`game_text_in`](super::game_text_in) reads it.
    #[script]
    #[must_use]
    pub fn game_text_in(self, id: u32, language: Language) -> Option<String> {
        super::game_text_in(id, language).map(str::to_owned)
    }

    /// Texts from TOML, a table per language code, as [`Texts::parse`]
    /// reads them.
    ///
    /// # Errors
    ///
    /// As [`Texts::parse`].
    #[script]
    pub fn parse(self, toml: &str) -> Result<Texts, Off> {
        Texts::parse(toml)
    }
}

impl Bound for I18nFns {
    const NAME: &'static str = "i18n";
    const DOC: &'static str = "The game's languages and texts, and a mod's own translations.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_I18N_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

#[crate::script(i18n)]
impl Load<'_> {
    /// The mod's texts: the `texts.toml` of its folder, handed to the
    /// loader as [`Load::texts`] hands a Rust mod's.
    ///
    /// # Errors
    ///
    /// [`Off`] when the folder has no `texts.toml` or it is malformed.
    #[script(name = "texts")]
    pub(crate) fn texts_script(&mut self) -> Result<Texts, Off> {
        let path = self.mod_dir()?.join(super::TEXTS_FILE);
        let toml = std::fs::read_to_string(&path)
            .map_err(|e| Off::new(format!("texts: {}: {e}", path.display())))?;
        self.texts_from(&toml)
    }

    /// Texts from `toml`, handed to the loader as [`Load::texts_from`]
    /// hands them.
    ///
    /// # Errors
    ///
    /// As [`Load::texts_from`].
    #[script(name = "texts_from")]
    pub(crate) fn texts_from_script(&mut self, toml: &str) -> Result<Texts, Off> {
        self.texts_from(toml)
    }

    /// Gives each game tag the translations of its key, as
    /// [`Load::add_texts`] does; `tags` is a list of `{ key, content id }`.
    ///
    /// # Errors
    ///
    /// As [`Load::add_texts`], and for a tag that is no content id.
    #[script(name = "add_texts")]
    pub(crate) fn add_texts_script(
        &mut self,
        texts: &Texts,
        tags: Vec<(String, String)>,
    ) -> Result<(), Off> {
        let tags = tags
            .into_iter()
            .map(|(key, tag)| {
                Id::from_value(Value::String(tag))
                    .map(|tag| (key, tag))
                    .map_err(|e| Off::new(format!("texts: {e}")))
            })
            .collect::<Result<Vec<_>, Off>>()?;
        let tags: Vec<(&str, Id)> = tags.iter().map(|(key, tag)| (key.as_str(), *tag)).collect();
        self.add_texts(texts, &tags)
    }
}
