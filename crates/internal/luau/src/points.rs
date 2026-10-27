//! Another mod's extension point as a script links it: the description
//! `cargo enshwick point --luau` writes, read into the layout the loader
//! checks, and the input and answer between bytes and Luau values.

use core::fmt::Write as _;

use enshwick::points::{DescribedField, DescribedPoint, Mode};
use mlua::{Lua, Table, Value as LuaValue};

/// How one value is read from and written to the owner's bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Read {
    /// A primitive, by its schema name.
    Primitive(Primitive),
    /// A struct: its size and each field at its offset.
    Struct(Vec<(String, usize, Read)>),
    /// An enum: its integer and the values it has, the only ones written.
    Enum(Primitive, Vec<i64>),
}

/// The primitives a point's values are made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Primitive {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    Bool,
    Guid,
    Entity,
}

impl Primitive {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "u8" => Self::U8,
            "i8" => Self::I8,
            "u16" => Self::U16,
            "i16" => Self::I16,
            "u32" => Self::U32,
            "i32" => Self::I32,
            "u64" => Self::U64,
            "i64" => Self::I64,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "bool" => Self::Bool,
            "guid" => Self::Guid,
            "entity" => Self::Entity,
            _ => return None,
        })
    }

    pub(crate) const fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 | Self::Bool => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 | Self::Entity => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
            Self::Guid => 16,
        }
    }
}

/// A point a script links, with how its input and answer read.
#[derive(Clone, Debug)]
pub(crate) struct PointDesc {
    pub(crate) point: DescribedPoint,
    pub(crate) input: Vec<(String, usize, Read)>,
    pub(crate) output: Read,
}

fn bad(why: impl core::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!(
        "enshwick.extend: the point's description: {why}; write it again with cargo enshwick point --luau"
    ))
}

/// Why a description's read does not read, for a point, an event or a call
/// alike.
fn described(why: impl core::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!(
        "the description: {why}; write it again with the cargo enshwick command that wrote it"
    ))
}

/// The [`Read`] the description value `value` gives, for a value of `size`
/// bytes at most.
pub(crate) fn read_of(value: &LuaValue, depth: u32) -> mlua::Result<Read> {
    if depth > 16 {
        return Err(described("types nest too deep"));
    }
    match value {
        LuaValue::String(name) => {
            let name = name.to_str()?;
            Primitive::parse(&name)
                .map(Read::Primitive)
                .ok_or_else(|| described(format!("no primitive {name:?}")))
        }
        LuaValue::Table(table) => {
            if let Some(variants) = table.get::<Option<Vec<i64>>>("variants")? {
                let Read::Primitive(repr) = read_of(&table.get("repr")?, depth + 1)? else {
                    return Err(described("an enum's repr is an integer"));
                };
                return Ok(Read::Enum(repr, variants));
            }
            let fields: Table = table.get("fields")?;
            Ok(Read::Struct(fields_of(&fields, depth + 1)?))
        }
        _ => Err(described("a read is a name or a struct")),
    }
}

/// The fields of a description's `fields` list.
fn fields_of(fields: &Table, depth: u32) -> mlua::Result<Vec<(String, usize, Read)>> {
    let mut out = Vec::new();
    for field in fields.sequence_values::<Table>() {
        let field = field?;
        let name: String = field.get("name")?;
        let offset: usize = field.get("offset")?;
        let read = read_of(&field.get("read")?, depth)?;
        out.push((name, offset, read));
    }
    Ok(out)
}

/// The size of a value read as `read`, from its fields' offsets.
pub(crate) fn size_of_read(read: &Read) -> usize {
    match read {
        Read::Primitive(p) | Read::Enum(p, _) => p.size(),
        Read::Struct(fields) => fields
            .iter()
            .map(|(_, offset, read)| offset.saturating_add(size_of_read(read)))
            .max()
            .unwrap_or(0),
    }
}

impl PointDesc {
    /// The description `cargo enshwick point --luau` writes, read from the
    /// table a script passes.
    pub(crate) fn parse(table: &Table) -> mlua::Result<Self> {
        let name: String = table.get("name")?;
        let mode = match table.get::<String>("mode")?.as_str() {
            "observe" => Mode::Observe,
            "advise" => Mode::Advise,
            "override" => Mode::Override,
            other => return Err(bad(format!("no mode {other:?}"))),
        };
        let refusable: bool = table.get("refusable")?;
        let input: Table = table.get("input")?;
        let input_size: u32 = input.get("size")?;
        let input_list: Table = input.get("fields")?;
        let mut described = Vec::new();
        let mut fields = Vec::new();
        for field in input_list.sequence_values::<Table>() {
            let field = field?;
            let field_name: String = field.get("name")?;
            let hash: String = field.get("hash")?;
            let hash: u64 = hash
                .parse()
                .map_err(|_| bad("a layout hash is no number"))?;
            let offset: usize = field.get("offset")?;
            let read = read_of(&field.get("read")?, 0)?;
            if offset.saturating_add(size_of_read(&read)) > usize::try_from(input_size).unwrap_or(0)
            {
                return Err(bad(format!("{field_name} lies past the input's size")));
            }
            described.push(DescribedField::new(&field_name, hash));
            fields.push((field_name, offset, read));
        }
        let output: Table = table.get("output")?;
        let output_size: u32 = output.get("size")?;
        let output_hash: String = output.get("hash")?;
        let output_hash: u64 = output_hash
            .parse()
            .map_err(|_| bad("a layout hash is no number"))?;
        let output_read = read_of(&output.get("read")?, 0)?;
        if mode.changes() {
            let verified = matches!(&output_read, Read::Primitive(p)
                if usize::try_from(output_size).is_ok_and(|size| size == p.size())
                    && enshwick::services::layout_hash("output", p.shape(), p.size(), p.align(), 0) == output_hash);
            if !verified {
                return Err(bad(
                    "a script changes only an answer of a primitive type, whose layout hash it checks",
                ));
            }
        }
        if size_of_read(&output_read) > usize::try_from(output_size).unwrap_or(0) {
            return Err(bad("the answer lies past its size"));
        }
        let point = DescribedPoint::new(&name, mode)
            .refusable(refusable)
            .with_input(described, input_size)
            .with_output(output_hash, output_size);
        Ok(Self {
            point,
            input: fields,
            output: output_read,
        })
    }
}

/// `bytes` at `offset` read as `read`, as a Luau value; nil past the end.
pub(crate) fn decode(
    lua: &Lua,
    bytes: &[u8],
    offset: usize,
    read: &Read,
) -> mlua::Result<LuaValue> {
    match read {
        Read::Struct(fields) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (name, at, inner) in fields {
                let value = match offset.checked_add(*at) {
                    Some(at) => decode(lua, bytes, at, inner)?,
                    None => LuaValue::Nil,
                };
                table.raw_set(name.as_str(), value)?;
            }
            Ok(LuaValue::Table(table))
        }
        Read::Enum(p, _) => decode(lua, bytes, offset, &Read::Primitive(*p)),
        Read::Primitive(p) => {
            let Some(raw) = offset
                .checked_add(p.size())
                .and_then(|end| bytes.get(offset..end))
            else {
                return Ok(LuaValue::Nil);
            };
            let mut word = [0u8; 16];
            word[..raw.len()].copy_from_slice(raw);
            let le8 = |w: &[u8; 16]| {
                let mut b = [0u8; 8];
                b.copy_from_slice(&w[..8]);
                b
            };
            Ok(match p {
                Primitive::U8 => LuaValue::Number(f64::from(raw[0])),
                Primitive::I8 => LuaValue::Number(f64::from(i8::from_le_bytes([raw[0]]))),
                Primitive::U16 => LuaValue::Number(f64::from(u16::from_le_bytes([raw[0], raw[1]]))),
                Primitive::I16 => LuaValue::Number(f64::from(i16::from_le_bytes([raw[0], raw[1]]))),
                Primitive::U32 | Primitive::Entity => {
                    LuaValue::Number(f64::from(u32::from_le_bytes([
                        raw[0], raw[1], raw[2], raw[3],
                    ])))
                }
                Primitive::I32 => LuaValue::Number(f64::from(i32::from_le_bytes([
                    raw[0], raw[1], raw[2], raw[3],
                ]))),
                Primitive::F32 => LuaValue::Number(f64::from(f32::from_le_bytes([
                    raw[0], raw[1], raw[2], raw[3],
                ]))),
                Primitive::U64 => {
                    LuaValue::String(lua.create_string(u64::from_le_bytes(le8(&word)).to_string())?)
                }
                Primitive::I64 => {
                    LuaValue::String(lua.create_string(i64::from_le_bytes(le8(&word)).to_string())?)
                }
                Primitive::F64 => LuaValue::Number(f64::from_le_bytes(le8(&word))),
                Primitive::Bool => LuaValue::Boolean(raw[0] != 0),
                Primitive::Guid => {
                    let hex = raw.iter().fold(String::with_capacity(32), |mut hex, b| {
                        let _written = write!(hex, "{b:02x}");
                        hex
                    });
                    LuaValue::String(lua.create_string(hex)?)
                }
            })
        }
    }
}

/// The Luau value `value` written as `read` into `bytes` at `offset`.
pub(crate) fn encode(
    value: &LuaValue,
    read: &Read,
    bytes: &mut [u8],
    offset: usize,
) -> mlua::Result<()> {
    let wrong = |what: &str| mlua::Error::runtime(format!("expected {what}"));
    match read {
        Read::Struct(fields) => {
            let LuaValue::Table(table) = value else {
                return Err(wrong("a table"));
            };
            for (name, at, inner) in fields {
                let at = offset
                    .checked_add(*at)
                    .ok_or_else(|| wrong("a value that fits its type"))?;
                encode(&table.raw_get(name.as_str())?, inner, bytes, at)?;
            }
            Ok(())
        }
        Read::Primitive(p) => {
            let out = bytes
                .get_mut(offset..offset.saturating_add(p.size()))
                .ok_or_else(|| wrong("a value that fits its type"))?;
            encode_primitive(value, *p, out)
        }
        Read::Enum(p, variants) => {
            let listed = match value {
                LuaValue::Integer(i) => variants.contains(i),
                #[expect(clippy::cast_possible_truncation, reason = "compared only when whole")]
                LuaValue::Number(n) => n.fract() == 0.0 && variants.contains(&(*n as i64)),
                _ => false,
            };
            if !listed {
                return Err(wrong("one of the enum's values"));
            }
            let out = bytes
                .get_mut(offset..offset.saturating_add(p.size()))
                .ok_or_else(|| wrong("a value that fits its type"))?;
            encode_primitive(value, *p, out)
        }
    }
}

/// The Luau value `value` written as the primitive `p` into `out`, its
/// bytes.
fn encode_primitive(value: &LuaValue, p: Primitive, out: &mut [u8]) -> mlua::Result<()> {
    let wrong = |what: &str| mlua::Error::runtime(format!("expected {what}"));
    let number = || match value {
        #[expect(
            clippy::cast_precision_loss,
            reason = "an answer past 2^53 has no exact f64; the range check below refuses it"
        )]
        LuaValue::Integer(i) => Ok(*i as f64),
        LuaValue::Number(n) => Ok(*n),
        _ => Err(wrong("a number")),
    };
    let whole = |min: f64, max: f64| -> mlua::Result<i128> {
        let n = number()?;
        if n.fract() != 0.0 || n < min || n > max {
            return Err(wrong(&format!("a whole number from {min} to {max}")));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked whole and in range above"
        )]
        Ok(n as i128)
    };
    let digits = |text: &LuaValue| -> mlua::Result<String> {
        match text {
            LuaValue::String(s) => Ok(s.to_str()?.to_owned()),
            _ => Err(wrong("a string of digits")),
        }
    };
    match p {
        Primitive::U8 => {
            out.copy_from_slice(&u8::try_from(whole(0.0, 255.0)?).unwrap_or(0).to_le_bytes());
        }
        Primitive::I8 => out.copy_from_slice(
            &i8::try_from(whole(-128.0, 127.0)?)
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Primitive::U16 => out.copy_from_slice(
            &u16::try_from(whole(0.0, 65535.0)?)
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Primitive::I16 => out.copy_from_slice(
            &i16::try_from(whole(-32768.0, 32767.0)?)
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Primitive::U32 | Primitive::Entity => out.copy_from_slice(
            &u32::try_from(whole(0.0, f64::from(u32::MAX))?)
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        Primitive::I32 => out.copy_from_slice(
            &i32::try_from(whole(f64::from(i32::MIN), f64::from(i32::MAX))?)
                .unwrap_or(0)
                .to_le_bytes(),
        ),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an f32 answer keeps what it can"
        )]
        Primitive::F32 => out.copy_from_slice(&(number()? as f32).to_le_bytes()),
        Primitive::F64 => out.copy_from_slice(&number()?.to_le_bytes()),
        Primitive::U64 => {
            let n: u64 = digits(value)?.parse().map_err(|_| wrong("a 64-bit key"))?;
            out.copy_from_slice(&n.to_le_bytes());
        }
        Primitive::I64 => {
            let n: i64 = digits(value)?
                .parse()
                .map_err(|_| wrong("a 64-bit number"))?;
            out.copy_from_slice(&n.to_le_bytes());
        }
        Primitive::Bool => match value {
            LuaValue::Boolean(b) => out[0] = u8::from(*b),
            _ => return Err(wrong("a boolean")),
        },
        Primitive::Guid => {
            let text = digits(value)?;
            if text.len() != 32 || !text.is_ascii() {
                return Err(wrong("32 hex digits"));
            }
            for (i, byte) in out.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&text[i * 2..i * 2 + 2], 16)
                    .map_err(|_| wrong("32 hex digits"))?;
            }
        }
    }
    Ok(())
}

impl Primitive {
    /// The type's shape hash, as `#[derive(Shape)]` gives it in Rust.
    pub(crate) const fn shape(self) -> u64 {
        use enshwick::Shape;
        match self {
            Self::U8 => u8::SHAPE,
            Self::I8 => i8::SHAPE,
            Self::U16 => u16::SHAPE,
            Self::I16 => i16::SHAPE,
            Self::U32 => u32::SHAPE,
            Self::I32 => i32::SHAPE,
            Self::U64 => u64::SHAPE,
            Self::I64 => i64::SHAPE,
            Self::F32 => f32::SHAPE,
            Self::F64 => f64::SHAPE,
            Self::Bool => bool::SHAPE,
            Self::Guid => enshwick::content::Guid::SHAPE,
            Self::Entity => enshwick::ecs::Entity::SHAPE,
        }
    }

    /// The type's alignment.
    pub(crate) const fn align(self) -> usize {
        match self {
            Self::Guid => 1,
            other => other.size(),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn primitive() -> impl Strategy<Value = Primitive> {
        prop::sample::select(vec![
            Primitive::U8,
            Primitive::I8,
            Primitive::U16,
            Primitive::I16,
            Primitive::U32,
            Primitive::I32,
            Primitive::U64,
            Primitive::I64,
            Primitive::F32,
            Primitive::F64,
            Primitive::Bool,
            Primitive::Guid,
            Primitive::Entity,
        ])
    }

    fn read() -> impl Strategy<Value = Read> {
        let leaf = prop_oneof![
            primitive().prop_map(Read::Primitive),
            (primitive(), prop::collection::vec(any::<i64>(), 0..4))
                .prop_map(|(p, v)| Read::Enum(p, v)),
        ];
        leaf.prop_recursive(3, 16, 4, |inner| {
            prop::collection::vec(("[a-z]{1,4}", any::<usize>(), inner), 0..4)
                .prop_map(Read::Struct)
        })
    }

    proptest! {
        #[test]
        fn decoding_any_layout_over_any_bytes_never_panics(
            read in read(),
            offset in any::<usize>(),
            bytes in prop::collection::vec(any::<u8>(), 0..64),
        ) {
            let lua = Lua::new();
            let _value = decode(&lua, &bytes, offset, &read);
        }

        #[test]
        fn encoding_any_value_into_any_layout_never_panics(
            read in read(),
            offset in any::<usize>(),
            size in 0usize..64,
            number in any::<f64>(),
            text in ".{0,40}",
        ) {
            let lua = Lua::new();
            let mut bytes = vec![0u8; size];
            for value in [
                LuaValue::Number(number),
                LuaValue::String(lua.create_string(&text).unwrap()),
                LuaValue::Boolean(true),
                LuaValue::Nil,
            ] {
                let _written = encode(&value, &read, &mut bytes, offset);
            }
        }

        #[test]
        fn a_u32_written_reads_back(value in any::<u32>()) {
            let lua = Lua::new();
            let mut bytes = [0u8; 4];
            encode(&LuaValue::Number(f64::from(value)), &Read::Primitive(Primitive::U32), &mut bytes, 0).unwrap();
            let read = decode(&lua, &bytes, 0, &Read::Primitive(Primitive::U32)).unwrap();
            prop_assert_eq!(read.as_f64(), Some(f64::from(value)));
        }
    }

    #[test]
    fn an_enum_takes_only_its_variants() {
        let mut bytes = [0u8; 1];
        let read = Read::Enum(Primitive::U8, vec![0, 2]);
        assert!(encode(&LuaValue::Number(2.0), &read, &mut bytes, 0).is_ok());
        assert!(encode(&LuaValue::Number(1.0), &read, &mut bytes, 0).is_err());
    }

    fn description(lua: &Lua, mode: &str, read: &str, hash: u64) -> Table {
        lua.load(format!(
            r#"return {{ name = "you.post:price", mode = "{mode}", refusable = false,
                input = {{ size = 4, fields = {{ {{ name = "item", offset = 0, read = "u32", hash = "1" }} }} }},
                output = {{ size = 4, hash = "{hash}", read = "{read}" }} }}"#
        ))
        .eval()
        .unwrap()
    }

    #[test]
    fn a_changing_link_needs_an_answer_whose_hash_it_checks() {
        let lua = Lua::new();
        let right = enshwick::services::layout_hash("output", Primitive::U32.shape(), 4, 4, 0);
        assert!(PointDesc::parse(&description(&lua, "override", "u32", right)).is_ok());
        assert!(PointDesc::parse(&description(&lua, "override", "u32", right ^ 1)).is_err());
        assert!(PointDesc::parse(&description(&lua, "override", "u8", right)).is_err());
        assert!(PointDesc::parse(&description(&lua, "observe", "u8", 7)).is_ok());
    }
}

/// A mod event as a script subscribes to or defines it, with how its data
/// reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EventDesc {
    pub(crate) event: enshwick::services::DescribedEvent,
    pub(crate) fields: Vec<(String, usize, Read)>,
}

impl EventDesc {
    /// The description `cargo enshwick event` writes, or one a script writes
    /// for its own event: fields without `offset` and `hash` are laid out
    /// as `repr(C)` and hashed as Rust does for primitives.
    ///
    /// For an event the script defines, every field reads a primitive and
    /// its hash is the one worked out here, so a script cannot claim the
    /// hash of another type and send bytes that type forbids.
    pub(crate) fn parse_definition(table: &Table) -> mlua::Result<Self> {
        let desc = Self::parse(table)?;
        for ((name, offset, read), field) in desc.fields.iter().zip(&desc.event.fields) {
            let Read::Primitive(p) = read else {
                return Err(bad(format!(
                    "{name}: an event a script defines has primitive fields"
                )));
            };
            let hash =
                enshwick::services::layout_hash(name, p.shape(), p.size(), p.align(), *offset);
            if hash != field.hash {
                return Err(bad(format!(
                    "{name}: its hash is not the one its read and offset give"
                )));
            }
        }
        Ok(desc)
    }

    /// The description `cargo enshwick event` writes, or one a script writes
    /// for its own event, read as [`EventDesc::parse_definition`] says.
    pub(crate) fn parse(table: &Table) -> mlua::Result<Self> {
        let name: String = table.get("name")?;
        let list: Table = table.get("fields")?;
        let mut described = Vec::new();
        let mut fields = Vec::new();
        let mut end = 0usize;
        let mut align = 1usize;
        let mut by_hand = None;
        for field in list.sequence_values::<Table>() {
            let field = field?;
            let field_name: String = field.get("name")?;
            let read = read_of(&field.get("read")?, 0)?;
            let offset: Option<usize> = field.get("offset")?;
            let hash: Option<String> = field.get("hash")?;
            let explicit = offset.is_some() || hash.is_some();
            if *by_hand.get_or_insert(explicit) != explicit {
                return Err(bad(
                    "give every field an offset and a hash, or none of them",
                ));
            }
            let (offset, hash) = match (offset, hash, &read) {
                (Some(offset), Some(hash), _) => {
                    let hash = hash
                        .parse()
                        .map_err(|_| bad("a layout hash is no number"))?;
                    (offset, hash)
                }
                (None, None, Read::Primitive(p)) => {
                    let offset = end.div_ceil(p.align()) * p.align();
                    align = align.max(p.align());
                    let hash = enshwick::services::layout_hash(
                        &field_name,
                        p.shape(),
                        p.size(),
                        p.align(),
                        offset,
                    );
                    (offset, hash)
                }
                _ => {
                    return Err(bad(format!(
                        "{field_name}: give offset and hash together, or neither for a primitive"
                    )));
                }
            };
            end = end.max(offset.saturating_add(size_of_read(&read)));
            described.push(DescribedField::new(&field_name, hash));
            fields.push((field_name, offset, read));
        }
        let size: Option<u32> = table.get("size")?;
        let size = match size {
            Some(size) => size,
            None => u32::try_from(end.div_ceil(align) * align)
                .map_err(|_| bad("the data is too large"))?,
        };
        if end > usize::try_from(size).unwrap_or(0) {
            return Err(bad("a field lies past the data's size"));
        }
        Ok(Self {
            event: enshwick::services::DescribedEvent::new(&name, described, size),
            fields,
        })
    }
}
