//! A generated type described as data, so a script runtime crosses every
//! generated type through one walk instead of compiled conversions per type:
//! the loader links the descriptions, and only Rust mods link the types they
//! use.

use super::{Constant, Flags, Reflected, ShapeError};
use crate::content::Guid;
use crate::resources::{Value as Data, Variant};
use crate::script::convert::expected;
use crate::script::{Error, FromScript, IntoScript, Value};

/// How a generated type reads from the codec's value and crosses into a
/// script and back: the same values as its Rust type takes, refused the
/// same way.
#[doc(hidden)]
#[derive(Debug)]
pub enum Shape {
    /// A type that is no generated item, such as `u8`, `String` or
    /// [`Guid`], crossing through its own conversions.
    Leaf(Leaf),
    /// An optional value; a box is its inner type's shape.
    Optional(&'static Shape),
    /// An array.
    List(&'static Shape),
    /// A bitmask, the set flags of its flag enum.
    Flags(&'static Shape),
    /// A generated struct.
    Struct(&'static Struct),
    /// A generated enum.
    Enum(&'static Enum),
}

/// A [`Shape::Leaf`]'s conversions, one pair per leaf type.
#[doc(hidden)]
pub struct Leaf {
    shown: fn(&Data) -> Result<Value, ShapeError>,
    taken: fn(Value) -> Result<Data, Error>,
}

impl core::fmt::Debug for Leaf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Leaf")
    }
}

impl Leaf {
    /// The conversions of `T`.
    const fn of<T: Reflected + IntoScript + FromScript>() -> Self {
        Self {
            shown: |value| <T as Reflected>::from_value(value).map(IntoScript::into_value),
            taken: |value| <T as FromScript>::from_value(value).map(|typed| typed.to_value()),
        }
    }
}

/// A generated struct: its Rust path, the type it derives from, and its
/// own fields.
#[doc(hidden)]
#[derive(Debug)]
pub struct Struct {
    name: &'static str,
    base: Option<fn() -> &'static Shape>,
    fields: &'static [Field],
}

impl Struct {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        name: &'static str,
        base: Option<fn() -> &'static Shape>,
        fields: &'static [Field],
    ) -> Self {
        Self { name, base, fields }
    }
}

/// A field of a [`Struct`]: its name in a script, its name in the game, and
/// its shape, asked for later since a type may hold itself.
#[doc(hidden)]
#[derive(Debug)]
pub struct Field {
    rust: &'static str,
    game: &'static str,
    shape: fn() -> &'static Shape,
}

impl Field {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        rust: &'static str,
        game: &'static str,
        shape: fn() -> &'static Shape,
    ) -> Self {
        Self { rust, game, shape }
    }
}

/// A generated enum: its Rust path and each variant's Rust and game name.
#[doc(hidden)]
#[derive(Debug)]
pub struct Enum {
    name: &'static str,
    variants: &'static [(&'static str, &'static str)],
}

impl Enum {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        name: &'static str,
        variants: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self { name, variants }
    }
}

/// The field of a derived struct that holds its base's fields in a script.
const BASE: &str = "base";

impl Shape {
    /// Whether `value` reads as the type.
    ///
    /// # Errors
    ///
    /// [`ShapeError`] when it does not.
    pub fn check(&self, value: &Data) -> Result<(), ShapeError> {
        self.shown(value).map(drop)
    }

    /// `value` read as the type, as a script gets it: a table by the
    /// generated item's field names.
    ///
    /// # Errors
    ///
    /// [`ShapeError`] when it does not read as the type.
    pub fn shown(&self, value: &Data) -> Result<Value, ShapeError> {
        match self {
            Self::Leaf(leaf) => (leaf.shown)(value),
            Self::Optional(inner) => match value {
                Data::Nil => Ok(Value::Nil),
                some => inner.shown(some),
            },
            Self::List(inner) => match value {
                Data::List(items) => items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| inner.shown(item).map_err(|e| e.within(&i.to_string())))
                    .collect::<Result<_, _>>()
                    .map(Value::List),
                other => Err(ShapeError::new("an array", other)),
            },
            Self::Flags(inner) => match value {
                Data::Flags(names) => names
                    .iter()
                    .map(|name| inner.shown(&Data::Name(name.clone())))
                    .collect::<Result<_, _>>()
                    .map(Value::List),
                Data::Uint(bits) => (0..64)
                    .filter(|bit| bits & (1 << bit) != 0)
                    .map(|bit| inner.shown(&Data::Uint(bit)))
                    .collect::<Result<_, _>>()
                    .map(Value::List),
                other => Err(ShapeError::new("a set of flags", other)),
            },
            Self::Struct(record) => record.shown(value),
            Self::Enum(choice) => choice.shown(value),
        }
    }

    /// A script's `value` read as the type, as the codec writes it: a field
    /// the table leaves out is nil, and a key no field has is refused.
    ///
    /// # Errors
    ///
    /// [`Error`] when it does not read as the type.
    pub fn taken(&self, value: Value) -> Result<Data, Error> {
        match self {
            Self::Leaf(leaf) => (leaf.taken)(value),
            Self::Optional(inner) => match value {
                Value::Nil => Ok(Data::Nil),
                some => inner.taken(some),
            },
            Self::List(inner) => inner.taken_list(value).map(Data::List),
            Self::Flags(inner) => inner
                .taken_list(value)
                .map(|values| super::flags_value(&values)),
            Self::Struct(record) => record.taken(value),
            Self::Enum(choice) => choice.taken(value),
        }
    }

    /// The items of the script's list `value`, each read as the type.
    fn taken_list(&self, value: Value) -> Result<Vec<Data>, Error> {
        let Value::List(items) = value else {
            return Err(expected("a list", &value));
        };
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                self.taken(item)
                    .map_err(|e| e.within(&format!("[{}]", i + 1)))
            })
            .collect()
    }
}

impl Struct {
    fn shown(&self, value: &Data) -> Result<Value, ShapeError> {
        let mut fields = Vec::with_capacity(usize::from(self.base.is_some()) + self.fields.len());
        if let Some(base) = self.base {
            fields.push((BASE.to_owned(), base().shown(value)?));
        }
        for field in self.fields {
            let found = value.field(field.game).ok_or_else(|| ShapeError {
                path: vec![field.game.to_owned()],
                expected: "a field this build lacks".to_owned(),
            })?;
            let shown = (field.shape)()
                .shown(found)
                .map_err(|e| e.within(field.game))?;
            fields.push((field.rust.to_owned(), shown));
        }
        Ok(Value::Map(fields))
    }

    fn taken(&self, value: Value) -> Result<Data, Error> {
        let mut entries = match value {
            Value::Map(entries) => entries,
            // An empty table reads as a list: a struct without fields, such
            // as a base, written `{}`.
            Value::List(items) if items.is_empty() => Vec::new(),
            other => return Err(expected("a table", &other)),
        };
        let known = |name: &str| {
            (self.base.is_some() && name == BASE) || self.fields.iter().any(|f| f.rust == name)
        };
        if let Some((name, _)) = entries.iter().find(|(name, _)| !known(name)) {
            return Err(Error::new(format!("{name:?} is no field of {}", self.name)));
        }
        let mut take = |name: &str| {
            entries
                .iter()
                .position(|(held, _)| held == name)
                .map_or(Value::Nil, |at| entries.swap_remove(at).1)
        };
        let mut fields = Vec::with_capacity(self.fields.len());
        if let Some(base) = self.base
            && let Data::Struct(inherited) = base().taken(take(BASE)).map_err(|e| e.within(BASE))?
        {
            fields = inherited;
        }
        for field in self.fields {
            let taken = (field.shape)()
                .taken(take(field.rust))
                .map_err(|e| e.within(field.rust))?;
            fields.push((field.game.to_owned(), taken));
        }
        Ok(Data::Struct(fields))
    }
}

impl Enum {
    fn shown(&self, value: &Data) -> Result<Value, ShapeError> {
        let constant = Constant::from_value(value)?;
        if let Constant::Name(name) = &constant
            && let Some((rust, _)) = self.variants.iter().find(|(_, game)| game == name)
        {
            return Ok(Value::String((*rust).to_owned()));
        }
        Ok(constant.into_script())
    }

    fn taken(&self, value: Value) -> Result<Data, Error> {
        match value {
            Value::String(text) => self
                .variants
                .iter()
                .find(|(rust, _)| *rust == text)
                .map(|(_, game)| Data::Name((*game).to_owned()))
                .ok_or_else(|| super::no_variant(&text, self.name)),
            // A name the build gives a variant is that variant's own value.
            other => Constant::from_script(other).map(|constant| constant.to_value()),
        }
    }
}

/// A type with a [`Shape`]: every generated item and every type a
/// generated field holds.
#[doc(hidden)]
pub trait Shaped {
    /// The type's shape.
    const SHAPE: &'static Shape;
}

macro_rules! leaves {
    ($($ty:ty),* $(,)?) => {$(
        impl Shaped for $ty {
            const SHAPE: &'static Shape = &Shape::Leaf(Leaf::of::<$ty>());
        }
    )*};
}

leaves!(
    u8,
    u16,
    u32,
    u64,
    i8,
    i16,
    i32,
    i64,
    bool,
    f32,
    String,
    Guid,
    Data,
    Option<Variant>
);

impl<T: Shaped> Shaped for Box<T> {
    const SHAPE: &'static Shape = T::SHAPE;
}

impl<T: Shaped> Shaped for Option<T> {
    const SHAPE: &'static Shape = &Shape::Optional(T::SHAPE);
}

impl<T: Shaped> Shaped for Vec<T> {
    const SHAPE: &'static Shape = &Shape::List(T::SHAPE);
}

impl<E: Shaped> Shaped for Flags<E> {
    const SHAPE: &'static Shape = &Shape::Flags(E::SHAPE);
}
