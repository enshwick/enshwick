//! Resource values as script data, and the resources API as the script
//! namespace `resources`.
//!
//! A value crosses as a plain table: a struct by field name, an array as a
//! list, an enum constant or a GUID as text, a bitmask as the list of its
//! flags' names, a variant as `{ type = <qualified name>, value = ... }`,
//! raw bytes as `{ bytes = { ... } }`. An integer past 2^53, which a script
//! number cannot hold exactly, crosses as its decimal text; text is taken
//! back for any integer.

use super::{Registry, Type, Value as Data};
use crate::abi::ResourceKey;
use crate::content::Guid;
use crate::script::{Answer, Error, FromScript, IntoScript, Ty, Typed, Value};
use crate::{Host, Off};

/// The largest integer a script number holds exactly, 2^53.
const EXACT: u64 = 1 << 53;

fn integer(magnitude: u64, text: impl FnOnce() -> String, number: f64) -> Value {
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
            Self::Bytes(bytes) => Value::Map(vec![(
                "bytes".to_owned(),
                Value::List(
                    bytes
                        .into_iter()
                        .map(|b| Value::Number(f64::from(b)))
                        .collect(),
                ),
            )]),
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
            Value::Number(n) if n.fract() == 0.0 && n.abs() < 9.2e18 => Self::Int(n as i64),
            Value::Number(n) => Self::Float(n),
            Value::String(text) => Self::Text(text),
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

impl Answer for Data {
    const TY: &'static Ty = &Ty::Data;

    /// The script's value; nothing for an error, which leaves the resource
    /// as it was.
    fn from_answer(answer: Result<Value, Error>) -> Self {
        answer
            .ok()
            .and_then(|value| Self::from_value(value).ok())
            .unwrap_or(Self::Nil)
    }
}

fn text(value: impl Into<String>) -> Data {
    Data::Text(value.into())
}

fn attributes(list: &[super::Attribute<'_>]) -> Data {
    Data::List(
        list.iter()
            .map(|a| {
                Data::Struct(vec![
                    ("name".into(), text(a.name())),
                    ("value_type".into(), a.value_type().map_or(Data::Nil, text)),
                    ("value".into(), text(a.value())),
                ])
            })
            .collect(),
    )
}

/// `ty` as script data, by the names of its accessors.
fn describe(ty: &Type<'_>) -> Data {
    let optional = |name: Option<&str>| name.map_or(Data::Nil, text);
    Data::Struct(vec![
        ("name".into(), text(ty.name())),
        ("impact_name".into(), text(ty.impact_name())),
        ("qualified_name".into(), text(ty.qualified_name())),
        ("namespace".into(), text(ty.namespace())),
        ("name_hash".into(), Data::Uint(ty.name_hash().into())),
        ("impact_hash".into(), Data::Uint(ty.impact_hash().into())),
        (
            "qualified_hash".into(),
            Data::Uint(ty.qualified_hash().into()),
        ),
        (
            "internal_hash".into(),
            Data::Uint(ty.internal_hash().into()),
        ),
        ("kind".into(), text(format!("{:?}", ty.kind()))),
        ("size".into(), Data::Uint(ty.size().into())),
        ("align".into(), Data::Uint(ty.align().into())),
        (
            "element_align".into(),
            Data::Uint(ty.element_align().into()),
        ),
        ("flags".into(), Data::Uint(ty.flags().into())),
        ("field_count".into(), Data::Uint(ty.field_count().into())),
        ("inner".into(), optional(ty.inner())),
        ("base".into(), optional(ty.base())),
        (
            "fields".into(),
            Data::List(
                ty.fields()
                    .iter()
                    .map(|f| {
                        Data::Struct(vec![
                            ("name".into(), text(f.name())),
                            ("type_name".into(), optional(f.type_name())),
                            ("offset".into(), Data::Uint(f.offset().into())),
                            ("attributes".into(), attributes(f.attributes())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "constants".into(),
            Data::List(
                ty.constants()
                    .iter()
                    .map(|c| {
                        Data::Struct(vec![
                            ("name".into(), text(c.name())),
                            ("value".into(), Data::Uint(c.value())),
                        ])
                    })
                    .collect(),
            ),
        ),
        ("attributes".into(), attributes(ty.attributes())),
    ])
}

/// The script resources API: the running game's types, its resources by
/// type and as values, and resources served from a script's edit.
///
/// The Rust API offers the same through [`Registry::game`],
/// [`Host::resource_keys`], [`Host::game_value`] and
/// [`Load::serve`](crate::Load::serve).
///
/// # Examples
///
/// ```lua
/// local table = resources.named("demo::Table")
/// for _, key in resources.resource_keys(table.qualified_name) do
///     resources.serve(key, key, function(value)
///         value.damage *= 2
///         return value
///     end)
/// end
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Resources {
    host: Host,
    registry: &'static Registry<'static>,
}

impl Resources {
    /// `key` as script data: `{ guid, type, part }`, the type by its
    /// qualified name when the build has it.
    fn key_data(&self, key: ResourceKey) -> Data {
        let ty = self
            .registry
            .hashed(key.type_hash)
            .map_or(Data::Uint(key.type_hash.into()), |ty| {
                text(ty.qualified_name())
            });
        Data::Struct(vec![
            ("guid".into(), Data::Guid(Guid(key.guid))),
            ("type".into(), ty),
            ("part".into(), Data::Uint(key.part.into())),
        ])
    }

    /// The key `data` names: a GUID as text, a type by qualified name or
    /// hash, a part (0 when left out).
    fn key_of(&self, data: &Data) -> Result<ResourceKey, Off> {
        let guid = match data.field("guid") {
            Some(Data::Text(text)) => Guid::try_parse(text),
            Some(Data::Guid(guid)) => Some(*guid),
            _ => None,
        }
        .ok_or_else(|| Off::new("a key's guid is GUID text"))?;
        let type_hash = match data.field("type") {
            Some(Data::Text(name)) => self
                .registry
                .named(name)
                .map(Type::qualified_hash)
                .ok_or_else(|| Off::new(format!("{name} is not a type of this build")))?,
            Some(Data::Int(hash)) => {
                u32::try_from(*hash).map_err(|_| Off::new("a key's type hash is a u32"))?
            }
            Some(Data::Uint(hash)) => {
                u32::try_from(*hash).map_err(|_| Off::new("a key's type hash is a u32"))?
            }
            _ => return Err(Off::new("a key's type is a type name or hash")),
        };
        let part = match data.field("part") {
            None | Some(Data::Nil) => 0,
            Some(Data::Int(part)) => {
                u32::try_from(*part).map_err(|_| Off::new("a key's part is a u32"))?
            }
            Some(Data::Uint(part)) => {
                u32::try_from(*part).map_err(|_| Off::new("a key's part is a u32"))?
            }
            Some(_) => return Err(Off::new("a key's part is a number")),
        };
        Ok(ResourceKey {
            guid: guid.0,
            type_hash,
            part,
        })
    }
}

#[crate::script(lookups)]
impl Resources {
    /// The type of qualified name `name`, the full name with its namespace, as
    /// a table of its names, hashes, layout, fields, constants and
    /// attributes; nil when the build lacks it.
    #[script]
    pub fn named(self, name: &str) -> Data {
        self.registry.named(name).map_or(Data::Nil, describe)
    }

    /// The type whose qualified name hashes to `hash`, as `named` gives it.
    #[script]
    pub fn hashed(self, hash: u32) -> Data {
        self.registry.hashed(hash).map_or(Data::Nil, describe)
    }

    /// The type of impact name `name`, such as `demo.Table`, as `named`
    /// gives it.
    #[script]
    pub fn impact_named(self, name: &str) -> Data {
        self.registry.impact_named(name).map_or(Data::Nil, describe)
    }

    /// The type whose impact name hashes to `hash`, as `named` gives it.
    #[script]
    pub fn impact_hashed(self, hash: u32) -> Data {
        self.registry
            .impact_hashed(hash)
            .map_or(Data::Nil, describe)
    }

    /// The qualified name of every type of the build.
    #[script]
    #[must_use]
    pub fn type_names(self) -> Vec<String> {
        self.registry
            .iter()
            .map(|ty| ty.qualified_name().to_owned())
            .collect()
    }

    /// The keys of the game's own resources of the type qualified as
    /// `type_name`, or of every resource for nil, as `{ guid, type, part }`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the build lacks and when the archive does not read.
    #[script]
    pub fn resource_keys(self, type_name: Option<String>) -> Result<Data, Off> {
        let ty = match type_name {
            Some(name) => Some(
                self.registry
                    .named(&name)
                    .ok_or_else(|| Off::new(format!("{name} is not a type of this build")))?,
            ),
            None => None,
        };
        let keys = self.host.resource_keys(ty)?;
        Ok(Data::List(
            keys.into_iter().map(|key| self.key_data(key)).collect(),
        ))
    }

    /// The hashes of the game's own contents, each a list of four numbers.
    ///
    /// # Errors
    ///
    /// [`Off`] when the archive does not read.
    #[script]
    pub fn content_hashes(self) -> Result<Data, Off> {
        Ok(Data::List(
            self.host
                .content_hashes()?
                .into_iter()
                .map(|hash| Data::List(hash.into_iter().map(|w| Data::Uint(w.into())).collect()))
                .collect(),
        ))
    }

    /// The value of the game's own resource `key`, read from its archive.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, one the archive lacks, and a resource
    /// that does not read as its type.
    #[script]
    pub fn game_value(self, key: &Data) -> Result<Data, Off> {
        self.host.game_value(self.key_of(key)?)
    }

    /// Serves the resource `key`, built from the resource `base` the first
    /// time the game reads it: `patch` gets the base's value and returns
    /// the value to serve, which may name only the fields it changes; nil
    /// serves the base unchanged. Allowed while the mod initialises.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, a base type the build lacks, and when
    /// the loader refuses the serve, such as after init.
    #[script]
    pub fn serve(
        self,
        key: &Data,
        base: &Data,
        patch: impl Fn(Data) -> Data + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let (key, base) = (self.key_of(key)?, self.key_of(base)?);
        let registry = self.registry;
        let name = registry
            .hashed(base.type_hash)
            .ok_or_else(|| Off::new("the base's type is not a type of this build"))?
            .qualified_name();
        crate::content::serve(self.host, key, base, move |bytes| {
            let value = registry.decode(name, bytes).ok()?;
            match patch(value) {
                Data::Nil => Some(bytes.to_vec()),
                edited => registry.encode(name, &edited, Some(bytes)).ok(),
            }
        })
    }
}

impl crate::script::Bound for Resources {
    const NAME: &'static str = "resources";
    const DOC: &'static str =
        "The game's types, its resources by type and as values, and resources served from an edit.";
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_LOOKUPS];

    fn resolve(load: &mut crate::Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            host: load.host(),
            registry: Registry::game()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::Variant;

    #[test]
    fn values_cross_into_script_data_and_back() {
        let value = Data::Struct(vec![
            ("big".into(), Data::Uint(u64::MAX)),
            ("small".into(), Data::Int(-3)),
            ("ratio".into(), Data::Float(1.5)),
            ("id".into(), Data::Guid(Guid([1; 16]))),
            ("flags".into(), Data::Flags(vec!["Hot".into()])),
            (
                "shape".into(),
                Data::Variant(Box::new(Variant::new("demo::Sphere", Data::Int(2)))),
            ),
            ("raw".into(), Data::Bytes(vec![1, 2])),
        ]);
        let back = Data::from_value(value.into_value()).unwrap();
        assert_eq!(back.field("big"), Some(&Data::Text(u64::MAX.to_string())));
        assert_eq!(back.field("small"), Some(&Data::Int(-3)));
        assert_eq!(back.field("ratio"), Some(&Data::Float(1.5)));
        assert_eq!(
            back.field("id"),
            Some(&Data::Text(Guid([1; 16]).to_string()))
        );
        assert_eq!(
            back.field("flags"),
            Some(&Data::List(vec![Data::Text("Hot".into())]))
        );
        let shape = back.field("shape").unwrap();
        assert_eq!(
            shape.field("type"),
            Some(&Data::Text("demo::Sphere".into()))
        );
        assert_eq!(
            back.field("raw").and_then(|raw| raw.field("bytes")),
            Some(&Data::List(vec![Data::Int(1), Data::Int(2)]))
        );
        assert!(Data::from_value(Value::Vector([0.0; 3])).is_err());
    }
}
