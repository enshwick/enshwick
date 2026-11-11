use std::ops::ControlFlow;
use std::time::Duration;

use super::{Answer, Callback, Error, FromScript, IntoScript, Signature, Ty, Typed, Value};

/// The error for a `value` that is not `what`.
#[must_use]
pub fn expected(what: &str, value: &Value) -> Error {
    let got = match value {
        Value::Nil => "nil",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) | Value::Name(_) => "a string",
        Value::Vector(_) => "a vector",
        Value::List(_) => "a list",
        Value::Record(_) => "a table",
        Value::Function(_) => "a function",
        Value::Map(_) => "a table of data",
        Value::Bytes(_) => "bytes",
        Value::Object(_) => "an object",
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

impl Typed for usize {
    const TY: &'static Ty = &Ty::Number;
}

impl IntoScript for usize {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a count or a size past 2^53 does not occur"
    )]
    fn into_value(self) -> Value {
        Value::Number(self as f64)
    }
}

impl FromScript for usize {
    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::Number(n) = value else {
            return Err(expected("a number", &value));
        };
        if n.fract() != 0.0 || !(0.0..=EXACT_NUMBER).contains(&n) {
            return Err(Error::new(format!(
                "{n} is not a whole number from 0 to 2^53"
            )));
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "checked above: a whole number from 0 to 2^53"
        )]
        Ok(n as Self)
    }
}

macro_rules! number_arrays {
    ($(($t:ty, $n:literal)),*) => {$(
        impl Typed for [$t; $n] {
            const TY: &'static Ty = &Ty::List(<$t as Typed>::TY);
        }

        impl IntoScript for [$t; $n] {
            fn into_value(self) -> Value {
                Value::List(self.into_iter().map(IntoScript::into_value).collect())
            }
        }

        impl FromScript for [$t; $n] {
            fn from_value(value: Value) -> Result<Self, Error> {
                let list: Vec<$t> = Vec::from_value(value)?;
                list.try_into().map_err(|list: Vec<$t>| {
                    Error::new(format!("expected {} numbers, got {}", $n, list.len()))
                })
            }
        }
    )*};
}

number_arrays!(
    (f32, 4),
    (u32, 3),
    (u32, 2),
    (u16, 2),
    (f64, 2),
    ([i64; 3], 2)
);

/// An offset in voxels or metres, small enough for a script's number.
impl Typed for [i64; 3] {
    const TY: &'static Ty = &Ty::List(&Ty::Number);
}

impl IntoScript for [i64; 3] {
    #[expect(
        clippy::cast_precision_loss,
        reason = "an offset within the world is far below 2^53"
    )]
    fn into_value(self) -> Value {
        Value::List(self.into_iter().map(|n| Value::Number(n as f64)).collect())
    }
}

impl FromScript for [i64; 3] {
    fn from_value(value: Value) -> Result<Self, Error> {
        // A 32.32 fixed-point coordinate passes 2^31 at half a metre.
        const LIMIT: f64 = 9_007_199_254_740_992.0;
        let list: Vec<f64> = Vec::from_value(value)?;
        let list = list
            .into_iter()
            .map(|n| {
                if n.fract() != 0.0 || !(-LIMIT..=LIMIT).contains(&n) {
                    return Err(Error::new(format!(
                        "{n} is not a whole number from -2^53 to 2^53"
                    )));
                }
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "checked above: a whole number within 2^53 either side of zero"
                )]
                Ok(n as i64)
            })
            .collect::<Result<Vec<i64>, Error>>()?;
        list.try_into()
            .map_err(|list: Vec<i64>| Error::new(format!("expected 3 numbers, got {}", list.len())))
    }
}

impl Typed for std::path::PathBuf {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for std::path::PathBuf {
    fn into_value(self) -> Value {
        Value::String(self.to_string_lossy().into_owned())
    }
}

/// A pair of texts, such as a name and its value: a list of two strings.
impl Typed for (String, String) {
    const TY: &'static Ty = &Ty::List(&Ty::String);
}

impl IntoScript for (String, String) {
    fn into_value(self) -> Value {
        Value::List(vec![Value::String(self.0), Value::String(self.1)])
    }
}

impl FromScript for (String, String) {
    fn from_value(value: Value) -> Result<Self, Error> {
        let pair: Vec<String> = Vec::from_value(value)?;
        let [first, second]: [String; 2] = pair.try_into().map_err(|pair: Vec<String>| {
            Error::new(format!("expected 2 strings, got {}", pair.len()))
        })?;
        Ok((first, second))
    }
}

impl FromScript for std::path::PathBuf {
    fn from_value(value: Value) -> Result<Self, Error> {
        String::from_value(value).map(Self::from)
    }
}

number_arrays!((u8, 3));

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
                let bad = |shown: &dyn core::fmt::Debug| {
                    Error::new(format!("{shown:?} is not a {}-bit key", <$t>::BITS))
                };
                match value {
                    Value::String(text) => text.parse().map_err(|_| bad(&text)),
                    // A whole number a script number holds exactly, as a
                    // script computes it, is taken as well.
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a whole number within 2^53 converts exactly"
                    )]
                    Value::Number(n) if n.fract() == 0.0 && n.abs() <= EXACT_NUMBER => {
                        <$t>::try_from(n as i128).map_err(|_| bad(&n))
                    }
                    other => Err(expected("a string of digits or a whole number", &other)),
                }
            }
        }
    )*};
}

/// The largest whole number a script number holds exactly, 2^53.
pub const EXACT: u64 = 1 << 53;

/// [`EXACT`] as a script number.
#[expect(clippy::cast_precision_loss, reason = "2^53 converts exactly")]
pub const EXACT_NUMBER: f64 = EXACT as f64;

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
            Value::Name(name) => Ok(name.to_owned()),
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

impl FromScript for [f32; 3] {
    fn from_value(value: Value) -> Result<Self, Error> {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a script's vector is f32 already; f64 holds it exactly"
        )]
        <[f64; 3]>::from_value(value).map(|v| v.map(|n| n as f32))
    }
}

/// A GUID in memory order, such as a template's, as the text
/// [`Guid`](crate::content::Guid) writes, so every GUID a script holds has
/// one form.
impl Typed for [u8; 16] {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for [u8; 16] {
    fn into_value(self) -> Value {
        crate::content::Guid(self).into_value()
    }
}

impl FromScript for [u8; 16] {
    fn from_value(value: Value) -> Result<Self, Error> {
        crate::content::Guid::from_value(value).map(|guid| guid.0)
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

/// The script's text; nil or an error is empty text. A runtime turns the mod
/// off for an error or an answer of another type, as for every callback.
impl Answer for String {
    const TY: &'static Ty = &Ty::Optional(&Ty::String);

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::String(text)) => text,
            _ => Self::new(),
        }
    }
}

/// A filter's answer: `false` refuses; nil or an error lets it through, as
/// without the script, and the runtime turns the mod off for the error.
impl Answer for bool {
    const TY: &'static Ty = &Ty::Optional(&Ty::Bool);

    fn from_answer(answer: Result<Value, Error>) -> Self {
        !matches!(answer, Ok(Value::Bool(false)))
    }
}

/// The script's value; nil or an error is `None`, as without the script.
impl<T: FromScript> Answer for Option<T> {
    const TY: &'static Ty = &Ty::Optional(T::TY);

    fn from_answer(answer: Result<Value, Error>) -> Self {
        answer.ok().and_then(|value| T::from_value(value).ok())
    }
}

/// `false` stops; nil goes on; an error stops.
impl Answer for ControlFlow<()> {
    const TY: &'static Ty = &Ty::Optional(&Ty::Bool);

    fn from_answer(answer: Result<Value, Error>) -> Self {
        match answer {
            Ok(Value::Bool(false)) | Err(_) => Self::Break(()),
            Ok(_) => Self::Continue(()),
        }
    }
}

impl Typed for &'static [u8] {
    const TY: &'static Ty = &Ty::Bytes;
}

impl IntoScript for &'static [u8] {
    fn into_value(self) -> Value {
        Value::Bytes(self.to_vec())
    }
}

impl FromScript for &'static [u8] {
    fn from_value(value: Value) -> Result<Self, Error> {
        let Value::Bytes(bytes) = value else {
            return Err(Error::new("expected a buffer"));
        };
        // shortcut: an icon's PNG from a script lives for the process, as an
        // embedded one does; icons are few and given while the mod starts.
        Ok(Box::leak(bytes.into_boxed_slice()))
    }
}

/// The script's value as a `T`, or why there is none: an error the runtime
/// reported, or a return that is not a `T`.
impl<T: FromScript> Answer for Result<T, Error> {
    const TY: &'static Ty = T::TY;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        answer.and_then(T::from_value)
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
    args: &mut impl Iterator<Item = Value>,
    name: &str,
) -> Result<T, Error> {
    T::from_value(args.next().unwrap_or(Value::Nil)).map_err(|e| e.within(name))
}

#[doc(hidden)]
/// [`argument`] for a field `#[script(default)]` marks: nil is `T`'s
/// default.
///
/// # Errors
///
/// As [`argument`].
pub fn argument_or_default<T: FromScript + Default>(
    args: &mut impl Iterator<Item = Value>,
    name: &str,
) -> Result<T, Error> {
    match args.next() {
        None | Some(Value::Nil) => Ok(T::default()),
        Some(value) => T::from_value(value).map_err(|e| e.within(name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_takes_only_whole_numbers_from_zero_to_two_to_the_53() {
        assert_eq!(usize::from_value(Value::Number(3.0)).ok(), Some(3));
        for refused in [2.5, -1.0, 1e16] {
            assert!(
                usize::from_value(Value::Number(refused)).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_fixed_point_position_past_half_a_metre_reads_back() {
        let far = [100_i64 << 32, -(50_i64 << 32), 1];
        assert_eq!(<[i64; 3]>::from_value(far.into_value()).ok(), Some(far));
        for refused in [0.5, 1e16] {
            let list = Value::List(vec![
                Value::Number(refused),
                Value::Number(0.0),
                Value::Number(0.0),
            ]);
            assert!(<[i64; 3]>::from_value(list).is_err(), "{refused}");
        }
    }

    #[test]
    fn a_guid_in_memory_order_reads_as_the_content_guid_text() {
        let bytes: [u8; 16] = core::array::from_fn(|i| u8::try_from(i * 15 + 1).unwrap());
        assert_eq!(<[u8; 16]>::from_value(bytes.into_value()).ok(), Some(bytes));
        let text = "6943053a-32c1-4718-ae22-e61427ddf54d";
        let guid = crate::content::Guid::parse(text);
        assert!(matches!(guid.0.into_value(), Value::String(ref t) if t == text));
        assert_eq!(
            <[u8; 16]>::from_value(Value::String(text.into())).ok(),
            Some(guid.0)
        );
        for refused in ["0".repeat(32), "0".repeat(36), "+f".repeat(18)] {
            assert!(
                <[u8; 16]>::from_value(Value::String(refused.clone())).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn an_answer_that_does_not_fit_reads_as_without_the_script() {
        assert_eq!(String::from_answer(Ok(Value::String("hi".into()))), "hi");
        assert_eq!(String::from_answer(Ok(Value::Nil)), "");
        assert!(!bool::from_answer(Ok(Value::Bool(false))));
        assert!(bool::from_answer(Ok(Value::Bool(true))));
        assert!(bool::from_answer(Ok(Value::Nil)));
        assert!(bool::from_answer(Err(Error::new("boom"))));
    }

    #[test]
    fn a_result_answer_is_the_value_as_its_type_or_why_it_is_not() {
        assert!(core::ptr::eq(<Result<u8, Error>>::TY, u8::TY));
        assert_eq!(
            <Result<u8, Error>>::from_answer(Ok(Value::Number(7.0))).ok(),
            Some(7)
        );
        assert!(<Result<u8, Error>>::from_answer(Ok(Value::Number(300.0))).is_err());
        assert!(<Result<u8, Error>>::from_answer(Ok(Value::Bool(true))).is_err());
        let reported = <Result<String, Error>>::from_answer(Err(Error::new("boom")));
        assert_eq!(reported.map_err(|e| e.to_string()), Err("boom".to_owned()));
    }
}
