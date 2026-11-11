//! The world's mid level as a script reaches it: a writable component type,
//! its fields read and written as one kind each, and their values as game
//! data; a 64-bit value crosses as decimal text, as other game data does.

use super::{World, WritableType};
use crate::__load::*;
use crate::abi::Str;
use crate::game_events::{Field, Scalar};
use crate::resources::{Bytes, Value as Data};
use crate::script::{Class, Function, Object};
use crate::{Entity, Off, WorldError};

/// How a script reads and writes a component field: one of the scalar types
/// [`World::get`] takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub(crate) enum FieldKind {
    /// `u8`.
    U8,
    /// `i8`.
    I8,
    /// `u16`.
    U16,
    /// `i16`.
    I16,
    /// `u32`.
    U32,
    /// `i32`.
    I32,
    /// `u64`.
    U64,
    /// `i64`.
    I64,
    /// `f32`.
    F32,
    /// `f64`.
    F64,
    /// `bool`, one byte.
    Bool,
    /// An entity id, four bytes.
    Entity,
}

/// A field of a [`WritableType`] as a script names it: where it lies and
/// the kind it is read as.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptField {
    type_hash: u32,
    size: u32,
    offset: u64,
    kind: FieldKind,
}

impl ScriptField {
    /// The field as a `T`; `None` when a `T` at its offset leaves the type.
    fn typed<T: Scalar>(self) -> Option<Field<T>> {
        Field::within(self.type_hash, self.size, self.offset)
    }
}

impl Object for ScriptField {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Field",
        "A field of a writable component type, read and written as its kind.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[];
}

impl Object for WritableType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "WritableType",
        "A reflected component type a mod may write, from `world.writable_type`.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_WRITABLE];
}

#[crate::script(writable)]
impl WritableType {
    /// The reflected name, as [`WritableType::name`] gives it.
    #[script(name = "name")]
    pub(crate) fn name_script(&self) -> String {
        self.name().to_owned()
    }

    /// The reflected size, as [`WritableType::size`] gives it.
    #[script(name = "size")]
    pub(crate) fn size_script(&self) -> u32 {
        self.size()
    }

    /// The component type, as [`WritableType::ty`] gives it.
    #[script(name = "ty")]
    pub(crate) fn ty_script(&self) -> crate::ecs::ComponentType {
        self.ty()
    }

    /// The field `name`, read and written as `kind`, as
    /// [`WritableType::field`] resolves one for a Rust type.
    ///
    /// # Errors
    ///
    /// As [`WritableType::field`], and [`Off`] where no loader runs.
    #[script(name = "field")]
    pub(crate) fn field_script(&self, name: &str, kind: FieldKind) -> Result<ScriptField, Off> {
        let host = crate::host().ok_or_else(|| Off::new("no loader runs"))?;
        let mut offset = 0;
        let status =
            (host.api().field_offset)(Str::new(self.name()), Str::new(name), &raw mut offset);
        crate::load::ok(status, (), || format!("field {}.{name}", self.name()))?;
        let field = ScriptField {
            type_hash: self.ty().hash(),
            size: self.size(),
            offset,
            kind,
        };
        let fits = match kind {
            FieldKind::U8 | FieldKind::I8 | FieldKind::Bool => field.typed::<u8>().is_some(),
            FieldKind::U16 | FieldKind::I16 => field.typed::<u16>().is_some(),
            FieldKind::U32 | FieldKind::I32 | FieldKind::F32 | FieldKind::Entity => {
                field.typed::<u32>().is_some()
            }
            FieldKind::U64 | FieldKind::I64 | FieldKind::F64 => field.typed::<u64>().is_some(),
        };
        if fits {
            Ok(field)
        } else {
            Err(Off::new(format!(
                "field {}.{name}: past the end of the type",
                self.name()
            )))
        }
    }
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::writable_type_script`.
    #[doc(hidden)]
    fn writable_type_script(&mut self, name: &str) -> Result<WritableType, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn writable_type_script(&mut self, name: &str) -> Result<WritableType, Off> {
        // Resolved at start only, so each name leaks once per start.
        WritableType::resolve(self, Box::leak(name.to_owned().into_boxed_str()))
    }
}

/// `value` as a whole number, when it is one.
fn whole(value: &Data) -> Option<i128> {
    match value {
        Data::Int(i) => Some(i128::from(*i)),
        Data::Uint(u) => Some(i128::from(*u)),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a whole float in range converts exactly; others fail the range check after"
        )]
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 2f64.powi(64) => Some(*f as i128),
        Data::Text(text) => text.parse().ok(),
        _ => None,
    }
}

/// `value` as a float, when it is a number.
#[expect(
    clippy::cast_precision_loss,
    reason = "a script's numbers are doubles already"
)]
fn float(value: &Data) -> Option<f64> {
    match value {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::Uint(u) => Some(*u as f64),
        _ => None,
    }
}

#[crate::script(fields)]
impl World {
    /// The value of `field` in `entity`'s component, as [`World::get`]
    /// reads it; a 64-bit one as decimal text.
    #[script(name = "get")]
    pub(crate) fn get_script(self, entity: Entity, field: ScriptField) -> Option<Data> {
        macro_rules! read {
            ($t:ty, $to:expr) => {
                self.get(entity, field.typed::<$t>()?).map($to)
            };
        }
        match field.kind {
            FieldKind::U8 => read!(u8, |v| Data::Uint(v.into())),
            FieldKind::I8 => read!(i8, |v| Data::Int(v.into())),
            FieldKind::U16 => read!(u16, |v| Data::Uint(v.into())),
            FieldKind::I16 => read!(i16, |v| Data::Int(v.into())),
            FieldKind::U32 => read!(u32, |v| Data::Uint(v.into())),
            FieldKind::I32 => read!(i32, |v| Data::Int(v.into())),
            FieldKind::U64 => read!(u64, |v| Data::Text(v.to_string())),
            FieldKind::I64 => read!(i64, |v| Data::Text(v.to_string())),
            FieldKind::F32 => read!(f32, |v| Data::Float(v.into())),
            FieldKind::F64 => read!(f64, Data::Float),
            FieldKind::Bool => read!(bool, Data::Bool),
            FieldKind::Entity => read!(Entity, |e| Data::Uint(e.0.into())),
        }
    }

    /// Writes `value` to `field` in `entity`'s component, as [`World::set`]
    /// does.
    ///
    /// # Errors
    ///
    /// As [`World::set`], and [`WorldError::Refused`] for a value the field's
    /// kind does not hold.
    #[script(name = "set")]
    pub(crate) fn set_script(
        self,
        entity: Entity,
        field: ScriptField,
        value: &Data,
    ) -> Result<(), WorldError> {
        macro_rules! write {
            ($t:ty, $value:expr) => {{
                let value: $t = $value.ok_or(WorldError::Refused)?;
                self.set(
                    entity,
                    field.typed::<$t>().ok_or(WorldError::Missing)?,
                    value,
                )
            }};
        }
        let int = whole(value);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "an f32 field takes the nearest f32"
        )]
        let single = float(value).map(|f| f as f32);
        match field.kind {
            FieldKind::U8 => write!(u8, int.and_then(|i| i.try_into().ok())),
            FieldKind::I8 => write!(i8, int.and_then(|i| i.try_into().ok())),
            FieldKind::U16 => write!(u16, int.and_then(|i| i.try_into().ok())),
            FieldKind::I16 => write!(i16, int.and_then(|i| i.try_into().ok())),
            FieldKind::U32 => write!(u32, int.and_then(|i| i.try_into().ok())),
            FieldKind::I32 => write!(i32, int.and_then(|i| i.try_into().ok())),
            FieldKind::U64 => write!(u64, int.and_then(|i| i.try_into().ok())),
            FieldKind::I64 => write!(i64, int.and_then(|i| i.try_into().ok())),
            FieldKind::F32 => write!(f32, single),
            FieldKind::F64 => write!(f64, float(value)),
            FieldKind::Bool => write!(
                bool,
                match value {
                    Data::Bool(b) => Some(*b),
                    _ => None,
                }
            ),
            FieldKind::Entity => write!(Entity, int.and_then(|i| i.try_into().ok()).map(Entity)),
        }
    }

    /// Lets `write` change `entity`'s component of type `ty`, as
    /// [`World::update`] does: it gets the component's bytes and returns
    /// them changed, of the same length, or nil to keep them.
    ///
    /// # Errors
    ///
    /// As [`World::update`], and [`WorldError::Refused`] for bytes of
    /// another length.
    #[script(name = "update")]
    pub(crate) fn update_script(
        self,
        entity: Entity,
        ty: WritableType,
        write: impl Fn(Bytes) -> Option<Bytes>,
    ) -> Result<(), WorldError> {
        let fits = self.update(entity, ty, |bytes| match write(Bytes(bytes.to_vec())) {
            Some(new) if new.0.len() == bytes.len() => {
                bytes.copy_from_slice(&new.0);
                true
            }
            Some(_) => false,
            None => true,
        })?;
        if fits {
            Ok(())
        } else {
            Err(WorldError::Refused)
        }
    }
}
