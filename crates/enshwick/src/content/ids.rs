use crate::Off;
use crate::abi::ResourceKey;

use super::ResourceType;

/// A GUID in memory order: the `bytes_le` of its textual form.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
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

    /// FNV-1a 32 over the memory bytes, the number the game gives the
    /// resource with this GUID; [`ItemId::from_guid`](crate::ItemId::from_guid)
    /// and its siblings type it.
    ///
    /// # Examples
    ///
    /// See [`Guid`].
    #[must_use]
    pub const fn fnv1a(self) -> u32 {
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
    /// this GUID, part 0: for a type [`types`](crate::content::types) does not name, and for tools
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
pub(super) const NAMESPACE_BYTES: [u8; 16] = [
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
/// Content calls such as [`Load::add_item`](crate::Load::add_item) take an `Id` for what they
/// create and declare it to the loader, which turns the mod off when its id
/// lies outside the mod's namespace or its game id ([`Guid::fnv1a`]) is
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
/// assert_eq!(Guid::from(LOCKER), LOCKER.guid());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Id {
    pub(super) name: &'static str,
    pub(super) guid: Guid,
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
pub(super) const fn digit(c: u8) -> u8 {
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
