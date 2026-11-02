//! What another mod offers, read from its schemas and written as the
//! declarations a mod that depends on it compiles.
//!
//! Each of the owner's own points, calls and events puts its schema into
//! the `.enshpt` section of the owner's DLL, beside the head its
//! `declare_mod!` writes, so the schema is read here without running the
//! owner's code. A schema comes from another author:
//! every name, type and text it carries is checked before it enters
//! generated code. The declarations name every type they reach and check
//! their own layout against the hashes the owner's build wrote.

mod calls;
mod dll;
mod luau;
mod rust;

use std::fmt::Write as _;

use enshwick::schema_section::ReadSectionError;
use enshwick_gamedata::reflect::ImageError;
use serde::Deserialize;

pub use dll::{SECTION, from_dll};
pub use luau::{LuauDeps, luau_deps};
pub use rust::rust_deps;

/// The schema format this tool reads.
const FORMAT: u32 = enshwick::schema_section::FORMAT;

/// Why a point schema does not read or does not make a definition.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The text is no TOML of a schema.
    #[error("the schema: {0}")]
    Toml(String),
    /// The schema is of another format than this tool reads.
    #[error("the schema is format {0}; this cargo-enshwick reads {FORMAT}: update it")]
    Format(u32),
    /// A name, type or version is not one generated code may carry.
    #[error("the schema's {what} {value:?} is not one a definition may carry")]
    Invalid {
        /// What the value is.
        what: &'static str,
        /// The value.
        value: String,
    },
    /// The server serves no such call.
    #[error("{owner} serves no call {name}")]
    NoCall {
        /// The server's id.
        owner: String,
        /// The call asked for.
        name: String,
    },
    /// The owner defines no such point.
    #[error("{owner} defines no point {name}")]
    NoPoint {
        /// The owner's id.
        owner: String,
        /// The point asked for.
        name: String,
    },
    /// Two declarations of one owner take the same type, which one module
    /// cannot declare twice.
    #[error("two declarations of {owner} take the type {ty}")]
    SharedType {
        /// The owner's id.
        owner: String,
        /// The type.
        ty: String,
    },
    /// A type the point reaches is not in the schema.
    #[error("the schema of {point} lacks the type {ty}")]
    NoType {
        /// The point.
        point: String,
        /// The missing type.
        ty: String,
    },
    /// The point's input is no struct of its layout's fields.
    #[error("the schema of {0} describes its input wrongly")]
    BadInput(String),
    /// A call's input field is of a type a script cannot be trusted to
    /// fill: only a type every bit pattern of which is valid may be.
    #[error(
        "the call {call} takes {field} as {ty}; a Luau mod calls only with numbers, booleans, entities and GUIDs in the input"
    )]
    ScriptInput {
        /// The call.
        call: String,
        /// The field.
        field: String,
        /// The field's type in the schema.
        ty: String,
    },
    /// The point's mode is none this tool knows.
    #[error("the point {point} has the unknown mode {mode:?}")]
    UnknownMode {
        /// The point.
        point: String,
        /// The mode.
        mode: String,
    },
    /// A layout hash is no number.
    #[error("the schema of {0} has a bad layout hash")]
    BadHash(String),
    /// A newtype has no field.
    #[error("the newtype {0} has no field")]
    EmptyNewtype(String),
    /// The DLL is no x64 image.
    #[error("the DLL: {0}")]
    Image(#[from] ImageError),
    /// The point section ends before its header says.
    #[error("the point section is cut short")]
    CutShort,
    /// A part of the point section is damaged, or the owner was built with
    /// an older enshwick.
    #[error(transparent)]
    Section(#[from] ReadSectionError),
}

/// A mod's point schemas, as `declare_mod!` writes them.
///
/// # Examples
///
/// ```
/// use cargo_enshwick::points::Schema;
///
/// assert!(Schema::parse("schema = 3\nmod = \"you.stalls\"\nversion = \"1.0.0\"").is_err());
/// ```
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Schema {
    /// The format, [`FORMAT`].
    #[serde(rename = "schema")]
    format: u32,
    /// The content and stat kinds the owner's `exports` lists, each
    /// `<kind>:<name>`; its exported declarations are the tables below.
    #[serde(default)]
    exports: Vec<String>,
    /// The owner's id.
    #[serde(rename = "mod")]
    owner: String,
    /// The owner's version.
    version: String,
    /// Its points.
    #[serde(default)]
    point: Vec<PointSchema>,
    /// Its mod events.
    #[serde(default)]
    event: Vec<EventSchema>,
    /// The calls it serves.
    #[serde(default)]
    call: Vec<calls::CallSchema>,
    /// The types its points reach.
    #[serde(default, rename = "type")]
    types: Vec<TypeSchema>,
}

/// One point.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PointSchema {
    name: String,
    mode: String,
    refusable: bool,
    input: String,
    doc: String,
    output: String,
    output_layout: String,
    layout: Vec<String>,
}

/// One mod event.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct EventSchema {
    name: String,
    data: String,
    doc: String,
    layout: Vec<String>,
}

/// One named type.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TypeSchema {
    name: String,
    doc: String,
    kind: Kind,
    #[serde(default)]
    repr: String,
    #[serde(default)]
    field: Vec<FieldSchema>,
    #[serde(default)]
    value: Vec<ValueSchema>,
    #[serde(default)]
    variant: Vec<ValueSchema>,
}

/// What a named type is.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Kind {
    /// A `repr(C)` struct of named fields.
    Struct,
    /// A `repr(transparent)` struct over one field.
    Newtype,
    /// A newtype over an integer with named values.
    Values,
    /// An enum with an integer `repr` and unit variants.
    Enum,
}

/// A field of a struct.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FieldSchema {
    name: String,
    doc: String,
    #[serde(rename = "type")]
    ty: String,
}

/// A named value or a variant.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ValueSchema {
    name: String,
    #[serde(default)]
    doc: String,
    value: i64,
}

impl Schema {
    /// Reads a schema text and checks every value it would write into code.
    ///
    /// # Errors
    ///
    /// [`Error`] when the text is no schema this tool reads, or a name,
    /// type or version in it is not one a definition may carry.
    ///
    /// # Examples
    ///
    /// ```
    /// use cargo_enshwick::points::Schema;
    ///
    /// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
    /// assert_eq!(schema.owner(), "you.stalls");
    /// # Ok::<(), cargo_enshwick::points::Error>(())
    /// ```
    pub fn parse(text: &str) -> Result<Self, Error> {
        let schema: Self = toml::from_str(text).map_err(|e| Error::Toml(e.to_string()))?;
        if schema.format != FORMAT {
            return Err(Error::Format(schema.format));
        }
        schema.check()?;
        Ok(schema)
    }

    /// The owner's mod id.
    ///
    /// # Examples
    ///
    /// See [`Schema::parse`], whose example reads it.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// The owner's version.
    ///
    /// # Examples
    ///
    /// ```
    /// use cargo_enshwick::points::Schema;
    ///
    /// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
    /// assert_eq!(schema.version(), "1.0.0");
    /// # Ok::<(), cargo_enshwick::points::Error>(())
    /// ```
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The names of the points, `<mod id>:<what>`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cargo_enshwick::points::Schema;
    ///
    /// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
    /// assert_eq!(schema.points().count(), 0);
    /// # Ok::<(), cargo_enshwick::points::Error>(())
    /// ```
    pub fn points(&self) -> impl Iterator<Item = &str> {
        self.point.iter().map(|p| p.name.as_str())
    }

    /// The names of the mod events it defines.
    ///
    /// # Examples
    ///
    /// ```
    /// use cargo_enshwick::points::Schema;
    ///
    /// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
    /// assert_eq!(schema.events().count(), 0);
    /// # Ok::<(), cargo_enshwick::points::Error>(())
    /// ```
    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.event.iter().map(|e| e.name.as_str())
    }

    /// The names of the calls it serves.
    ///
    /// # Examples
    ///
    /// ```
    /// use cargo_enshwick::points::Schema;
    ///
    /// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
    /// assert_eq!(schema.calls().count(), 0);
    /// # Ok::<(), cargo_enshwick::points::Error>(())
    /// ```
    pub fn calls(&self) -> impl Iterator<Item = &str> {
        self.call.iter().map(|c| c.name.as_str())
    }

    /// Refuses every value that would leave its place in generated code.
    fn check(&self) -> Result<(), Error> {
        let invalid = |what: &'static str, value: &str| Error::Invalid {
            what,
            value: value.to_owned(),
        };
        if !is_owner(&self.owner) {
            return Err(invalid("owner", &self.owner));
        }
        if semver::Version::parse(&self.version).is_err() {
            return Err(invalid("version", &self.version));
        }
        let named = |name: &str| self.types.iter().any(|t| t.name == name);
        let is_type = |ty: &str| PRIMITIVES.contains(&ty) || built_in(ty).is_some() || named(ty);
        for point in &self.point {
            if !is_point_name(&point.name) {
                return Err(invalid("point name", &point.name));
            }
            if !is_ident(&point.input) {
                return Err(invalid("input type", &point.input));
            }
            if !(is_type(&point.output) && (!is_named(&point.output) || is_ident(&point.output))) {
                return Err(invalid("output type", &point.output));
            }
        }
        for call in &self.call {
            if !is_point_name(&call.name) {
                return Err(invalid("call name", &call.name));
            }
            if !is_ident(&call.input) {
                return Err(invalid("input type", &call.input));
            }
            if !(is_type(&call.output) && (!is_named(&call.output) || is_ident(&call.output))) {
                return Err(invalid("output type", &call.output));
            }
        }
        for event in &self.event {
            if !is_point_name(&event.name) {
                return Err(invalid("event name", &event.name));
            }
            if !is_ident(&event.data) {
                return Err(invalid("event data type", &event.data));
            }
        }
        for ty in &self.types {
            if !is_ident(&ty.name) {
                return Err(invalid("type name", &ty.name));
            }
            let reprs: &[&str] = match ty.kind {
                Kind::Values => &INTEGERS,
                Kind::Enum => &ENUM_REPRS,
                Kind::Struct | Kind::Newtype => &[],
            };
            if !reprs.is_empty() && !reprs.contains(&ty.repr.as_str()) {
                return Err(invalid("repr", &ty.repr));
            }
            for field in &ty.field {
                if ty.kind != Kind::Newtype && !is_ident(&field.name) {
                    return Err(invalid("field name", &field.name));
                }
                if !(is_type(&field.ty) && (!is_named(&field.ty) || is_ident(&field.ty))) {
                    return Err(invalid("field type", &field.ty));
                }
            }
            for value in ty.value.iter().chain(&ty.variant) {
                if !is_ident(&value.name) {
                    return Err(invalid("value name", &value.name));
                }
            }
        }
        Ok(())
    }
}

/// The integer types a value type wraps.
const INTEGERS: [&str; 8] = ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64"];
/// The `repr` an enum takes, as the macros allow for a shared enum.
const ENUM_REPRS: [&str; 10] = [
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize",
];
/// Every primitive a field may be.
const PRIMITIVES: [&str; 11] = [
    "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64", "bool",
];
/// An API type a schema names by its schema name: its Rust path, layout,
/// the read the Luau runtime takes for it and its Luau type.
struct BuiltIn {
    name: &'static str,
    rust: &'static str,
    size: u64,
    align: u64,
    read: &'static str,
    luau: &'static str,
}

/// The API's own types a schema names, the one list every writer reads.
const BUILT_IN: [BuiltIn; 9] = [
    BuiltIn {
        name: "enshwick.buff_type_id",
        rust: "enshwick::buffs::BuffTypeId",
        size: 4,
        align: 4,
        read: "buff_type_id",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.entity",
        rust: "enshwick::Entity",
        size: 4,
        align: 4,
        read: "entity",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.guid",
        rust: "enshwick::content::Guid",
        size: 16,
        align: 1,
        read: "guid",
        luau: "string",
    },
    BuiltIn {
        name: "enshwick.item_id",
        rust: "enshwick::ItemId",
        size: 4,
        align: 4,
        read: "item_id",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.knowledge_id",
        rust: "enshwick::knowledge::KnowledgeId",
        size: 4,
        align: 4,
        read: "knowledge_id",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.player_key",
        rust: "enshwick::PlayerKey",
        size: 8,
        align: 8,
        read: "player_key",
        luau: "string",
    },
    BuiltIn {
        name: "enshwick.recipe_id",
        rust: "enshwick::RecipeId",
        size: 4,
        align: 4,
        read: "recipe_id",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.skill_node",
        rust: "enshwick::abi::SkillNode",
        size: 4,
        align: 4,
        read: "skill_node",
        luau: "number",
    },
    BuiltIn {
        name: "enshwick.npc_id",
        rust: "enshwick::survivors::NpcId",
        size: 4,
        align: 4,
        read: "npc_id",
        luau: "number",
    },
];

/// The API type the schema name `name` names.
fn built_in(name: &str) -> Option<&'static BuiltIn> {
    BUILT_IN.iter().find(|b| b.name == name)
}

fn is_ident(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && text.len() <= 128
        && text != "_"
        && !RUST_KEYWORDS.contains(&text)
}

/// The words Rust keeps for itself in edition 2024, strict and reserved,
/// which no generated name may take.
const RUST_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// Whether `id` is a mod id, of the current or the old form.
fn is_owner(id: &str) -> bool {
    enshwick::abi::is_valid_old_form_id(id)
}

/// Whether `name` is `<mod id>:<what>`, the rule `declare_mod!` holds a point's
/// name to, with nothing that would break the comment line it appears on.
fn is_point_name(name: &str) -> bool {
    name.split_once(':')
        .is_some_and(|(id, what)| is_owner(id) && !what.is_empty() && !what.chars().any(is_hidden))
}

/// The local name of `name` of `schema`: what follows the owner's id and
/// `:`, as `of = <alias>` takes it.
fn local_name<'a>(schema: &Schema, name: &'a str) -> Result<&'a str, Error> {
    name.strip_prefix(schema.owner.as_str())
        .and_then(|rest| rest.strip_prefix(':'))
        .filter(|local| !local.contains(':'))
        .ok_or_else(|| Error::Invalid {
            what: "name of its owner",
            value: name.to_owned(),
        })
}

/// The Rust path of a schema type name.
fn rust_type(name: &str) -> String {
    built_in(name).map_or_else(|| name.to_owned(), |b| b.rust.to_owned())
}

/// Whether `name` is a type the schema itself describes.
fn is_named(name: &str) -> bool {
    !name.starts_with("enshwick.")
        && !matches!(
            name,
            "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64" | "bool"
        )
}

/// `text` as doc comment lines at `indent`; `fallback` when it is empty.
///
/// The first sentence becomes a paragraph of its own, the summary rustdoc
/// shows in lists. A schema's doc comes from another author, so it is made
/// one line of prose: whitespace runs become one space, control and
/// invisible formatting characters are dropped, and a paragraph that would
/// open a code block, which rustdoc runs as a test, starts escaped.
fn doc(code: &mut String, indent: &str, text: &str, fallback: &str) {
    let text = prose(text);
    let text = if text.is_empty() {
        fallback
    } else {
        text.as_str()
    };
    let (summary, more) = match text.split_once(". ") {
        Some((summary, more)) => (format!("{summary}."), more.trim_start()),
        None => (text.to_owned(), ""),
    };
    for (i, paragraph) in [summary.as_str(), more]
        .iter()
        .filter(|p| !p.is_empty())
        .enumerate()
    {
        if i > 0 {
            let _ = writeln!(code, "{indent}///");
        }
        let escape = if paragraph.starts_with("```") || paragraph.starts_with("~~~") {
            "\\"
        } else {
            ""
        };
        let _ = writeln!(code, "{indent}/// {escape}{paragraph}");
    }
}

/// `text` as one line of visible prose.
fn prose(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || is_hidden(c))
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether `c` is a control character or one that changes how text around it
/// shows without showing itself: bidirectional overrides, zero-width
/// characters and the soft hyphen.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

/// The named type `type_name` the point `name` of `schema` reaches.
fn find<'a>(schema: &'a Schema, name: &str, type_name: &str) -> Result<&'a TypeSchema, Error> {
    schema
        .types
        .iter()
        .find(|t| t.name == type_name)
        .ok_or_else(|| Error::NoType {
            point: name.to_owned(),
            ty: type_name.to_owned(),
        })
}

/// Every named type `point` reaches from its `input` and its output, by
/// name.
fn reached<'a>(
    schema: &'a Schema,
    point: &'a PointSchema,
    input: &'a TypeSchema,
) -> Result<Vec<&'a TypeSchema>, Error> {
    reached_from(
        schema,
        &point.name,
        input
            .field
            .iter()
            .map(|f| f.ty.as_str())
            .chain([point.output.as_str()])
            .collect(),
    )
}

/// Every named type `pending` reaches, by name, for the point or event
/// `owner_name`.
fn reached_from<'a>(
    schema: &'a Schema,
    owner_name: &str,
    mut pending: Vec<&'a str>,
) -> Result<Vec<&'a TypeSchema>, Error> {
    let mut reached: Vec<&TypeSchema> = Vec::new();
    while let Some(type_name) = pending.pop() {
        if !is_named(type_name) || reached.iter().any(|t| t.name == type_name) {
            continue;
        }
        let found = find(schema, owner_name, type_name)?;
        pending.extend(found.field.iter().map(|f| f.ty.as_str()));
        reached.push(found);
    }
    reached.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(reached)
}

/// `hash` as a hex literal in groups of four digits.
fn hex(hash: u64) -> String {
    let digits = format!("{hash:016x}");
    let groups: Vec<&str> = (0..4).map(|i| &digits[i * 4..i * 4 + 4]).collect();
    format!("0x{}", groups.join("_"))
}

/// The Rust definition of the named type `named` of `owner`'s schema.
fn named_type(code: &mut String, named: &TypeSchema, owner: &str) -> Result<(), Error> {
    doc(code, "", &named.doc, &format!("A type of `{owner}`."));
    match named.kind {
        Kind::Struct => {
            let _ = writeln!(
                code,
                "#[repr(C)]\n#[derive(Clone, Copy, Debug, enshwick::Shape)]\npub struct {} {{",
                named.name
            );
            for field in &named.field {
                doc(
                    code,
                    "    ",
                    &field.doc,
                    &format!("The `{}` of `{}`.", field.name, named.name),
                );
                let _ = writeln!(code, "    pub {}: {},", field.name, rust_type(&field.ty));
            }
            code.push_str("}\n");
        }
        Kind::Newtype => {
            let inner = named
                .field
                .first()
                .ok_or_else(|| Error::EmptyNewtype(named.name.clone()))?;
            let _ = writeln!(
                code,
                "#[repr(transparent)]\n#[derive(Clone, Copy, Debug, enshwick::Shape)]\npub struct {}(pub {});",
                named.name,
                rust_type(&inner.ty)
            );
        }
        Kind::Values => {
            let constants: Vec<&str> = named.value.iter().map(|v| v.name.as_str()).collect();
            let _ = writeln!(
                code,
                "#[repr(transparent)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]\n\
                 #[shape(values({}))]\npub struct {}(pub {});\n\nimpl {} {{",
                constants.join(", "),
                named.name,
                named.repr,
                named.name
            );
            for value in &named.value {
                doc(
                    code,
                    "    ",
                    &value.doc,
                    &format!("`{}`, as `{owner}` names it.", value.name),
                );
                let _ = writeln!(
                    code,
                    "    pub const {}: Self = Self({});",
                    value.name, value.value
                );
            }
            code.push_str("}\n");
        }
        Kind::Enum => {
            let _ = writeln!(
                code,
                "#[repr({})]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]\npub enum {} {{",
                named.repr, named.name
            );
            for variant in &named.variant {
                doc(
                    code,
                    "    ",
                    &variant.doc,
                    &format!("`{}`, as `{owner}` names it.", variant.name),
                );
                let _ = writeln!(code, "    {} = {},", variant.name, variant.value);
            }
            code.push_str("}\n");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = include_str!("../tests/fixtures/lockers.points.toml");

    #[test]
    fn a_point_becomes_a_declaration_of_the_alias_with_its_types_and_layout_checks() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let code = rust_deps(&schema, "lockers").unwrap();
        assert_eq!(code, include_str!("../tests/fixtures/lockers_points.rs"));
    }

    #[test]
    fn the_examples_declarations_are_what_the_trading_post_publishes() {
        let schema =
            Schema::parse(include_str!("../../../examples/trading-post/points.toml")).unwrap();
        assert_eq!(
            rust_deps(&schema, "trading_post").unwrap(),
            include_str!("../../../examples/haggling/src/deps/trading_post.rs"),
            "run cargo enshwick deps trading_post again"
        );
    }

    #[test]
    fn a_doc_keeps_its_first_sentence_as_a_paragraph_of_its_own() {
        let mut code = String::new();
        doc(&mut code, "    ", "The price. A refusal means no deal.", "");
        assert_eq!(
            code,
            "    /// The price.\n    ///\n    /// A refusal means no deal.\n"
        );
        let mut code = String::new();
        doc(&mut code, "", "", "The input.");
        assert_eq!(code, "/// The input.\n");
    }

    #[test]
    fn a_doc_can_open_no_code_block_and_hides_nothing() {
        let hostile = [
            "The character.\n\n```\nstd::fs::write(\"x\", \"\").ok();\n```",
            "The deal.\n\n    std::process::Command::new(\"true\").status().ok();",
            "The deal. ~~~ std::fs::write(\"x\", \"\").ok();",
            "The deal. ```rust",
            "The \u{202e}deal\u{200b}.",
        ];
        for text in hostile {
            let mut code = String::new();
            doc(&mut code, "", text, "");
            for line in code.lines() {
                let body = line.strip_prefix("/// ").unwrap_or_default();
                assert!(
                    !body.starts_with([' '])
                        && !body.starts_with("```")
                        && !body.starts_with("~~~"),
                    "{line}"
                );
                assert!(!body.chars().any(is_hidden), "{line:?}");
            }
            assert!(code.lines().count() <= 3, "{code}");
        }
    }

    #[test]
    fn a_schema_value_that_would_leave_its_place_in_the_code_is_refused() {
        let hostile = [
            (
                "mod = \"you.lockers\"",
                "mod = \"you.lockers\\nfn evil() {}\"",
            ),
            ("version = \"1.2.0\"", "version = \"1.2.0 }\""),
            (
                "name = \"you.lockers:price\"",
                "name = \"you.lockers:price\\nfn evil() {}\"",
            ),
            ("input = \"Price\"", "input = \"Price {}\""),
            ("output = \"u32\"", "output = \"u32> evil\""),
            (
                "name = \"character\"",
                "name = \"character: u8, } fn evil() { let x\"",
            ),
            ("type = \"bool\"", "type = \"bool, } fn evil() {}\""),
            ("repr = \"u32\"", "repr = \"u32)] fn evil() {} #[allow(\""),
            (
                "name = \"BUY\"",
                "name = \"BUY: Self = Self(0); fn evil() {} const X\"",
            ),
        ];
        for (good, bad) in hostile {
            assert!(SCHEMA.contains(good), "{good}");
            let text = SCHEMA.replacen(good, bad, 1);
            assert!(
                matches!(Schema::parse(&text), Err(Error::Invalid { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_schema_name_that_rust_keeps_for_itself_is_refused() {
        let keywords = [
            ("name = \"BUY\"", "name = \"while\""),
            ("name = \"character\"", "name = \"type\""),
            ("name = \"character\"", "name = \"self\""),
            ("input = \"Price\"", "input = \"Self\""),
            ("name = \"BUY\"", "name = \"_\""),
            ("name = \"BUY\"", "name = \"async\""),
        ];
        for (good, bad) in keywords {
            assert!(SCHEMA.contains(good), "{good}");
            let text = SCHEMA.replacen(good, bad, 1);
            assert!(
                matches!(Schema::parse(&text), Err(Error::Invalid { .. })),
                "{bad}"
            );
        }
    }
    #[test]
    fn a_quote_in_a_point_name_stays_inside_its_string() {
        let text = SCHEMA.replace("you.lockers:price", "you.lockers:price\\\", evil = (\\\"");
        let schema = Schema::parse(&text).unwrap();
        let code = rust_deps(&schema, "lockers").unwrap();
        assert!(
            code.contains(r#"name = "price\", evil = (\"", of = lockers"#),
            "{code}"
        );
    }

    #[test]
    fn an_alias_that_is_no_rust_name_is_refused() {
        let schema = Schema::parse(SCHEMA).unwrap();
        for alias in ["", "two words", "lockers::evil", "self", "9lives"] {
            assert!(
                matches!(
                    rust_deps(&schema, alias),
                    Err(Error::Invalid { what: "alias", .. })
                ),
                "{alias:?}"
            );
        }
    }

    #[test]
    fn control_characters_in_a_doc_are_dropped() {
        let mut code = String::new();
        doc(&mut code, "", "A price\rpub fn evil() {}\u{7}.", "");
        assert_eq!(code, "/// A price pub fn evil() {} .\n");
    }

    #[test]
    fn a_name_of_another_mod_or_another_format_is_refused() {
        let other = SCHEMA.replace("you.lockers:price", "you.lanterns:price");
        let schema = Schema::parse(&other).unwrap();
        assert!(matches!(
            rust_deps(&schema, "lockers"),
            Err(Error::Invalid {
                what: "name of its owner",
                ..
            })
        ));
        let newer = SCHEMA.replace("schema = 2", "schema = 3");
        assert!(
            Schema::parse(&newer)
                .unwrap_err()
                .to_string()
                .contains("update it")
        );
    }
}
