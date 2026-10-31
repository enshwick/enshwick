//! Reflection data and ECS system records of Keen's Holistic engine, read from
//! an Enshrouded server or client image.
//!
//! The engine keeps a descriptor for every reflected type and a record for
//! every ECS system in `.rdata`. Both are found by the names they carry, so a
//! lookup survives a game update that moves code and data around.
//!
//! # Examples
//!
//! ```no_run
//! use enshwick_gamedata::reflect::{Image, Layout, Reflection};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let bytes = std::fs::read("enshrouded_server.exe")?;
//! let image = Image::parse(&bytes, Layout::File, None)?;
//! let reflection = Reflection::scan(&image);
//! let offset = reflection.field_offset("keen::ecs::TriggerShape", "rangeX")?;
//! assert_eq!(offset, 4);
//! # Ok(())
//! # }
//! ```

mod pe;

use std::ops::Range;

use rustc_hash::FxHashMap;

pub use pe::{Fingerprint, Image, ImageError, Layout, Section};

/// A reflected type: its layout as the engine describes it.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let reflection = Reflection::scan(&image);
/// let shape = reflection.type_info("keen::ecs::TriggerShape")?;
/// assert!(shape.size() >= 0x10);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeInfo<'a> {
    va: u64,
    name: &'a str,
    size: u32,
    align: u16,
    kind: Kind,
    base: Option<&'a str>,
    inner: Option<&'a str>,
    count: u32,
    fields: Vec<Field<'a>>,
    enum_values: Vec<EnumValue<'a>>,
}

impl<'a> TypeInfo<'a> {
    /// Where the descriptor is.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo`].
    #[must_use]
    pub fn va(&self) -> u64 {
        self.va
    }

    /// The full name, such as `keen::ecs::TriggerShape`.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo`].
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The size in bytes.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo`].
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The alignment in bytes.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo`].
    #[must_use]
    pub fn align(&self) -> u16 {
        self.align
    }

    /// The full name of the type this struct derives from, such as
    /// `keen::ecs::GameEvent` for an event; `None` for a struct without a
    /// base and for every type that is not a struct.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// let died = reflection.type_info("keen::ecs::EntityDiedEvent")?;
    /// assert_eq!(died.base(), Some("keen::ecs::GameEvent"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn base(&self) -> Option<&'a str> {
        self.base
    }

    /// What the type is: a primitive, a struct, an enum, an array and so
    /// on.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Kind, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// assert_eq!(reflection.type_info("keen::uint32")?.kind(), Kind::Uint32);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The full name of the type this one holds or names: an array's or an
    /// optional's element, a typedef's target. `None` for a struct, whose
    /// inner type is its [`base`](Self::base), and for a type without one.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// assert_eq!(reflection.type_info("keen::ItemId")?.inner(), Some("keen::HashKey32"));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn inner(&self) -> Option<&'a str> {
        self.inner
    }

    /// How many elements a [`Kind::StaticArray`] holds; `None` for every
    /// other kind, whose length, if any, is in the data.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// let array = reflection.type_info("keen::StaticArray<keen::uint32,4>")?;
    /// assert_eq!(array.element_count(), Some(4));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn element_count(&self) -> Option<u32> {
        (self.kind == Kind::StaticArray).then_some(self.count)
    }

    /// The fields, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
    }

    /// The named constants of an enum type, in the engine's order; empty
    /// for any other type.
    ///
    /// # Examples
    ///
    /// See [`EnumValue`].
    #[must_use]
    pub fn enum_values(&self) -> &[EnumValue<'a>] {
        &self.enum_values
    }
}

/// A named constant of a reflected enum type.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let reflection = Reflection::scan(&image);
/// let slots = reflection.type_info("keen::EquipmentSlotType")?;
/// let invalid = slots.enum_values().iter().find(|v| v.name() == "Invalid").ok_or("no Invalid")?;
/// assert_eq!(invalid.value(), 0);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnumValue<'a> {
    name: &'a str,
    value: u64,
}

impl<'a> EnumValue<'a> {
    /// The constant's name, without its type.
    ///
    /// # Examples
    ///
    /// See [`EnumValue`].
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// Its value; the type's [`size`](TypeInfo::size) says how wide the
    /// engine stores it.
    ///
    /// # Examples
    ///
    /// See [`EnumValue`].
    #[must_use]
    pub fn value(&self) -> u64 {
        self.value
    }
}

/// A field of a reflected type.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let reflection = Reflection::scan(&image);
/// let shape = reflection.type_info("keen::ecs::TriggerShape")?;
/// let range_x = shape.fields().iter().find(|f| f.name() == "rangeX").ok_or("no rangeX")?;
/// assert_eq!(range_x.offset(), 4);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field<'a> {
    name: &'a str,
    offset: u64,
    type_name: Option<&'a str>,
}

impl<'a> Field<'a> {
    /// The field's name.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The offset from the start of the type.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The full name of the field's type, when that type is reflected.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn type_name(&self) -> Option<&'a str> {
        self.type_name
    }
}

/// An ECS system record: the system's update function and the components it
/// works on.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let reflection = Reflection::scan(&image);
/// let upgrade = reflection.system("upgrade_building_range")?;
/// assert!(upgrade.writes().contains(&"TriggerShape"));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct System<'a> {
    name: &'a str,
    entry: u64,
    triggers: Vec<&'a str>,
    reads: Vec<&'a str>,
    writes: Vec<&'a str>,
}

impl<'a> System<'a> {
    /// The system's name, such as `add_buildzones_server`.
    ///
    /// # Examples
    ///
    /// See [`System`].
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The update function, or 0 when this binary does not run the system (a
    /// client-only system in the server, for instance).
    ///
    /// # Examples
    ///
    /// See [`System`].
    #[must_use]
    pub fn entry(&self) -> u64 {
        self.entry
    }

    /// Short names of the components the system triggers on.
    ///
    /// # Examples
    ///
    /// See [`System`].
    #[must_use]
    pub fn triggers(&self) -> &[&'a str] {
        &self.triggers
    }

    /// Short names of the components the system reads.
    ///
    /// # Examples
    ///
    /// See [`System`].
    #[must_use]
    pub fn reads(&self) -> &[&'a str] {
        &self.reads
    }

    /// Short names of the components the system writes.
    ///
    /// # Examples
    ///
    /// See [`System`].
    #[must_use]
    pub fn writes(&self) -> &[&'a str] {
        &self.writes
    }
}

/// Why a lookup by name has no single answer.
///
/// # Examples
///
/// ```no_run
/// use enshwick_gamedata::reflect::{Image, Layout, LookupError, Reflection};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let bytes = std::fs::read("enshrouded_server.exe")?;
/// let image = Image::parse(&bytes, Layout::File, None)?;
/// let reflection = Reflection::scan(&image);
/// let error = reflection.system("no_such_system").unwrap_err();
/// assert_eq!(error, LookupError::Missing("no_such_system".into()));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LookupError {
    /// Nothing carries the name.
    #[error("{0} not found")]
    Missing(String),
    /// More than one candidate carries it: a lookup never guesses.
    #[error("{0} is ambiguous")]
    Ambiguous(String),
}

const DESCRIPTOR_SHORT_NAME: u64 = 0x40;
const DESCRIPTOR_FULL_NAME: u64 = 0x60;
/// The inner type: a struct's base, an array's element.
const DESCRIPTOR_INNER: u64 = 0x78;
const DESCRIPTOR_SIZE_ALIGN: u64 = 0x80;
/// The count of fields, or of constants in an enum.
const DESCRIPTOR_FIELD_COUNT: u64 = 0x88;
/// The type's kind byte: [`KIND_STRUCT`], [`KIND_ENUM`] and others.
const DESCRIPTOR_KIND: u64 = 0x8c;
/// The kind byte of a struct; arrays, strings and handles also carry an
/// inner type, which is not a base.
const KIND_STRUCT: u8 = 0x12;
const KIND_ENUM: u8 = 0x0c;

/// What a reflected type is, from its descriptor's kind byte.
///
/// The engine numbers the kinds; the names are the engine's kind names in
/// Rust casing, and the numbers held on client build 426 for every
/// descriptor. A byte no variant names reads as [`Kind::Other`].
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::reflect::Kind;
///
/// assert_eq!(Kind::from_byte(0x12), Kind::Struct);
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
    /// An enum, with its constants in [`TypeInfo::enum_values`].
    Enum,
    /// A set of flags in one byte.
    Bitmask8,
    /// A set of flags in two bytes.
    Bitmask16,
    /// A set of flags in four bytes.
    Bitmask32,
    /// A set of flags in eight bytes.
    Bitmask64,
    /// Another name for its [`inner`](TypeInfo::inner) type.
    Typedef,
    /// A struct, with its fields and its [`base`](TypeInfo::base).
    Struct,
    /// A fixed number of [`inner`](TypeInfo::inner) elements in place.
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

impl Kind {
    /// The kind a descriptor's kind byte names.
    ///
    /// # Examples
    ///
    /// See [`Kind`].
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        match byte {
            0x01 => Self::Bool,
            0x02 => Self::Uint8,
            0x03 => Self::Sint8,
            0x04 => Self::Uint16,
            0x05 => Self::Sint16,
            0x06 => Self::Uint32,
            0x07 => Self::Sint32,
            0x08 => Self::Uint64,
            0x09 => Self::Sint64,
            0x0a => Self::Float32,
            KIND_ENUM => Self::Enum,
            0x0d => Self::Bitmask8,
            0x0e => Self::Bitmask16,
            0x0f => Self::Bitmask32,
            0x10 => Self::Bitmask64,
            0x11 => Self::Typedef,
            KIND_STRUCT => Self::Struct,
            0x13 => Self::StaticArray,
            0x14 => Self::DsArray,
            0x15 => Self::DsString,
            0x16 => Self::DsOptional,
            0x17 => Self::DsVariant,
            0x18 => Self::BlobArray,
            0x19 => Self::BlobString,
            0x1a => Self::BlobOptional,
            0x1b => Self::BlobVariant,
            0x1c => Self::ObjectReference,
            0x1d => Self::Guid,
            other => Self::Other(other),
        }
    }
}

/// Bases deeper than this are taken for a loop in bad data.
const MAX_BASES: usize = 32;
/// Where a type descriptor holds its hash, whose low 32 bits are the
/// FNV-1a hash of the type's full name and its ECS type hash. Every lookup
/// by name checks it, so it holds on any build reflection reads.
pub const DESCRIPTOR_HASH: u64 = 0x90;
const DESCRIPTOR_FIELDS: u64 = 0x98;
const FIELD_RECORD: u64 = 48;
const FIELD_TYPE: u64 = 16;
const FIELD_OFFSET: u64 = 24;
/// An enum's constants: records of a name slice, then the value as a u64.
const DESCRIPTOR_ENUM_VALUES: u64 = 0xa0;
const ENUM_RECORD: u64 = 0x28;
const ENUM_VALUE: u64 = 16;
const SYSTEM_ENTRY: u64 = 0x10;
/// Component lists in a system record: triggers, reads, writes.
const SYSTEM_LISTS: [u64; 3] = [0x18, 0x38, 0x58];
const SLICE: u64 = 16;
const MAX_NAME: u64 = 250;
const MAX_FIELDS: u32 = 1024;
const MAX_LIST: u64 = 64;

/// FNV-1a over 32 bits, the hash the engine keys names and ids with.
///
/// A descriptor carries the hash of its full name; ids of templates, items
/// and survivors are the same hash over a GUID's bytes.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::reflect::fnv1a32;
///
/// assert_eq!(fnv1a32(b""), 0x811c_9dc5);
/// assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
/// ```
#[must_use]
pub const fn fnv1a32(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ bytes[i] as u32).wrapping_mul(0x0100_0193);
        i += 1;
    }
    hash
}

/// Every reflected type and ECS system of an image, looked up by name.
///
/// # Examples
///
/// See the [module documentation](crate::reflect).
#[derive(Debug)]
pub struct Reflection<'a> {
    types: FxHashMap<&'a str, Vec<TypeInfo<'a>>>,
    systems: FxHashMap<&'a str, Vec<System<'a>>>,
}

impl<'a> Reflection<'a> {
    /// Scans the image's `.rdata` for type descriptors and system records.
    ///
    /// An image without `.rdata` gives an empty reflection, in which every
    /// lookup fails.
    ///
    /// # Examples
    ///
    /// See the [module documentation](crate::reflect).
    #[must_use]
    pub fn scan(image: &Image<'a>) -> Self {
        let mut types: FxHashMap<&'a str, Vec<TypeInfo<'a>>> = FxHashMap::default();
        let mut systems: FxHashMap<&'a str, Vec<System<'a>>> = FxHashMap::default();
        let Some(rdata) = image.section(".rdata").map(|s| image.range(s)) else {
            return Self { types, systems };
        };
        let text = image.section(".text").map_or(0..0, |s| image.range(s));
        let reader = Reader {
            image,
            rdata: rdata.clone(),
        };

        let mut va = rdata.start;
        while va.checked_add(SLICE).is_some_and(|end| end <= rdata.end) {
            if let Some(name) = reader.slice(va) {
                if let Some(info) = va
                    .checked_sub(DESCRIPTOR_SHORT_NAME)
                    .and_then(|at| reader.descriptor(at))
                {
                    types.entry(info.name).or_default().push(info);
                } else if is_system_name(name)
                    && let Some(system) = reader.system(va, name, &text)
                {
                    systems.entry(system.name).or_default().push(system);
                }
            }
            va += 8;
        }
        Self { types, systems }
    }

    /// The type named `name`, such as `keen::ecs::TriggerShape`.
    ///
    /// # Errors
    ///
    /// [`LookupError`] when no type or more than one carries the name.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo`].
    pub fn type_info(&self, name: &str) -> Result<&TypeInfo<'a>, LookupError> {
        unique(self.types.get(name), name)
    }

    /// The system named `name`, such as `add_buildzones_server`.
    ///
    /// # Errors
    ///
    /// [`LookupError`] when no system or more than one carries the name.
    ///
    /// # Examples
    ///
    /// See [`System`].
    pub fn system(&self, name: &str) -> Result<&System<'a>, LookupError> {
        unique(self.systems.get(name), name)
    }

    /// The offset of `field` in the type named `type_name`.
    ///
    /// # Errors
    ///
    /// [`LookupError`] when the type or the field is missing or ambiguous.
    ///
    /// # Examples
    ///
    /// See the [module documentation](crate::reflect).
    pub fn field_offset(&self, type_name: &str, field: &str) -> Result<u64, LookupError> {
        let info = self.type_info(type_name)?;
        let mut found = info.fields.iter().filter(|f| f.name == field);
        match (found.next(), found.next()) {
            (Some(f), None) => Ok(f.offset),
            (None, _) => Err(LookupError::Missing(format!("{type_name}.{field}"))),
            (Some(_), Some(_)) => Err(LookupError::Ambiguous(format!("{type_name}.{field}"))),
        }
    }

    /// Every type whose chain of bases reaches the type named `root`, in
    /// no set order: every event type for `keen::ecs::Event`. Names more
    /// than one type carries are left out, as lookups leave them.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// let events = reflection.derived_from("keen::ecs::Event");
    /// assert!(events.map(|ty| ty.name()).any(|name| name == "keen::ecs::HitEvent"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn derived_from<'r>(
        &'r self,
        root: &'r str,
    ) -> impl Iterator<Item = &'r TypeInfo<'a>> + 'r {
        self.types
            .values()
            .filter_map(|found| match found.as_slice() {
                [one] => Some(one),
                _ => None,
            })
            .filter(move |info| {
                let mut base = info.base;
                for _ in 0..MAX_BASES {
                    match base {
                        Some(name) if name == root => return true,
                        Some(name) => base = self.type_info(name).ok().and_then(TypeInfo::base),
                        None => return false,
                    }
                }
                false
            })
    }

    /// The value of the constant `name` of the enum type named `type_name`,
    /// such as `Invalid` in `keen::EquipmentSlotType`.
    ///
    /// # Errors
    ///
    /// [`LookupError`] when the type or the constant is missing or
    /// ambiguous.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// assert_eq!(reflection.enum_value("keen::EquipmentSlotType", "Invalid")?, 0);
    /// # Ok(())
    /// # }
    /// ```
    pub fn enum_value(&self, type_name: &str, name: &str) -> Result<u64, LookupError> {
        let info = self.type_info(type_name)?;
        let mut found = info.enum_values.iter().filter(|v| v.name == name);
        match (found.next(), found.next()) {
            (Some(v), None) => Ok(v.value),
            (None, _) => Err(LookupError::Missing(format!("{type_name}::{name}"))),
            (Some(_), Some(_)) => Err(LookupError::Ambiguous(format!("{type_name}::{name}"))),
        }
    }

    /// Every type name found, in no set order.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let reflection = Reflection::scan(&image);
    /// assert!(reflection.type_names().any(|name| name == "keen::ecs::Health"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn type_names(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.types.keys().copied()
    }

    /// How many distinct type names were found.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// assert!(Reflection::scan(&image).type_count() > 10_000);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    /// How many distinct system names were found.
    ///
    /// # Examples
    ///
    /// Works as [`Reflection::type_count`] does.
    #[must_use]
    pub fn system_count(&self) -> usize {
        self.systems.len()
    }
}

fn unique<'r, T>(found: Option<&'r Vec<T>>, name: &str) -> Result<&'r T, LookupError> {
    match found.map(Vec::as_slice) {
        Some([one]) => Ok(one),
        Some([_, _, ..]) => Err(LookupError::Ambiguous(name.to_owned())),
        _ => Err(LookupError::Missing(name.to_owned())),
    }
}

fn is_system_name(name: &str) -> bool {
    name.len() >= 4
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

struct Reader<'i, 'a> {
    image: &'i Image<'a>,
    rdata: Range<u64>,
}

impl<'a> Reader<'_, 'a> {
    /// The string slice at `va`: a pointer into `.rdata` and a length, the
    /// bytes NUL-terminated right at the length.
    fn slice(&self, va: u64) -> Option<&'a str> {
        let ptr = self.image.u64(va)?;
        let len = self.image.u64(va.checked_add(8)?)?;
        if len == 0 || len >= MAX_NAME || !self.rdata.contains(&ptr) {
            return None;
        }
        let (nul, text) = self.image.read(ptr, len + 1)?.split_last()?;
        if *nul != 0 || text.contains(&0) || !text.is_ascii() {
            return None;
        }
        std::str::from_utf8(text).ok()
    }

    fn descriptor(&self, va: u64) -> Option<TypeInfo<'a>> {
        let at = |offset: u64| va.checked_add(offset);
        self.slice(at(DESCRIPTOR_SHORT_NAME)?)?;
        let name = self.slice(at(DESCRIPTOR_FULL_NAME)?)?;
        if self.image.u32(at(DESCRIPTOR_HASH)?)? != fnv1a32(name.as_bytes()) {
            return None;
        }
        let size = self.image.u32(at(DESCRIPTOR_SIZE_ALIGN)?)?;
        let align = self.image.u16(at(DESCRIPTOR_SIZE_ALIGN + 4)?)?;
        let count = self.image.u32(at(DESCRIPTOR_FIELD_COUNT)?)?;
        let kind = Kind::from_byte(*self.image.read(at(DESCRIPTOR_KIND)?, 1)?.first()?);
        let (mut fields, mut enum_values) = (Vec::new(), Vec::new());
        // A static array's count is its length, and it has no fields.
        if count > 0 && count <= MAX_FIELDS && kind != Kind::StaticArray {
            if kind == Kind::Enum {
                enum_values = self.enum_values(self.image.u64(at(DESCRIPTOR_ENUM_VALUES)?)?, count);
            } else {
                fields = self.fields(self.image.u64(at(DESCRIPTOR_FIELDS)?)?, count);
            }
        }
        let inner = at(DESCRIPTOR_INNER)
            .and_then(|slot| self.image.u64(slot))
            .and_then(|ptr| self.type_name(ptr));
        let (base, inner) = if kind == Kind::Struct {
            (inner, None)
        } else {
            (None, inner)
        };
        Some(TypeInfo {
            va,
            name,
            size,
            align,
            kind,
            base,
            inner,
            count,
            fields,
            enum_values,
        })
    }

    fn enum_values(&self, table: u64, count: u32) -> Vec<EnumValue<'a>> {
        (0..u64::from(count))
            .filter_map(|i| {
                let record = table.checked_add(i * ENUM_RECORD)?;
                Some(EnumValue {
                    name: self.slice(record)?,
                    value: self.image.u64(record.checked_add(ENUM_VALUE)?)?,
                })
            })
            .collect()
    }

    /// The full name of the descriptor whose short name slot `ptr` points
    /// at, as field types and bases point.
    fn type_name(&self, ptr: u64) -> Option<&'a str> {
        let descriptor = ptr.checked_sub(DESCRIPTOR_SHORT_NAME)?;
        self.slice(descriptor.checked_add(DESCRIPTOR_FULL_NAME)?)
    }

    fn fields(&self, table: u64, count: u32) -> Vec<Field<'a>> {
        (0..u64::from(count))
            .map(|i| table.checked_add(i * FIELD_RECORD))
            .filter_map(|record| {
                let record = record?;
                let name = self.slice(record)?;
                let type_name = record
                    .checked_add(FIELD_TYPE)
                    .and_then(|at| self.image.u64(at))
                    .and_then(|ptr| self.type_name(ptr));
                Some(Field {
                    name,
                    offset: self.image.u64(record.checked_add(FIELD_OFFSET)?)?,
                    type_name,
                })
            })
            .collect()
    }

    fn system(&self, va: u64, name: &'a str, text: &Range<u64>) -> Option<System<'a>> {
        let entry = self.image.u64(va.checked_add(SYSTEM_ENTRY)?)?;
        if entry != 0 && !text.contains(&entry) {
            return None;
        }
        let [triggers, reads, writes] =
            SYSTEM_LISTS.map(|at| va.checked_add(at).map_or_else(Vec::new, |at| self.list(at)));
        if triggers.is_empty() && reads.is_empty() && writes.is_empty() {
            return None;
        }
        Some(System {
            name,
            entry,
            triggers,
            reads,
            writes,
        })
    }

    fn list(&self, va: u64) -> Vec<&'a str> {
        let (Some(ptr), Some(count)) = (
            self.image.u64(va),
            va.checked_add(8).and_then(|at| self.image.u64(at)),
        ) else {
            return Vec::new();
        };
        if ptr == 0 || count == 0 || count > MAX_LIST || !self.rdata.contains(&ptr) {
            return Vec::new();
        }
        (0..count)
            .map_while(|i| self.slice(ptr.checked_add(i * SLICE)?))
            .collect()
    }
}

#[cfg(test)]
mod tests;
