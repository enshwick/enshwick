//! What a script subscribes to: a loader event, a typed game event, or a
//! game event read by its reflected type and the fields the script names.

use enshwick::Entity;
use enshwick::abi::Event;
use enshwick::game_events::{Field, RawGameEvent};
use enshwick::script::{self, EventKind, GameEventKind};
use enshwick::{Load, Off};
use mlua::{Lua, Table, Value as LuaValue};

/// One subscription; handlers of equal ones share the loader's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Subscription {
    /// A loader event of `enshwick::events`.
    Loader(Event),
    /// A typed game event, by its name.
    Game(&'static str),
    /// A game event by reflected type, with the fields it reads.
    Raw(RawSpec),
    /// Another mod's extension point, by its name.
    Point(String),
    /// A mod event, by its name.
    ModEvent(String),
}

/// A game event type and the fields a script reads from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawSpec {
    pub(crate) type_name: String,
    pub(crate) fields: Vec<(String, FieldKind)>,
}

/// How a field of a reflected game event is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    Bool,
    Entity,
}

impl FieldKind {
    /// Every kind with the name a script gives it.
    const NAMES: [(&'static str, Self); 12] = [
        ("u8", Self::U8),
        ("i8", Self::I8),
        ("u16", Self::U16),
        ("i16", Self::I16),
        ("u32", Self::U32),
        ("i32", Self::I32),
        ("u64", Self::U64),
        ("i64", Self::I64),
        ("f32", Self::F32),
        ("f64", Self::F64),
        ("bool", Self::Bool),
        ("entity", Self::Entity),
    ];

    fn parse(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, k)| *k)
    }

    /// The names a script may give, for an error.
    fn known() -> String {
        Self::NAMES.map(|(n, _)| n).join(", ")
    }
}

impl Subscription {
    /// The subscription `name` names: a loader event or a typed game event.
    pub(crate) fn named(name: &str) -> Option<Self> {
        if let Some(kind) = script::events().find(|k| k.name() == name) {
            return Some(Self::Loader(kind.event()));
        }
        script::game_events()
            .find(|k| k.name() == name)
            .map(|k| Self::Game(k.name()))
    }

    /// A reflected game event `type_name` with the field kinds `fields`
    /// names, `{ field = "u32" }`.
    pub(crate) fn raw(type_name: String, fields: &Table) -> mlua::Result<Self> {
        let mut read = Vec::new();
        for pair in fields.pairs::<String, String>() {
            let (field, kind) = pair?;
            let kind = FieldKind::parse(&kind).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "enshwick.on_game_type: {field}: {kind:?} is none of {}",
                    FieldKind::known()
                ))
            })?;
            read.push((field, kind));
        }
        read.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        Ok(Self::Raw(RawSpec {
            type_name,
            fields: read,
        }))
    }

    /// What the log calls it.
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Loader(event) => script::events()
                .find(|k| k.event() == *event)
                .map_or_else(|| format!("event {}", event.0), |k| k.name().to_owned()),
            Self::Game(name) => (*name).to_owned(),
            Self::Raw(spec) => spec.type_name.clone(),
            Self::Point(name) | Self::ModEvent(name) => name.clone(),
        }
    }

    /// The loader event's script entry.
    pub(crate) fn loader_kind(event: Event) -> Result<&'static EventKind, Off> {
        script::events()
            .find(|k| k.event() == event)
            .ok_or_else(|| Off::new(format!("event {} has no script entry", event.0)))
    }

    /// The typed game event's script entry.
    pub(crate) fn game_kind(name: &str) -> Result<&'static GameEventKind, Off> {
        script::game_events()
            .find(|k| k.name() == name)
            .ok_or_else(|| Off::new(format!("game event {name} has no script entry")))
    }
}

/// A value read from a raw game event.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RawValue {
    Number(f64),
    Key(u64),
    Signed(i64),
    Bool(bool),
}

/// A field resolved for the running build.
#[derive(Debug)]
enum Resolved {
    U8(Field<u8>),
    I8(Field<i8>),
    U16(Field<u16>),
    I16(Field<i16>),
    U32(Field<u32>),
    I32(Field<i32>),
    U64(Field<u64>),
    I64(Field<i64>),
    F32(Field<f32>),
    F64(Field<f64>),
    Bool(Field<bool>),
    Entity(Field<Entity>),
}

/// The fields of `spec` resolved for the running build, each read into a
/// [`RawValue`] by `read`.
pub(crate) struct RawReader {
    fields: Vec<(std::sync::Arc<str>, Resolved)>,
}

impl core::fmt::Debug for RawReader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RawReader")
            .field("fields", &self.fields.len())
            .finish()
    }
}

impl RawReader {
    /// Resolves `spec` with `load`, the type found by name among `types`,
    /// the build's game event types.
    pub(crate) fn resolve(
        load: &mut Load<'_>,
        spec: &RawSpec,
        types: &[(&'static str, u32)],
    ) -> Result<(enshwick::game_events::EventType, Self), Off> {
        let name = types
            .iter()
            .map(|(name, _)| *name)
            .find(|name| *name == spec.type_name)
            .ok_or_else(|| {
                Off::new(format!(
                    "enshwick.on_game_type: no game event type {} on this build",
                    spec.type_name
                ))
            })?;
        let ty = load.game_event_type(name)?;
        let mut fields = Vec::new();
        for (field, kind) in &spec.fields {
            let resolved = match kind {
                FieldKind::U8 => Resolved::U8(ty.field(load, field)?),
                FieldKind::I8 => Resolved::I8(ty.field(load, field)?),
                FieldKind::U16 => Resolved::U16(ty.field(load, field)?),
                FieldKind::I16 => Resolved::I16(ty.field(load, field)?),
                FieldKind::U32 => Resolved::U32(ty.field(load, field)?),
                FieldKind::I32 => Resolved::I32(ty.field(load, field)?),
                FieldKind::U64 => Resolved::U64(ty.field(load, field)?),
                FieldKind::I64 => Resolved::I64(ty.field(load, field)?),
                FieldKind::F32 => Resolved::F32(ty.field(load, field)?),
                FieldKind::F64 => Resolved::F64(ty.field(load, field)?),
                FieldKind::Bool => Resolved::Bool(ty.field(load, field)?),
                FieldKind::Entity => Resolved::Entity(ty.field(load, field)?),
            };
            fields.push((std::sync::Arc::from(field.as_str()), resolved));
        }
        Ok((ty, Self { fields }))
    }

    /// Every field of `event` this reader reads; a field the event lacks is
    /// left out.
    pub(crate) fn read(&self, event: &RawGameEvent<'_>) -> Vec<(std::sync::Arc<str>, RawValue)> {
        self.fields
            .iter()
            .filter_map(|(name, field)| {
                let value = match field {
                    Resolved::U8(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::I8(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::U16(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::I16(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::U32(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::I32(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::U64(f) => RawValue::Key(event.get(*f)?),
                    Resolved::I64(f) => RawValue::Signed(event.get(*f)?),
                    Resolved::F32(f) => RawValue::Number(f64::from(event.get(*f)?)),
                    Resolved::F64(f) => RawValue::Number(event.get(*f)?),
                    Resolved::Bool(f) => RawValue::Bool(event.get(*f)?),
                    Resolved::Entity(f) => RawValue::Number(f64::from(event.get(*f)?.0)),
                };
                Some((std::sync::Arc::clone(name), value))
            })
            .collect()
    }
}

/// `values` as a Luau table; 64-bit values as their decimal digits, as the
/// API gives keys.
pub(crate) fn raw_table(
    lua: &Lua,
    values: &[(std::sync::Arc<str>, RawValue)],
) -> mlua::Result<LuaValue> {
    let table = lua.create_table_with_capacity(0, values.len())?;
    for (name, value) in values {
        let value = match value {
            RawValue::Number(n) => LuaValue::Number(*n),
            RawValue::Key(k) => LuaValue::String(lua.create_string(k.to_string())?),
            RawValue::Signed(i) => LuaValue::String(lua.create_string(i.to_string())?),
            RawValue::Bool(b) => LuaValue::Boolean(*b),
        };
        table.raw_set(&**name, value)?;
    }
    Ok(LuaValue::Table(table))
}
