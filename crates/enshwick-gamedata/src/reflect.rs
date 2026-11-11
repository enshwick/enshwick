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
mod reader;

use reader::Reader;
use rustc_hash::FxHashMap;

pub use enshwick_core::resources::Attribute;
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
    short_name: &'a str,
    impact_name: &'a str,
    size: u32,
    align: u16,
    element_align: u16,
    kind: Kind,
    flags: u8,
    internal_hash: u32,
    base: Option<&'a str>,
    inner: Option<&'a str>,
    count: u32,
    fields: Vec<Field<'a>>,
    enum_values: Vec<EnumValue<'a>>,
    attributes: Vec<Attribute<'a>>,
}

impl<'a> TypeInfo<'a> {
    /// The type as the API's type registry holds it, for
    /// [`Registry`](enshwick_core::resources::Registry).
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
    /// let ty = reflection.type_info("keen::ecs::TemplateResource")?.to_type();
    /// assert_eq!(ty.impact_name(), "ecs.TemplateResource");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn to_type(&self) -> enshwick_core::resources::Type<'a> {
        use enshwick_core::resources::{Constant, Type};

        let mut ty = Type::new(self.name, self.kind, self.size, self.align)
            .with_names(self.short_name, self.impact_name)
            .with_layout(self.element_align, self.flags, self.internal_hash)
            .with_attributes(self.attributes.clone());
        if let Some(inner) = self.base.or(self.inner) {
            ty = ty.with_inner(inner);
        }
        match self.kind {
            Kind::Struct => ty.with_fields(
                self.fields
                    .iter()
                    .filter_map(|f| {
                        let offset = u32::try_from(f.offset).ok()?;
                        let field = match f.type_name {
                            Some(type_name) => {
                                enshwick_core::resources::Field::new(f.name, offset, type_name)
                            }
                            None => {
                                enshwick_core::resources::Field::new(f.name, offset, "").untyped()
                            }
                        };
                        Some(field.with_attributes(f.attributes.clone()))
                    })
                    .collect(),
            ),
            Kind::Enum => ty.with_constants(
                self.enum_values
                    .iter()
                    .map(|v| Constant::new(v.name, v.value))
                    .collect(),
            ),
            _ => ty.with_count(self.count),
        }
    }

    /// The name without its namespace, such as `TriggerShape`.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo::to_type`].
    #[must_use]
    pub fn short_name(&self) -> &'a str {
        self.short_name
    }

    /// The name the engine's scripts use, such as `ecs.TemplateResource`.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo::to_type`].
    #[must_use]
    pub fn impact_name(&self) -> &'a str {
        self.impact_name
    }

    /// The attributes the engine attaches to the type.
    ///
    /// # Examples
    ///
    /// See [`TypeInfo::to_type`].
    #[must_use]
    pub fn attributes(&self) -> &[Attribute<'a>] {
        &self.attributes
    }

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field<'a> {
    name: &'a str,
    offset: u64,
    type_name: Option<&'a str>,
    attributes: Vec<Attribute<'a>>,
}

impl<'a> Field<'a> {
    /// The attributes the engine attaches to the field, such as its
    /// `tooltip`.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn attributes(&self) -> &[Attribute<'a>] {
        &self.attributes
    }

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
/// The name the engine's scripts use, such as `ecs.TemplateResource`
/// (server build 26319703).
const DESCRIPTOR_IMPACT_NAME: u64 = 0x50;
const DESCRIPTOR_FULL_NAME: u64 = 0x60;
/// The inner type: a struct's base, an array's element.
const DESCRIPTOR_INNER: u64 = 0x78;
const DESCRIPTOR_SIZE_ALIGN: u64 = 0x80;
/// The alignment as an array element, a u16 after size and alignment.
const DESCRIPTOR_ELEMENT_ALIGN: u64 = 0x86;
/// The count of fields, or of constants in an enum.
const DESCRIPTOR_FIELD_COUNT: u64 = 0x88;
/// The type's kind byte, [`Kind::byte`].
const DESCRIPTOR_KIND: u64 = 0x8c;
/// A flags byte right after the kind byte (server build 26319703).
const DESCRIPTOR_FLAGS: u64 = 0x8d;
/// The second word of the hash slot: a hash the engine keeps beside the
/// name's (server build 26319703).
const DESCRIPTOR_INTERNAL_HASH: u64 = 0x94;
/// The type's attributes: a pointer and a count of [`ATTRIBUTE_RECORD`]s
/// (server build 26319703).
const DESCRIPTOR_ATTRIBUTES: u64 = 0xc0;
/// An attribute: a pointer to its name record, then its value as a slice.
/// The name record holds a namespace pointer, the name slice at
/// [`ATTRIBUTE_NAME`] and the value's type at [`ATTRIBUTE_TYPE`].
const ATTRIBUTE_RECORD: u64 = 24;
const ATTRIBUTE_VALUE: u64 = 8;
const ATTRIBUTE_NAME: u64 = 8;
const ATTRIBUTE_TYPE: u64 = 0x18;
const MAX_ATTRIBUTES: u64 = 64;
#[cfg(test)]
const KIND_STRUCT: u8 = Kind::Struct.byte();
#[cfg(test)]
const KIND_ENUM: u8 = Kind::Enum.byte();

/// What a reflected type is; the API's own, so values and types read the
/// same in the loader, tools and mods.
pub use enshwick_core::resources::Kind;

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
/// A field's attributes, as [`DESCRIPTOR_ATTRIBUTES`] holds a type's.
const FIELD_ATTRIBUTES: u64 = 0x20;
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
/// With the `test` feature the default is empty, every lookup failing, for
/// tests run without a game.
///
/// # Examples
///
/// See the [module documentation](crate::reflect).
#[derive(Debug)]
#[cfg_attr(feature = "test", derive(Default))]
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

    /// The offset of the last field of `path` from the start of the first
    /// type, each step a type and its field; `None` when a field is missing
    /// or ambiguous, or does not hold the next step's type inline, or the
    /// last does not hold `last`, when given, so a changed layout finds
    /// nothing rather than a wrong sum.
    pub(crate) fn field_path<S: AsRef<str>>(
        &self,
        path: &[(S, S)],
        last: Option<&str>,
    ) -> Option<u64> {
        let mut sum = 0u64;
        for (at, (ty, field)) in path.iter().enumerate() {
            let info = self.type_info(ty.as_ref()).ok()?;
            let mut found = info.fields.iter().filter(|f| f.name == field.as_ref());
            let (Some(held), None) = (found.next(), found.next()) else {
                return None;
            };
            let next = path.get(at + 1).map(|(next, _)| next.as_ref()).or(last);
            if next.is_some_and(|next| held.type_name != Some(next)) {
                return None;
            }
            sum = sum.checked_add(held.offset)?;
        }
        Some(sum)
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

    /// Every type one name names alone, as the API's type registry, which
    /// reads and writes resource values.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use enshwick_gamedata::reflect::{Image, Layout, Reflection};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bytes = std::fs::read("enshrouded_server.exe")?;
    /// let image = Image::parse(&bytes, Layout::File, None)?;
    /// let registry = Reflection::scan(&image).registry();
    /// assert!(registry.named("keen::BalancingTable").is_some());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn registry(&self) -> enshwick_core::resources::Registry<'a> {
        enshwick_core::resources::Registry::new(self.types.values().filter_map(|found| {
            match found.as_slice() {
                [one] => Some(one.to_type()),
                _ => None,
            }
        }))
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

    /// Every system record found, those sharing a name included, in no set
    /// order.
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
    /// assert!(reflection.systems().any(|s| s.name() == "inventory_actions"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn systems(&self) -> impl Iterator<Item = &System<'a>> + '_ {
        self.systems.values().flatten()
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

#[cfg(test)]
mod tests;
