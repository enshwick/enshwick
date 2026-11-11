//! What a reflected type is, by its descriptor's kind byte.

/// What a reflected type is, from its descriptor's kind byte.
///
/// The engine numbers the kinds; the names are the engine's kind names in
/// Rust casing, and the numbers held on client build 426 and server build
/// 26319703 for every descriptor. A byte no variant names reads as
/// [`Kind::Other`].
///
/// # Examples
///
/// ```
/// # extern crate enshwick_base as enshwick;
/// use enshwick::resources::Kind;
///
/// assert_eq!(Kind::from_byte(0x12), Kind::Struct);
/// assert_eq!(Kind::Struct.byte(), 0x12);
/// assert_eq!(Kind::from_byte(0x7f), Kind::Other(0x7f));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// `keen::bool`, one byte.
    Bool,
    /// `keen::uint8`.
    Uint8,
    /// `keen::sint8`.
    Sint8,
    /// `keen::uint16`.
    Uint16,
    /// `keen::sint16`.
    Sint16,
    /// `keen::uint32`.
    Uint32,
    /// `keen::sint32`.
    Sint32,
    /// `keen::uint64`.
    Uint64,
    /// `keen::sint64`.
    Sint64,
    /// `keen::float32`.
    Float32,
    /// An enum, with its constants in [`Type::constants`](super::Type::constants).
    Enum,
    /// A set of flags in one byte.
    Bitmask8,
    /// A set of flags in two bytes.
    Bitmask16,
    /// A set of flags in four bytes.
    Bitmask32,
    /// A set of flags in eight bytes.
    Bitmask64,
    /// Another name for its [`inner`](super::Type::inner) type.
    Typedef,
    /// A struct, with its fields and its [`base`](super::Type::base).
    Struct,
    /// A fixed number of [`inner`](super::Type::inner) elements in place.
    StaticArray,
    /// An array of designer data.
    DsArray,
    /// A string of designer data.
    DsString,
    /// An optional value of designer data.
    DsOptional,
    /// A variant of designer data.
    DsVariant,
    /// An array in a resource: a self-relative offset and a count.
    BlobArray,
    /// A string in a resource.
    BlobString,
    /// An optional value in a resource.
    BlobOptional,
    /// A variant in a resource.
    BlobVariant,
    /// A reference to a resource by GUID.
    ObjectReference,
    /// `keen::Guid`, 16 bytes.
    Guid,
    /// A kind byte no variant names.
    Other(u8),
}

/// The kind bytes, one list for both directions; byte `0x0b` names no kind
/// on any build read so far.
macro_rules! kind_bytes {
    ($($kind:ident = $byte:literal),* $(,)?) => {
        impl Kind {
            /// The kind a descriptor's kind byte names.
            ///
            /// # Examples
            ///
            /// See [`Kind`].
            #[must_use]
            pub const fn from_byte(byte: u8) -> Self {
                match byte {
                    $($byte => Self::$kind,)*
                    other => Self::Other(other),
                }
            }

            /// The descriptor's kind byte that names this kind.
            ///
            /// # Examples
            ///
            /// See [`Kind`].
            #[must_use]
            pub const fn byte(self) -> u8 {
                match self {
                    $(Self::$kind => $byte,)*
                    Self::Other(byte) => byte,
                }
            }
        }
    };
}

kind_bytes!(
    Bool = 0x01,
    Uint8 = 0x02,
    Sint8 = 0x03,
    Uint16 = 0x04,
    Sint16 = 0x05,
    Uint32 = 0x06,
    Sint32 = 0x07,
    Uint64 = 0x08,
    Sint64 = 0x09,
    Float32 = 0x0a,
    Enum = 0x0c,
    Bitmask8 = 0x0d,
    Bitmask16 = 0x0e,
    Bitmask32 = 0x0f,
    Bitmask64 = 0x10,
    Typedef = 0x11,
    Struct = 0x12,
    StaticArray = 0x13,
    DsArray = 0x14,
    DsString = 0x15,
    DsOptional = 0x16,
    DsVariant = 0x17,
    BlobArray = 0x18,
    BlobString = 0x19,
    BlobOptional = 0x1a,
    BlobVariant = 0x1b,
    ObjectReference = 0x1c,
    Guid = 0x1d,
);

impl Kind {
    /// How many bytes an integer, float or flag kind takes; `None` for
    /// every other kind, whose size its [`Type::size`](super::Type::size) gives.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_base as enshwick;
    /// use enshwick::resources::Kind;
    ///
    /// assert_eq!(Kind::Bitmask16.width(), Some(2));
    /// assert_eq!(Kind::Struct.width(), None);
    /// ```
    #[must_use]
    pub const fn width(self) -> Option<usize> {
        Some(match self {
            Self::Bool | Self::Uint8 | Self::Sint8 | Self::Bitmask8 => 1,
            Self::Uint16 | Self::Sint16 | Self::Bitmask16 => 2,
            Self::Uint32 | Self::Sint32 | Self::Float32 | Self::Bitmask32 => 4,
            Self::Uint64 | Self::Sint64 | Self::Bitmask64 => 8,
            _ => return None,
        })
    }
}
