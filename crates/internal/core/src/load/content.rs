//! `Load`'s methods of a module below core, forwarded to the base handle.

use crate::abi::{ResourceKey, Str};
use crate::content::{Guid, Id};
use crate::{Load, Off};
use enshwick_content::__load::*;

mod changes;
mod changes_items;
mod changes_live;
mod changes_recipes;
#[cfg(feature = "typed")]
mod enemies;
mod icons;
mod items;
#[cfg(feature = "typed")]
mod journal;
#[cfg(feature = "typed")]
mod knowledge;
mod props;
mod recipes;
#[cfg(feature = "typed")]
mod removal;
mod renames;
mod script;
#[cfg(feature = "typed")]
mod skills;
#[cfg(feature = "typed")]
mod spawns;
mod templates;
mod workshops;

#[enshwick_macros::forward(base)]
#[crate::script(content_ids)]
impl Load<'_> {
    /// Declares the mod's content id `id` and returns its GUID. The content
    /// calls declare what they create themselves; call it for content the
    /// mod serves with [`Load::serve`]. Declaring one id again is allowed.
    ///
    /// # Errors
    ///
    /// [`Off`] after init, for an id outside the
    /// mod's namespace, or for one whose game id another content id has,
    /// which the loader logs with both ids and both mods.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, id};
    ///
    /// const LOCKER: Id = id!("locker");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let guid = load.declare(LOCKER)?;
    ///     assert_eq!(guid, LOCKER.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn declare(&mut self, id: Id) -> Result<Guid, Off>;

    /// Gives `LocaTag` `tag` the text `text` in `language`; the client shows it
    /// wherever the game asks for the tag, such as the name of an item whose
    /// `name` field holds `tag`. [`Load::add_texts`] gives many at once.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the text.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::i18n::Language;
    ///
    /// const LOCKER_NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.add_text(LOCKER_NAME, Language::FALLBACK, "Locker")?;
    ///     load.add_text(LOCKER_NAME, Language::UK_UA, "Шафка")
    /// }
    /// ```
    #[script]
    pub fn add_text(
        &mut self,
        tag: Guid,
        language: crate::i18n::Language,
        text: &str,
    ) -> Result<(), Off>;

    /// Serves the resource `key`, built by `builder` from the resource
    /// `base` the first time the game reads it: a new resource when the keys
    /// differ, a changed vanilla one when they are equal. `None` from the
    /// builder turns the mod off and the resource reads as missing.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses it, such as after init. Several mods
    /// may serve one key: their builders run in load order, each on the one
    /// before's result.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Blob, Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    /// const LOCKER: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000001");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     load.serve(types::ITEM_INFO.key(LOCKER)?, types::ITEM_INFO.key(CHEST)?, |chest| {
    ///         let mut item = Blob::new(chest);
    ///         item.write(0, &enshwick::ItemId::from_guid(LOCKER).get().to_le_bytes());
    ///         item.write(4, &LOCKER.0);
    ///         Some(item.into_bytes())
    ///     })
    /// }
    /// ```
    pub fn serve(
        &mut self,
        key: ResourceKey,
        base: ResourceKey,
        builder: impl Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static,
    ) -> Result<(), Off>;
}

#[enshwick_macros::forward(base)]
impl Load<'_> {
    /// Lets [`rebuild`] build the resource `key`, which this mod serves,
    /// again later: the loader keeps a copy of its base from its first
    /// build. Call it during `init`, after [`Load::serve`].
    ///
    /// # Errors
    ///
    /// [`Off`] for a key the mod does not serve.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let key = types::ITEM_INFO.key(CHEST)?;
    ///     load.serve(key, key, |chest| Some(chest.to_vec()))?;
    ///     load.allow_rebuild(key)
    /// }
    /// ```
    pub fn allow_rebuild(&mut self, key: ResourceKey) -> Result<(), Off>;
}
