use crate::abi::ResourceKey;

/// A GUID in memory order: the `bytes_le` of its textual form.
///
/// # Examples
///
/// ```
/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
/// # }
/// ```
#[repr(transparent)]
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Shape, serde::Serialize, serde::Deserialize,
)]
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
    /// # extern crate enshwick_values as enshwick;
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

    /// The nil GUID, all zero: no resource.
    pub const NIL: Self = Self([0; 16]);

    /// The GUID a content hash reads as: its four words in memory order.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Guid;
    ///
    /// let hash = [0x1_5570, 0xfa5b_71ba, 0x1c35_6c97, 0x170c_1f07];
    /// assert_eq!(Guid::from_content_hash(hash).content_hash(), hash);
    /// assert_eq!(Guid::from_content_hash([0; 4]), Guid::NIL);
    /// ```
    #[must_use]
    pub const fn from_content_hash(hash: [u32; 4]) -> Self {
        let mut bytes = [0u8; 16];
        let mut i = 0;
        while i < 16 {
            bytes[i] = hash[i / 4].to_le_bytes()[i % 4];
            i += 1;
        }
        Self(bytes)
    }

    /// The content hash this GUID reads as: its memory bytes as four words.
    ///
    /// # Examples
    ///
    /// See [`Guid::from_content_hash`].
    #[must_use]
    pub const fn content_hash(self) -> [u32; 4] {
        let b = self.0;
        let mut hash = [0u32; 4];
        let mut i = 0;
        while i < 4 {
            hash[i] = u32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
            i += 1;
        }
        hash
    }

    /// FNV-1a 32 over the memory bytes, the number the game gives the
    /// resource with this GUID; [`ItemId::from_guid`](../abi/struct.ItemId.html#method.from_guid)
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

    /// The key of the resource whose type has the hash `type_hash` with
    /// this GUID, part 0: for a type [`types`](types/index.html) does not name, and for tools
    /// that know the hash.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
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
    /// malformed literal fails the build when used in a constant. Every kind
    /// of id follows
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    ///
    /// # Panics
    ///
    /// On a malformed id.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
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
    /// Every kind of id follows
    /// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
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
    #[doc(hidden)]
    #[must_use]
    pub const fn hashed(name: &str) -> Self {
        Self::hashed_parts("", name, "")
    }

    /// [`Guid::hashed`] of `owner`, `:` and `name` (`name` alone when
    /// `owner` is empty), then `.` and `part` unless `part` is empty,
    /// without joining them.
    const fn hashed_parts(owner: &str, name: &str, part: &str) -> Self {
        let mut input = const_sha1::ConstSlice::new().push_slice(&NAMESPACE_BYTES);
        if !owner.is_empty() {
            input = input.push_slice(owner.as_bytes()).push_slice(b":");
        }
        input = input.push_slice(name.as_bytes());
        if !part.is_empty() {
            input = input.push_slice(b".").push_slice(part.as_bytes());
        }
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
/// # extern crate enshwick_values as enshwick;
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
/// `[a-z0-9_.-]`, the name 1 to 128 of `[a-z0-9_./-]`. Every kind of id
/// follows
/// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
///
/// # Examples
///
/// ```
/// # extern crate enshwick_values as enshwick;
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
    match colon(id) {
        Some(at) => {
            let (owner, rest) = id.as_bytes().split_at(at);
            is_owner(owner) && is_name(rest.split_at(1).1)
        }
        None => false,
    }
}

/// Where the first `:` of `id` is.
const fn colon(id: &str) -> Option<usize> {
    let bytes = id.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Whether `bytes` is the mod side of a content id: 1 to 64 of `[a-z0-9_.-]`.
const fn is_owner(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 64 && all_of(bytes, false)
}

/// Whether `bytes` is the name side of a content id: 1 to 128 of
/// `[a-z0-9_./-]`.
const fn is_name(bytes: &[u8]) -> bool {
    !bytes.is_empty() && bytes.len() <= 128 && all_of(bytes, true)
}

/// Whether every byte is of `[a-z0-9_.-]`, or `/` too with `slash`.
const fn all_of(bytes: &[u8], slash: bool) -> bool {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let common =
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.');
        if !(common || (slash && b == b'/')) {
            return false;
        }
        i += 1;
    }
    true
}

/// A piece of content a mod defines: its id, `<mod>:<name>`, and the GUID
/// [`Guid::named`] derives from it.
///
/// A mod names its own content with [`id!`](crate::content::id), which puts
/// the mod's id in front of a local name, and derives related ids with
/// [`Id::part`]. Content calls such as [`Load::add_item`](../struct.Load.html#method.add_item)
/// take an `Id` for what they create and declare it to the loader, which turns
/// the mod off when its id lies outside the mod's namespace or its game id
/// ([`Guid::fnv1a`]) is another content id's; the log names both. Another
/// mod's content is named through [`deps!`](crate::deps), by the alias the
/// crate's `Cargo.toml` gives that mod.
///
/// Two ids are equal when they name the same content, however they were
/// built. Equal ids may still differ in what they allow: a derived id takes
/// no further part, while the same id written in full with [`Id::new`] does.
/// Every kind of id follows
/// [how ids are namespaced](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md).
///
/// # Examples
///
/// ```
/// # extern crate enshwick_values as enshwick;
/// use enshwick::content::{Guid, Id};
///
/// const LOCKER: Id = Id::new("you.lockers:locker");
/// assert_eq!(LOCKER.to_string(), "you.lockers:locker");
/// assert_eq!(LOCKER.guid(), Guid::named("you.lockers:locker"));
/// assert_eq!(Guid::from(LOCKER), LOCKER.guid());
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Id {
    /// The mod side, `<mod>`.
    pub(super) owner: &'static str,
    /// The name side the part follows.
    pub(super) name: &'static str,
    /// The derived part after `name` and a `.`; empty for none.
    pub(super) part: &'static str,
    pub(super) guid: Guid,
}

impl Id {
    /// The content id `name` in full, `<mod>:<name>`, for content a test or
    /// a tool names outside its mod; a mod names its own with
    /// [`id!`](crate::content::id). A malformed literal fails the build when
    /// used in a constant.
    ///
    /// # Panics
    ///
    /// On a malformed id, see [`is_id`].
    ///
    /// # Examples
    ///
    /// See [`Id`].
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "const evaluation: a malformed literal fails the build"
    )]
    pub const fn new(name: &'static str) -> Self {
        match Self::try_new(name) {
            Some(id) => id,
            None => panic!("a content id is <mod>:<name>"),
        }
    }

    /// [`Id::new`] for when a malformed id should not panic; `None` when
    /// malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Id;
    ///
    /// assert!(Id::try_new("pack:lantern").is_some());
    /// assert!(Id::try_new("lantern").is_none());
    /// ```
    #[must_use]
    pub const fn try_new(name: &'static str) -> Option<Self> {
        if !is_id(name) {
            return None;
        }
        let Some(at) = colon(name) else {
            return None;
        };
        let (owner, rest) = name.split_at(at);
        Some(Self::joined(owner, rest.split_at(1).1, ""))
    }

    /// The content id `<owner>:<name>` from its two sides, in a constant
    /// too: what a mod's [`deps!`](crate::deps) module names another
    /// mod's content with. A malformed side fails the build when used in a
    /// constant.
    ///
    /// # Panics
    ///
    /// When `owner` is not 1 to 64 of `[a-z0-9_.-]` or `name` not 1 to 128
    /// of `[a-z0-9_./-]`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Id;
    ///
    /// const LOCKER: Id = Id::of("you.lockers", "locker");
    /// assert_eq!(LOCKER, Id::new("you.lockers:locker"));
    /// assert_eq!(LOCKER.to_string(), "you.lockers:locker");
    /// ```
    #[must_use]
    #[expect(
        clippy::panic,
        reason = "const evaluation: a malformed literal fails the build"
    )]
    pub const fn of(owner: &'static str, name: &'static str) -> Self {
        match Self::try_of(owner, name) {
            Some(id) => id,
            None => panic!(
                "a content id is <mod>:<name>, the mod 1 to 64 of [a-z0-9_.-], the name 1 to 128 of [a-z0-9_./-]"
            ),
        }
    }

    /// [`Id::of`] for when a malformed side should not panic; `None` when
    /// either is malformed.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Id;
    ///
    /// assert!(Id::try_of("you.lockers", "locker").is_some());
    /// assert!(Id::try_of("you.lockers", "Locker").is_none());
    /// assert!(Id::try_of("", "locker").is_none());
    /// ```
    #[must_use]
    pub const fn try_of(owner: &'static str, name: &'static str) -> Option<Self> {
        if is_owner(owner.as_bytes()) && is_name(name.as_bytes()) {
            Some(Self::joined(owner, name, ""))
        } else {
            None
        }
    }

    /// The id of the checked sides and part.
    const fn joined(owner: &'static str, name: &'static str, part: &'static str) -> Self {
        Self {
            owner,
            name,
            part,
            guid: Guid::hashed_parts(owner, name, part),
        }
    }

    /// The id `<id>.<part>`, such as a locker's name tag `<mod>:locker.name`,
    /// in a constant too: what one piece of content derives from another's
    /// name. A malformed part fails the build when used in a constant.
    ///
    /// # Panics
    ///
    /// When `part` is not 1 or more of `[a-z0-9_./-]`, the derived name is
    /// longer than 128, or `self` is a part already.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::{Guid, Id};
    ///
    /// const LOCKER: Id = Id::new("you.lockers:locker");
    /// const LOCKER_NAME: Id = LOCKER.part("name");
    /// assert_eq!(LOCKER_NAME.to_string(), "you.lockers:locker.name");
    /// assert_eq!(LOCKER_NAME, Id::new("you.lockers:locker.name"));
    /// assert_eq!(LOCKER_NAME.guid(), Guid::named("you.lockers:locker.name"));
    /// ```
    #[must_use]
    pub const fn part(self, part: &'static str) -> Self {
        assert!(self.part.is_empty(), "a derived id takes no further part");
        assert!(
            fits(self.name, part),
            "a part is 1 or more of [a-z0-9_./-], and the name with it at most 128"
        );
        self.join(part)
    }

    /// [`Id::part`] for when a malformed part should not panic; `None` when
    /// the part is malformed, the name too long or `self` a part already.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Id;
    ///
    /// let lantern = Id::new("pack:lantern");
    /// assert!(lantern.try_part("recipe").is_some());
    /// assert!(lantern.try_part("Recipe").is_none());
    /// assert!(lantern.try_part("recipe").and_then(|r| r.try_part("copy")).is_none());
    /// ```
    #[must_use]
    pub const fn try_part(self, part: &'static str) -> Option<Self> {
        if self.part.is_empty() && fits(self.name, part) {
            Some(self.join(part))
        } else {
            None
        }
    }

    /// `self` with `part` after it, both checked.
    const fn join(self, part: &'static str) -> Self {
        Self::joined(self.owner, self.name, part)
    }

    /// The mod side of the id.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// assert_eq!(enshwick::content::Id::new("you.lockers:locker").namespace(), "you.lockers");
    /// ```
    #[must_use]
    pub const fn namespace(self) -> &'static str {
        self.owner
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

    /// [`Guid::key_by_hash`] of its GUID.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
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
/// # extern crate enshwick_values as enshwick;
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

impl PartialEq for Id {
    fn eq(&self, other: &Self) -> bool {
        self.guid == other.guid
    }
}

impl Eq for Id {}

impl core::hash::Hash for Id {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.guid.hash(state);
    }
}

impl core::fmt::Display for Id {
    /// The id, `<mod>:<name>`.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
    /// use enshwick::content::Id;
    ///
    /// assert_eq!(Id::new("a:b").to_string(), "a:b");
    /// assert_eq!(Id::new("a:b").part("c").to_string(), "a:b.c");
    /// ```
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}:{}", self.owner, self.name)?;
        if self.part.is_empty() {
            return Ok(());
        }
        write!(f, ".{}", self.part)
    }
}

/// Whether `part` after the name side `name` and a `.` makes a content id:
/// `part` 1 or more of `[a-z0-9_./-]`, the name side at most 128 with it.
const fn fits(name: &str, part: &str) -> bool {
    let bytes = part.as_bytes();
    !bytes.is_empty() && all_of(bytes, true) && name.len() + 1 + bytes.len() <= 128
}

/// The part that names a piece of content in its owner's texts.
pub const NAME_PART: &str = "name";

/// The name side `<name>.<part>` of the id [`Id::try_part`] derives from
/// the content id `id`, for an id known only at run time; `None` where
/// [`is_id`] or [`Id::try_part`] refuses.
#[doc(hidden)]
#[must_use]
pub fn part_name(id: &str, part: &str) -> Option<String> {
    let (_, name) = id.split_once(':')?;
    (is_id(id) && fits(name, part)).then(|| format!("{name}.{part}"))
}

impl core::fmt::Display for Guid {
    /// The textual form [`Guid::parse`] reads.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_values as enshwick;
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

/// The value of the hex digit `c`.
///
/// # Panics
///
/// When `c` is no hex digit.
#[must_use]
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
