use core::fmt::Write as _;
use std::ops::ControlFlow;
use std::time::Duration;

use super::{Answer, Callback, Error, FromScript, IntoScript, Signature, Ty, Typed, Value};

fn expected(what: &str, value: &Value) -> Error {
    let got = match value {
        Value::Nil => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Vector(_) => "a vector",
        Value::List(_) => "a list",
        Value::Record(_) => "a table",
        Value::Function(_) => "a function",
    };
    Error::new(format!("expected {what}, got {got}"))
}

impl Typed for () {
    const TY: &'static Ty = &Ty::Nil;
}

impl IntoScript for () {
    fn into_value(self) -> Value {
        Value::Nil
    }
}

impl Typed for bool {
    const TY: &'static Ty = &Ty::Bool;
}

impl IntoScript for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl FromScript for bool {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Bool(b) => Ok(b),
            other => Err(expected("a boolean", &other)),
        }
    }
}

macro_rules! whole_numbers {
    ($($t:ty),*) => {$(
        impl Typed for $t {
            const TY: &'static Ty = &Ty::Number;
        }

        impl IntoScript for $t {
            fn into_value(self) -> Value {
                Value::Number(f64::from(self))
            }
        }

        impl FromScript for $t {
            fn from_value(value: Value) -> Result<Self, Error> {
                let Value::Number(n) = value else {
                    return Err(expected("a number", &value));
                };
                let out_of_range = || {
                    Error::new(format!(
                        "{n} is not a whole number from {} to {}",
                        <$t>::MIN,
                        <$t>::MAX
                    ))
                };
                if n.fract() != 0.0 || n < f64::from(<$t>::MIN) || n > f64::from(<$t>::MAX) {
                    return Err(out_of_range());
                }
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "checked above: a whole number within a 32-bit range"
                )]
                let whole = n as i64;
                <$t>::try_from(whole).map_err(|_| out_of_range())
            }
        }
    )*};
}

whole_numbers!(u8, u16, u32, i8, i16, i32);

impl Typed for f64 {
    const TY: &'static Ty = &Ty::Number;
}

impl IntoScript for f64 {
    fn into_value(self) -> Value {
        Value::Number(self)
    }
}

impl FromScript for f64 {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Number(n) => Ok(n),
            other => Err(expected("a number", &other)),
        }
    }
}

impl Typed for f32 {
    const TY: &'static Ty = &Ty::Number;
}

impl IntoScript for f32 {
    fn into_value(self) -> Value {
        Value::Number(f64::from(self))
    }
}

impl FromScript for f32 {
    fn from_value(value: Value) -> Result<Self, Error> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a script's number is an f64; f32 keeps what it can"
        )]
        f64::from_value(value).map(|n| n as f32)
    }
}

macro_rules! keys {
    ($($t:ty),*) => {$(
        impl Typed for $t {
            const TY: &'static Ty = &Ty::String;
        }

        impl IntoScript for $t {
            fn into_value(self) -> Value {
                Value::String(self.to_string())
            }
        }

        impl FromScript for $t {
            fn from_value(value: Value) -> Result<Self, Error> {
                let Value::String(text) = value else {
                    return Err(expected("a string of digits", &value));
                };
                text.parse()
                    .map_err(|_| Error::new(format!("{text:?} is not a {}-bit key", <$t>::BITS)))
            }
        }
    )*};
}

keys!(u64, i64);

impl Typed for String {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for String {
    fn into_value(self) -> Value {
        Value::String(self)
    }
}

impl FromScript for String {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::String(text) => Ok(text),
            other => Err(expected("a string", &other)),
        }
    }
}

impl Typed for [f64; 3] {
    const TY: &'static Ty = &Ty::Vector;
}

impl IntoScript for [f64; 3] {
    fn into_value(self) -> Value {
        Value::Vector(self)
    }
}

impl FromScript for [f64; 3] {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Vector(v) => Ok(v),
            other => Err(expected("a vector", &other)),
        }
    }
}

impl Typed for [f32; 3] {
    const TY: &'static Ty = &Ty::Vector;
}

impl IntoScript for [f32; 3] {
    fn into_value(self) -> Value {
        Value::Vector(self.map(f64::from))
    }
}

/// A GUID, such as a template's, as 32 lowercase hex digits.
impl Typed for [u8; 16] {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for [u8; 16] {
    fn into_value(self) -> Value {
        let mut text = String::with_capacity(32);
        for byte in self {
            let _written = write!(text, "{byte:02x}");
        }
        Value::String(text)
    }
}

/// Seconds.
impl Typed for Duration {
    const TY: &'static Ty = &Ty::Number;
}

impl IntoScript for Duration {
    fn into_value(self) -> Value {
        Value::Number(self.as_secs_f64())
    }
}

impl FromScript for Duration {
    fn from_value(value: Value) -> Result<Self, Error> {
        let seconds = f64::from_value(value)?;
        Self::try_from_secs_f64(seconds)
            .map_err(|_| Error::new(format!("{seconds} is not a number of seconds")))
    }
}

impl<T: Typed> Typed for Option<T> {
    const TY: &'static Ty = &Ty::Optional(T::TY);
}

impl<T: IntoScript> IntoScript for Option<T> {
    fn into_value(self) -> Value {
        self.map_or(Value::Nil, IntoScript::into_value)
    }
}

impl<T: FromScript> FromScript for Option<T> {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Nil => Ok(None),
            value => T::from_value(value).map(Some),
        }
    }
}

impl<T: Typed> Typed for Vec<T> {
    const TY: &'static Ty = &Ty::List(T::TY);
}

impl<T: IntoScript> IntoScript for Vec<T> {
    fn into_value(self) -> Value {
        Value::List(self.into_iter().map(IntoScript::into_value).collect())
    }
}

impl<T: FromScript> FromScript for Vec<T> {
    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::List(items) = value else {
            return Err(expected("a list", &value));
        };
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| T::from_value(item).map_err(|e| e.within(&format!("[{}]", i + 1))))
            .collect()
    }
}

/// A function taking nothing and answering nothing; a parameter that takes
/// a function declares its own signature.
impl Typed for Callback {
    const TY: &'static Ty = &Ty::Function(&Signature::new(&[], &Ty::Nil));
}

impl FromScript for Callback {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Function(callback) => Ok(callback),
            other => Err(expected("a function", &other)),
        }
    }
}

impl Answer for () {
    const TY: &'static Ty = &Ty::Nil;

    fn from_answer(_answer: Result<Value, Error>) -> Self {}
}

/// `false` stops; nothing or anything else goes on; an error stops.
impl Answer for ControlFlow<()> {
    const TY: &'static Ty = &Ty::Optional(&Ty::Bool);

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Bool(false)) | Err(_) => Self::Break(()),
            Ok(_) => Self::Continue(()),
        }
    }
}

#[doc(hidden)]
/// The record fields of `value` for a type with `count` fields, for
/// [`#[derive(Script)]`](macro@crate::Script).
///
/// # Errors
///
/// [`Error`] when `value` is not a record of `count` fields.
pub fn record_fields(value: Value, count: usize) -> Result<std::vec::IntoIter<Value>, Error> {
    match value {
        Value::Record(fields) if fields.len() == count => Ok(fields.into_iter()),
        other => Err(expected("a table", &other)),
    }
}

#[doc(hidden)]
/// The argument at `index` of `args`, nil when the script passed fewer, as
/// `T`, for [`#[script]`](macro@crate::script).
///
/// # Errors
///
/// [`Error`] naming the parameter when the argument is not a `T`.
pub fn argument<T: FromScript>(
    args: &mut std::vec::IntoIter<Value>,
    name: &str,
) -> Result<T, Error> {
    T::from_value(args.next().unwrap_or(Value::Nil)).map_err(|e| e.within(name))
}
