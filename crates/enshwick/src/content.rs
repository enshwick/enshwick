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
pub mod items;
pub mod loot_tables;
pub mod menu;
pub mod recipes;
pub mod vanilla;
pub mod workshops;

/// A GUID in memory order: the `bytes_le` of its textual form.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(CHEST.id(), 1_302_084_846);
/// ```
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Shape)]
#[shape(api = "guid")]
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
    /// use enshwick::content::Guid;
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

    /// The key of the resource of type `ty` with this GUID, part 0, with
    /// the type's hash on the running build.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game or when the build lacks the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, types};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let chest = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///     assert_eq!(chest.key(types::ITEM_INFO)?.part, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn key(self, ty: ResourceType) -> Result<ResourceKey, Off> {
        Ok(self.key_by_hash(ty.hash()?))
    }

    /// The key of the resource whose type has the hash `type_hash` with
    /// this GUID, part 0: for a type [`types`] does not name, and for tools
    /// that know the hash.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    ///
    /// let key = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d").key_by_hash(7);
    /// assert_eq!((key.type_hash, key.part), (7, 0));
    /// ```
    #[must_use]
    pub const fn key_by_hash(self, type_hash: u32) -> ResourceKey {
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
    /// use enshwick::content::Guid;
    ///
    /// const LOCKER: Guid = Guid::named("you.lockers:locker");
    /// assert_eq!(LOCKER.to_string(), "67842cb8-d551-542b-8765-fe1af9c28aca");
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
    /// use enshwick::content::Guid;
    ///
    /// assert_eq!(Guid::try_named("you.lockers:locker"), Some(Guid::named("you.lockers:locker")));
    /// assert_eq!(Guid::try_named("Locker"), None);
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
/// use enshwick::content::NAMESPACE;
///
/// assert_eq!(NAMESPACE.to_string(), "e2185889-0e5e-4977-9edd-d6e7bcd1393f");
/// ```
pub const NAMESPACE: Guid = Guid::parse("e2185889-0e5e-4977-9edd-d6e7bcd1393f");

/// [`NAMESPACE`] in the order UUID version 5 hashes it.
const NAMESPACE_BYTES: [u8; 16] = [
    0xe2, 0x18, 0x58, 0x89, 0x0e, 0x5e, 0x49, 0x77, 0x9e, 0xdd, 0xd6, 0xe7, 0xbc, 0xd1, 0x39, 0x3f,
];

/// Whether `id` is a content id: `<mod>:<name>`, the mod side 1 to 64 of
/// `[a-z0-9_.-]`, the name 1 to 128 of `[a-z0-9_./-]`.
///
/// # Examples
///
/// ```
/// use enshwick::content::is_id;
///
/// assert!(is_id("you.lockers:locker"));
/// assert!(is_id("you.market-stalls:stall/blacksmith.name"));
/// assert!(!is_id("locker"));
/// assert!(!is_id("Lockers:locker"));
/// assert!(!is_id("you.lockers:locker:2"));
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
            let in_name = colon.is_some() && b == b'/';
            if !common && !in_name && b != b'.' {
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
/// use enshwick::content::{Guid, Id};
///
/// const LOCKER: Id = Id::new("you.lockers:locker");
/// assert_eq!(LOCKER.name(), "you.lockers:locker");
/// assert_eq!(LOCKER.guid(), Guid::named("you.lockers:locker"));
/// assert_eq!(Guid::from(LOCKER).id(), LOCKER.id());
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
    /// use enshwick::content::Id;
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
    /// assert_eq!(enshwick::content::Id::new("you.lockers:locker").namespace(), "you.lockers");
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
    /// # Errors
    ///
    /// As [`Guid::key`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Id, types};
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let key = Id::new("you.lockers:locker").key(types::ITEM_INFO)?;
    ///     assert_eq!(key.guid, Id::new("you.lockers:locker").guid().0);
    ///     Ok(())
    /// }
    /// ```
    pub fn key(self, ty: ResourceType) -> Result<ResourceKey, Off> {
        self.guid.key(ty)
    }

    /// [`Guid::key_by_hash`] of its GUID.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Id;
    ///
    /// let key = Id::new("you.lockers:locker").key_by_hash(7);
    /// assert_eq!(key.guid, Id::new("you.lockers:locker").guid().0);
    /// ```
    #[must_use]
    pub const fn key_by_hash(self, type_hash: u32) -> ResourceKey {
        self.guid.key_by_hash(type_hash)
    }
}

/// The debug name the game's data gives a resource, such as the item
/// `Tool_Torch01_T0`: what the loader resolves against a registry where no
/// GUID or content id is at hand.
///
/// # Examples
///
/// ```
/// use enshwick::content::DebugName;
///
/// let torch = DebugName("Tool_Torch01_T0");
/// assert_eq!(torch.0, "Tool_Torch01_T0");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DebugName<'a>(pub &'a str);

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
    /// assert_eq!(enshwick::content::Id::new("a:b").to_string(), "a:b");
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
    /// use enshwick::content::Guid;
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

/// A vanilla resource's bytes from the game's own archive, as the game reads
/// them before it resolves their references.
///
/// Game data a mod builds on, such as an item's name tag or the template a
/// station item places, read the same at init and later, on the server and the
/// client.
///
/// `None` for a key the archive lacks. Each call reads the archive's files; keep what a mod reads
/// more than once.
///
/// # Examples
///
/// ```
/// use enshwick::content::{game_resource, types};
/// use enshwick::content::workshops::stations;
///
/// let key = stations::WORKBENCH.item.key(types::ITEM_INFO);
/// if let Some(item) = key.ok().and_then(game_resource) {
///     enshwick::info!("the Workbench item is {} bytes", item.len());
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
    pub const ITEM_REGISTRY: ResourceType = ResourceType(ids::ITEM_REGISTRY);
    /// Every recipe.
    pub const RECIPE_REGISTRY: ResourceType = ResourceType(ids::RECIPE_REGISTRY);
    /// What a placed object or creature is made of.
    pub const TEMPLATE: ResourceType = ResourceType(ids::TEMPLATE);
    /// The list of every template.
    pub const TEMPLATE_COLLECTION: ResourceType = ResourceType(ids::TEMPLATE_COLLECTION);
    /// Every workshop.
    pub const WORKSHOP_REGISTRY: ResourceType = ResourceType(ids::WORKSHOP_REGISTRY);
    /// The client's UI image, such as an item icon.
    pub const UI_IMAGE: ResourceType = ResourceType(ids::UI_IMAGE);
    /// The client's UI data, the crafting menu among it.
    pub const UI_BUNDLE: ResourceType = ResourceType(ids::UI_BUNDLE);
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
