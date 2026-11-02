//! The Rust module of what a dependency offers: each point, call and mod
//! event, declared by its local name `of` the dependency's alias, then every
//! named type they reach, once.

use std::fmt::Write as _;

use super::calls::CallSchema;
use super::{
    Error, EventSchema, FieldSchema, Kind, PointSchema, Schema, TypeSchema, doc, find, hex,
    is_ident, local_name, named_type, reached_from, rust_type,
};

/// The Rust module of everything `schema` offers other mods, for a mod that
/// names its owner `alias` in its `Cargo.toml`: `enshwick::deps!()` makes
/// it `deps::<alias>`.
///
/// # Errors
///
/// [`Error`] when `alias` is no Rust name, a name is not of the schema's
/// owner, two declarations take one type, or the schema lacks a type it
/// names.
///
/// # Examples
///
/// ```
/// use cargo_enshwick::points::{Schema, rust_deps};
///
/// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
/// assert!(rust_deps(&schema, "stalls")?.contains("`cargo enshwick deps stalls`"));
/// assert!(rust_deps(&schema, "two words").is_err());
/// # Ok::<(), cargo_enshwick::points::Error>(())
/// ```
pub fn rust_deps(schema: &Schema, alias: &str) -> Result<String, Error> {
    if !is_ident(alias) {
        return Err(Error::Invalid {
            what: "alias",
            value: alias.to_owned(),
        });
    }
    let owner = schema.owner.as_str();
    let mut code = String::new();
    let _ = writeln!(
        code,
        "//! What {owner} {} offers other mods, by the alias `{alias}`.\n\
         //!\n\
         //! Written by `cargo enshwick deps {alias}`. Write it again for a newer\n\
         //! {owner} rather than edit it; the checks hold each declaration to the\n\
         //! owner's layout. `enshwick::deps!()` makes it `deps::{alias}`.",
        schema.version
    );
    let inputs: Vec<&str> = (schema.point.iter().map(|p| p.input.as_str()))
        .chain(schema.call.iter().map(|c| c.input.as_str()))
        .chain(schema.event.iter().map(|e| e.data.as_str()))
        .collect();
    let shared =
        (inputs.iter().enumerate()).find_map(|(i, t)| inputs[..i].contains(t).then_some(*t));
    if let Some(shared) = shared {
        return Err(Error::SharedType {
            owner: owner.to_owned(),
            ty: shared.to_owned(),
        });
    }
    let mut reached: Vec<&TypeSchema> = Vec::new();
    for point in &schema.point {
        reached.extend(declare_point(&mut code, schema, point, alias)?);
    }
    for call in &schema.call {
        reached.extend(declare_call(&mut code, schema, call, alias)?);
    }
    for event in &schema.event {
        reached.extend(declare_event(&mut code, schema, event, alias)?);
    }
    reached.retain(|t| !inputs.contains(&t.name.as_str()));
    reached.sort_by(|a, b| a.name.cmp(&b.name));
    reached.dedup_by(|a, b| a.name == b.name);
    for named in reached {
        code.push('\n');
        named_type(&mut code, named, owner)?;
    }
    Ok(code)
}

/// The input struct `input` of the declaration `name`, checked against the
/// `layout` hashes it carries one per field.
fn input_of<'a>(
    schema: &'a Schema,
    name: &str,
    input: &str,
    layout: &[String],
) -> Result<&'a TypeSchema, Error> {
    let input = find(schema, name, input)?;
    if input.kind != Kind::Struct || input.field.len() != layout.len() {
        return Err(Error::BadInput(name.to_owned()));
    }
    Ok(input)
}

/// Writes `attribute`, then the struct `input` with its fields' docs;
/// `fallback` documents the struct when the schema does not.
fn declaration(
    code: &mut String,
    doc_text: &str,
    fallback: &str,
    attribute: &str,
    input: &TypeSchema,
) {
    code.push('\n');
    doc(code, "", doc_text, fallback);
    let _ = writeln!(
        code,
        "{attribute}\n#[derive(Clone, Copy, Debug)]\npub struct {} {{",
        input.name
    );
    for field in &input.field {
        doc(
            code,
            "    ",
            &field.doc,
            &format!("The `{}` of the input.", field.name),
        );
        let _ = writeln!(code, "    pub {}: {},", field.name, rust_type(&field.ty));
    }
    code.push_str("}\n");
}

/// What a block of layout checks holds a declaration to.
struct Checks<'a> {
    /// The declaration, for an error.
    name: &'a str,
    /// The trait whose layout the checks read.
    of: &'a str,
    /// Whose layout it must equal: `owner's` or `server's`.
    whose: &'a str,
    fields: &'a [FieldSchema],
    layout: &'a [String],
    /// The answer's hash, for a point or a call.
    output: Option<&'a str>,
    /// The input's size, for a call.
    size: Option<u32>,
}

/// Writes the checks that hold the declaration of `input` to the layout
/// hashes, and a call's size, its owner's build wrote.
fn checks(code: &mut String, input: &str, checks: &Checks<'_>) -> Result<(), Error> {
    let Checks {
        name,
        of,
        whose,
        fields,
        layout,
        output,
        size,
    } = *checks;
    let hash = |text: &str| -> Result<u64, Error> {
        text.parse().map_err(|_| Error::BadHash(name.to_owned()))
    };
    code.push_str("\nconst _: () = {\n");
    for (i, (layout, field)) in layout.iter().zip(fields).enumerate() {
        let _ = writeln!(
            code,
            "    assert!(\n        <{input} as {of}>::LAYOUT[{i}].hash == {},\n        \"`{}` differs from the {whose}\",\n    );",
            hex(hash(layout)?),
            field.name
        );
    }
    if let Some(output) = output {
        let _ = writeln!(
            code,
            "    assert!(\n        <{input} as {of}>::OUTPUT[0].hash == {},\n        \"the answer differs from the {whose}\",\n    );",
            hex(hash(output)?)
        );
    }
    if let Some(size) = size {
        let _ = writeln!(
            code,
            "    assert!(\n        size_of::<{input}>() == {size},\n        \"the input's size differs from the {whose}\"\n    );"
        );
    }
    code.push_str("};\n");
    Ok(())
}

/// Writes the declaration of `point` with its checks, and returns the named
/// types it reaches.
fn declare_point<'a>(
    code: &mut String,
    schema: &'a Schema,
    point: &'a PointSchema,
    alias: &str,
) -> Result<Vec<&'a TypeSchema>, Error> {
    let name = point.name.as_str();
    let local = local_name(schema, name)?;
    let input = input_of(schema, name, &point.input, &point.layout)?;
    let mode = match point.mode.as_str() {
        "observe" => "Observe",
        "advise" => "Advise",
        "override" => "Override",
        other => {
            return Err(Error::UnknownMode {
                point: name.to_owned(),
                mode: other.to_owned(),
            });
        }
    };
    let refusable = if point.refusable { ", refusable" } else { "" };
    let attribute = format!(
        "#[enshwick::point(name = {local:?}, of = {alias}, mode = {mode}, output = {}{refusable})]",
        rust_type(&point.output)
    );
    let fallback = format!("The input of `{name}`.");
    declaration(code, &point.doc, &fallback, &attribute, input);
    checks(
        code,
        &point.input,
        &Checks {
            name,
            of: "enshwick::points::Point",
            whose: "owner's",
            fields: &input.field,
            layout: &point.layout,
            output: Some(&point.output_layout),
            size: None,
        },
    )?;
    reached_from(
        schema,
        name,
        input
            .field
            .iter()
            .map(|f| f.ty.as_str())
            .chain([point.output.as_str()])
            .collect(),
    )
}

/// Writes the declaration of `call` with its checks, and returns the named
/// types it reaches.
fn declare_call<'a>(
    code: &mut String,
    schema: &'a Schema,
    call: &'a CallSchema,
    alias: &str,
) -> Result<Vec<&'a TypeSchema>, Error> {
    let name = call.name.as_str();
    let local = local_name(schema, name)?;
    let input = input_of(schema, name, &call.input, &call.layout)?;
    let attribute = format!(
        "#[enshwick::call(name = {local:?}, of = {alias}, output = {}, version = {})]",
        rust_type(&call.output),
        call.major
    );
    let fallback = format!("The input of `{name}`.");
    declaration(code, &call.doc, &fallback, &attribute, input);
    checks(
        code,
        &call.input,
        &Checks {
            name,
            of: "enshwick::services::Call",
            whose: "server's",
            fields: &input.field,
            layout: &call.layout,
            output: Some(&call.output_layout),
            size: Some(call.input_size),
        },
    )?;
    reached_from(
        schema,
        name,
        input
            .field
            .iter()
            .map(|f| f.ty.as_str())
            .chain([call.output.as_str()])
            .collect(),
    )
}

/// Writes the declaration of `event` with its checks, and returns the named
/// types its data reaches.
fn declare_event<'a>(
    code: &mut String,
    schema: &'a Schema,
    event: &'a EventSchema,
    alias: &str,
) -> Result<Vec<&'a TypeSchema>, Error> {
    let name = event.name.as_str();
    let local = local_name(schema, name)?;
    let data = input_of(schema, name, &event.data, &event.layout)?;
    let attribute = format!("#[enshwick::event(name = {local:?}, of = {alias})]");
    let fallback = format!("The data of `{name}`.");
    declaration(code, &event.doc, &fallback, &attribute, data);
    checks(
        code,
        &event.data,
        &Checks {
            name,
            of: "enshwick::services::ModEvent",
            whose: "owner's",
            fields: &data.field,
            layout: &event.layout,
            output: None,
            size: None,
        },
    )?;
    reached_from(
        schema,
        name,
        data.field.iter().map(|f| f.ty.as_str()).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::super::{Error, Schema};
    use super::rust_deps;

    const SCHEMA: &str = include_str!("../../tests/fixtures/lockers.points.toml");

    #[test]
    fn every_type_is_declared_once_though_two_declarations_reach_it() {
        let code = rust_deps(&Schema::parse(SCHEMA).unwrap(), "lockers").unwrap();
        assert_eq!(code.matches("pub struct Deal(").count(), 1, "{code}");
        assert!(
            code.contains("#[enshwick::event(name = \"opened\", of = lockers)]"),
            "{code}"
        );
    }

    #[test]
    fn two_declarations_taking_one_type_are_refused() {
        let text = SCHEMA.replace("data = \"Opened\"", "data = \"Price\"");
        let error = rust_deps(&Schema::parse(&text).unwrap(), "lockers").unwrap_err();
        assert!(
            matches!(&error, Error::SharedType { ty, .. } if ty == "Price"),
            "{error}"
        );
    }
}
