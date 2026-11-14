//! The types of the ABI that the value types here name.

/// A game resource's key: its GUID in memory order, its type hash and part.
///
/// # Examples
///
/// ```
/// use enshwick::abi::ResourceKey;
///
/// let item = ResourceKey { guid: [0x3a; 16], type_hash: 0xb5ce_8765, part: 0 };
/// assert_eq!(item.part, 0);
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    /// The resource's GUID, in memory order (`bytes_le` of the textual form).
    pub guid: [u8; 16],
    /// The reflected type hash of the resource's root type.
    pub type_hash: u32,
    /// The part of a resource split in parts; 0 for most.
    pub part: u32,
}
