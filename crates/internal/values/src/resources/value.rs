//! A resource's data as a tree of typed values.

use crate::content::Guid;

/// A value of a reflected type, as [`Registry::decode`](struct.Registry.html#method.decode)
/// reads it from a resource and [`Registry::encode`](struct.Registry.html#method.encode)
/// writes it back.
///
/// Each kind has one form: a struct is its fields by name, an enum its
/// constant's name, a bitmask the names of its set flags, a GUID or a
/// reference its GUID, an absent optional [`Value::Nil`]. A value the
/// type's constants do not name, such as an enum value without a name,
/// stays a number, so every resource reads and writes back unchanged.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Value;
///
/// let mut table = Value::Struct(vec![("damage".into(), Value::Float(2.5))]);
/// if let Some(Value::Float(damage)) = table.field_mut("damage") {
///     *damage *= 2.0;
/// }
/// assert_eq!(table.field("damage"), Some(&Value::Float(5.0)));
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum Value {
    /// An absent optional or variant.
    Nil,
    /// A `bool`.
    Bool(bool),
    /// A signed integer.
    Int(i64),
    /// An unsigned integer, and an enum or bitmask value no constant names.
    Uint(u64),
    /// A `float32`, held exactly.
    Float(f64),
    /// An enum constant by name.
    Name(String),
    /// The names of a bitmask's set flags, in bit order.
    Flags(Vec<String>),
    /// A string, in UTF-8.
    Text(String),
    /// A GUID, or a reference to the resource of that GUID.
    Guid(Guid),
    /// The elements of an array.
    List(Vec<Value>),
    /// A struct's fields by name, its bases' first, in the engine's order;
    /// one being written may name only the fields it changes.
    Struct(Vec<(String, Value)>),
    /// A variant: the qualified name of the type it holds and its value.
    Variant(Box<Variant>),
    /// Bytes the codec keeps as they are: designer data, an array whose data
    /// is packed, a string that is not UTF-8. A field without a reflected
    /// type is left out of a struct: written over a base it keeps its bytes,
    /// written into new bytes it is zero.
    Bytes(Vec<u8>),
}

/// The value of a [`Value::Variant`].
///
/// # Examples
///
/// ```
/// use enshwick::resources::{Value, Variant};
///
/// let shape = Value::Variant(Box::new(Variant::new("demo::ecs::SphereShape", Value::Float(1.0))));
/// let Value::Variant(variant) = &shape else { unreachable!() };
/// assert_eq!(variant.type_name(), "demo::ecs::SphereShape");
/// ```
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Variant {
    type_name: String,
    value: Value,
}

impl Variant {
    /// A variant holding `value` of the type qualified as `type_name`.
    ///
    /// # Examples
    ///
    /// See [`Variant`].
    pub fn new(type_name: impl Into<String>, value: Value) -> Self {
        Self {
            type_name: type_name.into(),
            value,
        }
    }

    /// The qualified name of the held type.
    ///
    /// # Examples
    ///
    /// See [`Variant`].
    #[must_use]
    pub fn type_name(&self) -> &str {
        &self.type_name
    }

    /// The held value.
    ///
    /// # Examples
    ///
    /// See [`Variant`].
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// The held value, to change.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::{Value, Variant};
    ///
    /// let mut variant = Variant::new("demo::float32", Value::Float(1.0));
    /// *variant.value_mut() = Value::Float(2.0);
    /// assert_eq!(variant.value(), &Value::Float(2.0));
    /// ```
    pub const fn value_mut(&mut self) -> &mut Value {
        &mut self.value
    }
}

impl Value {
    /// What kind of value this is, as a refusal names it.
    ///
    /// The one match on every kind that the compiler holds whole: crates
    /// above see `Value` as `non_exhaustive`, so a new kind is named here
    /// and decided in `data_to_toml` of the API crate's world store.
    #[doc(hidden)]
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "a bool",
            Self::Int(_) | Self::Uint(_) | Self::Float(_) => "a number",
            Self::Name(_) => "a name",
            Self::Flags(_) => "flags",
            Self::Text(_) => "text",
            Self::Guid(_) => "a GUID",
            Self::List(_) => "a list",
            Self::Struct(_) => "a struct",
            Self::Variant(_) => "a variant",
            Self::Bytes(_) => "bytes",
        }
    }

    /// The field `name` of a struct; `None` for another kind of value or a
    /// field it lacks.
    ///
    /// # Examples
    ///
    /// See [`Value`].
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&Self> {
        let Self::Struct(fields) = self else {
            return None;
        };
        fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// The field `name` of a struct, to change.
    ///
    /// # Examples
    ///
    /// See [`Value`].
    pub fn field_mut(&mut self, name: &str) -> Option<&mut Self> {
        let Self::Struct(fields) = self else {
            return None;
        };
        fields.iter_mut().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}
