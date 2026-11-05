//! What every generated item stands on: the conversion between a Rust value
//! and the codec's [`Value`], and the handles that resolve a component,
//! event or resource on the running build and check its shape.

use crate::Off;
use crate::content::Guid;
use crate::resources::{Value, Variant};

mod handles;
mod sample;
pub(crate) mod script;
pub use handles::{ComponentType, EventType, ResourceType};
pub(crate) use handles::{RawComponent, RawEvent, RawResource};
pub use sample::sample;

/// Why a [`Value`] is not the shape a generated type expects: a field the
/// build lacks, or a value of another kind.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Value;
/// use enshwick::typed::Reflected;
///
/// let error = u8::from_value(&Value::Text("no".into())).unwrap_err();
/// assert_eq!(error.to_string(), "a u8, not Text(\"no\")");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{}{expected}", path_prefix(.path))]
pub struct ShapeError {
    path: Vec<String>,
    expected: String,
}

fn path_prefix(path: &[String]) -> String {
    if path.is_empty() {
        String::new()
    } else {
        format!("{}: ", path.join("."))
    }
}

impl ShapeError {
    /// An error for a value that is not `expected`.
    #[doc(hidden)]
    #[must_use]
    pub fn new(expected: &str, found: &Value) -> Self {
        Self {
            path: Vec::new(),
            expected: format!("{expected}, not {found:?}"),
        }
    }

    /// The error inside the field `name`.
    fn within(mut self, name: &str) -> Self {
        self.path.insert(0, name.to_owned());
        self
    }
}

impl From<ShapeError> for Off {
    fn from(error: ShapeError) -> Self {
        Self::new(error.to_string())
    }
}

/// A Rust type for a reflected game type, read from and written to the
/// [`Value`] the resource codec reads game bytes into, by the game's field
/// names; every generated item implements it.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Value;
/// use enshwick::typed::Reflected;
///
/// let health: Option<i32> = Option::from_value(&Value::Int(40))?;
/// assert_eq!(health.to_value(), Value::Int(40));
/// # Ok::<(), enshwick::typed::ShapeError>(())
/// ```
pub trait Reflected: Sized {
    /// The Rust value of `value`.
    ///
    /// # Errors
    ///
    /// [`ShapeError`] when `value` is not this type's shape.
    fn from_value(value: &Value) -> Result<Self, ShapeError>;

    /// The value as the codec writes it.
    fn to_value(&self) -> Value;
}

/// A generated type the loader finds by enshwick's id for it: a component,
/// an event or a resource.
///
/// # Examples
///
/// ```
/// fn id<T: enshwick::typed::Named>() -> &'static str {
///     T::ID
/// }
/// ```
pub trait Named: Reflected + sealed::Sealed + 'static {
    /// Enshwick's id for the type, which the loader maps to the running
    /// build's type.
    const ID: &'static str;
    /// Its roles by their traits' names: `Component`, `Event`, `Resource`.
    #[doc(hidden)]
    const ROLES: &'static [&'static str];
}

/// A generated ECS component, read and written through [`ComponentType`].
///
/// # Examples
///
/// See [`ComponentType`].
pub trait Component: Named {}

/// A generated game event, handled through [`EventType`].
///
/// # Examples
///
/// See [`EventType`].
pub trait Event: Named {}

/// A generated resource of the game's content, read and served through
/// [`ResourceType`].
///
/// # Examples
///
/// See [`ResourceType`].
pub trait Resource: Named {}

pub(crate) mod sealed {
    /// Keeps [`Named`](super::Named) to the generated types.
    pub trait Sealed {}
}

/// An enum constant a generated enum does not name: one a newer build adds,
/// or a number the game leaves without a name.
///
/// # Examples
///
/// ```
/// use enshwick::typed::Constant;
///
/// assert_ne!(Constant::Number(3), Constant::Name("Three".into()));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Constant {
    /// A constant by its name on the running build.
    Name(String),
    /// A value no constant names.
    Number(u64),
}

impl Constant {
    /// The constant `value` holds.
    #[doc(hidden)]
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Name(name) => Ok(Self::Name(name.clone())),
            Value::Uint(number) => Ok(Self::Number(*number)),
            other => Err(ShapeError::new("an enum constant", other)),
        }
    }

    /// The constant as a value.
    #[doc(hidden)]
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Name(name) => Value::Name(name.clone()),
            Self::Number(number) => Value::Uint(*number),
        }
    }

    /// The constant as a generated enum's `Other` in a script:
    /// `{ Other = <name or number> }`, a number past 2^53 as its decimal
    /// text, which a script number cannot hold exactly.
    #[doc(hidden)]
    #[must_use]
    pub fn into_script(self) -> crate::script::Value {
        let held = match self {
            Self::Name(name) => crate::script::Value::String(name),
            #[expect(
                clippy::cast_precision_loss,
                reason = "only a number a script number holds exactly becomes one"
            )]
            Self::Number(number) if number <= EXACT => crate::script::Value::Number(number as f64),
            Self::Number(number) => crate::script::Value::String(number.to_string()),
        };
        crate::script::Value::Map(vec![(OTHER.to_owned(), held)])
    }

    /// The constant a script's `{ Other = <name or number> }` names; text
    /// of digits is a number, since no constant's name is.
    ///
    /// # Errors
    ///
    /// [`Error`](crate::script::Error) for any other value.
    #[doc(hidden)]
    pub fn from_script(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        let malformed = || {
            crate::script::Error::new("expected a constant's name, or { Other = <name or number> }")
        };
        let crate::script::Value::Map(mut entries) = value else {
            return Err(malformed());
        };
        if entries.len() != 1 || entries[0].0 != OTHER {
            return Err(malformed());
        }
        match entries.remove(0).1 {
            crate::script::Value::String(text) => {
                Ok(text.parse().map_or_else(|_| Self::Name(text), Self::Number))
            }
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "checked first: a whole number from 0 to 2^53"
            )]
            crate::script::Value::Number(n)
                if n.fract() == 0.0 && (0.0..=EXACT_NUMBER).contains(&n) =>
            {
                Ok(Self::Number(n as u64))
            }
            _ => Err(malformed()),
        }
    }
}

use crate::script::convert::{EXACT, EXACT_NUMBER};

/// The variant of a generated enum that holds a constant it does not name.
pub(crate) const OTHER: &str = "Other";

/// Why `text` names no variant of the generated enum `ty`.
#[must_use]
pub(crate) fn no_variant(text: &str, ty: &crate::script::Ty) -> crate::script::Error {
    let name = match ty {
        crate::script::Ty::Union(union) => union.name(),
        _ => "",
    };
    crate::script::Error::new(format!(
        "{text:?} is no constant of {name}; a constant it does not name is {{ Other = <name or number> }}"
    ))
}

/// The set flags of a bitmask, each a constant of its generated flag enum
/// `E`, in bit order.
///
/// A bitmask with a bit no constant names reads as numbers: each flag is
/// `E`'s `Other` with [`Constant::Number`] of its bit. Written back, flags
/// that are all numbered write their bits, and any others write their
/// names; a numbered flag among named ones fails the write.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Value;
/// use enshwick::typed::{Flags, Reflected};
///
/// let flags: Flags<u8> = Flags::from_value(&Value::Uint(0b101))?;
/// assert_eq!(flags.0, [0, 2]);
/// # Ok::<(), enshwick::typed::ShapeError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags<E>(pub Vec<E>);

impl<E: Reflected> Reflected for Flags<E> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Flags(names) => names
                .iter()
                .map(|name| E::from_value(&Value::Name(name.clone())))
                .collect::<Result<_, _>>()
                .map(Self),
            Value::Uint(bits) => (0..64)
                .filter(|bit| bits & (1 << bit) != 0)
                .map(|bit| E::from_value(&Value::Uint(bit)))
                .collect::<Result<_, _>>()
                .map(Self),
            other => Err(ShapeError::new("a set of flags", other)),
        }
    }

    fn to_value(&self) -> Value {
        let values: Vec<Value> = self.0.iter().map(E::to_value).collect();
        if !values.is_empty() && values.iter().all(|v| matches!(v, Value::Uint(_))) {
            return Value::Uint(values.iter().fold(0, |bits, v| match v {
                Value::Uint(bit) if *bit < 64 => bits | (1 << bit),
                _ => bits,
            }));
        }
        // A numbered flag among named ones has no name the codec can write:
        // it stays as its number's text, which the codec refuses on writing,
        // rather than silently dropping a flag.
        Value::Flags(
            values
                .iter()
                .map(|v| match v {
                    Value::Name(name) => name.clone(),
                    other => format!("{other:?}"),
                })
                .collect(),
        )
    }
}

/// A script's flags: the list of each set flag's constant.
impl<E: crate::script::Typed> crate::script::Typed for Flags<E> {
    const TY: &'static crate::script::Ty = <Vec<E> as crate::script::Typed>::TY;
}

impl<E: crate::script::IntoScript> crate::script::IntoScript for Flags<E> {
    fn into_value(self) -> crate::script::Value {
        crate::script::IntoScript::into_value(self.0)
    }
}

impl<E: crate::script::FromScript> crate::script::FromScript for Flags<E> {
    fn from_value(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        <Vec<E> as crate::script::FromScript>::from_value(value).map(Self)
    }
}

/// The field `name` of the struct `value`, as `T`.
///
/// # Errors
///
/// [`ShapeError`] when the struct lacks the field or it is not `T`'s shape.
pub(crate) fn field<T: Reflected>(value: &Value, name: &str) -> Result<T, ShapeError> {
    let found = value.field(name).ok_or_else(|| ShapeError {
        path: vec![name.to_owned()],
        expected: "a field this build lacks".to_owned(),
    })?;
    T::from_value(found).map_err(|e| e.within(name))
}

/// The fields of the struct `value`, written before a derived type's own.
#[must_use]
pub(crate) fn fields_of(value: Value) -> Vec<(String, Value)> {
    match value {
        Value::Struct(fields) => fields,
        _ => Vec::new(),
    }
}

/// Whether `value` reads as `T`, for the table of every generated type.
///
/// # Errors
///
/// [`ShapeError`] when it does not.
#[doc(hidden)]
pub fn check<T: Reflected>(value: &Value) -> Result<(), ShapeError> {
    T::from_value(value).map(drop)
}

/// A generated type with a role, its Rust type erased: its id, roles and
/// script type, and its conversions, for a script runtime and for the tests
/// that hold the generated items against a build's types.
#[doc(hidden)]
#[derive(Debug)]
pub struct Row {
    /// Enshwick's id for the type, its [`Named::ID`].
    pub id: &'static str,
    /// Its roles, as [`Named::ROLES`] names them.
    pub roles: &'static [&'static str],
    /// Whether a value reads as the type.
    pub check: fn(&Value) -> Result<(), ShapeError>,
    /// Its type in a script.
    pub script: &'static crate::script::Ty,
    /// A value of the codec read as the type, as a script gets it.
    pub to_script: fn(&Value) -> Result<crate::script::Value, ShapeError>,
    /// A script's value read as the type, as the codec writes it.
    pub from_script: fn(crate::script::Value) -> Result<Value, crate::script::Error>,
}

impl Row {
    /// The row of `T`.
    #[doc(hidden)]
    #[must_use]
    pub const fn of<T: Named + crate::script::IntoScript + crate::script::FromScript>() -> Self {
        Self {
            id: T::ID,
            roles: T::ROLES,
            check: check::<T>,
            script: <T as crate::script::Typed>::TY,
            to_script: to_script::<T>,
            from_script: from_script::<T>,
        }
    }
}

fn to_script<T: Reflected + crate::script::IntoScript>(
    value: &Value,
) -> Result<crate::script::Value, ShapeError> {
    T::from_value(value).map(crate::script::IntoScript::into_value)
}

fn from_script<T: Reflected + crate::script::FromScript>(
    value: crate::script::Value,
) -> Result<Value, crate::script::Error> {
    <T as crate::script::FromScript>::from_value(value).map(|typed| typed.to_value())
}

/// The table of every generated type with a role, a [`Row`] each.
macro_rules! named_table {
    ($($id:literal => $ty:ty,)*) => {
        /// Every generated type with a role, by id, with its script type
        /// and a check that a value reads as it.
        #[doc(hidden)]
        pub const NAMED: &[$crate::typed::runtime::Row] =
            &[$($crate::typed::runtime::Row::of::<$ty>()),*];
    };
}
pub(crate) use named_table;

/// `name` without the `r#` of a raw identifier: a field's name in a script.
#[must_use]
pub(crate) const fn unraw(name: &'static str) -> &'static str {
    match name.as_bytes() {
        [b'r', b'#', ..] => name.split_at(2).1,
        _ => name,
    }
}

/// The number of fields of the record `ty`, for a generated struct's
/// script conversion.
#[must_use]
pub(crate) const fn field_count(ty: &crate::script::Ty) -> usize {
    match ty {
        crate::script::Ty::Record(record) => record.fields().len(),
        _ => 0,
    }
}

/// The doc of a derived type's field `base` in a script.
pub(crate) const BASE_DOC: &str = "The fields of the type this one derives from.";

/// The doc of a generated enum's variant `Other` in a script.
pub(crate) const OTHER_DOC: &str =
    "A constant this enum does not name: its name on the running build, or its number.";

/// A generated struct: its fields by the game's names, read from and
/// written to a [`Value`], and by its Rust names in a script, as a record
/// named by its Rust path; a derived type holds its base first.
macro_rules! reflected_struct {
    (
        #[doc = $doc:literal]
        $(#[doc = $more:literal])*
        $name:ident $(: $base:ty)? {
            $($(#[doc = $field_doc:literal])* $field:ident: $ty:ty = $game:literal,)*
        }
    ) => {
        #[doc = $doc]
        $(#[doc = $more])*
        #[derive(Clone, Debug, PartialEq)]
        #[non_exhaustive]
        pub struct $name {
            $(
                /// The fields of the type this one derives from.
                pub base: $base,
            )?
            $($(#[doc = $field_doc])* pub $field: $ty,)*
        }

        impl $crate::script::Typed for $name {
            // Each field's type is asked for later: a type may hold itself,
            // such as in an optional, which a constant cannot describe.
            const TY: &'static $crate::script::Ty =
                &$crate::script::Ty::Record(&$crate::script::Record::new(
                    ::core::concat!(::core::module_path!(), "::", ::core::stringify!($name)),
                    $doc,
                    &[
                        $($crate::script::Field::later(
                            "base",
                            $crate::typed::runtime::BASE_DOC,
                            || <$base as $crate::script::Typed>::TY,
                        ),)?
                        $($crate::script::Field::later(
                            $crate::typed::runtime::unraw(::core::stringify!($field)),
                            ::core::concat!($($field_doc),*),
                            || <$ty as $crate::script::Typed>::TY,
                        ),)*
                    ],
                ));
        }

        impl $crate::script::IntoScript for $name {
            fn into_value(self) -> $crate::script::Value {
                $crate::script::Value::Record(::std::vec![
                    $(<$base as $crate::script::IntoScript>::into_value(self.base),)?
                    $($crate::script::IntoScript::into_value(self.$field),)*
                ])
            }
        }

        impl $crate::script::FromScript for $name {
            fn from_value(
                value: $crate::script::Value,
            ) -> ::core::result::Result<Self, $crate::script::Error> {
                let mut fields = $crate::__private::record_fields(
                    value,
                    $crate::typed::runtime::field_count(<Self as $crate::script::Typed>::TY),
                )?;
                // A type without fields takes none of them.
                let _ = &mut fields;
                ::core::result::Result::Ok(Self {
                    $(base: $crate::__private::argument::<$base>(&mut fields, "base")?,)?
                    $($field: $crate::__private::argument(
                        &mut fields,
                        $crate::typed::runtime::unraw(::core::stringify!($field)),
                    )?,)*
                })
            }
        }

        // An optional struct is boxed; one impl per type, since a generic
        // one for `Box` would overlap the impls for every `Object`.
        impl $crate::script::Typed for ::std::boxed::Box<$name> {
            const TY: &'static $crate::script::Ty = <$name as $crate::script::Typed>::TY;
        }

        impl $crate::script::IntoScript for ::std::boxed::Box<$name> {
            fn into_value(self) -> $crate::script::Value {
                $crate::script::IntoScript::into_value(*self)
            }
        }

        impl $crate::script::FromScript for ::std::boxed::Box<$name> {
            fn from_value(
                value: $crate::script::Value,
            ) -> ::core::result::Result<Self, $crate::script::Error> {
                <$name as $crate::script::FromScript>::from_value(value).map(::std::boxed::Box::new)
            }
        }

        impl $crate::typed::Reflected for $name {
            fn from_value(
                value: &$crate::resources::Value,
            ) -> ::core::result::Result<Self, $crate::typed::ShapeError> {
                // A type without fields reads nothing of it.
                let _ = value;
                ::core::result::Result::Ok(Self {
                    $(base: <$base as $crate::typed::Reflected>::from_value(value)?,)?
                    $($field: $crate::typed::runtime::field(value, $game)?,)*
                })
            }

            fn to_value(&self) -> $crate::resources::Value {
                let parts: [::std::vec::Vec<(::std::string::String, $crate::resources::Value)>; _] = [
                    $($crate::typed::runtime::fields_of(
                        <$base as $crate::typed::Reflected>::to_value(&self.base),
                    ),)?
                    ::std::vec![$(($game.to_owned(), $crate::typed::Reflected::to_value(&self.$field)),)*],
                ];
                $crate::resources::Value::Struct(parts.into_iter().flatten().collect())
            }
        }
    };
}
pub(crate) use reflected_struct;

/// A generated enum: a variant per constant the build names, and `Other`
/// for a constant it does not; in a script, a variant's Rust name, or
/// `{ Other = <name or number> }`.
macro_rules! reflected_enum {
    (
        #[doc = $doc:literal]
        $(#[doc = $more:literal])*
        $name:ident {
            $($(#[doc = $variant_doc:literal])* $variant:ident = $game:literal,)*
        }
    ) => {
        impl $crate::script::Typed for $name {
            const TY: &'static $crate::script::Ty =
                &$crate::script::Ty::Union(&$crate::script::Union::new(
                    ::core::concat!(::core::module_path!(), "::", ::core::stringify!($name)),
                    $doc,
                    &[
                        $($crate::script::Variant::new(
                            ::core::stringify!($variant),
                            ::core::concat!($($variant_doc),*),
                            ::core::option::Option::None,
                        ),)*
                        $crate::script::Variant::new(
                            $crate::typed::runtime::OTHER,
                            $crate::typed::runtime::OTHER_DOC,
                            ::core::option::Option::Some(&$crate::script::Ty::Data),
                        ),
                    ],
                ));
        }

        impl $crate::script::IntoScript for $name {
            fn into_value(self) -> $crate::script::Value {
                match self {
                    $(Self::$variant => $crate::script::Value::String(
                        ::std::string::String::from(::core::stringify!($variant)),
                    ),)*
                    Self::Other(constant) => constant.into_script(),
                }
            }
        }

        impl $crate::script::FromScript for $name {
            fn from_value(
                value: $crate::script::Value,
            ) -> ::core::result::Result<Self, $crate::script::Error> {
                match value {
                    $crate::script::Value::String(text) => match text.as_str() {
                        $(::core::stringify!($variant) => ::core::result::Result::Ok(Self::$variant),)*
                        _ => ::core::result::Result::Err($crate::typed::runtime::no_variant(
                            &text,
                            <Self as $crate::script::Typed>::TY,
                        )),
                    },
                    // A name the build gives a variant reads as that variant,
                    // as from the game, so `Other` holds only unnamed ones.
                    other => $crate::typed::Constant::from_script(other).map(|constant| {
                        match <Self as $crate::typed::Reflected>::from_value(&constant.to_value()) {
                            ::core::result::Result::Ok(read) => read,
                            ::core::result::Result::Err(_) => Self::Other(constant),
                        }
                    }),
                }
            }
        }

        #[doc = $doc]
        $(#[doc = $more])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[doc = $variant_doc])* $variant,)*
            /// A constant this enum does not name: a newer build's, or a
            /// number the game leaves without a name.
            Other($crate::typed::Constant),
        }

        impl $crate::typed::Reflected for $name {
            fn from_value(
                value: &$crate::resources::Value,
            ) -> ::core::result::Result<Self, $crate::typed::ShapeError> {
                let constant = $crate::typed::Constant::from_value(value)?;
                ::core::result::Result::Ok(match &constant {
                    $($crate::typed::Constant::Name(name) if name == $game => Self::$variant,)*
                    _ => Self::Other(constant),
                })
            }

            fn to_value(&self) -> $crate::resources::Value {
                match self {
                    $(Self::$variant => $crate::resources::Value::Name($game.to_owned()),)*
                    Self::Other(constant) => constant.to_value(),
                }
            }
        }
    };
}
pub(crate) use reflected_enum;

/// Gives a generated type enshwick's id and its roles.
macro_rules! named {
    ($name:ident = $id:literal: $($role:ident),+) => {
        impl $crate::typed::runtime::sealed::Sealed for $name {}
        impl $crate::typed::Named for $name {
            const ID: &'static str = $id;
            const ROLES: &'static [&'static str] = &[$(::core::stringify!($role)),+];
        }
        $(impl $crate::typed::$role for $name {})+
    };
}
pub(crate) use named;

macro_rules! integers {
    ($($int:ty => $name:literal, $form:ident),* $(,)?) => {$(
        impl Reflected for $int {
            fn from_value(value: &Value) -> Result<Self, ShapeError> {
                let fits = match value {
                    Value::Uint(n) => Self::try_from(*n).ok(),
                    Value::Int(n) => Self::try_from(*n).ok(),
                    _ => None,
                };
                fits.ok_or_else(|| ShapeError::new($name, value))
            }

            fn to_value(&self) -> Value {
                Value::$form((*self).into())
            }
        }
    )*};
}

// Each in the form the codec reads its kind as: unsigned as `Uint`, signed
// as `Int`.
integers!(
    u8 => "a u8", Uint, u16 => "a u16", Uint, u32 => "a u32", Uint, u64 => "a u64", Uint,
    i8 => "an i8", Int, i16 => "an i16", Int, i32 => "an i32", Int, i64 => "an i64", Int,
);

impl Reflected for bool {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Bool(b) => Ok(*b),
            // A byte the game holds other than 0 or 1 reads as a number.
            Value::Uint(n) => Ok(*n != 0),
            other => Err(ShapeError::new("a bool", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}

impl Reflected for f32 {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the codec reads a float32 into an f64 exactly"
            )]
            Value::Float(f) => Ok(*f as Self),
            other => Err(ShapeError::new("a float", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Float(f64::from(*self))
    }
}

impl Reflected for String {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Text(text) => Ok(text.clone()),
            Value::Bytes(bytes) => Ok(Self::from_utf8_lossy(bytes).into_owned()),
            other => Err(ShapeError::new("a string", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}

impl Reflected for Guid {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Guid(guid) => Ok(*guid),
            other => Err(ShapeError::new("a GUID", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::Guid(*self)
    }
}

impl Reflected for Value {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        Ok(value.clone())
    }

    fn to_value(&self) -> Value {
        self.clone()
    }
}

impl Reflected for Option<Variant> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Nil => Ok(None),
            Value::Variant(variant) => Ok(Some((**variant).clone())),
            other => Err(ShapeError::new("a variant", other)),
        }
    }

    fn to_value(&self) -> Value {
        self.as_ref()
            .map_or(Value::Nil, |v| Value::Variant(Box::new(v.clone())))
    }
}

impl<T: Reflected> Reflected for Box<T> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        T::from_value(value).map(Box::new)
    }

    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}

/// An optional value; `None` is an absent one.
impl<T: Reflected> Reflected for Option<T> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::Nil => Ok(None),
            some => T::from_value(some).map(Some),
        }
    }

    fn to_value(&self) -> Value {
        self.as_ref().map_or(Value::Nil, T::to_value)
    }
}

/// An array. One the codec keeps as bytes, such as a packed array whose
/// elements lie outside the bytes read, does not read: those bytes are its
/// header, not its elements.
impl<T: Reflected> Reflected for Vec<T> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::List(items) => items
                .iter()
                .enumerate()
                .map(|(i, item)| T::from_value(item).map_err(|e| e.within(&i.to_string())))
                .collect(),
            other => Err(ShapeError::new("an array", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(T::to_value).collect())
    }
}

#[cfg(test)]
mod tests;
