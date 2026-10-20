//! Game content from mods: new resources such as items and recipes, and
//! changes to vanilla ones, served to the game as if they came from its
//! archive.
//!
//! Resources are blobs: plain data whose arrays and strings are offsets from
//! the field that holds them. [`Blob`] edits them safely: it grows arrays by
//! moving them to the end and keeps every offset right.

use core::ffi::c_void;

use crate::abi::{ResourceKey, ResourcePatchFn, Status};
use crate::{Load, Off};

pub mod items;
pub mod recipes;
pub mod workshops;

/// A GUID in memory order: the `bytes_le` of its textual form.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(CHEST.id(), 1_302_084_846);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Guid(pub [u8; 16]);

impl Guid {
    /// Parses `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`; a malformed literal
    /// fails the build when used in a constant.
    ///
    /// # Panics
    ///
    /// On a malformed GUID.
    ///
    /// # Examples
    ///
    /// See [`Guid`].
    #[must_use]
    pub const fn parse(text: &str) -> Self {
        let text = text.as_bytes();
        assert!(text.len() == 36, "a GUID has 36 characters");
        let mut raw = [0u8; 16];
        let (mut i, mut out) = (0, 0);
        while i < 36 {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                assert!(text[i] == b'-', "a GUID has dashes at 8, 13, 18 and 23");
                i += 1;
                continue;
            }
            raw[out] = (digit(text[i]) << 4) | digit(text[i + 1]);
            out += 1;
            i += 2;
        }
        // The first three groups are little-endian in memory.
        let bytes = [
            raw[3], raw[2], raw[1], raw[0], raw[5], raw[4], raw[7], raw[6], raw[8], raw[9],
            raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ];
        assert!(out == 16, "a GUID has 32 hex digits");
        Self(bytes)
    }

    /// The game's 32-bit id for this GUID: FNV-1a over its memory bytes, as
    /// items and recipes are numbered.
    ///
    /// # Examples
    ///
    /// See [`Guid`].
    #[must_use]
    pub const fn id(self) -> u32 {
        let mut hash: u32 = 0x811c_9dc5;
        let mut i = 0;
        while i < 16 {
            hash ^= self.0[i] as u32;
            hash = hash.wrapping_mul(0x0100_0193);
            i += 1;
        }
        hash
    }

    /// The key of the resource of `type_hash` with this GUID, part 0.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::{Guid, types};
    ///
    /// let key = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d").key(types::ITEM_INFO);
    /// assert_eq!(key.part, 0);
    /// ```
    #[must_use]
    pub const fn key(self, type_hash: u32) -> ResourceKey {
        ResourceKey {
            guid: self.0,
            type_hash,
            part: 0,
        }
    }
}

impl core::fmt::Display for Guid {
    /// The textual form [`Guid::parse`] reads.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    ///
    /// let text = "6943053a-32c1-4718-ae22-e61427ddf54d";
    /// assert_eq!(Guid::parse(text).to_string(), text);
    /// ```
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let b = &self.0;
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-",
            b[3], b[2], b[1], b[0], b[5], b[4], b[7], b[6], b[8], b[9]
        )?;
        b[10..].iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

#[expect(
    clippy::panic,
    reason = "const evaluation: a malformed literal fails the build"
)]
const fn digit(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("a GUID is hex"),
    }
}

/// Type hashes of resources mods commonly serve.
pub mod types {
    /// `ItemInfo`, one item.
    pub const ITEM_INFO: u32 = 0xb5ce_8765;
    /// `ItemRegistryResource`, the list of every item.
    pub const ITEM_REGISTRY: u32 = 0x72ab_39b6;
    /// `RecipeRegistryResource`, every recipe.
    pub const RECIPE_REGISTRY: u32 = 0x5f53_82cf;
    /// `ecs::TemplateResource`, what a placed object or creature is made of.
    pub const TEMPLATE: u32 = 0x3976_8775;
    /// `ecs::TemplateCollectionResource`, the list of every template.
    pub const TEMPLATE_COLLECTION: u32 = 0xd80f_e371;
    /// `WorkshopRegistryResource`, every workshop.
    pub const WORKSHOP_REGISTRY: u32 = 0xa8ea_5924;
}

/// A resource blob being edited.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::Blob;
///
/// // A root with one array field at 0: two u32 elements right after it.
/// let mut base = vec![8, 0, 0, 0, 2, 0, 0, 0];
/// base.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
/// let mut blob = Blob::new(&base);
/// blob.grow_array(0, 4, 4, &[], &[(vec![3, 0, 0, 0], None)]).unwrap();
/// let (start, count) = blob.array(0).unwrap();
/// assert_eq!(count, 3);
/// assert_eq!(blob.read_u32(start + 8), Some(3));
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
    /// let mut blob = enshwick_loader_api::content::Blob::new(&[0; 8]);
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
    /// let mut blob = enshwick_loader_api::content::Blob::new(&[0; 3]);
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

unsafe extern "system" fn build(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    // SAFETY: `user` is the leaked builder this function was registered with.
    let builder = unsafe { &*user.cast::<Builder>() };
    // SAFETY: the loader passes the base's bytes for the call.
    let base = unsafe { core::slice::from_raw_parts(base, base_len) };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder(base)));
    let Ok(Some(bytes)) = built else {
        return Status::REFUSED;
    };
    // The game reads the resource for the life of the process.
    let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    // SAFETY: the loader's out pointers.
    unsafe {
        out.write(bytes.as_ptr());
    }
    // SAFETY: as above.
    unsafe {
        out_len.write(bytes.len());
    }
    Status::OK
}

impl Load<'_> {
    /// Gives `LocaTag` `tag` the text `text` in `language` (a
    /// [`Language`](crate::i18n::Language) number); the client shows it
    /// wherever the game asks for the tag, such as the name of an item whose
    /// `name` field holds `tag`. [`Load::add_texts`] gives many at once.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    /// use enshwick_loader_api::i18n::Language;
    ///
    /// const SAFE_NAME: Guid = Guid::parse("b7c1d2e3-4f50-4a61-8b72-c3d4e5f60718");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.add_text(SAFE_NAME, Language::FALLBACK.0, "Safe")?;
    ///     load.add_text(SAFE_NAME, Language::UK_UA.0, "Сейф")
    /// }
    /// ```
    pub fn add_text(&mut self, tag: Guid, language: u32, text: &str) -> Result<(), Off> {
        let api = self.host().api();
        let end = core::mem::offset_of!(crate::abi::HostApi, add_text) + size_of::<usize>();
        if !api.header.covers(end) {
            return Err(Off::new("mod texts need a newer loader"));
        }
        let status = (api.add_text)(
            self.host().handle(),
            tag.id(),
            language,
            crate::abi::Str::new(text),
        );
        crate::load::ok(status, (), || "add text".to_owned())
    }

    /// Serves the resource `key`, built by `builder` from the resource
    /// `base` the first time the game reads it: a new resource when the keys
    /// differ, a changed vanilla one when they are equal. `None` from the
    /// builder turns the mod off and the resource reads as missing.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader is too old or another mod serves `key`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::{Blob, Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    /// const SAFE: Guid = Guid::parse("5afe5afe-0000-4000-8000-000000000001");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     load.serve(SAFE.key(types::ITEM_INFO), CHEST.key(types::ITEM_INFO), |chest| {
    ///         let mut item = Blob::new(chest);
    ///         item.write(0, &SAFE.id().to_le_bytes());
    ///         item.write(4, &SAFE.0);
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
        let end = core::mem::offset_of!(crate::abi::HostApi, serve_resource) + size_of::<usize>();
        if !api.header.covers(end) {
            return Err(Off::new("served resources need a newer loader"));
        }
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
            // SAFETY: refused, so the loader keeps no copy of the pointer.
            drop(unsafe { Box::from_raw(user) });
        }
        crate::load::ok(status, (), || "serve resource".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guid_parses_to_memory_order_and_its_game_id() {
        let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
        assert_eq!(&chest.0[..4], &[0x3a, 0x05, 0x43, 0x69]);
        assert_eq!(chest.id(), 1_302_084_846);
    }

    #[test]
    fn moved_elements_keep_pointing_at_their_data() {
        // Root: array at 0 of one 8-byte element at 8; the element is a string
        // header pointing at "ab" at 16.
        let mut base = vec![0u8; 18];
        base[0..4].copy_from_slice(&8u32.to_le_bytes());
        base[4..8].copy_from_slice(&1u32.to_le_bytes());
        base[8..12].copy_from_slice(&8u32.to_le_bytes());
        base[12..16].copy_from_slice(&2u32.to_le_bytes());
        base[16..18].copy_from_slice(b"ab");
        let mut blob = Blob::new(&base);
        let copy = blob.bytes()[8..16].to_vec();
        assert_eq!(blob.grow_array(0, 8, 4, &[0], &[(copy, Some(8))]), Some(1));
        let (start, count) = blob.array(0).unwrap();
        assert_eq!(count, 2);
        for element in [start, start + 8] {
            let (text, len) = blob.array(element).unwrap();
            assert_eq!(&blob.bytes()[text..text + len as usize], b"ab");
        }
    }
}
