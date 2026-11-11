//! Resource values as script data, and the resources API as the script
//! namespace `resources`.
//!
//! A value crosses as a plain table: a struct by field name, an array as a
//! list, an enum constant or a GUID as text, a bitmask as the list of its
//! flags' names, a variant as `{ type = <qualified name>, value = ... }`,
//! raw bytes as a `buffer`, an absent optional or variant as an absent field,
//! or as `resources.none()` in a list, which nil would cut short. An integer
//! past 2^53, which a script
//! number cannot hold exactly, crosses as its decimal text; text is taken
//! back for any integer.

use super::{Registry, Type, Value as Data};
use crate::abi::ResourceKey;
use crate::script::{Error, FromScript, IntoScript, Ty, Typed, Value};
use crate::{Host, Off};
use enshwick_values::content::Guid;

/// Raw bytes a script passes and gets as a Luau `buffer`: a content's bytes,
/// a resource's bytes before or after the codec.
///
/// # Examples
///
/// ```
/// # extern crate enshwick_core as enshwick;
/// let bytes = enshwick::resources::Bytes(vec![1, 2, 3]);
/// assert_eq!(bytes.0.len(), 3);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl Typed for Bytes {
    const TY: &'static Ty = &Ty::Bytes;
}

impl IntoScript for Bytes {
    fn into_value(self) -> Value {
        Value::Bytes(self.0)
    }
}

impl FromScript for Bytes {
    fn from_value(value: Value) -> Result<Self, Error> {
        match value {
            Value::Bytes(bytes) => Ok(Self(bytes)),
            Value::String(text) => Ok(Self(text.into_bytes())),
            _ => Err(Error::new("expected a buffer")),
        }
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

/// The content hash `data` names: a list of four numbers.
fn hash_of(data: &Data) -> Result<[u32; 4], Off> {
    let malformed = || Off::new("a content hash is four numbers");
    let Data::List(words) = data else {
        return Err(malformed());
    };
    let words: Vec<u32> = words
        .iter()
        .map(|word| match word {
            Data::Int(w) => u32::try_from(*w).ok(),
            Data::Uint(w) => u32::try_from(*w).ok(),
            _ => None,
        })
        .collect::<Option<_>>()
        .ok_or_else(malformed)?;
    words.try_into().map_err(|_| malformed())
}

/// The script resources API: the running game's types, its resources by
/// type and as values, and resources served from a script's edit.
///
/// The Rust API offers the same through [`Registry::game`],
/// [`Host::resource_keys`], [`Host::game_value`], [`Host::served_value`]
/// and [`Load::serve`](crate::Load::serve).
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
}

impl Resources {
    /// `key` as script data: `{ guid, type, part }`, the type by its
    /// qualified name when the build has it.
    pub(crate) fn key_data(registry: &Registry<'_>, key: ResourceKey) -> Data {
        let ty = registry
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
    pub(crate) fn key_of(data: &Data) -> Result<ResourceKey, Off> {
        let guid = match data.field("guid") {
            Some(Data::Text(text)) => Guid::try_parse(text),
            Some(Data::Guid(guid)) => Some(*guid),
            _ => None,
        }
        .ok_or_else(|| Off::new("a key's guid is GUID text"))?;
        let type_hash = match data.field("type") {
            Some(Data::Text(name)) => Registry::game()?
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
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a pure helper reads nothing of it"
)]
impl Resources {
    /// The type of qualified name `name`, the full name with its namespace, as
    /// a table of its names, hashes, layout, fields, constants and
    /// attributes; nil when the build lacks it.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game and when the game holds no reflection data,
    /// as for every call that reads the types.
    #[script]
    pub fn named(self, name: &str) -> Result<Option<Data>, Off> {
        Ok(Registry::game()?.named(name).map(describe))
    }

    /// The type whose qualified name hashes to `hash`, as `named` gives it.
    ///
    /// # Errors
    ///
    /// [`Off`] as for `named`.
    #[script]
    pub fn hashed(self, hash: u32) -> Result<Option<Data>, Off> {
        Ok(Registry::game()?.hashed(hash).map(describe))
    }

    /// The type of impact name `name`, such as `demo.Table`, as `named`
    /// gives it.
    ///
    /// # Errors
    ///
    /// [`Off`] as for `named`.
    #[script]
    pub fn impact_named(self, name: &str) -> Result<Option<Data>, Off> {
        Ok(Registry::game()?.impact_named(name).map(describe))
    }

    /// The type whose impact name hashes to `hash`, as `named` gives it.
    ///
    /// # Errors
    ///
    /// [`Off`] as for `named`.
    #[script]
    pub fn impact_hashed(self, hash: u32) -> Result<Option<Data>, Off> {
        Ok(Registry::game()?.impact_hashed(hash).map(describe))
    }

    /// The qualified name of every type of the build.
    ///
    /// # Errors
    ///
    /// [`Off`] as for `named`.
    #[script]
    pub fn type_names(self) -> Result<Vec<String>, Off> {
        Ok(Registry::game()?
            .iter()
            .map(|ty| ty.qualified_name().to_owned())
            .collect())
    }

    /// The value that stands for an absent optional or variant where nil
    /// cannot: an item of a list, which nil would cut short, and a field an
    /// edit clears, since a field the edit leaves out keeps its value.
    #[script]
    #[must_use]
    pub fn none(self) -> Data {
        Data::Nil
    }

    /// The keys of the game's own resources of the type qualified as
    /// `type_name`, or of every resource for nil, as `{ guid, type, part }`.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the build lacks and when the archive does not read.
    #[script]
    pub fn resource_keys(self, type_name: Option<String>) -> Result<Data, Off> {
        let registry = Registry::game()?;
        let ty = match type_name {
            Some(name) => Some(
                registry
                    .named(&name)
                    .ok_or_else(|| Off::new(format!("{name} is not a type of this build")))?,
            ),
            None => None,
        };
        let keys = self.host.resource_keys(ty)?;
        Ok(Data::List(
            keys.into_iter()
                .map(|key| Self::key_data(registry, key))
                .collect(),
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

    /// The bytes of the game's own content `hash`, four numbers as
    /// `content_hashes` lists them; nil for a hash the archive lacks.
    #[script]
    #[must_use]
    pub fn game_content(self, hash: &Data) -> Option<Bytes> {
        self.host.game_content(hash_of(hash).ok()?).map(Bytes)
    }

    /// Serves `bytes` as game content, such as a texture's pixels, and
    /// returns the hash a resource names it by, four numbers; the same bytes
    /// give the same hash.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the content.
    #[script]
    pub fn serve_content(self, bytes: &Bytes) -> Result<Data, Off> {
        let hash = self.host.serve_content(&bytes.0)?;
        Ok(Data::List(
            hash.into_iter().map(|w| Data::Uint(w.into())).collect(),
        ))
    }

    /// The value of `bytes`, a resource of the type qualified as
    /// `type_name`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the bytes do not read as that type.
    #[script]
    pub fn decode(self, type_name: &str, bytes: &Bytes) -> Result<Data, Off> {
        Ok(Registry::game()?.decode(type_name, &bytes.0)?)
    }

    /// `value`, of the type qualified as `type_name`, as resource bytes:
    /// written over `base` when given, keeping the bytes of what did not
    /// change, else into new bytes.
    ///
    /// # Errors
    ///
    /// [`Off`] when the value does not fit the type.
    #[script]
    pub fn encode(self, type_name: &str, value: &Data, base: Option<Bytes>) -> Result<Bytes, Off> {
        let base = base.map(|base| base.0);
        Ok(Bytes(Registry::game()?.encode(
            type_name,
            value,
            base.as_deref(),
        )?))
    }

    /// Declares the mod's content id `id`, `<mod>:<name>`, and returns its
    /// GUID's text, for a resource the mod serves. Allowed while the mod
    /// initialises.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed id, one outside the mod's namespace, or one
    /// whose game id another content id has.
    #[script]
    pub fn declare(self, id: &str) -> Result<String, Off> {
        Ok(super::table::declare(self.host, id)?.to_string())
    }

    /// Serves `value` as the new resource `key`, of the type its key names,
    /// built from the value alone: every field the value leaves out is zero.
    /// The game must hold a resource of that type, which the loader opens as
    /// the base whose bytes the value replaces. Allowed while the mod
    /// initialises.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, a type the build lacks or has no resource
    /// of, a value that does not fit the type, and when the loader refuses
    /// the serve.
    #[script]
    pub fn serve_new(self, key: &Data, value: &Data) -> Result<(), Off> {
        super::table::serve_new(self.host, Self::key_of(key)?, value)
    }

    /// The version string of the game's archive: the build number, the
    /// branch and when the build was made, split by `|`.
    ///
    /// # Errors
    ///
    /// [`Off`] when the archive does not read.
    #[script]
    pub fn game_version(self) -> Result<String, Off> {
        Ok(self.host.game_version()?.to_owned())
    }

    /// The nil GUID's text, all zero: no resource.
    #[script]
    #[must_use]
    pub fn nil_guid(self) -> String {
        Guid::NIL.to_string()
    }

    /// The GUID a content hash reads as, four numbers as `content_hashes`
    /// lists them: its words in memory order.
    ///
    /// # Errors
    ///
    /// [`Off`] for a hash that is not four numbers.
    #[script]
    pub fn guid_from_content_hash(self, hash: &Data) -> Result<String, Off> {
        Ok(Guid::from_content_hash(hash_of(hash)?).to_string())
    }

    /// The content hash a GUID reads as, four numbers.
    ///
    /// # Errors
    ///
    /// [`Off`] for text that is not a GUID.
    #[script]
    pub fn content_hash_of_guid(self, guid: &str) -> Result<Data, Off> {
        let guid =
            Guid::try_parse(guid).ok_or_else(|| Off::new(format!("{guid} is not a GUID")))?;
        Ok(Data::List(
            guid.content_hash()
                .into_iter()
                .map(|w| Data::Uint(w.into()))
                .collect(),
        ))
    }

    /// The number the game gives the resource of GUID `guid`: FNV-1a over
    /// its memory bytes, as item and template ids are.
    ///
    /// # Errors
    ///
    /// [`Off`] for text that is not a GUID.
    #[script]
    pub fn guid_hash(self, guid: &str) -> Result<u32, Off> {
        Ok(Guid::try_parse(guid)
            .ok_or_else(|| Off::new(format!("{guid} is not a GUID")))?
            .fnv1a())
    }

    /// FNV-1a over 32 bits of `bytes`, a buffer or text.
    #[script]
    #[must_use]
    pub fn fnv1a32(self, bytes: &Bytes) -> u32 {
        super::fnv1a32(&bytes.0)
    }

    /// CRC-32/ISO-HDLC of `bytes`, a buffer or text.
    #[script]
    #[must_use]
    pub fn crc32(self, bytes: &Bytes) -> u32 {
        super::crc32(&bytes.0)
    }

    /// CRC-64/ECMA-182 of `bytes`, a buffer or text; past 2^53 as its
    /// decimal text.
    #[script]
    #[must_use]
    pub fn crc64(self, bytes: &Bytes) -> Data {
        Data::Uint(super::crc64(&bytes.0))
    }

    /// The value of the game's own resource `key`, read from its archive.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, one the archive lacks, and a resource
    /// that does not read as its type.
    #[script]
    pub fn game_value(self, key: &Data) -> Result<Data, Off> {
        self.host.game_value(Self::key_of(key)?)
    }

    /// The value of resource `key` as the game's first read would build it
    /// now, with every mod's patches; after init only, and refused inside
    /// a patch of the same key. Each read runs every patch of the key again
    /// and frees what they built; when every patch fails, a changed vanilla
    /// key reads as the archive has it.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, during init, for a key neither served
    /// nor archived, inside a build of the same key, and a resource that
    /// does not read as its type.
    #[script]
    pub fn served_value(self, key: &Data) -> Result<Data, Off> {
        self.host.served_value(Self::key_of(key)?)
    }

    /// Serves the resource `key`, built from the resource `base` the first
    /// time the game reads it: `patch` gets the base's value and returns
    /// the value to serve, which may name only the fields it changes; nil
    /// serves the base unchanged. A patch that fails, returns what is not
    /// game data or does not fit the type is logged; a game resource patched
    /// in place is then served unchanged, and a new resource turns the mod
    /// off. Allowed while the mod initialises.
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
        patch: impl Fn(Data) -> Result<Data, Error> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let (key, base) = (Self::key_of(key)?, Self::key_of(base)?);
        super::table::serve_value(self.host, key, base, move |value| {
            patch(value).map_err(|error| Off::new(error.to_string()))
        })
    }
}

impl Resources {
    /// The namespace for the mod `host` serves.
    #[doc(hidden)]
    #[must_use]
    pub fn open(host: Host) -> Self {
        Self { host }
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
        assert_eq!(back.field("raw"), Some(&Data::Bytes(vec![1, 2])));
        assert!(Data::from_value(Value::Vector([0.0; 3])).is_err());
    }

    #[test]
    fn a_variant_crosses_as_its_type_and_value_and_comes_back() {
        let variant = Variant::new("demo::Sphere", Data::Float(2.5));
        let back = Variant::from_value(variant.clone().into_value()).unwrap();
        assert_eq!(back, variant);
        let untyped = Data::Struct(vec![("value".into(), Data::Int(1))]).into_value();
        assert!(Variant::from_value(untyped).is_err());
    }

    #[test]
    fn negative_zero_from_a_script_stays_a_float_with_its_sign() {
        let Ok(Data::Float(zero)) = Data::from_value(Value::Number(-0.0)) else {
            panic!("-0.0 became an integer");
        };
        assert!(zero.is_sign_negative());
        assert_eq!(Data::from_value(Value::Number(0.0)).unwrap(), Data::Int(0));
    }
}
