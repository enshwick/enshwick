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

pub mod icons;
pub mod items;
pub mod recipes;
pub mod vanilla;
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

    /// [`Guid::parse`] for text read at run time; `None` when malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    ///
    /// let text = "6943053a-32c1-4718-ae22-e61427ddf54d";
    /// assert_eq!(Guid::try_parse(text), Some(Guid::parse(text)));
    /// assert_eq!(Guid::try_parse("6943053a"), None);
    /// ```
    #[must_use]
    pub fn try_parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let shaped = bytes.len() == 36
            && bytes.iter().enumerate().all(|(i, &b)| {
                if matches!(i, 8 | 13 | 18 | 23) {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit()
                }
            });
        shaped.then(|| Self::parse(text))
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

    /// The GUID of the content id `id` (`<mod>:<name>`, see [`is_id`]):
    /// a name-based UUID, version 5, of `id`'s UTF-8 bytes in
    /// [`NAMESPACE`], the same on every machine and in every release. A
    /// malformed literal fails the build when used in a constant.
    ///
    /// # Panics
    ///
    /// On a malformed id.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    ///
    /// const SAFE: Guid = Guid::named("safes:safe");
    /// assert_eq!(SAFE.to_string(), "f7eb124c-c132-5cf1-9c6c-e9e49ed3c896");
    /// ```
    #[must_use]
    pub const fn named(id: &str) -> Self {
        assert!(is_id(id), "a content id is <mod>:<name>");
        Self::hashed(id)
    }

    /// [`Guid::named`] for an id read at run time; `None` when malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Guid;
    ///
    /// assert_eq!(Guid::try_named("safes:safe"), Some(Guid::named("safes:safe")));
    /// assert_eq!(Guid::try_named("Safe"), None);
    /// ```
    #[must_use]
    pub const fn try_named(id: &str) -> Option<Self> {
        if is_id(id) {
            Some(Self::hashed(id))
        } else {
            None
        }
    }

    /// The version 5 UUID of `name` in [`NAMESPACE`], in memory order.
    pub(crate) const fn hashed(name: &str) -> Self {
        let input = const_sha1::ConstSlice::new()
            .push_slice(&NAMESPACE_BYTES)
            .push_slice(name.as_bytes());
        let hash = const_sha1::sha1_from_const_slice(&input).as_bytes();
        let mut raw = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            raw[i] = hash[i];
            i += 1;
        }
        raw[6] = (raw[6] & 0x0f) | 0x50;
        raw[8] = (raw[8] & 0x3f) | 0x80;
        Self::from_big_endian(raw)
    }

    /// The GUID whose textual form is `raw` in order.
    const fn from_big_endian(raw: [u8; 16]) -> Self {
        Self([
            raw[3], raw[2], raw[1], raw[0], raw[5], raw[4], raw[7], raw[6], raw[8], raw[9],
            raw[10], raw[11], raw[12], raw[13], raw[14], raw[15],
        ])
    }
}

/// The namespace of every content id's UUID, fixed forever.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::NAMESPACE;
///
/// assert_eq!(NAMESPACE.to_string(), "e2185889-0e5e-4977-9edd-d6e7bcd1393f");
/// ```
pub const NAMESPACE: Guid = Guid::parse("e2185889-0e5e-4977-9edd-d6e7bcd1393f");

/// [`NAMESPACE`] in the order UUID version 5 hashes it.
const NAMESPACE_BYTES: [u8; 16] = [
    0xe2, 0x18, 0x58, 0x89, 0x0e, 0x5e, 0x49, 0x77, 0x9e, 0xdd, 0xd6, 0xe7, 0xbc, 0xd1, 0x39, 0x3f,
];

/// Whether `id` is a content id: `<mod>:<name>`, the mod side 1 to 64 of
/// `[a-z0-9_-]`, the name 1 to 128 of `[a-z0-9_./-]`.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::is_id;
///
/// assert!(is_id("safes:safe"));
/// assert!(is_id("survivors-per-altar:staff/blacksmith.name"));
/// assert!(!is_id("safe"));
/// assert!(!is_id("Safes:safe"));
/// assert!(!is_id("safes:safe:2"));
/// ```
#[must_use]
pub const fn is_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    let mut colon = None;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b':' {
            if colon.is_some() {
                return false;
            }
            colon = Some(i);
        } else {
            let common = b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-';
            let in_name = colon.is_some() && (b == b'.' || b == b'/');
            if !common && !in_name {
                return false;
            }
        }
        i += 1;
    }
    match colon {
        Some(at) => {
            let name = bytes.len() - at - 1;
            at >= 1 && at <= 64 && name >= 1 && name <= 128
        }
        None => false,
    }
}

/// A piece of content a mod defines: its id, `<mod>:<name>`, and the GUID
/// [`Guid::named`] derives from it.
///
/// Content calls such as [`Load::add_item`] take an `Id` for what they
/// create and declare it to the loader, which turns the mod off when its id
/// lies outside the mod's namespace or its game id ([`Guid::id`]) is
/// another content id's; the log names both. References to existing
/// content, vanilla or another mod's, stay [`Guid`]s.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::{Guid, Id};
///
/// const SAFE: Id = Id::new("safes:safe");
/// assert_eq!(SAFE.name(), "safes:safe");
/// assert_eq!(SAFE.guid(), Guid::named("safes:safe"));
/// assert_eq!(Guid::from(SAFE).id(), SAFE.id());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id {
    name: &'static str,
    guid: Guid,
}

impl Id {
    /// The content `name`, `<mod>:<name>`; a malformed literal fails the
    /// build when used in a constant.
    ///
    /// # Panics
    ///
    /// On a malformed id, see [`is_id`].
    ///
    /// # Examples
    ///
    /// See [`Id`].
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            guid: Guid::named(name),
        }
    }

    /// [`Id::new`] for an id read at run time; `None` when malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::Id;
    ///
    /// assert!(Id::try_new("pack:lantern").is_some());
    /// assert!(Id::try_new("lantern").is_none());
    /// ```
    #[must_use]
    pub const fn try_new(name: &'static str) -> Option<Self> {
        match Guid::try_named(name) {
            Some(guid) => Some(Self { name, guid }),
            None => None,
        }
    }

    /// The id, `<mod>:<name>`.
    ///
    /// # Examples
    ///
    /// See [`Id`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The mod side of the id.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick_loader_api::content::Id::new("safes:safe").namespace(), "safes");
    /// ```
    #[must_use]
    pub fn namespace(self) -> &'static str {
        self.name
            .split_once(':')
            .map_or("", |(namespace, _)| namespace)
    }

    /// Its GUID.
    ///
    /// # Examples
    ///
    /// See [`Id`].
    #[must_use]
    pub const fn guid(self) -> Guid {
        self.guid
    }

    /// Its game id, [`Guid::id`] of its GUID.
    ///
    /// # Examples
    ///
    /// See [`Id`].
    #[must_use]
    pub const fn id(self) -> u32 {
        self.guid.id()
    }

    /// [`Guid::key`] of its GUID.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::{Id, types};
    ///
    /// let key = Id::new("safes:safe").key(types::ITEM_INFO);
    /// assert_eq!(key.guid, Id::new("safes:safe").guid().0);
    /// ```
    #[must_use]
    pub const fn key(self, type_hash: u32) -> ResourceKey {
        self.guid.key(type_hash)
    }
}

impl From<Id> for Guid {
    fn from(id: Id) -> Self {
        id.guid
    }
}

impl core::fmt::Display for Id {
    /// The id, `<mod>:<name>`.
    ///
    /// # Examples
    ///
    /// ```
    /// assert_eq!(enshwick_loader_api::content::Id::new("a:b").to_string(), "a:b");
    /// ```
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name)
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

/// A vanilla resource's bytes from the game's own archive, as the game
/// reads them before it resolves their references: game data a mod builds
/// on, such as an item's name tag or the template a station item places,
/// read the same at init and later, on the server and the client.
///
/// `None` for a key the archive lacks. Each call reads the archive's files; keep what a mod reads
/// more than once.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::content::{game_resource, types};
/// use enshwick_loader_api::content::workshops::stations;
///
/// if let Some(item) = game_resource(stations::WORKBENCH.item.key(types::ITEM_INFO)) {
///     enshwick_loader_api::info!("the Workbench item is {} bytes", item.len());
/// }
/// ```
#[must_use]
pub fn game_resource(key: ResourceKey) -> Option<Vec<u8>> {
    crate::host()?.game_resource(key)
}

impl crate::Host {
    /// A vanilla resource's bytes from the game's archive; see
    /// [`game_resource`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader_api::content::{Guid, types};
    ///
    /// if let Some(host) = enshwick_loader_api::host() {
    ///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///     let _ = host.game_resource(chest.key(types::ITEM_INFO));
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
    /// The client's UI image, such as an item icon.
    pub const UI_IMAGE: u32 = super::icons::UI_IMAGE;
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
    if user.is_null() || out.is_null() || out_len.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `user` is the leaked builder this function was registered with.
    let builder = unsafe { &*user.cast::<Builder>() };
    let base = if base.is_null() || base_len == 0 {
        &[][..]
    } else {
        // SAFETY: the loader passes the base's bytes for the call.
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
    // SAFETY: the loader's out pointers.
    unsafe {
        out.write(bytes.cast::<u8>().cast_const());
    }
    // SAFETY: as above.
    unsafe {
        out_len.write(len);
    }
    Status::OK
}

/// Builds the resource `key`, which this mod serves and asked to rebuild
/// with [`Load::allow_rebuild`], again from its base with the patches of
/// every mod still on, from any thread, and writes the bytes that differ
/// over the resource the game holds: for values a patch reads that changed
/// after the first build, such as a server's join values. Nothing is done
/// while the game has not built `key` yet, as its first build takes the
/// values in force. Each rebuild keeps the patches' new bytes for the
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
/// use enshwick_loader_api::content::{Guid, rebuild, types};
///
/// const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///
/// fn settings_changed() {
///     if let Err(off) = rebuild(TABLE.key(types::ITEM_INFO)) {
///         enshwick_loader_api::warn!("the change applies at the next start: {off}");
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
    /// use enshwick_loader_api::content::{Guid, Id};
    ///
    /// const LANTERN: Id = Id::new("my-mod:lantern");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let guid = load.declare(LANTERN)?;
    ///     assert_eq!(guid, Guid::named("my-mod:lantern"));
    ///     Ok(())
    /// }
    /// ```
    pub fn declare(&mut self, id: Id) -> Result<Guid, Off> {
        let api = self.host().api();
        let mut out = [0u8; 16];
        let status = (api.declare_content)(
            self.host().handle(),
            crate::abi::Str::new(id.name()),
            &raw mut out,
        );
        crate::load::ok(status, Guid(out), || format!("content id {id}"))
    }

    /// Gives `LocaTag` `tag` the text `text` in `language` (a
    /// [`Language`](crate::i18n::Language) number); the client shows it
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
    /// [`Off`] when another mod serves `key`.
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
    /// use enshwick_loader_api::content::{Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///
    /// fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    ///     let key = CHEST.key(types::ITEM_INFO);
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
mod tests {
    use super::*;

    #[test]
    fn a_builder_reads_a_null_base_as_empty() {
        let builder: Builder = Box::new(|base: &[u8]| Some(vec![u8::from(base.is_empty())]));
        let user = core::ptr::from_ref(&builder).cast_mut().cast();
        let (mut out, mut out_len) = (core::ptr::null::<u8>(), 0usize);
        // SAFETY: a null base, the builder as user and writable out pointers.
        let status = unsafe { build(core::ptr::null(), 0, user, &raw mut out, &raw mut out_len) };
        assert_eq!(status, Status::OK);
        // SAFETY: the leaked bytes the builder made.
        assert_eq!(unsafe { core::slice::from_raw_parts(out, out_len) }, [1]);
        // SAFETY: null out pointers are refused before anything is written.
        let refused = unsafe {
            build(
                core::ptr::null(),
                0,
                user,
                core::ptr::null_mut(),
                &raw mut out_len,
            )
        };
        assert_eq!(refused, Status::BAD_ARGS);
    }

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
