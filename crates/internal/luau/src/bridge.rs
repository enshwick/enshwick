//! Values between Luau and the API, shaped by the script [`Ty`] of each.

use std::sync::Weak;

use enshwick::script::{Callback, Error, Signature, Ty, Value};
use mlua::{Lua, MultiValue, Value as LuaValue};

use crate::run::Inner;

/// What a Luau value is called in an error.
fn kind(value: &LuaValue) -> &'static str {
    match value {
        LuaValue::Nil => "nil",
        LuaValue::Boolean(_) => "a boolean",
        LuaValue::Integer(_) | LuaValue::Number(_) => "a number",
        LuaValue::String(_) => "a string",
        LuaValue::Vector(_) => "a vector",
        LuaValue::Table(_) => "a table",
        LuaValue::Function(_) => "a function",
        _ => "another kind of value",
    }
}

fn expected(what: &str, value: &LuaValue) -> Error {
    Error::new(format!("expected {what}, got {}", kind(value)))
}

/// The Luau value of `value`, a `ty`.
pub(crate) fn to_lua(
    lua: &Lua,
    inner: &Weak<Inner>,
    value: Value,
    ty: &'static Ty,
) -> mlua::Result<LuaValue> {
    Ok(match (value, ty) {
        (Value::Bool(b), _) => LuaValue::Boolean(b),
        (Value::Number(n), _) => LuaValue::Number(n),
        (Value::String(s), _) => LuaValue::String(lua.create_string(s)?),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a Luau vector holds three f32"
        )]
        (Value::Vector([x, y, z]), _) => {
            LuaValue::Vector(mlua::Vector::new(x as f32, y as f32, z as f32))
        }
        (Value::List(items), Ty::List(inner_ty)) => {
            let table = lua.create_table_with_capacity(items.len(), 0)?;
            for item in items {
                table.raw_push(to_lua(lua, inner, item, inner_ty)?)?;
            }
            LuaValue::Table(table)
        }
        (Value::Record(fields), Ty::Record(record)) => {
            let table = lua.create_table_with_capacity(0, fields.len())?;
            for (value, field) in fields.into_iter().zip(record.fields()) {
                table.raw_set(field.name(), to_lua(lua, inner, value, field.ty())?)?;
            }
            LuaValue::Table(table)
        }
        (value, Ty::Optional(inner_ty)) => to_lua(lua, inner, value, inner_ty)?,
        (Value::Function(callback), Ty::Function(signature)) => {
            let inner = Weak::clone(inner);
            LuaValue::Function(lua.create_function(move |lua, args: MultiValue| {
                let values = arguments(lua, &inner, args, signature, "function")?;
                let answer = callback.call(values).map_err(mlua::Error::runtime)?;
                to_lua(lua, &inner, answer, signature.ret())
            })?)
        }
        _ => LuaValue::Nil,
    })
}

/// The API value of the Luau value `value`, which is to be a `ty`.
pub(crate) fn from_lua(
    inner: &Weak<Inner>,
    value: LuaValue,
    ty: &'static Ty,
) -> Result<Value, Error> {
    match ty {
        Ty::Nil => Ok(Value::Nil),
        Ty::Bool => match value {
            LuaValue::Boolean(b) => Ok(Value::Bool(b)),
            other => Err(expected("a boolean", &other)),
        },
        Ty::Number => match value {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a Luau integer past 2^53 has no exact f64; the API's numbers are smaller"
            )]
            LuaValue::Integer(i) => Ok(Value::Number(i as f64)),
            LuaValue::Number(n) => Ok(Value::Number(n)),
            other => Err(expected("a number", &other)),
        },
        Ty::String | Ty::Choice(_) => match value {
            LuaValue::String(s) => Ok(Value::String(
                s.to_str()
                    .map_err(|_| Error::new("expected text in UTF-8"))?
                    .to_owned(),
            )),
            other => Err(expected("a string", &other)),
        },
        Ty::Vector => match value {
            LuaValue::Vector(v) => Ok(Value::Vector([v.x(), v.y(), v.z()].map(f64::from))),
            other => Err(expected("a vector", &other)),
        },
        Ty::Optional(inner_ty) => match value {
            LuaValue::Nil => Ok(Value::Nil),
            value => from_lua(inner, value, inner_ty),
        },
        Ty::List(inner_ty) => {
            let LuaValue::Table(table) = value else {
                return Err(expected("a list", &value));
            };
            let mut items = Vec::new();
            for (i, item) in table.sequence_values::<LuaValue>().enumerate() {
                let item = item.map_err(|e| Error::new(e.to_string()))?;
                items.push(
                    from_lua(inner, item, inner_ty)
                        .map_err(|e| Error::new(format!("[{}]: {e}", i + 1)))?,
                );
            }
            Ok(Value::List(items))
        }
        Ty::Record(record) => {
            let LuaValue::Table(table) = value else {
                return Err(expected("a table", &value));
            };
            record
                .fields()
                .iter()
                .map(|field| {
                    let value: LuaValue = table
                        .raw_get(field.name())
                        .map_err(|e| Error::new(e.to_string()))?;
                    from_lua(inner, value, field.ty())
                        .map_err(|e| Error::new(format!("{}: {e}", field.name())))
                })
                .collect::<Result<_, _>>()
                .map(Value::Record)
        }
        Ty::Function(signature) => {
            let LuaValue::Function(function) = value else {
                return Err(expected("a function", &value));
            };
            let runtime = inner
                .upgrade()
                .ok_or_else(|| Error::new("the mod has stopped"))?;
            Ok(Value::Function(Callback::new(move |args| {
                runtime.call_function(&function, args, signature)
            })))
        }
        _ => Err(Error::new("a type this runtime does not know")),
    }
}

/// The API values of the Luau arguments `args` of a call to `what`, one per
/// parameter of `signature`.
pub(crate) fn arguments(
    _lua: &Lua,
    inner: &Weak<Inner>,
    args: MultiValue,
    signature: &'static Signature,
    what: &str,
) -> mlua::Result<Vec<Value>> {
    let params = signature.params();
    if args.len() > params.len() {
        return Err(mlua::Error::runtime(format!(
            "{what} takes {} arguments, not {}",
            params.len(),
            args.len()
        )));
    }
    let mut args = args.into_iter();
    params
        .iter()
        .map(|param| {
            let value = args.next().unwrap_or(LuaValue::Nil);
            from_lua(inner, value, param.ty()).map_err(|e| {
                let name = if param.name().is_empty() {
                    "argument"
                } else {
                    param.name()
                };
                mlua::Error::runtime(format!("{what}: {name}: {e}"))
            })
        })
        .collect()
}
