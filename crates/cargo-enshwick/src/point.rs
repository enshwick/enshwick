//! `cargo enshwick point`: another mod's extension point, written as a
//! definition this mod compiles.
//!
//! The owner's `manifest!(points: [..])` puts a schema of each point it
//! defines into its manifest and into the `.enshpt` section of its DLL, so
//! the schema is read here without running the owner's code. The definition
//! names every type the point reaches and checks its own layout against the
//! hashes the owner's build wrote.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use enshwick_reflect::{Image, Layout};
use serde::Deserialize;

/// The section of a mod DLL that holds its point schemas.
pub(crate) const SECTION: &str = ".enshpt";

/// The schema format this tool reads.
const FORMAT: u32 = 1;

/// A mod's point schemas, as `manifest!` writes them.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Schema {
    /// The format, 1.
    #[serde(rename = "schema")]
    format: u32,
    /// The owner's id.
    #[serde(rename = "mod")]
    owner: String,
    /// The owner's version.
    version: String,
    /// Its points.
    #[serde(default)]
    point: Vec<PointSchema>,
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
    /// Reads a schema text.
    ///
    /// # Errors
    ///
    /// When the text is no schema this tool reads.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let schema: Self = toml::from_str(text).map_err(|e| format!("the schema: {e}"))?;
        if schema.format != FORMAT {
            return Err(format!(
                "the schema is format {}; this cargo-enshwick reads {FORMAT}: update it",
                schema.format
            ));
        }
        Ok(schema)
    }
}

/// The point schemas in the DLL `bytes`; `None` for a DLL without points.
///
/// # Errors
///
/// When the bytes are no x64 DLL, or the section is no text.
pub(crate) fn from_dll(bytes: &[u8]) -> Result<Option<String>, String> {
    let image = Image::parse(bytes, Layout::File, None).map_err(|e| e.to_string())?;
    let Some(section) = image.section(SECTION) else {
        return Ok(None);
    };
    let range = image.range(section);
    let raw = image
        .read(range.start, range.end - range.start)
        .ok_or("the point section is cut short")?;
    let end = raw.iter().rposition(|&b| b != 0).map_or(0, |at| at + 1);
    let text = std::str::from_utf8(&raw[..end]).map_err(|e| format!("the point section: {e}"))?;
    Ok((!text.is_empty()).then(|| text.to_owned()))
}

/// The schema texts `from` holds: of a schema file, of a DLL, or of every
/// DLL in a folder and in its `mods/`, each with where it came from.
fn schemas(from: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));
    if from.extension().is_some_and(|e| e == "toml") {
        let text =
            String::from_utf8(read(from)?).map_err(|e| format!("{}: {e}", from.display()))?;
        return Ok(vec![(from.to_path_buf(), text)]);
    }
    let mut found = Vec::new();
    for dll in dlls(from)? {
        if let Ok(Some(text)) = from_dll(&read(&dll)?) {
            found.push((dll, text));
        }
    }
    Ok(found)
}

/// The DLLs `from` names: the file itself, or every DLL in the folder and in
/// its `mods/`.
fn dlls(from: &Path) -> Result<Vec<PathBuf>, String> {
    if from.is_file() {
        return Ok(vec![from.to_path_buf()]);
    }
    let mut found = Vec::new();
    for dir in [from.to_path_buf(), from.join("mods")] {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
            {
                found.push(path);
            }
        }
    }
    found.sort();
    if found.is_empty() {
        return Err(format!("no mod DLL in {}", from.display()));
    }
    Ok(found)
}

/// Writes the definition of the point `name`, `<mod id>:<point>`, read from
/// the owner's DLL or schema file in `from`, to `out` or as the result.
///
/// # Errors
///
/// When no DLL there defines the point, or its schema does not read.
pub(crate) fn run(name: &str, from: &Path, out: Option<&Path>) -> Result<String, String> {
    let (owner, _) = name
        .split_once(':')
        .ok_or_else(|| format!("{name:?} is no point: <mod id>:<point>"))?;
    let mut owners_seen = Vec::new();
    for (dll, text) in schemas(from)? {
        let schema = Schema::parse(&text).map_err(|e| format!("{}: {e}", dll.display()))?;
        if schema.point.iter().any(|p| p.name == name) {
            let code = rust(&schema, name)?;
            return match out {
                Some(path) => {
                    std::fs::write(path, &code).map_err(|e| format!("{}: {e}", path.display()))?;
                    Ok(format!(
                        "wrote {name} of {} {} to {}",
                        schema.owner,
                        schema.version,
                        path.display()
                    ))
                }
                None => Ok(code.trim_end().to_owned()),
            };
        }
        owners_seen.push(schema.owner);
    }
    if owners_seen.iter().any(|o| o == owner) {
        Err(format!("{owner} defines no point {name}"))
    } else {
        Err(format!(
            "no DLL in {} defines points of {owner}",
            from.display()
        ))
    }
}

/// The Rust path of a schema type name.
fn rust_type(name: &str) -> String {
    match name {
        "enshwick.entity" => "enshwick::ecs::Entity".to_owned(),
        "enshwick.guid" => "enshwick::content::Guid".to_owned(),
        other => other.to_owned(),
    }
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
fn doc(code: &mut String, indent: &str, text: &str, fallback: &str) {
    let text = if text.trim().is_empty() {
        fallback
    } else {
        text
    };
    for line in text.lines() {
        let _ = writeln!(code, "{indent}/// {line}");
    }
}

/// The Rust definition of the point `name` of `schema`, with every named
/// type it reaches.
///
/// # Errors
///
/// When the schema lacks the point or a type it names.
pub(crate) fn rust(schema: &Schema, name: &str) -> Result<String, String> {
    let point = schema
        .point
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("{} defines no point {name}", schema.owner))?;
    let find = |type_name: &str| {
        schema
            .types
            .iter()
            .find(|t| t.name == type_name)
            .ok_or_else(|| format!("the schema of {name} lacks the type {type_name}"))
    };
    let input = find(&point.input)?;
    if input.kind != Kind::Struct || input.field.len() != point.layout.len() {
        return Err(format!("the schema of {name} describes its input wrongly"));
    }
    let mut reached: Vec<&TypeSchema> = Vec::new();
    let mut pending: Vec<&str> = input
        .field
        .iter()
        .map(|f| f.ty.as_str())
        .chain([point.output.as_str()])
        .collect();
    while let Some(type_name) = pending.pop() {
        if !is_named(type_name) || reached.iter().any(|t| t.name == type_name) {
            continue;
        }
        let found = find(type_name)?;
        pending.extend(found.field.iter().map(|f| f.ty.as_str()));
        reached.push(found);
    }
    reached.sort_by(|a, b| a.name.cmp(&b.name));

    let mode = match point.mode.as_str() {
        "observe" => "Observe",
        "advise" => "Advise",
        "override" => "Override",
        other => return Err(format!("the point {name} has the unknown mode {other:?}")),
    };
    let mut code = String::new();
    let _ = writeln!(
        code,
        "//! The extension point `{name}` of {} {}, written by\n\
         //! `cargo enshwick point {name}`. Write it again for a newer {}\n\
         //! rather than edit it; the checks at the end hold it to the owner's\n\
         //! layout. Declare this module `pub`.\n",
        schema.owner, schema.version, schema.owner
    );
    doc(
        &mut code,
        "",
        &point.doc,
        &format!("The input of `{name}`."),
    );
    let refusable = if point.refusable { ", refusable" } else { "" };
    let _ = writeln!(
        code,
        "#[enshwick::point(name = \"{name}\", mode = {mode}, output = {}{refusable})]\n\
         #[derive(Clone, Copy, Debug)]\npub struct {} {{",
        rust_type(&point.output),
        point.input
    );
    for field in &input.field {
        doc(
            &mut code,
            "    ",
            &field.doc,
            &format!("The `{}` of the input.", field.name),
        );
        let _ = writeln!(code, "    pub {}: {},", field.name, rust_type(&field.ty));
    }
    code.push_str("}\n");
    for named in &reached {
        code.push('\n');
        named_type(&mut code, named, &schema.owner)?;
    }
    code.push_str("\nconst _: () = {\n");
    for (i, hash) in point.layout.iter().enumerate() {
        let hash: u64 = hash
            .parse()
            .map_err(|_| format!("the schema of {name} has a bad layout hash"))?;
        let _ = writeln!(
            code,
            "    assert!(\n        <{} as enshwick::points::Point>::LAYOUT[{i}].hash == {},\n        \"`{}` differs from the owner's\",\n    );",
            point.input,
            hex(hash),
            input.field[i].name
        );
    }
    let output: u64 = point
        .output_layout
        .parse()
        .map_err(|_| format!("the schema of {name} has a bad layout hash"))?;
    let _ = writeln!(
        code,
        "    assert!(\n        <{} as enshwick::points::Point>::OUTPUT[0].hash == {},\n        \"the answer differs from the owner's\",\n    );\n}};",
        point.input,
        hex(output)
    );
    Ok(code)
}

/// `hash` as a hex literal in groups of four digits.
fn hex(hash: u64) -> String {
    let digits = format!("{hash:016x}");
    let groups: Vec<&str> = (0..4).map(|i| &digits[i * 4..i * 4 + 4]).collect();
    format!("0x{}", groups.join("_"))
}

/// The Rust definition of the named type `named` of `owner`'s schema.
fn named_type(code: &mut String, named: &TypeSchema, owner: &str) -> Result<(), String> {
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
                .ok_or_else(|| format!("the newtype {} has no field", named.name))?;
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

    const SCHEMA: &str = include_str!("../tests/fixtures/stalls.points.toml");

    #[test]
    fn a_point_becomes_a_definition_with_its_types_and_layout_checks() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let code = rust(&schema, "you.stalls:price").unwrap();
        assert_eq!(code, include_str!("../tests/fixtures/stalls_price.rs"));
    }

    #[test]
    fn the_examples_definition_is_what_the_trading_post_publishes() {
        let schema =
            Schema::parse(include_str!("../../../examples/trading-post/points.toml")).unwrap();
        assert_eq!(
            rust(&schema, "you.trading-post:price").unwrap(),
            include_str!("../../../examples/haggling/src/price.rs"),
            "run cargo enshwick point you.trading-post:price again"
        );
    }

    #[test]
    fn a_missing_point_or_another_format_is_refused() {
        let schema = Schema::parse(SCHEMA).unwrap();
        assert!(rust(&schema, "you.stalls:other").is_err());
        let newer = SCHEMA.replace("schema = 1", "schema = 2");
        assert!(Schema::parse(&newer).unwrap_err().contains("update it"));
    }
}
