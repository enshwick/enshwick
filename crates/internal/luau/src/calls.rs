//! Calls a script makes to another mod: the description `cargo enshwick
//! call --luau` writes, checked so a script never hands a server bytes its
//! types forbid, and the function a script calls.

use std::sync::{Arc, OnceLock};

use enshwick::points::DescribedField;
use enshwick::services::{CallError, DescribedCall, DescribedCaller};
use mlua::{Lua, Table, Value as LuaValue};

use crate::points::{Read, decode, encode, read_of, size_of_read};

/// Why a call's description does not read.
fn bad(why: impl core::fmt::Display) -> mlua::Error {
    mlua::Error::runtime(format!(
        "enshwick.caller: the call's description: {why}; write it again with cargo enshwick call --luau"
    ))
}

/// A call as a script's description gives it: the loader's description,
/// and how the input and the answer read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallDesc {
    pub(crate) call: DescribedCall,
    input: Read,
    output: Read,
}

impl CallDesc {
    /// The description `cargo enshwick call --luau` writes, read from the
    /// table a script passes.
    ///
    /// Every input field reads a primitive and its hash is the one worked
    /// out here from its name, primitive and offset, so a script cannot
    /// claim the hash of another type and send bytes that type forbids. A
    /// value type or an enum is refused: its hash would come from the
    /// script's own text. The answer is only read, so it may be of any type
    /// the description reads.
    pub(crate) fn parse(table: &Table) -> mlua::Result<Self> {
        let name: String = table.get("name")?;
        let major: u32 = table.get("major")?;
        let input: Table = table.get("input")?;
        let input_size: u32 = input.get("size")?;
        let limit = usize::try_from(input_size).unwrap_or(0);
        let mut described = Vec::new();
        let mut fields = Vec::new();
        for field in input.get::<Table>("fields")?.sequence_values::<Table>() {
            let field = field?;
            let field_name: String = field.get("name")?;
            let offset: usize = field.get("offset")?;
            let read = read_of(&field.get("read")?, 0)?;
            let shaped = field.contains_key("shape")?;
            let (Read::Primitive(p), false) = (&read, shaped) else {
                return Err(bad(format!(
                    "{field_name}: a script calls only with numbers, booleans, entities and GUIDs in the input"
                )));
            };
            let p = *p;
            let shape = p.shape();
            let given: String = field.get("hash")?;
            let hash =
                enshwick::services::layout_hash(&field_name, shape, p.size(), p.align(), offset);
            if given.parse::<u64>().ok() != Some(hash) {
                return Err(bad(format!(
                    "{field_name}: its hash is not the one its read and offset give"
                )));
            }
            if offset.saturating_add(p.size()) > limit {
                return Err(bad(format!("{field_name} lies past the input's size")));
            }
            described.push(DescribedField::new(&field_name, hash));
            fields.push((field_name, offset, read));
        }
        let output: Table = table.get("output")?;
        let output_size: u32 = output.get("size")?;
        let output_hash: u64 = output
            .get::<String>("hash")?
            .parse()
            .map_err(|_| bad("a layout hash is no number"))?;
        let output_read = read_of(&output.get("read")?, 0)?;
        if size_of_read(&output_read) > usize::try_from(output_size).unwrap_or(0) {
            return Err(bad("the answer lies past its size"));
        }
        Ok(Self {
            call: DescribedCall::new(
                &name,
                major,
                (described, input_size),
                (
                    vec![DescribedField::new("output", output_hash)],
                    output_size,
                ),
            ),
            input: Read::Struct(fields),
            output: output_read,
        })
    }

    /// The function a script calls: it encodes the input table, runs the
    /// call once the loader gave `slot` its caller, and returns the answer,
    /// or nil and why not.
    pub(crate) fn function(
        &self,
        lua: &Lua,
        slot: Arc<OnceLock<DescribedCaller>>,
    ) -> mlua::Result<mlua::Function> {
        let input = self.input.clone();
        let output = self.output.clone();
        let input_size = usize::try_from(self.call.input_size).unwrap_or(0);
        let output_size = usize::try_from(self.call.output_size).unwrap_or(0);
        lua.create_function(move |lua, data: LuaValue| {
            let mut bytes = vec![0u8; input_size];
            encode(&data, &input, &mut bytes, 0)?;
            let Some(caller) = slot.get() else {
                return Ok((LuaValue::Nil, Some("unmatched")));
            };
            let mut answer = vec![0u8; output_size];
            match caller.call(&bytes, &mut answer) {
                Ok(()) => Ok((decode(lua, &answer, 0, &output)?, None)),
                Err(error) => Ok((LuaValue::Nil, Some(why(error)))),
            }
        })
    }
}

/// A call's error, as a script reads it.
fn why(error: CallError) -> &'static str {
    match error {
        CallError::Unmatched => "unmatched",
        CallError::Off => "off",
        _ => "failed",
    }
}
