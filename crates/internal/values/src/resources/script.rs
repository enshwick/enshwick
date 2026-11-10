//! Resource values as script data.
//!
//! A value crosses as a plain table: a struct by field name, an array as a
//! list, an enum constant or a GUID as text, a bitmask as the list of its
//! flags' names, a variant as `{ type = <qualified name>, value = ... }`,
//! raw bytes as a `buffer`, an absent optional or variant as an absent
//! field. An integer past 2^53, which a script number cannot hold exactly,
//! crosses as its decimal text; text is taken back for any integer.

use super::{Value as Data, Variant};
use crate::script::convert::EXACT;
use crate::script::{Answer, Error, FromScript, IntoScript, Ty, Typed, Value};

/// `number`, or `text` when its `magnitude` is past what a script number
/// holds exactly.
#[doc(hidden)]
pub fn integer(magnitude: u64, text: impl FnOnce() -> String, number: f64) -> Value {
    if magnitude <= EXACT {
        Value::Number(number)
    } else {
        Value::String(text())
    }
}

impl Typed for Data {
    const TY: &'static Ty = &Ty::Data;
}

impl IntoScript for Data {
    #[expect(
        clippy::cast_precision_loss,
        reason = "only integers a script number holds exactly become numbers"
    )]
    fn into_value(self) -> Value {
        match self {
            Self::Nil => Value::Nil,
            Self::Bool(b) => Value::Bool(b),
            Self::Int(i) => integer(i.unsigned_abs(), || i.to_string(), i as f64),
            Self::Uint(u) => integer(u, || u.to_string(), u as f64),
            Self::Float(f) => Value::Number(f),
            Self::Name(text) | Self::Text(text) => Value::String(text),
            Self::Flags(names) => Value::List(names.into_iter().map(Value::String).collect()),
            Self::Guid(guid) => Value::String(guid.to_string()),
            Self::List(items) => Value::List(items.into_iter().map(Self::into_value).collect()),
            Self::Struct(fields) => Value::Map(
                fields
                    .into_iter()
                    .map(|(n, v)| (n, v.into_value()))
                    .collect(),
            ),
            Self::Variant(variant) => Value::Map(vec![
                (
                    "type".to_owned(),
                    Value::String(variant.type_name().to_owned()),
                ),
                ("value".to_owned(), variant.value().clone().into_value()),
            ]),
            Self::Bytes(bytes) => Value::Bytes(bytes),
        }
    }
}

impl FromScript for Data {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a whole number inside the i64 range converts exactly"
    )]
    fn from_value(value: Value) -> Result<Self, Error> {
        Ok(match value {
            Value::Nil => Self::Nil,
            Value::Bool(b) => Self::Bool(b),
            // -0.0 stays a float: as an integer it would lose its sign.
            Value::Number(n)
                if n.fract() == 0.0 && n.abs() < 9.2e18 && (n != 0.0 || n.is_sign_positive()) =>
            {
                Self::Int(n as i64)
            }
            Value::Number(n) => Self::Float(n),
            Value::String(text) => Self::Text(text),
            Value::Bytes(bytes) => Self::Bytes(bytes),
            Value::List(items) => Self::List(
                items
                    .into_iter()
                    .map(Self::from_value)
                    .collect::<Result<_, _>>()?,
            ),
            Value::Map(fields) => Self::Struct(
                fields
                    .into_iter()
                    .map(|(name, value)| Ok((name, Self::from_value(value)?)))
                    .collect::<Result<_, Error>>()?,
            ),
            _ => {
                return Err(Error::new(
                    "expected game data: nil, a boolean, a number, text or a table",
                ));
            }
        })
    }
}

impl Answer for Result<Data, Error> {
    const TY: &'static Ty = &Ty::Data;

    /// The script's value, or why there is none: an error the runtime
    /// reported, or a return that is not game data.
    fn from_answer(answer: Result<Value, Error>) -> Self {
        answer.and_then(Data::from_value)
    }
}

/// A variant as game data: `{ type = <qualified name>, value = ... }`.
impl Typed for Variant {
    const TY: &'static Ty = &Ty::Data;
}

impl IntoScript for Variant {
    fn into_value(self) -> Value {
        Data::Variant(Box::new(self)).into_value()
    }
}

impl FromScript for Variant {
    fn from_value(value: Value) -> Result<Self, Error> {
        let data = Data::from_value(value)?;
        match (data.field("type"), data.field("value")) {
            (Some(Data::Text(name)), held) => {
                Ok(Self::new(name.clone(), held.cloned().unwrap_or(Data::Nil)))
            }
            _ => Err(Error::new(
                "expected a variant: { type = <qualified name>, value = ... }",
            )),
        }
    }
}
