//! Reads a resource's bytes into a [`Value`] by its reflected type, and
//! writes a [`Value`] back.
//!
//! Arrays, strings, optionals and variants in a resource hold an `i32`
//! offset from their own header to their data. Writing keeps every byte of
//! the base whose value did not change, writes changed values in place, and
//! moves data that no longer fits to the end of the bytes, as
//! [`Blob`](crate::content::Blob) does.

mod rebase;

use std::borrow::Cow;

use super::{Field, Kind, Registry, Type, Value, Variant};
use crate::content::Guid;

/// Nesting deeper than this is taken for a loop in bad data.
const MAX_DEPTH: usize = 64;
/// Typedef chains longer than this are taken for a loop.
const MAX_TYPEDEFS: usize = 16;
/// A variant's header: the held type's qualified hash, the offset to its
/// value counted from the offset's own position, and the value's size.
const VARIANT_OFFSET: usize = 4;
const VARIANT_SIZE: usize = 8;

/// Why a value does not read from or write to a resource: the path to the
/// value and the reason.
///
/// # Examples
///
/// ```
/// use enshwick::resources::{Kind, Registry, Type, Value};
///
/// let registry = Registry::new([Type::new("demo::uint8", Kind::Uint8, 1, 1)]);
/// let error = registry.encode("demo::uint8", &Value::Uint(300), None).unwrap_err();
/// assert_eq!(error.to_string(), "300 does not fit demo::uint8");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}{why}", if path.is_empty() { String::new() } else { format!("{path}: ") })]
pub struct CodecError {
    path: String,
    why: String,
}

impl CodecError {
    fn new(why: impl Into<String>) -> Self {
        Self {
            path: String::new(),
            why: why.into(),
        }
    }

    /// The same error inside the field or element `part`.
    fn within(mut self, part: &str) -> Self {
        self.path = if self.path.is_empty() {
            part.to_owned()
        } else if self.path.starts_with('[') {
            format!("{part}{}", self.path)
        } else {
            format!("{part}.{}", self.path)
        };
        self
    }

    /// Where in the value the error is, such as `components[2].name`; empty
    /// for the value itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Field, Kind, Registry, Type, Value};
    ///
    /// let registry = Registry::new([
    ///     Type::new("demo::uint8", Kind::Uint8, 1, 1),
    ///     Type::new("demo::Pair", Kind::Struct, 2, 1)
    ///         .with_fields(vec![Field::new("a", 0, "demo::uint8"), Field::new("b", 1, "demo::uint8")]),
    /// ]);
    /// let value = Value::Struct(vec![("b".into(), Value::Text("x".into()))]);
    /// let error = registry.encode("demo::Pair", &value, None).unwrap_err();
    /// assert_eq!(error.path(), "b");
    /// ```
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

fn out_of_range(at: usize) -> CodecError {
    CodecError::new(format!("data at byte {at} lies outside the resource"))
}

fn index(i: usize) -> String {
    format!("[{i}]")
}

/// The little-endian unsigned integer of `bytes`, at most eight.
fn le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .rev()
        .fold(0, |value, byte| (value << 8) | u64::from(*byte))
}

/// The low `width` bytes of `value`, little-endian.
fn le_bytes(value: u64, width: usize) -> Vec<u8> {
    value.to_le_bytes()[..width.min(8)].to_vec()
}

fn mask(width: usize) -> u64 {
    if width >= 8 {
        u64::MAX
    } else {
        (1u64 << (width * 8)) - 1
    }
}

fn size_of(ty: &Type<'_>) -> usize {
    usize::try_from(ty.size()).unwrap_or(usize::MAX)
}

/// The stride of `ty` as an array element.
fn stride(ty: &Type<'_>) -> usize {
    size_of(ty).next_multiple_of(usize::from(ty.align().max(1)))
}

/// Where the data of the header at `at` starts, from its `i32` offset at
/// `from`.
fn target(bytes: &[u8], at: usize, from: usize) -> Result<usize, CodecError> {
    let offset = i32::from_le_bytes(
        bytes
            .get(from..from + 4)
            .ok_or_else(|| out_of_range(from))?
            .try_into()
            .map_err(|_| out_of_range(from))?,
    );
    at.checked_add_signed(isize::try_from(offset).map_err(|_| out_of_range(at))?)
        .ok_or_else(|| out_of_range(at))
}

fn word(bytes: &[u8], at: usize) -> Result<u32, CodecError> {
    let slot = bytes.get(at..at + 4).ok_or_else(|| out_of_range(at))?;
    Ok(u32::try_from(le(slot)).unwrap_or(u32::MAX))
}

impl<'a> Registry<'a> {
    /// The type `ty` names, through its typedefs.
    fn underlying<'r>(&'r self, ty: &'r Type<'a>) -> Result<&'r Type<'a>, CodecError> {
        let mut ty = ty;
        for _ in 0..MAX_TYPEDEFS {
            if ty.kind() != Kind::Typedef {
                return Ok(ty);
            }
            let inner = ty
                .inner()
                .ok_or_else(|| CodecError::new(format!("{} names no type", ty.qualified_name())))?;
            ty = self.require(inner)?;
        }
        Err(CodecError::new(format!(
            "{} names types in a loop",
            ty.qualified_name()
        )))
    }

    fn require(&self, name: &str) -> Result<&Type<'a>, CodecError> {
        self.named(name)
            .ok_or_else(|| CodecError::new(format!("{name} is not a type of this build")))
    }

    fn inner_of(&self, ty: &Type<'a>) -> Result<&Type<'a>, CodecError> {
        let inner = ty.inner().ok_or_else(|| {
            CodecError::new(format!("{} has no element type", ty.qualified_name()))
        })?;
        self.require(inner)
    }

    /// The fields of struct `ty`, its bases' first, each name once.
    fn struct_fields<'r>(&'r self, ty: &'r Type<'a>) -> Vec<&'r Field<'a>> {
        let mut fields: Vec<&Field<'a>> = Vec::new();
        for field in self.all_fields(ty) {
            // A derived field of a base field's name shadows it: the base's bytes
            // stay as they are.
            if let Some(same) = fields.iter_mut().find(|f| f.name() == field.name()) {
                *same = field;
            } else {
                fields.push(field);
            }
        }
        fields
    }

    /// Reads `bytes`, a resource of the type qualified as `type_name`, into
    /// its value. Only the bytes the value reaches are read, so a resource
    /// decodes when it is needed and not before.
    ///
    /// # Errors
    ///
    /// [`CodecError`] for a type the registry lacks, data that lies outside
    /// `bytes`, and nesting deeper than 64.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Field, Kind, Registry, Type, Value};
    ///
    /// let registry = Registry::new([
    ///     Type::new("demo::float32", Kind::Float32, 4, 4),
    ///     Type::new("demo::Table", Kind::Struct, 4, 4)
    ///         .with_fields(vec![Field::new("damage", 0, "demo::float32")]),
    /// ]);
    /// let table = registry.decode("demo::Table", &2.5f32.to_le_bytes())?;
    /// assert_eq!(table.field("damage"), Some(&Value::Float(2.5)));
    /// # Ok::<(), enshwick::resources::CodecError>(())
    /// ```
    pub fn decode(&self, type_name: &str, bytes: &[u8]) -> Result<Value, CodecError> {
        let ty = self.require(type_name)?;
        Decoder {
            registry: self,
            bytes,
        }
        .value(ty, 0, 0)
    }

    /// Writes `value`, of the type qualified as `type_name`, as resource
    /// bytes: over `base`, the bytes it was read from, keeping every byte
    /// whose value did not change, or into new bytes for `None`. A struct may
    /// name only the fields it changes; the rest keep `base`'s bytes, or
    /// zero.
    ///
    /// # Errors
    ///
    /// [`CodecError`] for a value of another kind than its type, a number
    /// out of its type's range, a name no constant or field carries, a static
    /// array of another length, and a `base` too short for the type.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Field, Kind, Registry, Type, Value};
    ///
    /// let registry = Registry::new([
    ///     Type::new("demo::float32", Kind::Float32, 4, 4),
    ///     Type::new("demo::Table", Kind::Struct, 8, 4).with_fields(vec![
    ///         Field::new("damage", 0, "demo::float32"),
    ///         Field::new("speed", 4, "demo::float32"),
    ///     ]),
    /// ]);
    /// let base = [2.5f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat();
    /// let doubled = Value::Struct(vec![("damage".into(), Value::Float(5.0))]);
    /// let bytes = registry.encode("demo::Table", &doubled, Some(&base))?;
    /// assert_eq!(bytes, [5.0f32.to_le_bytes(), 1.0f32.to_le_bytes()].concat());
    /// # Ok::<(), enshwick::resources::CodecError>(())
    /// ```
    pub fn encode(
        &self,
        type_name: &str,
        value: &Value,
        base: Option<&[u8]>,
    ) -> Result<Vec<u8>, CodecError> {
        let ty = self.require(type_name)?;
        let size = size_of(ty);
        let (bytes, fresh) = match base {
            Some(base) if base.len() >= size => (base.to_vec(), false),
            Some(base) => {
                return Err(CodecError::new(format!(
                    "the base holds {} bytes, {} needs {size}",
                    base.len(),
                    ty.qualified_name()
                )));
            }
            None => (vec![0; size], true),
        };
        let mut encoder = Encoder {
            registry: self,
            bytes,
        };
        encoder.value(ty, value, 0, fresh, 0)?;
        Ok(encoder.bytes)
    }
}

struct Decoder<'r, 'a, 'b> {
    registry: &'r Registry<'a>,
    bytes: &'b [u8],
}

impl<'a> Decoder<'_, 'a, '_> {
    fn slot(&self, at: usize, len: usize) -> Result<&[u8], CodecError> {
        self.bytes
            .get(at..at.checked_add(len).ok_or_else(|| out_of_range(at))?)
            .ok_or_else(|| out_of_range(at))
    }

    fn value(&self, ty: &Type<'a>, at: usize, depth: usize) -> Result<Value, CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::new("nested deeper than 64"));
        }
        let ty = self.registry.underlying(ty)?;
        let slot = self.slot(at, size_of(ty))?;
        let raw = || le(&slot[..ty.kind().width().unwrap_or(0).min(slot.len())]);
        Ok(match ty.kind() {
            Kind::Bool => match raw() {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                other => Value::Uint(other),
            },
            Kind::Uint8 | Kind::Uint16 | Kind::Uint32 | Kind::Uint64 => Value::Uint(raw()),
            Kind::Sint8 | Kind::Sint16 | Kind::Sint32 | Kind::Sint64 => {
                let width = ty.kind().width().unwrap_or(8);
                let shift = 64 - width * 8;
                Value::Int((raw().cast_signed() << shift) >> shift)
            }
            Kind::Float32 => {
                let bits = u32::try_from(raw()).unwrap_or(0);
                Value::Float(f64::from(f32::from_bits(bits)))
            }
            Kind::Enum => {
                let raw = le(slot);
                let mask = mask(slot.len());
                ty.constants()
                    .iter()
                    .find(|c| c.value() & mask == raw)
                    .map_or(Value::Uint(raw), |c| Value::Name(c.name().to_owned()))
            }
            Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64 => {
                self.flags(ty, raw())?
            }
            Kind::Struct => self.fields(ty, at, depth)?,
            Kind::StaticArray => {
                let inner = self.registry.inner_of(ty)?;
                let count = usize::try_from(ty.element_count().unwrap_or(0)).unwrap_or(0);
                let stride = size_of(ty).checked_div(count).unwrap_or(0);
                self.list(inner, at, count, stride, depth)?
            }
            Kind::BlobArray => {
                let inner = self.registry.inner_of(ty)?;
                let start = target(self.bytes, at, at)?;
                let count = usize::try_from(word(self.bytes, at + 4)?).unwrap_or(usize::MAX);
                let stride = stride(inner);
                // An array whose elements run past the resource holds packed
                // data its count does not measure in elements, such as a
                // voxel grid (server build 26319703): its header stays bytes.
                let fits = count
                    .checked_mul(stride)
                    .is_some_and(|len| self.slot(start, len).is_ok());
                if !fits {
                    return Ok(Value::Bytes(slot.to_vec()));
                }
                self.list(inner, start, count, stride, depth)?
            }
            Kind::BlobString => {
                let len = usize::try_from(word(self.bytes, at + 4)?).unwrap_or(usize::MAX);
                let start = if len == 0 {
                    at
                } else {
                    target(self.bytes, at, at)?
                };
                let text = self.slot(start, len)?;
                core::str::from_utf8(text).map_or_else(
                    |_| Value::Bytes(text.to_vec()),
                    |text| Value::Text(text.to_owned()),
                )
            }
            Kind::BlobOptional => {
                if word(self.bytes, at)? == 0 {
                    Value::Nil
                } else {
                    let inner = self.registry.inner_of(ty)?;
                    self.value(inner, target(self.bytes, at, at)?, depth + 1)?
                }
            }
            Kind::BlobVariant => {
                let hash = word(self.bytes, at)?;
                if hash == 0 {
                    Value::Nil
                } else {
                    let held = self.registry.hashed(hash).ok_or_else(|| {
                        CodecError::new(format!("a variant holds unknown type {hash:#010x}"))
                    })?;
                    let from = at + VARIANT_OFFSET;
                    let value = self.value(held, target(self.bytes, from, from)?, depth + 1)?;
                    Value::Variant(Box::new(Variant::new(held.qualified_name(), value)))
                }
            }
            Kind::ObjectReference | Kind::Guid if slot.len() == 16 => {
                Value::Guid(Guid(slot.try_into().map_err(|_| out_of_range(at))?))
            }
            _ => Value::Bytes(slot.to_vec()),
        })
    }

    fn flags(&self, ty: &Type<'a>, raw: u64) -> Result<Value, CodecError> {
        let flags = self.registry.inner_of(ty)?;
        let mut names = Vec::new();
        for bit in 0..64u64 {
            if raw & (1 << bit) == 0 {
                continue;
            }
            match flags.constants().iter().find(|c| c.value() == bit) {
                Some(constant) => names.push(constant.name().to_owned()),
                None => return Ok(Value::Uint(raw)),
            }
        }
        Ok(Value::Flags(names))
    }

    fn fields(&self, ty: &Type<'a>, at: usize, depth: usize) -> Result<Value, CodecError> {
        let mut values = Vec::new();
        for field in self.registry.struct_fields(ty) {
            let Some(type_name) = field.type_name() else {
                continue;
            };
            let read = || {
                let field_ty = self.registry.require(type_name)?;
                let offset = usize::try_from(field.offset()).map_err(|_| out_of_range(at))?;
                self.value(field_ty, at + offset, depth + 1)
            };
            values.push((
                field.name().to_owned(),
                read().map_err(|e| e.within(field.name()))?,
            ));
        }
        Ok(Value::Struct(values))
    }

    fn list(
        &self,
        inner: &Type<'a>,
        start: usize,
        count: usize,
        stride: usize,
        depth: usize,
    ) -> Result<Value, CodecError> {
        // Every element takes at least one byte, so a damaged count fails
        // here instead of allocating.
        self.slot(start, count.min(self.bytes.len() + 1))?;
        (0..count)
            .map(|i| {
                self.value(inner, start + i * stride, depth + 1)
                    .map_err(|e| e.within(&index(i)))
            })
            .collect::<Result<_, _>>()
            .map(Value::List)
    }
}

struct Encoder<'r, 'a> {
    registry: &'r Registry<'a>,
    bytes: Vec<u8>,
}

/// A value of `ty` that another kind was given for.
fn mismatch(ty: &Type<'_>, value: &Value) -> CodecError {
    let given = match value {
        Value::Nil => "nil",
        Value::Bool(_) => "a bool",
        Value::Int(_) | Value::Uint(_) | Value::Float(_) => "a number",
        Value::Name(_) => "a name",
        Value::Flags(_) => "flags",
        Value::Text(_) => "text",
        Value::Guid(_) => "a GUID",
        Value::List(_) => "a list",
        Value::Struct(_) => "a struct",
        Value::Variant(_) => "a variant",
        Value::Bytes(_) => "bytes",
    };
    CodecError::new(format!(
        "{} ({:?}) does not take {given}",
        ty.qualified_name(),
        ty.kind()
    ))
}

fn not_fit(value: impl core::fmt::Display, ty: &Type<'_>) -> CodecError {
    CodecError::new(format!("{value} does not fit {}", ty.qualified_name()))
}

/// `value` in the form `ty` takes when it came in a form a script writes:
/// a constant, GUID or integer as text, flags as a list of names, a variant
/// as `{ type, value }`.
fn coerce<'v>(ty: &Type<'_>, value: &'v Value) -> Result<Cow<'v, Value>, CodecError> {
    let kind = ty.kind();
    Ok(Cow::Owned(match (kind, value) {
        (
            Kind::Enum | Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64,
            Value::Text(text),
        ) if text.parse::<u64>().is_ok() => Value::Uint(text.parse().unwrap_or(0)),
        (Kind::Enum, Value::Text(name)) => Value::Name(name.clone()),
        (
            Kind::Enum | Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64,
            Value::Int(raw),
        ) if *raw >= 0 => Value::Uint(raw.unsigned_abs()),
        (Kind::Struct, Value::List(items)) if items.is_empty() => Value::Struct(Vec::new()),
        (Kind::Guid | Kind::ObjectReference, Value::Text(text)) => Value::Guid(
            Guid::try_parse(text)
                .ok_or_else(|| CodecError::new(format!("{text} is not a GUID")))?,
        ),
        (
            Kind::Uint8
            | Kind::Uint16
            | Kind::Uint32
            | Kind::Uint64
            | Kind::Sint8
            | Kind::Sint16
            | Kind::Sint32
            | Kind::Sint64,
            Value::Text(text),
        ) => match text.parse::<i64>() {
            Ok(i) => Value::Int(i),
            Err(_) => Value::Uint(
                text.parse()
                    .map_err(|_| CodecError::new(format!("{text} is not an integer")))?,
            ),
        },
        (
            Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64,
            Value::List(items),
        ) => Value::Flags(
            items
                .iter()
                .map(|item| match item {
                    Value::Text(name) | Value::Name(name) => Ok(name.clone()),
                    _ => Err(CodecError::new("a flag is a name")),
                })
                .collect::<Result<_, _>>()?,
        ),
        (Kind::BlobVariant, Value::Struct(_)) => {
            match (value.field("type"), value.field("value")) {
                (Some(Value::Text(name)), Some(held)) => {
                    Value::Variant(Box::new(Variant::new(name.clone(), held.clone())))
                }
                _ => return Err(CodecError::new("a variant is { type, value }")),
            }
        }
        _ => return Ok(Cow::Borrowed(value)),
    }))
}

impl<'a> Encoder<'_, 'a> {
    fn write(&mut self, at: usize, data: &[u8]) -> Result<(), CodecError> {
        self.bytes
            .get_mut(at..at + data.len())
            .ok_or_else(|| out_of_range(at))?
            .copy_from_slice(data);
        Ok(())
    }

    fn read(&self, at: usize, len: usize) -> Option<&[u8]> {
        self.bytes.get(at..at.checked_add(len)?)
    }

    /// Appends `len` zero bytes at the next multiple of `align` and returns
    /// where.
    fn append(&mut self, len: usize, align: usize) -> usize {
        let start = self.bytes.len().next_multiple_of(align.max(1));
        self.bytes.resize(start + len, 0);
        start
    }

    /// Points the `i32` offset at `from` to `start`, counted from `origin`.
    fn point(&mut self, from: usize, origin: usize, start: usize) -> Result<(), CodecError> {
        let offset = i32::try_from(start.cast_signed() - origin.cast_signed())
            .map_err(|_| CodecError::new("the resource grew past 2 GiB"))?;
        self.write(from, &offset.to_le_bytes())
    }

    /// Where the data of the header at `at` lies when `len` bytes of it are
    /// inside the bytes.
    fn existing(&self, at: usize, from: usize, len: usize) -> Option<usize> {
        let start = target(&self.bytes, at, from).ok()?;
        self.read(start, len).map(|_| start)
    }

    fn value(
        &mut self,
        ty: &Type<'a>,
        value: &Value,
        at: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        if depth > MAX_DEPTH {
            return Err(CodecError::new("nested deeper than 64"));
        }
        let registry = self.registry;
        let ty = registry.underlying(ty)?;
        let size = size_of(ty);
        if self.read(at, size).is_none() {
            return Err(out_of_range(at));
        }
        let coerced = coerce(ty, value)?;
        let value = coerced.as_ref();
        match (ty.kind(), value) {
            (Kind::BlobArray | Kind::BlobOptional | Kind::BlobVariant, Value::Bytes(_))
                if fresh =>
            {
                // Its offset counts from where it was read; moved, it would
                // point elsewhere.
                Err(CodecError::new(format!(
                    "{} kept as bytes holds an offset from its old place and cannot move",
                    ty.qualified_name()
                )))
            }
            // In place, the header is the element's own, or was rebased when
            // its element moved; the bytes read before would undo that.
            (Kind::BlobArray | Kind::BlobOptional | Kind::BlobVariant, Value::Bytes(_)) => Ok(()),
            (_, Value::Bytes(data)) if ty.kind() != Kind::BlobString => {
                if data.len() != size {
                    return Err(CodecError::new(format!(
                        "{} bytes do not fit {} of {size}",
                        data.len(),
                        ty.qualified_name()
                    )));
                }
                self.write(at, data)
            }
            (Kind::Bool, Value::Bool(b)) => {
                let current = self.read(at, 1).map_or(0, |s| s[0]);
                if fresh || (current != 0) != *b {
                    self.write(at, &[u8::from(*b)])?;
                }
                Ok(())
            }
            (
                Kind::Bool
                | Kind::Uint8
                | Kind::Uint16
                | Kind::Uint32
                | Kind::Uint64
                | Kind::Sint8
                | Kind::Sint16
                | Kind::Sint32
                | Kind::Sint64,
                Value::Int(_) | Value::Uint(_),
            ) => self.integer(ty, value, at),
            (Kind::Float32, Value::Float(_) | Value::Int(_) | Value::Uint(_)) => {
                self.float(value, at, fresh)
            }
            (Kind::Enum, Value::Name(_) | Value::Uint(_)) => self.enumeration(ty, value, at),
            (
                Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64,
                Value::Flags(_) | Value::Uint(_),
            ) => self.flags(ty, value, at),
            (Kind::Struct, Value::Struct(values)) => self.fields(ty, values, at, fresh, depth),
            (Kind::StaticArray, Value::List(items)) => {
                let inner = registry.inner_of(ty)?;
                let count = usize::try_from(ty.element_count().unwrap_or(0)).unwrap_or(0);
                if items.len() != count {
                    return Err(CodecError::new(format!(
                        "{} holds {count} elements, not {}",
                        ty.qualified_name(),
                        items.len()
                    )));
                }
                let stride = size.checked_div(count).unwrap_or(0);
                self.elements(inner, items, at, stride, fresh, depth)
            }
            (Kind::BlobArray, Value::List(items)) => self.array(ty, items, at, fresh, depth),
            (Kind::BlobString, Value::Text(text)) => self.string(text.as_bytes(), at, fresh),
            (Kind::BlobString, Value::Bytes(data)) => self.string(data, at, fresh),
            (Kind::BlobOptional, value) => self.optional(ty, value, at, fresh, depth),
            (Kind::BlobVariant, Value::Nil) => {
                if word(&self.bytes, at)? != 0 {
                    self.write(at, &[0; 12])?;
                }
                Ok(())
            }
            (Kind::BlobVariant, Value::Variant(variant)) => self.variant(variant, at, fresh, depth),
            (Kind::ObjectReference | Kind::Guid, Value::Guid(guid)) if size == 16 => {
                self.write(at, &guid.0)
            }
            _ => Err(mismatch(ty, value)),
        }
    }

    fn float(&mut self, value: &Value, at: usize, fresh: bool) -> Result<(), CodecError> {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a whole number given for a float is as close as f32 holds"
        )]
        let new = match value {
            Value::Int(i) => *i as f64,
            Value::Uint(u) => *u as f64,
            Value::Float(f) => *f,
            _ => 0.0,
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the resource stores a float32"
        )]
        let new = new as f32;
        let current = f32::from_bits(u32::try_from(self.read(at, 4).map_or(0, le)).unwrap_or(0));
        // A NaN given back for a NaN keeps its bytes: the trip through f64
        // may change its payload.
        if !fresh && (current.to_bits() == new.to_bits() || (current.is_nan() && new.is_nan())) {
            return Ok(());
        }
        self.write(at, &new.to_le_bytes())
    }

    fn enumeration(&mut self, ty: &Type<'a>, value: &Value, at: usize) -> Result<(), CodecError> {
        let size = size_of(ty);
        let raw = match value {
            Value::Name(name) => {
                ty.constants()
                    .iter()
                    .find(|c| c.name() == name)
                    .ok_or_else(|| {
                        CodecError::new(format!("{} has no constant {name}", ty.qualified_name()))
                    })?
                    .value()
                    & mask(size)
            }
            Value::Uint(raw) if raw & !mask(size) == 0 => *raw,
            Value::Uint(raw) => return Err(not_fit(raw, ty)),
            _ => return Err(mismatch(ty, value)),
        };
        self.write(at, &le_bytes(raw, size))
    }

    fn fields(
        &mut self,
        ty: &Type<'a>,
        values: &[(String, Value)],
        at: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        let registry = self.registry;
        let fields = registry.struct_fields(ty);
        for (name, value) in values {
            let field = fields.iter().find(|f| f.name() == name).ok_or_else(|| {
                CodecError::new(format!("{} has no field {name}", ty.qualified_name()))
            })?;
            let mut write = || {
                let field_ty = registry.require(
                    field
                        .type_name()
                        .ok_or_else(|| CodecError::new("the field has no reflected type"))?,
                )?;
                let offset = usize::try_from(field.offset()).map_err(|_| out_of_range(at))?;
                self.value(field_ty, value, at + offset, fresh, depth + 1)
            };
            write().map_err(|e| e.within(name))?;
        }
        Ok(())
    }

    /// Where data of `len` bytes goes: in place at `existing`, or appended
    /// at `align` with the `i32` offset at `from` pointed to it; and whether
    /// the place is new.
    fn place(
        &mut self,
        existing: Option<usize>,
        len: usize,
        align: u16,
        from: usize,
    ) -> Result<(usize, bool), CodecError> {
        if let Some(start) = existing {
            return Ok((start, false));
        }
        let start = self.append(len, usize::from(align));
        self.point(from, from, start)?;
        Ok((start, true))
    }

    fn optional(
        &mut self,
        ty: &Type<'a>,
        value: &Value,
        at: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        let set = word(&self.bytes, at)? != 0;
        if *value == Value::Nil {
            if set {
                self.write(at, &[0; 4])?;
            }
            return Ok(());
        }
        let inner = self.registry.inner_of(ty)?;
        let len = size_of(inner);
        let existing = if !fresh && set {
            self.existing(at, at, len)
        } else {
            None
        };
        let (start, fresh) = self.place(existing, len, inner.align(), at)?;
        self.value(inner, value, start, fresh, depth + 1)
    }

    fn variant(
        &mut self,
        variant: &Variant,
        at: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        let held = self.registry.require(variant.type_name())?;
        let len = size_of(held);
        let from = at + VARIANT_OFFSET;
        let existing = if !fresh && word(&self.bytes, at)? == held.qualified_hash() {
            self.existing(from, from, len)
        } else {
            None
        };
        let (start, fresh) = self.place(existing, len, held.align(), from)?;
        if fresh {
            self.write(at, &held.qualified_hash().to_le_bytes())?;
            self.write(at + VARIANT_SIZE, &held.size().to_le_bytes())?;
        }
        self.value(held, variant.value(), start, fresh, depth + 1)
            .map_err(|e| e.within(variant.type_name()))
    }

    fn integer(&mut self, ty: &Type<'a>, value: &Value, at: usize) -> Result<(), CodecError> {
        let width = ty.kind().width().unwrap_or(8);
        let signed = matches!(
            ty.kind(),
            Kind::Sint8 | Kind::Sint16 | Kind::Sint32 | Kind::Sint64
        );
        let number = match value {
            Value::Int(i) => i128::from(*i),
            Value::Uint(u) => i128::from(*u),
            _ => return Err(mismatch(ty, value)),
        };
        let bits = width * 8;
        let (min, max) = if signed {
            (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
        } else {
            (0, (1i128 << bits) - 1)
        };
        if !(min..=max).contains(&number) {
            return Err(not_fit(number, ty));
        }
        let raw = u64::try_from(number & i128::from(u64::MAX)).unwrap_or(0);
        self.write(at, &le_bytes(raw, width))
    }

    fn flags(&mut self, ty: &Type<'a>, value: &Value, at: usize) -> Result<(), CodecError> {
        let width = ty.kind().width().unwrap_or(8);
        let raw = match value {
            Value::Uint(raw) => *raw,
            Value::Flags(names) => {
                let flags = self.registry.inner_of(ty)?;
                let mut raw = 0u64;
                for name in names {
                    let bit = flags
                        .constants()
                        .iter()
                        .find(|c| c.name() == name)
                        .ok_or_else(|| {
                            CodecError::new(format!("{} has no flag {name}", ty.qualified_name()))
                        })?
                        .value();
                    raw |= 1u64
                        .checked_shl(u32::try_from(bit).unwrap_or(u32::MAX))
                        .unwrap_or(0);
                }
                raw
            }
            _ => return Err(mismatch(ty, value)),
        };
        if raw & !mask(width) != 0 {
            return Err(not_fit(raw, ty));
        }
        self.write(at, &le_bytes(raw, width))
    }

    fn elements(
        &mut self,
        inner: &Type<'a>,
        items: &[Value],
        start: usize,
        stride: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        for (i, item) in items.iter().enumerate() {
            self.value(inner, item, start + i * stride, fresh, depth + 1)
                .map_err(|e| e.within(&index(i)))?;
        }
        Ok(())
    }

    fn array(
        &mut self,
        ty: &Type<'a>,
        items: &[Value],
        at: usize,
        fresh: bool,
        depth: usize,
    ) -> Result<(), CodecError> {
        let inner = self.registry.inner_of(ty)?;
        let stride = stride(inner);
        let count = u32::try_from(items.len()).map_err(|_| not_fit(items.len(), ty))?;
        let len = items
            .len()
            .checked_mul(stride)
            .ok_or_else(|| not_fit(items.len(), ty))?;
        let current = word(&self.bytes, at + 4)?;
        let place = if !fresh && current == count {
            if count == 0 {
                return Ok(());
            }
            self.existing(at, at, len)
        } else {
            None
        };
        let (start, kept) = match place {
            Some(start) => (start, items.len()),
            None if count == 0 => {
                self.write(at, &[0; 8])?;
                return Ok(());
            }
            None => {
                // The old elements, when readable, move with their bytes, so
                // fields the type does not reflect keep their values.
                let old_count = usize::try_from(current).unwrap_or(0).min(items.len());
                let old = if fresh {
                    None
                } else {
                    self.existing(at, at, old_count.saturating_mul(stride))
                };
                let align = usize::from(inner.align().max(inner.element_align()));
                let start = self.append(len, align);
                let kept = match old {
                    Some(old_start) => {
                        self.bytes
                            .copy_within(old_start..old_start + old_count * stride, start);
                        for i in 0..old_count {
                            let (from, to) = (old_start + i * stride, start + i * stride);
                            self.rebase(inner, to, from.cast_signed() - to.cast_signed(), depth)?;
                        }
                        old_count
                    }
                    None => 0,
                };
                self.point(at, at, start)?;
                self.write(at + 4, &count.to_le_bytes())?;
                (start, kept)
            }
        };
        for (i, item) in items.iter().enumerate() {
            self.value(inner, item, start + i * stride, i >= kept, depth + 1)
                .map_err(|e| e.within(&index(i)))?;
        }
        Ok(())
    }

    fn string(&mut self, data: &[u8], at: usize, fresh: bool) -> Result<(), CodecError> {
        let count = u32::try_from(data.len()).map_err(|_| CodecError::new("text past 4 GiB"))?;
        let current = word(&self.bytes, at + 4)?;
        if !fresh && current == count {
            if count == 0 {
                return Ok(());
            }
            if let Some(start) = self.existing(at, at, data.len()) {
                // The engine may share one string between headers: a changed
                // one is written fresh, never over the shared bytes.
                if self.read(start, data.len()) == Some(data) {
                    return Ok(());
                }
            }
        }
        if count == 0 {
            return self.write(at, &[0; 8]);
        }
        // The engine's strings end in a NUL byte past their length.
        let start = self.append(data.len() + 1, 1);
        self.write(start, data)?;
        self.point(at, at, start)?;
        self.write(at + 4, &count.to_le_bytes())
    }
}
