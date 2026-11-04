//! What every generated item stands on: the conversion between a Rust value
//! and the codec's [`Value`], and the handles that resolve a component,
//! event or resource on the running build and check its shape.

use core::marker::PhantomData;

use crate::content::Guid;
use crate::resources::{Registry, Type, Value, Variant};
use crate::world::{World, WritableType};
use crate::{Entity, Host, Load, Off};

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
pub trait Named: Reflected + sealed::Sealed {
    /// Enshwick's id for the type, which the loader maps to the running
    /// build's type.
    const ID: &'static str;
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
}

/// The set flags of a bitmask, each a constant of its generated flag enum
/// `E`, in bit order.
///
/// A bitmask with a bit no constant names reads as numbers: each flag is
/// `E`'s `Other` with [`Constant::Number`] of its bit. Written back, the
/// flags are names when each has one, else the bits of the numbered ones.
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
        let names: Option<Vec<String>> = values
            .iter()
            .map(|v| match v {
                Value::Name(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.map_or_else(
            || {
                Value::Uint(values.iter().fold(0, |bits, v| match v {
                    Value::Uint(bit) if *bit < 64 => bits | (1 << bit),
                    _ => bits,
                }))
            },
            Value::Flags,
        )
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

/// The table of every generated type with a role: its id and a check that a
/// value reads as it, for tests that hold the generated items against a
/// build's types.
macro_rules! named_table {
    ($($id:literal => $ty:ty,)*) => {
        /// Every generated type with a role, by id, with a check that a
        /// value reads as it.
        #[doc(hidden)]
        pub const NAMED: &[(
            &str,
            fn(&$crate::resources::Value) -> ::core::result::Result<(), $crate::typed::ShapeError>,
        )] = &[$(($id, $crate::typed::runtime::check::<$ty>)),*];
    };
}
pub(crate) use named_table;

/// A generated struct: its fields by the game's names, read from and
/// written to a [`Value`]; a derived type holds its base first.
macro_rules! reflected_struct {
    (
        $(#[$meta:meta])*
        $name:ident $(: $base:ty)? {
            $($(#[$field_meta:meta])* $field:ident: $ty:ty = $game:literal,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq)]
        #[non_exhaustive]
        pub struct $name {
            $(
                /// The fields of the type this one derives from.
                pub base: $base,
            )?
            $($(#[$field_meta])* pub $field: $ty,)*
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
/// for a constant it does not.
macro_rules! reflected_enum {
    (
        $(#[$meta:meta])*
        $name:ident {
            $($(#[$variant_meta:meta])* $variant:ident = $game:literal,)*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)*
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
        }
        $(impl $crate::typed::$role for $name {})+
    };
}
pub(crate) use named;

macro_rules! integers {
    ($($int:ty => $name:literal),* $(,)?) => {$(
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
                integer_value(i128::from(*self))
            }
        }
    )*};
}

integers!(
    u8 => "a u8", u16 => "a u16", u32 => "a u32", u64 => "a u64",
    i8 => "an i8", i16 => "an i16", i32 => "an i32", i64 => "an i64",
);

/// An integer as the codec reads one: signed only when negative.
fn integer_value(n: i128) -> Value {
    u64::try_from(n).map_or_else(
        |_| Value::Int(i64::try_from(n).unwrap_or(i64::MIN)),
        Value::Uint,
    )
}

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

/// An array; one the codec keeps as bytes reads byte by byte.
impl<T: Reflected> Reflected for Vec<T> {
    fn from_value(value: &Value) -> Result<Self, ShapeError> {
        match value {
            Value::List(items) => items
                .iter()
                .enumerate()
                .map(|(i, item)| T::from_value(item).map_err(|e| e.within(&i.to_string())))
                .collect(),
            Value::Bytes(bytes) => bytes
                .iter()
                .map(|b| T::from_value(&Value::Uint(u64::from(*b))))
                .collect(),
            other => Err(ShapeError::new("an array", other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::List(self.iter().map(T::to_value).collect())
    }
}

/// The running build's type for `T`, its shape checked: the bytes of a
/// zeroed value of the build's type must read as `T`, so a field the build
/// renamed, removed or retyped turns off only `T`.
fn resolve<T: Named>(load: &Load<'_>) -> Result<&'static Type<'static>, Off> {
    let host = load.host();
    let hash = crate::ecs::ComponentType::resolve(load, T::ID)?.hash();
    let registry = Registry::game()?;
    let ty = registry
        .hashed(hash)
        .ok_or_else(|| off::<T>(host, "the type registry lacks it"))?;
    let size = usize::try_from(ty.size()).unwrap_or(usize::MAX);
    let zero = registry
        .decode(ty.qualified_name(), &vec![0; size])
        .map_err(|e| off::<T>(host, &e.to_string()))?;
    T::from_value(&zero).map_err(|e| off::<T>(host, &e.to_string()))?;
    Ok(ty)
}

/// Turns `T` off on the running build, naming the build and why.
fn off<T: Named>(host: Host, why: &str) -> Off {
    Off::new(format!(
        "typed {}: on game build {}, {why}",
        T::ID,
        host.build().fingerprint.age
    ))
}

/// A generated component type resolved on the running build: reads and
/// writes of an entity's component as its generated struct.
///
/// # Examples
///
/// ```
/// use enshwick::typed::{Component, ComponentType};
///
/// fn watch<T: Component + std::fmt::Debug>(
///     load: &mut enshwick::Load<'_>,
///     entity: enshwick::Entity,
/// ) -> Result<(), enshwick::Off> {
///     let ty = ComponentType::<T>::resolve(load)?;
///     let world = load.world()?;
///     enshwick::info!("{:?}", ty.read(world, entity)?);
///     Ok(())
/// }
/// ```
pub struct ComponentType<T> {
    ty: crate::ecs::ComponentType,
    name: &'static str,
    size: usize,
    writable: Option<WritableType>,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for ComponentType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ComponentType<T> {}

impl<T> core::fmt::Debug for ComponentType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ComponentType")
            .field("name", &self.name)
            .field("writable", &self.writable.is_some())
            .finish_non_exhaustive()
    }
}

impl<T: Component> ComponentType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let ty = resolve::<T>(load)?;
        Ok(Self {
            ty: crate::ecs::ComponentType::resolve(load, T::ID)?,
            name: ty.qualified_name(),
            size: usize::try_from(ty.size()).unwrap_or(usize::MAX),
            writable: WritableType::resolve(load, T::ID).ok(),
            of: PhantomData,
        })
    }

    /// `entity`'s component on the game thread; `None` when the entity has
    /// none or off the game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] when the component's bytes do not read as `T`.
    ///
    /// # Examples
    ///
    /// See [`ComponentType`].
    pub fn read(&self, world: World, entity: Entity) -> Result<Option<T>, Off> {
        let Some(data) = world.component(entity, self.ty) else {
            return Ok(None);
        };
        let mut bytes = vec![0; self.size];
        // SAFETY: `World::component` returned the entity's live component,
        // valid until the game thread moves on and read at once; `size` is
        // the reflected size of its type on the running build.
        unsafe { crate::raw::read_bytes_at(data.cast(), 0, &mut bytes) };
        let value = Registry::game()?.decode(self.name, &bytes)?;
        Ok(Some(T::from_value(&value)?))
    }

    /// Writes `value` to `entity`'s component, marked changed the game's
    /// way, keeping the bytes of what `T` does not hold. On the game thread
    /// only.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the loader does not let mods write, a value that
    /// does not fit the type, or as [`World::update`] fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::typed::{Component, ComponentType};
    ///
    /// fn copy<T: Component>(
    ///     ty: ComponentType<T>,
    ///     world: enshwick::world::World,
    ///     from: enshwick::Entity,
    ///     to: enshwick::Entity,
    /// ) -> Result<(), enshwick::Off> {
    ///     match ty.read(world, from)? {
    ///         Some(value) => ty.write(world, to, &value),
    ///         None => Ok(()),
    ///     }
    /// }
    /// ```
    pub fn write(&self, world: World, entity: Entity, value: &T) -> Result<(), Off> {
        let writable = self
            .writable
            .ok_or_else(|| Off::new(format!("typed {}: mods may not write it", T::ID)))?;
        let value = value.to_value();
        let name = self.name;
        world
            .update(entity, writable, |bytes| {
                let written = Registry::game()?.encode(name, &value, Some(bytes))?;
                if written.len() != bytes.len() {
                    return Err(Off::new(format!(
                        "typed {}: a value of {} bytes, not {}",
                        T::ID,
                        written.len(),
                        bytes.len()
                    )));
                }
                bytes.copy_from_slice(&written);
                Ok(())
            })
            .map_err(|why| Off::new(format!("typed {}: {why}", T::ID)))?
    }
}

/// A generated game event type resolved on the running build, to handle
/// its events as the generated struct.
///
/// # Examples
///
/// ```
/// use enshwick::typed::{Event, EventType};
///
/// fn log<T: Event + std::fmt::Debug>(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     EventType::<T>::resolve(load)?.on(load, |event| enshwick::info!("{event:?}"))
/// }
/// ```
pub struct EventType<T> {
    ty: crate::game_events::EventType,
    name: &'static str,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for EventType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for EventType<T> {}

impl<T> core::fmt::Debug for EventType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EventType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<T: Event + 'static> EventType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`EventType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let ty = resolve::<T>(load)?;
        Ok(Self {
            ty: load.game_event_type(T::ID)?,
            name: ty.qualified_name(),
            of: PhantomData,
        })
    }

    /// Calls `handler` with each event of the type the game raises, read
    /// as `T`; an event whose bytes do not read as `T` is logged once and
    /// skipped.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the handler, such as after init.
    ///
    /// # Examples
    ///
    /// See [`EventType`].
    pub fn on<F>(self, load: &mut Load<'_>, handler: F) -> Result<(), Off>
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let name = self.name;
        load.on_raw_game_event(self.ty, move |raw| {
            let read = Registry::game()
                .and_then(|registry| Ok(registry.decode(name, raw.bytes())?))
                .and_then(|value| Ok(T::from_value(&value)?));
            match read {
                Ok(event) => handler(&event),
                Err(why) => crate::warn_once!("typed {}: an event does not read: {why}", T::ID),
            }
        })
    }
}

/// A generated resource type resolved on the running build: the keys of
/// its resources, their game values and changes served in load order.
///
/// # Examples
///
/// ```
/// use enshwick::content::Guid;
/// use enshwick::typed::{Resource, ResourceType};
///
/// fn keep<T: Resource>(load: &mut enshwick::Load<'_>, guid: Guid) -> Result<(), enshwick::Off> {
///     let ty = ResourceType::<T>::resolve(load)?;
///     let key = ty.key(guid);
///     ty.serve(load, key, key, |_value| {})
/// }
/// ```
pub struct ResourceType<T> {
    hash: u32,
    name: &'static str,
    of: PhantomData<fn() -> T>,
}

impl<T> Clone for ResourceType<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ResourceType<T> {}

impl<T> core::fmt::Debug for ResourceType<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResourceType")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl<T: Resource + 'static> ResourceType<T> {
    /// `T` on the running build, its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] when the build lacks the type or its shape differs from
    /// `T`, naming the build.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn resolve(load: &Load<'_>) -> Result<Self, Off> {
        let ty = resolve::<T>(load)?;
        Ok(Self {
            hash: ty.qualified_hash(),
            name: ty.qualified_name(),
            of: PhantomData,
        })
    }

    /// The key of the resource of this type with GUID `guid`, part 0.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    #[must_use]
    pub fn key(&self, guid: Guid) -> crate::abi::ResourceKey {
        guid.key_by_hash(self.hash)
    }

    /// The game's own resource `key`, as the archive holds it.
    ///
    /// # Errors
    ///
    /// [`Off`] for a key of another type, outside the game, for a key the
    /// archive lacks, and when its bytes do not read as `T`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::Guid;
    /// use enshwick::typed::{Resource, ResourceType};
    ///
    /// fn read<T: Resource>(ty: ResourceType<T>, guid: Guid) -> Result<T, enshwick::Off> {
    ///     ty.game_value(ty.key(guid))
    /// }
    /// ```
    pub fn game_value(&self, key: crate::abi::ResourceKey) -> Result<T, Off> {
        if key.type_hash != self.hash {
            return Err(Off::new(format!(
                "typed {}: a key of type {:#010x}",
                T::ID,
                key.type_hash
            )));
        }
        let host = crate::host().ok_or_else(|| Off::new("typed resources: outside the game"))?;
        Ok(T::from_value(&host.game_value(key)?)?)
    }

    /// Serves the resource `key`, made from the resource `base` read as
    /// `T` and changed by `change`, the first time the game reads it; the
    /// bytes of what `T` does not hold stay as `base` has them. Mods that
    /// serve one key change it in load order, each on the one before's
    /// result, as [`Load::serve`] does.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses it, such as after init.
    ///
    /// # Examples
    ///
    /// See [`ResourceType`].
    pub fn serve<F>(
        &self,
        load: &mut Load<'_>,
        key: crate::abi::ResourceKey,
        base: crate::abi::ResourceKey,
        change: F,
    ) -> Result<(), Off>
    where
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        let name = self.name;
        load.serve(key, base, move |bytes| {
            let registry = Registry::game().ok()?;
            let mut value = T::from_value(&registry.decode(name, bytes).ok()?).ok()?;
            change(&mut value);
            registry.encode(name, &value.to_value(), Some(bytes)).ok()
        })
    }
}
