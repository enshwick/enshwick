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

pub mod changes;
pub mod containers;
pub mod icons;
mod ids;
pub mod items;
pub mod menu;
pub mod recipes;
pub mod vanilla;
pub mod workshops;

pub use ids::*;

impl crate::Host {
    /// A vanilla resource's bytes from the game's archive; see
    /// [`game_resource`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, types};
    ///
    /// if let Some(host) = enshwick::host() {
    ///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///     if let Ok(key) = chest.key(types::ITEM_INFO) {
    ///         let _ = host.game_resource(key);
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn game_resource(&self, key: ResourceKey) -> Option<Vec<u8>> {
        let api = self.api();
        let mut buffer = vec![0u8; 64 << 10];
        loop {
            let mut len = 0u32;
            let capacity = u32::try_from(buffer.len()).ok()?;
            let status = (api.resource_read)(
                self.handle(),
                &raw const key,
                buffer.as_mut_ptr(),
                capacity,
                &raw mut len,
            );
            let len = usize::try_from(len).ok()?;
            if status.is_ok() {
                buffer.truncate(len);
                return Some(buffer);
            }
            if status != Status::BAD_ARGS || len <= buffer.len() {
                return None;
            }
            buffer.resize(len, 0);
        }
    }
}

/// A type of game resource by enshwick's name for it, such as
/// [`types::ITEM_INFO`]; the loader maps it to the running build's type,
/// whose hash resource keys carry.
///
/// # Examples
///
/// ```
/// use enshwick::content::types;
///
/// assert_eq!(types::ITEM_INFO.id(), "item_info");
/// assert!(types::ITEM_INFO.hash().is_err(), "outside the game");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceType(&'static str);

impl ResourceType {
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
}

/// Resource types mods commonly serve.
pub mod types {
    use super::ResourceType;
    use crate::engine_ids as ids;

    /// One item.
    pub const ITEM_INFO: ResourceType = ResourceType(ids::ITEM_INFO);
    /// The list of every item.
    pub const ITEM_REGISTRY_RESOURCE: ResourceType = ResourceType(ids::ITEM_REGISTRY);
    /// Every recipe.
    pub const RECIPE_REGISTRY_RESOURCE: ResourceType = ResourceType(ids::RECIPE_REGISTRY);
    /// What a placed object or creature is made of.
    pub const TEMPLATE_RESOURCE: ResourceType = ResourceType(ids::TEMPLATE);
    /// The list of every template.
    pub const TEMPLATE_COLLECTION_RESOURCE: ResourceType = ResourceType(ids::TEMPLATE_COLLECTION);
    /// Every workshop.
    pub const WORKSHOP_REGISTRY_RESOURCE: ResourceType = ResourceType(ids::WORKSHOP_REGISTRY);
    /// The client's UI image, such as an item icon.
    pub const UI_TEXTURE_RESOURCE: ResourceType = ResourceType(ids::UI_IMAGE);
    /// The client's UI data, the crafting menu among it.
    pub const FB_UI_BUNDLE: ResourceType = ResourceType(ids::UI_BUNDLE);
}

/// Answers engine facts by enshwick id: the running game through [`Load`]
/// and [`crate::Host`], a tool through the game's reflection; the content
/// layouts resolve through it.
///
/// # Examples
///
/// ```
/// use enshwick::content::EngineFacts;
/// use enshwick::content::items::ItemLayout;
///
/// fn layout(facts: &impl EngineFacts) -> Result<ItemLayout, enshwick::Off> {
///     ItemLayout::resolve(facts)
/// }
/// ```
pub trait EngineFacts {
    /// The byte offset the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn offset(&self, id: &str) -> Result<usize, Off>;

    /// The size of the type the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn size(&self, id: &str) -> Result<usize, Off>;

    /// The hash of the type the id names.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks it.
    fn hash(&self, id: &str) -> Result<u32, Off>;
}

impl EngineFacts for crate::Host {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        let mut out = 0u64;
        let status = (self.api().constant)(self.handle(), Str::new(id), &raw mut out);
        let offset = crate::load::ok(status, out, || format!("offset {id}"))?;
        usize::try_from(offset).map_err(|_| Off::new(format!("offset {id} is out of range")))
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        let mut out = 0u32;
        let status = (self.api().type_size)(Str::new(id), &raw mut out);
        let size = crate::load::ok(status, out, || format!("type {id}"))?;
        usize::try_from(size).map_err(|_| Off::new(format!("type {id} is out of range")))
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        let mut out = 0u32;
        let status = (self.api().type_hash)(Str::new(id), &raw mut out);
        crate::load::ok(status, out, || format!("type {id}"))
    }
}

impl EngineFacts for Load<'_> {
    fn offset(&self, id: &str) -> Result<usize, Off> {
        self.host().offset(id)
    }

    fn size(&self, id: &str) -> Result<usize, Off> {
        self.host().size(id)
    }

    fn hash(&self, id: &str) -> Result<u32, Off> {
        self.host().hash(id)
    }
}

/// A resource blob being edited.
///
/// # Examples
///
/// ```
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

/// A served resource's builder, kept for the life of the process.
type Builder = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// Builds a served resource from its base for the loader.
///
/// # Safety
///
/// - `base` is null or readable for `base_len` bytes that stay unchanged for
///   the call.
/// - `user` is null or the `Box<Builder>` that [`Load::serve`] handed over,
///   which the loader keeps for the process.
/// - `out` and `out_len` are null or writable for one value each.
unsafe extern "system" fn build(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    if user.is_null() || out.is_null() || out_len.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `user` is not null, as the early return ensures, and the caller
    // guarantees it is the boxed `Builder` that `serve` leaked for the process,
    // so it is aligned, alive and only read through shared references.
    let builder = unsafe { &*user.cast::<Builder>() };
    let base = if base.is_null() || base_len == 0 {
        &[][..]
    } else {
        // SAFETY: `base` is not null in this branch, and the caller guarantees
        // `base_len` readable bytes there, unchanged for the call.
        unsafe { core::slice::from_raw_parts(base, base_len) }
    };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder(base)));
    let Ok(Some(bytes)) = built else {
        return Status::REFUSED;
    };
    // The game reads the resource for the life of the process, and a
    // rebuild writes over it: the bytes are handed over, never read here
    // again.
    let len = bytes.len();
    let bytes: *mut [u8] = Box::into_raw(bytes.into_boxed_slice());
    // SAFETY: `out` is not null, as the early return ensures, and the caller
    // guarantees it is writable for one pointer.
    unsafe {
        out.write(bytes.cast::<u8>().cast_const());
    }
    // SAFETY: `out_len` is not null, as the early return ensures, and the
    // caller guarantees it is writable for one `usize`.
    unsafe {
        out_len.write(len);
    }
    Status::OK
}

/// Builds the resource `key` again from its base with the patches of every mod
/// still on, and writes the bytes that differ over the resource the game holds.
///
/// The mod serves `key` and asked to rebuild it with [`Load::allow_rebuild`];
/// any thread may call this, for values a patch reads that changed after the
/// first build, such as a server's join values.
///
/// Nothing is done while the game has not built `key` yet, as its first build
/// takes the values in force. Each rebuild keeps the patches' new bytes for the
/// process, so it is for rare changes.
///
/// The game keeps reading the first build's bytes, so a rebuild must come
/// out the same length: a patch that only changes values qualifies, one
/// that adds entries does not. A copy of the data the game made from the
/// resource is not updated.
///
/// # Errors
///
/// [`Off`] for a key the mod does not serve or did not allow to rebuild,
/// and when the new build differs in length.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, rebuild, types};
///
/// const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///
/// fn settings_changed() {
///     if let Err(off) = TABLE.key(types::ITEM_INFO).and_then(rebuild) {
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
    /// use enshwick::content::{Guid, Id};
    ///
    /// const LOCKER: Id = Id::new("my-mod:locker");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let guid = load.declare(LOCKER)?;
    ///     assert_eq!(guid, Guid::named("my-mod:locker"));
    ///     Ok(())
    /// }
    /// ```
    pub fn declare(&mut self, id: Id) -> Result<Guid, Off> {
        let api = self.host().api();
        let mut out = [0u8; 16];
        let status = (api.declare_content)(self.host().handle(), Str::new(id.name()), &raw mut out);
        crate::load::ok(status, Guid(out), || format!("content id {id}"))
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
    pub fn add_text(
        &mut self,
        tag: Guid,
        language: crate::i18n::Language,
        text: &str,
    ) -> Result<(), Off> {
        let api = self.host().api();
        let status = (api.add_text)(self.host().handle(), tag.id(), language.0, Str::new(text));
        crate::load::ok(status, (), || "add text".to_owned())
    }

    /// Serves the resource `key`, built by `builder` from the resource
    /// `base` the first time the game reads it: a new resource when the keys
    /// differ, a changed vanilla one when they are equal. `None` from the
    /// builder turns the mod off and the resource reads as missing.
    ///
    /// # Errors
    ///
    /// [`Off`] when another mod serves `key`.
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
    ///     load.serve(LOCKER.key(types::ITEM_INFO)?, CHEST.key(types::ITEM_INFO)?, |chest| {
    ///         let mut item = Blob::new(chest);
    ///         item.write(0, &LOCKER.id().to_le_bytes());
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
        let api = self.host().api();
        let builder: Box<Builder> = Box::new(Box::new(builder));
        let user = Box::into_raw(builder);
        let patch: ResourcePatchFn = build;
        let status = (api.serve_resource)(
            self.host().handle(),
            &raw const key,
            &raw const base,
            patch,
            user.cast(),
        );
        if !status.is_ok() {
            // SAFETY: `user` came from this function's `Box::into_raw`, and the loader
            // refused it, so it keeps no copy and this is the only owner.
            drop(unsafe { Box::from_raw(user) });
        }
        crate::load::ok(status, (), || "serve resource".to_owned())
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
    /// use enshwick::content::{Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let key = CHEST.key(types::ITEM_INFO)?;
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
