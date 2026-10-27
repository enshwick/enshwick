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
            _ => return None,
        })
    }

    const fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 | Self::Bool => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
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

/// The [`Read`] the description value `value` gives, for a value of `size`
/// bytes at most.
fn read_of(value: &LuaValue, depth: u32) -> mlua::Result<Read> {
    if depth > 16 {
        return Err(bad("types nest too deep"));
    }
    match value {
        LuaValue::String(name) => {
            let name = name.to_str()?;
            Primitive::parse(&name)
                .map(Read::Primitive)
                .ok_or_else(|| bad(format!("no primitive {name:?}")))
        }
        LuaValue::Table(table) => {
            let fields: Table = table.get("fields")?;
            Ok(Read::Struct(fields_of(&fields, depth + 1)?))
        }
        _ => Err(bad("a read is a name or a struct")),
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
fn size_of_read(read: &Read) -> usize {
    match read {
        Read::Primitive(p) => p.size(),
        Read::Struct(fields) => fields
            .iter()
            .map(|(_, offset, read)| offset + size_of_read(read))
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
            if offset + size_of_read(&read) > usize::try_from(input_size).unwrap_or(0) {
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
                table.raw_set(name.as_str(), decode(lua, bytes, offset + at, inner)?)?;
            }
            Ok(LuaValue::Table(table))
        }
        Read::Primitive(p) => {
            let Some(raw) = bytes.get(offset..offset + p.size()) else {
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
                Primitive::U32 => LuaValue::Number(f64::from(u32::from_le_bytes([
                    raw[0], raw[1], raw[2], raw[3],
                ]))),
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
    let wrong = |what: &str| mlua::Error::runtime(format!("the answer: expected {what}"));
    match read {
        Read::Struct(fields) => {
            let LuaValue::Table(table) = value else {
                return Err(wrong("a table"));
            };
            for (name, at, inner) in fields {
                encode(&table.raw_get(name.as_str())?, inner, bytes, offset + at)?;
            }
            Ok(())
        }
        Read::Primitive(p) => {
            let out = bytes
                .get_mut(offset..offset + p.size())
                .ok_or_else(|| wrong("a value that fits the answer"))?;
            encode_primitive(value, *p, out)
        }
    }
}

/// The Luau value `value` written as the primitive `p` into `out`, its
/// bytes.
fn encode_primitive(value: &LuaValue, p: Primitive, out: &mut [u8]) -> mlua::Result<()> {
    let wrong = |what: &str| mlua::Error::runtime(format!("the answer: expected {what}"));
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
        Primitive::U32 => out.copy_from_slice(
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
            if text.len() != 32 {
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
