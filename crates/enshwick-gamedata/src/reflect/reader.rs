//! Reads type descriptors and system records from an image.

use std::ops::Range;

use super::{
    ATTRIBUTE_NAME, ATTRIBUTE_RECORD, ATTRIBUTE_TYPE, ATTRIBUTE_VALUE, Attribute,
    DESCRIPTOR_ATTRIBUTES, DESCRIPTOR_ELEMENT_ALIGN, DESCRIPTOR_ENUM_VALUES,
    DESCRIPTOR_FIELD_COUNT, DESCRIPTOR_FIELDS, DESCRIPTOR_FLAGS, DESCRIPTOR_FULL_NAME,
    DESCRIPTOR_HASH, DESCRIPTOR_IMPACT_NAME, DESCRIPTOR_INNER, DESCRIPTOR_INTERNAL_HASH,
    DESCRIPTOR_KIND, DESCRIPTOR_SHORT_NAME, DESCRIPTOR_SIZE_ALIGN, ENUM_RECORD, ENUM_VALUE,
    EnumValue, FIELD_ATTRIBUTES, FIELD_OFFSET, FIELD_RECORD, FIELD_TYPE, Field, Image, Kind,
    MAX_ATTRIBUTES, MAX_FIELDS, MAX_LIST, MAX_NAME, SLICE, SYSTEM_ENTRY, SYSTEM_LISTS, System,
    TypeInfo, fnv1a32,
};

pub(super) struct Reader<'i, 'a> {
    pub(super) image: &'i Image<'a>,
    pub(super) rdata: Range<u64>,
}

impl<'a> Reader<'_, 'a> {
    /// The string slice at `va`: a pointer into `.rdata` and a length, the
    /// bytes NUL-terminated right at the length.
    pub(super) fn slice(&self, va: u64) -> Option<&'a str> {
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

    pub(super) fn descriptor(&self, va: u64) -> Option<TypeInfo<'a>> {
        let at = |offset: u64| va.checked_add(offset);
        let short_name = self.slice(at(DESCRIPTOR_SHORT_NAME)?)?;
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
        let byte = |offset: u64| {
            at(offset)
                .and_then(|slot| self.image.read(slot, 1))
                .and_then(|bytes| bytes.first().copied())
        };
        Some(TypeInfo {
            va,
            name,
            short_name,
            impact_name: at(DESCRIPTOR_IMPACT_NAME)
                .and_then(|slot| self.slice(slot))
                .unwrap_or(short_name),
            size,
            align,
            element_align: at(DESCRIPTOR_ELEMENT_ALIGN)
                .and_then(|slot| self.image.u16(slot))
                .unwrap_or(align),
            kind,
            flags: byte(DESCRIPTOR_FLAGS).unwrap_or(0),
            internal_hash: at(DESCRIPTOR_INTERNAL_HASH)
                .and_then(|slot| self.image.u32(slot))
                .unwrap_or(0),
            base,
            inner,
            count,
            fields,
            enum_values,
            attributes: at(DESCRIPTOR_ATTRIBUTES)
                .map_or_else(Vec::new, |slot| self.attributes(slot)),
        })
    }

    /// The attributes whose pointer and count are at `slot`; an unreadable
    /// record is left out.
    fn attributes(&self, slot: u64) -> Vec<Attribute<'a>> {
        let (Some(table), Some(count)) = (
            self.image.u64(slot),
            slot.checked_add(8).and_then(|at| self.image.u64(at)),
        ) else {
            return Vec::new();
        };
        if table == 0 || count == 0 || count > MAX_ATTRIBUTES || !self.rdata.contains(&table) {
            return Vec::new();
        }
        (0..count)
            .filter_map(|i| {
                let record = table.checked_add(i * ATTRIBUTE_RECORD)?;
                let names = self.image.u64(record)?;
                let name = self.slice(names.checked_add(ATTRIBUTE_NAME)?)?;
                let value = self
                    .slice(record.checked_add(ATTRIBUTE_VALUE)?)
                    .unwrap_or("");
                let attribute = Attribute::new(name, value);
                Some(
                    match names
                        .checked_add(ATTRIBUTE_TYPE)
                        .and_then(|at| self.image.u64(at))
                        .and_then(|ptr| self.type_name(ptr))
                    {
                        Some(value_type) => attribute.with_value_type(value_type),
                        None => attribute,
                    },
                )
            })
            .collect()
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
                    attributes: record
                        .checked_add(FIELD_ATTRIBUTES)
                        .map_or_else(Vec::new, |slot| self.attributes(slot)),
                })
            })
            .collect()
    }

    pub(super) fn system(&self, va: u64, name: &'a str, text: &Range<u64>) -> Option<System<'a>> {
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
