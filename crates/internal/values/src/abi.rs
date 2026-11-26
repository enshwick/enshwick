//! The types of the ABI that the value types here name.

/// A GUID in the one byte order every ABI field uses, the game's memory
/// order: the `bytes_le` of its textual form, as the game's files store it.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
/// assert_eq!(CHEST.0[..4], [0x3a, 0x05, 0x43, 0x69]);
/// assert_eq!(enshwick::ItemId::from_guid(CHEST).get(), 1_302_084_846);
/// ```
/// cbindgen:field-names=\[bytes\]
#[expect(
    clippy::doc_paragraphs_missing_punctuation,
    reason = "cbindgen reads the annotation line as written"
)]
#[repr(C)]
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    bytemuck::Zeroable,
    bytemuck::Pod,
    crate::Shape,
    serde::Serialize,
    serde::Deserialize,
)]
#[shape(api = "guid")]
#[expect(
    clippy::exhaustive_structs,
    reason = "a newtype over one value, which never gains a field"
)]
pub struct Guid(pub [u8; 16]);

/// A game resource's key: its GUID, its type hash and part.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ResourceKey;
/// use enshwick::content::Guid;
///
/// let item = ResourceKey { guid: Guid([0x3a; 16]), type_hash: 0xb5ce_8765, part: 0 };
/// assert_eq!(item.part, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a value of fixed parts, which never gains one"
)]
pub struct ResourceKey {
    /// The resource's GUID.
    pub guid: Guid,
    /// The reflected type hash of the resource's root type.
    pub type_hash: u32,
    /// The part of a resource split in parts; 0 for most.
    pub part: u32,
}
