//! Game content from mods: new resources such as items and recipes, and
//! changes to vanilla ones, served to the game as if they came from its
//! archive.
//!
//! Resources are blobs: plain data whose arrays and strings are offsets from
//! the field that holds them. [`Blob`] edits them safely: it grows arrays by
//! moving them to the end and keeps every offset right.

use core::ffi::c_void;

use crate::abi::{ResourceKey, ResourcePatchFn, Status, Str};
use crate::{Load, Off};

pub mod categories;
pub mod changes;
pub mod containers;
#[cfg(feature = "typed")]
pub mod enemies;
pub mod icons;
mod ids;
pub mod items;
#[cfg(feature = "typed")]
pub mod journal;
#[cfg(feature = "typed")]
pub mod knowledge;
pub mod loot_tables;
pub mod menu;
pub mod props;
pub mod recipes;
#[cfg(feature = "typed")]
pub mod removal;
pub mod renames;
mod script;
#[cfg(feature = "typed")]
pub mod skills;
#[cfg(feature = "typed")]
pub mod spawns;
pub mod templates;
pub mod vanilla;
pub mod workshops;

#[doc(inline)]
pub use crate::facts::EngineFacts;
pub use ids::*;
#[doc(hidden)]
pub use script::{Content, Rule};

/// A type of game resource by enshwick's name for it, such as
/// [`types::ITEM_INFO`]; the loader maps it to the running build's type,
/// whose hash resource keys carry.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::{Guid, types};
///
/// assert_eq!(types::ITEM_INFO.id(), "item_info");
/// assert!(types::ITEM_INFO.hash().is_err(), "outside the game");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///     assert_eq!(types::ITEM_INFO.key(chest)?.part, 0);
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceType(&'static str);

impl ResourceType {
    /// The type the loader's catalogue names `id`.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    /// Its enshwick id, which the loader's catalogue maps to the engine.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    #[must_use]
    pub const fn id(self) -> &'static str {
        self.0
    }

    /// Its hash on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game or when the build lacks the type.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn hash(self) -> Result<u32, Off> {
        crate::host()
            .ok_or_else(|| Off::new(format!("type {}: outside the game", self.0)))?
            .hash(self.0)
    }

    /// The key of the resource of this type with GUID `guid`, part 0, with
    /// the type's hash on the running build; for a content id, its
    /// [`Id::guid`].
    ///
    /// # Errors
    ///
    /// As [`ResourceType::hash`].
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn key(self, guid: Guid) -> Result<ResourceKey, Off> {
        Ok(guid.key_by_hash(self.hash()?))
    }
}

/// Resource types mods commonly serve.
pub mod types {
    use super::ResourceType;
    use crate::engine_ids as ids;

    /// One item.
    pub const ITEM_INFO: ResourceType = ResourceType::new(ids::ITEM_INFO);
    /// The list of every item.
    pub const ITEM_REGISTRY_RESOURCE: ResourceType = ResourceType::new(ids::ITEM_REGISTRY);
    /// Every recipe.
    pub const RECIPE_REGISTRY_RESOURCE: ResourceType = ResourceType::new(ids::RECIPE_REGISTRY);
    /// What a placed object or creature is made of.
    pub const TEMPLATE_RESOURCE: ResourceType = ResourceType::new(ids::TEMPLATE);
    /// The list of every template.
    pub const TEMPLATE_COLLECTION_RESOURCE: ResourceType =
        ResourceType::new(ids::TEMPLATE_COLLECTION);
    /// Every workshop.
    pub const WORKSHOP_REGISTRY_RESOURCE: ResourceType = ResourceType::new(ids::WORKSHOP_REGISTRY);
    /// The client's UI image, such as an item icon.
    pub const UI_TEXTURE_RESOURCE: ResourceType = ResourceType::new(ids::UI_IMAGE);
    /// The client's UI data, the crafting menu among it.
    pub const FB_UI_BUNDLE: ResourceType = ResourceType::new(ids::UI_BUNDLE);
    /// A sound container: the sounds one event plays and how it plays them.
    pub const SOUND_CONTAINER_RESOURCE: ResourceType = ResourceType::new(ids::SOUND_CONTAINER);
    /// A sound: its samples and their format, which a sound container plays.
    pub const SOUND_RESOURCE: ResourceType = ResourceType::new(ids::SOUND);
}

/// A resource blob being edited.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::Blob;
///
/// // A root with one array field at 0: two u32 elements right after it.
/// let mut base = vec![8, 0, 0, 0, 2, 0, 0, 0];
/// base.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
/// let mut blob = Blob::new(&base);
/// blob.grow_array(0, 4, 4, &[], &[(vec![3, 0, 0, 0], None)]).ok_or("not an array")?;
/// let (start, count) = blob.array(0).ok_or("not an array")?;
/// assert_eq!(count, 3);
/// assert_eq!(blob.read_u32(start + 8), Some(3));
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob(Vec<u8>);

impl Blob {
    /// A copy of `base` to edit.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    #[must_use]
    pub fn new(base: &[u8]) -> Self {
        Self(base.to_vec())
    }

    /// The bytes as edited.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// The bytes as edited.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }

    /// The `u32` at `at`.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    #[must_use]
    pub fn read_u32(&self, at: usize) -> Option<u32> {
        let bytes = self.0.get(at..at.checked_add(4)?)?;
        Some(u32::from_le_bytes(bytes.try_into().ok()?))
    }

    /// Writes `bytes` at `at`; false when they do not fit.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// let mut blob = enshwick::content::Blob::new(&[0; 8]);
    /// assert!(blob.write(4, &7u32.to_le_bytes()));
    /// assert_eq!(blob.read_u32(4), Some(7));
    /// ```
    pub fn write(&mut self, at: usize, bytes: &[u8]) -> bool {
        match at
            .checked_add(bytes.len())
            .and_then(|end| self.0.get_mut(at..end))
        {
            Some(target) => {
                target.copy_from_slice(bytes);
                true
            }
            None => false,
        }
    }

    /// The array whose header is at `field`: where its elements start and how
    /// many there are.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    #[must_use]
    pub fn array(&self, field: usize) -> Option<(usize, u32)> {
        let offset = self.read_u32(field)?.cast_signed();
        let count = self.read_u32(field + 4)?;
        let start = field.checked_add_signed(isize::try_from(offset).ok()?)?;
        Some((start, count))
    }

    /// Points the array header at `field` to `count` elements at `start`.
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    pub fn set_array(&mut self, field: usize, start: usize, count: u32) -> bool {
        let Ok(offset) = i32::try_from(start.cast_signed() - field.cast_signed()) else {
            return false;
        };
        self.write(field, &offset.to_le_bytes()) && self.write(field + 4, &count.to_le_bytes())
    }

    /// Appends `data` at the next multiple of `align` and returns where.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// let mut blob = enshwick::content::Blob::new(&[0; 3]);
    /// assert_eq!(blob.append(&[9], 4), 4);
    /// ```
    pub fn append(&mut self, data: &[u8], align: usize) -> usize {
        let start = self.0.len().next_multiple_of(align.max(1));
        self.0.resize(start, 0);
        self.0.extend_from_slice(data);
        start
    }

    /// Moves the array at `field`, of `size`-byte elements, to the end with
    /// `extra` elements after the old ones, and returns the index of the
    /// first extra one.
    ///
    /// `relative` lists the offsets, inside an element, of its own array and
    /// string headers; they are corrected as elements move. An extra element
    /// copied from the blob gives the offset it was copied from, so its
    /// headers are corrected the same way; `None` marks one whose headers are
    /// empty or set later with [`Blob::set_array`].
    ///
    /// # Examples
    ///
    /// See [`Blob`].
    pub fn grow_array(
        &mut self,
        field: usize,
        size: usize,
        align: usize,
        relative: &[usize],
        extra: &[(Vec<u8>, Option<usize>)],
    ) -> Option<u32> {
        let (start, count) = self.array(field)?;
        let old = usize::try_from(count).ok()?;
        let moved = self
            .0
            .get(start..start.checked_add(old.checked_mul(size)?)?)?
            .to_vec();
        let mut elements: Vec<(Vec<u8>, Option<usize>)> = moved
            .chunks_exact(size)
            .enumerate()
            .map(|(i, chunk)| (chunk.to_vec(), Some(start + i * size)))
            .collect();
        if extra.iter().any(|(bytes, _)| bytes.len() != size) {
            return None;
        }
        elements.extend(extra.iter().cloned());
        let new_start = self.0.len().next_multiple_of(align.max(1));
        self.0.resize(new_start, 0);
        for (index, (mut bytes, from)) in elements.into_iter().enumerate() {
            let to = new_start + index * size;
            if let Some(from) = from {
                let delta = from.cast_signed() - to.cast_signed();
                for &at in relative {
                    let header = bytes.get(at..at + 4)?;
                    let offset = i32::from_le_bytes(header.try_into().ok()?);
                    if offset != 0 {
                        let fixed = i32::try_from(isize::try_from(offset).ok()? + delta).ok()?;
                        bytes
                            .get_mut(at..at + 4)?
                            .copy_from_slice(&fixed.to_le_bytes());
                    }
                }
            }
            self.0.extend_from_slice(&bytes);
        }
        let total = count.checked_add(u32::try_from(extra.len()).ok()?)?;
        self.set_array(field, new_start, total).then_some(count)
    }
}

/// Builds the resource `key` again from its base with the patches of every mod
/// still on, and writes the bytes that differ over the resource the game holds.
///
/// The mod serves `key` and asked to rebuild it with [`Load::allow_rebuild`];
/// any thread may call this, for values a patch reads that changed after the
/// first build, such as a server's join values.
///
/// `key` itself is left while the game has not built it yet, as its first
/// build takes the values in force. Every key whose build read `key` with
/// [`Host::served_resource`](crate::Host::served_resource) is built again too.
///
/// The game keeps reading the first build's bytes, so a rebuild must come
/// out the same length: a patch that only changes values qualifies, one
/// that adds entries does not. A copy of the data the game made from the
/// resource is not updated.
///
/// # Errors
///
/// [`Off`] for a key the mod does not serve, or whose base the loader did
/// not keep: the mod did not allow to rebuild it and no build read a served
/// key; and when the new build differs in length.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// use enshwick::content::{Guid, rebuild, types};
///
/// const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///
/// fn settings_changed() {
///     if let Err(off) = types::ITEM_INFO.key(TABLE).and_then(rebuild) {
///         enshwick::warn!("the change applies at the next start: {off}");
///     }
/// }
/// ```
pub fn rebuild(key: ResourceKey) -> Result<(), Off> {
    let host = crate::host().ok_or_else(|| Off::new("rebuild: no loader"))?;
    let api = host.api();
    let status = (api.rebuild_resource)(host.handle(), &raw const key);
    crate::load::ok(status, (), || "rebuild resource".to_owned())
}

/// `id`, another mod's content id, once the loader lets this mod name it:
/// its own, or one its mod exports. Outside the game every id passes.
pub(crate) fn named(id: Id) -> Result<Id, Off> {
    let Some(host) = crate::host() else {
        return Ok(id);
    };
    let name = id.to_string();
    let status = (host.api().use_content)(host.handle(), Str::new(&name));
    crate::load::ok(status, id, || format!("content id {id}"))
}

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
    /// # extern crate enshwick_core as enshwick;
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
    pub fn declare(&mut self, id: Id) -> Result<Guid, Off> {
        crate::resources::declare(self.host(), &id.to_string())
    }

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
    /// # extern crate enshwick_core as enshwick;
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
    ) -> Result<(), Off> {
        crate::i18n::add_text(self.host(), tag, language, text)
    }

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
    /// # extern crate enshwick_core as enshwick;
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
    ) -> Result<(), Off> {
        crate::resources::serve(self.host(), key, base, builder)
    }
}

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
    /// # extern crate enshwick_core as enshwick;
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
    pub fn allow_rebuild(&mut self, key: ResourceKey) -> Result<(), Off> {
        let api = self.host().api();
        let status = (api.rebuild_resource)(self.host().handle(), &raw const key);
        crate::load::ok(status, (), || "allow rebuild".to_owned())
    }
}

#[cfg(test)]
mod tests;
