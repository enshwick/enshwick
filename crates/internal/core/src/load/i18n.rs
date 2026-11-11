//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::locks::Relock;
use crate::{Host, Load, Off};
use crate::{Load, Off};
use enshwick_values::content::Id;
use enshwick_values::reason;
use rustc_hash::FxHashMap;
#[doc(hidden)]
pub use script::I18nFns;
use std::collections::BTreeMap;

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// The mod's texts: its `src/texts.toml`, which
    /// [`declare_mod!`](crate::declare_mod) embeds and checks. The loader
    /// keeps them too, to label the mod's settings on the Mods
    /// screen: `config.<field>` (or the field's `text` key) for a row,
    /// `<key>.doc` for its description and `<key>.<choice>` for a choice.
    /// [`texts!`](crate::texts) names the keys.
    ///
    /// # Errors
    ///
    /// [`Off`] when the crate has no `src/texts.toml`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::i18n::Language;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts()?;
    ///     enshwick::info!("{}", texts.fill("safe.name", Language::FALLBACK, &[]));
    ///     Ok(())
    /// }
    /// ```
    pub fn texts(&mut self) -> Result<Texts, Off>;

    /// Texts from `toml` as [`Load::texts`] hands them over, such as a data
    /// pack's; each call adds to what the loader keeps.
    ///
    /// # Errors
    ///
    /// [`Off`] when `toml` is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts_from("[en]\n\"locker.name\" = \"Locker\"\n")?;
    ///     let _ = texts;
    ///     Ok(())
    /// }
    /// ```
    pub fn texts_from(&mut self, toml: &str) -> Result<Texts, Off>;

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
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::{Id, id};
    ///
    /// const LOCKER_NAME: Id = id!("locker.name");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let texts = load.texts_from("[en]\n\"locker.name\" = \"Locker\"\n[uk]\n\"locker.name\" = \"Шафка\"\n")?;
    ///     load.add_texts(&texts, &[("locker.name", LOCKER_NAME)])
    /// }
    /// ```
    pub fn add_texts(&mut self, texts: &Texts, tags: &[(&str, Id)]) -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
#[crate::script(i18n)]
impl Load<'_> {
    /// The mod's texts: the `texts.toml` of its folder, handed to the
    /// loader as [`Load::texts`] hands a Rust mod's.
    ///
    /// # Errors
    ///
    /// [`Off`] when the folder has no `texts.toml` or it is malformed.
    #[script(name = "texts")]
    pub(crate) fn texts_script(&mut self) -> Result<Texts, Off>;

    /// Texts from `toml`, handed to the loader as [`Load::texts_from`]
    /// hands them.
    ///
    /// # Errors
    ///
    /// As [`Load::texts_from`].
    #[script(name = "texts_from")]
    pub(crate) fn texts_from_script(&mut self, toml: &str) -> Result<Texts, Off>;

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
    ) -> Result<(), Off>;
}
