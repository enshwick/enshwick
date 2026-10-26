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

#![forbid(unsafe_code)]

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
    fields: Vec<Field<'a>>,
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

    /// The fields, in the engine's order.
    ///
    /// # Examples
    ///
    /// See [`Field`].
    #[must_use]
    pub fn fields(&self) -> &[Field<'a>] {
        &self.fields
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
const DESCRIPTOR_SIZE_ALIGN: u64 = 0x80;
const DESCRIPTOR_FIELD_COUNT: u64 = 0x88;
/// Where a type descriptor holds its hash, whose low 32 bits are the
/// FNV-1a hash of the type's full name and its ECS type hash. Every lookup
/// by name checks it, so it holds on any build reflection reads.
pub const DESCRIPTOR_HASH: u64 = 0x90;
const DESCRIPTOR_FIELDS: u64 = 0x98;
const FIELD_RECORD: u64 = 48;
const FIELD_TYPE: u64 = 16;
const FIELD_OFFSET: u64 = 24;
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
        let fields = if count > 0 && count <= MAX_FIELDS {
            self.fields(self.image.u64(at(DESCRIPTOR_FIELDS)?)?, count)
        } else {
            Vec::new()
        };
        Some(TypeInfo {
            va,
            name,
            size,
            align,
            fields,
        })
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
                    .and_then(|ptr| ptr.checked_sub(DESCRIPTOR_SHORT_NAME))
                    .and_then(|descriptor| descriptor.checked_add(DESCRIPTOR_FULL_NAME))
                    .and_then(|at| self.slice(at));
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
mod tests {
    use super::*;

    /// A minimal PE32+ file: headers, then `.text` and `.rdata`, each laid out
    /// at the same offset in the file as its RVA, so tests can write data at
    /// virtual addresses directly.
    struct Fake {
        bytes: Vec<u8>,
    }

    const BASE: u64 = 0x1_4000_0000;
    const TEXT: u64 = 0x1000;
    const RDATA: u64 = 0x2000;
    const SIZE: u64 = 0x1000;

    impl Fake {
        fn new() -> Self {
            let mut bytes = vec![0u8; usize::try_from(RDATA + SIZE).unwrap()];
            bytes[..2].copy_from_slice(b"MZ");
            bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
            bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
            bytes[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
            bytes[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
            let optional = 0x98;
            bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
            bytes[optional + 24..optional + 32].copy_from_slice(&BASE.to_le_bytes());
            let headers = optional + 0xf0;
            for (i, (name, rva)) in [(&b".text\0\0\0"[..], TEXT), (&b".rdata\0\0"[..], RDATA)]
                .into_iter()
                .enumerate()
            {
                let at = headers + i * 40;
                bytes[at..at + 8].copy_from_slice(name);
                for (field, value) in [(8, SIZE), (12, rva), (16, SIZE), (20, rva)] {
                    let value = u32::try_from(value).unwrap();
                    bytes[at + field..at + field + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
            Self { bytes }
        }

        fn put(&mut self, va: u64, data: &[u8]) {
            let at = usize::try_from(va - BASE).unwrap();
            self.bytes[at..at + data.len()].copy_from_slice(data);
        }

        fn put_u64(&mut self, va: u64, value: u64) {
            self.put(va, &value.to_le_bytes());
        }

        /// Writes `text` NUL-terminated at `at` and a slice to it at `slot`.
        fn slice(&mut self, slot: u64, at: u64, text: &str) {
            self.put(at, text.as_bytes());
            self.put(at + text.len() as u64, &[0]);
            self.put_u64(slot, at);
            self.put_u64(slot + 8, text.len() as u64);
        }
    }

    fn rdata(offset: u64) -> u64 {
        BASE + RDATA + offset
    }

    #[test]
    fn fnv1a32_matches_the_reference_values() {
        assert_eq!(fnv1a32(b""), 0x811c_9dc5);
        assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
    }

    #[test]
    fn bytes_that_are_not_pe_are_refused() {
        assert_eq!(
            Image::parse(b"MZ", Layout::File, None).unwrap_err(),
            ImageError::Truncated
        );
        assert_eq!(
            Image::parse(b"ELF", Layout::File, None).unwrap_err(),
            ImageError::NotPe
        );
    }

    #[test]
    fn sections_and_addresses_resolve() {
        let mut fake = Fake::new();
        fake.put(rdata(0x10), b"abcd");
        let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
        assert_eq!(image.base(), BASE);
        assert_eq!(image.section(".rdata").unwrap().rva(), RDATA);
        assert_eq!(image.read(rdata(0x10), 4), Some(&b"abcd"[..]));
        assert_eq!(image.read(rdata(SIZE - 2), 4), None);
        assert!(image.fingerprint().is_none());
    }

    #[test]
    fn an_image_at_the_top_of_the_address_space_reads_nothing_and_does_not_panic() {
        let mut fake = Fake::new();
        // A debug directory of 0xffff_ffff bytes at `.rdata`.
        let optional = 0x98;
        let debug = optional + 112 + 6 * 8;
        let rva = u32::try_from(RDATA).unwrap();
        fake.bytes[debug..debug + 4].copy_from_slice(&rva.to_le_bytes());
        fake.bytes[debug + 4..debug + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        let top = u64::MAX - 0x100;
        let image = Image::parse(&fake.bytes, Layout::File, Some(top)).unwrap();
        assert!(image.fingerprint().is_none());
        assert_eq!(image.read(top, u64::MAX), None);
        let reflection = Reflection::scan(&image);
        assert_eq!(reflection.type_names().count(), 0);
    }

    #[test]
    fn a_descriptor_and_its_fields_are_found() {
        let mut fake = Fake::new();
        let descriptor = rdata(0x100);
        let name = "keen::ecs::TriggerShape";
        fake.slice(
            descriptor + DESCRIPTOR_SHORT_NAME,
            rdata(0x800),
            "TriggerShape",
        );
        fake.slice(descriptor + DESCRIPTOR_FULL_NAME, rdata(0x820), name);
        fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN, &0x10u32.to_le_bytes());
        fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN + 4, &4u16.to_le_bytes());
        fake.put(descriptor + DESCRIPTOR_FIELD_COUNT, &2u32.to_le_bytes());
        fake.put(
            descriptor + DESCRIPTOR_HASH,
            &fnv1a32(name.as_bytes()).to_le_bytes(),
        );
        let table = rdata(0x400);
        fake.put_u64(descriptor + DESCRIPTOR_FIELDS, table);
        fake.slice(table, rdata(0x840), "rangeX");
        fake.put_u64(table + FIELD_OFFSET, 4);
        fake.slice(table + FIELD_RECORD, rdata(0x850), "rangeY");
        fake.put_u64(table + FIELD_RECORD + FIELD_OFFSET, 8);

        let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let info = reflection.type_info(name).unwrap();
        assert_eq!(
            (info.size(), info.align(), info.fields().len()),
            (0x10, 4, 2)
        );
        assert_eq!(reflection.field_offset(name, "rangeY"), Ok(8));
        assert_eq!(
            reflection.field_offset(name, "rangeZ"),
            Err(LookupError::Missing(format!("{name}.rangeZ")))
        );
    }

    #[test]
    fn an_unreadable_field_record_drops_that_field_only() {
        let mut fake = Fake::new();
        let descriptor = rdata(0x100);
        let name = "keen::ecs::TriggerShape";
        fake.slice(
            descriptor + DESCRIPTOR_SHORT_NAME,
            rdata(0x800),
            "TriggerShape",
        );
        fake.slice(descriptor + DESCRIPTOR_FULL_NAME, rdata(0x820), name);
        fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN, &0x10u32.to_le_bytes());
        fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN + 4, &4u16.to_le_bytes());
        fake.put(descriptor + DESCRIPTOR_FIELD_COUNT, &3u32.to_le_bytes());
        fake.put(
            descriptor + DESCRIPTOR_HASH,
            &fnv1a32(name.as_bytes()).to_le_bytes(),
        );
        let table = rdata(0x400);
        fake.put_u64(descriptor + DESCRIPTOR_FIELDS, table);
        fake.slice(table, rdata(0x840), "rangeX");
        fake.put_u64(table + FIELD_OFFSET, 4);
        // The second record's name points outside the image.
        fake.put_u64(table + FIELD_RECORD, 0x10);
        fake.put_u64(table + FIELD_RECORD + 8, 6);
        fake.slice(table + 2 * FIELD_RECORD, rdata(0x850), "rangeZ");
        fake.put_u64(table + 2 * FIELD_RECORD + FIELD_OFFSET, 12);

        let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let info = reflection.type_info(name).unwrap();
        assert_eq!(info.fields().len(), 2);
        assert_eq!(reflection.field_offset(name, "rangeX"), Ok(4));
        assert_eq!(reflection.field_offset(name, "rangeZ"), Ok(12));
    }

    #[test]
    fn a_system_record_is_found_and_lookups_never_guess() {
        let mut fake = Fake::new();
        let entry = BASE + TEXT + 0x40;
        for record in [rdata(0x100), rdata(0x200)] {
            fake.slice(record, rdata(0x800), "add_buildzones_server");
            fake.put_u64(record + SYSTEM_ENTRY, entry);
            fake.put_u64(record + SYSTEM_LISTS[2], rdata(0x600));
            fake.put_u64(record + SYSTEM_LISTS[2] + 8, 1);
        }
        fake.slice(rdata(0x600), rdata(0x880), "TriggerShape");

        let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        assert_eq!(
            reflection.system("add_buildzones_server"),
            Err(LookupError::Ambiguous("add_buildzones_server".into()))
        );

        let mut single = Fake::new();
        single.bytes.clone_from(&fake.bytes);
        single.put(rdata(0x200), &[0; 0x80]);
        let image = Image::parse(&single.bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let system = reflection.system("add_buildzones_server").unwrap();
        assert_eq!(system.entry(), entry);
        assert_eq!(system.writes(), ["TriggerShape"]);
        assert!(system.triggers().is_empty());
    }
}
