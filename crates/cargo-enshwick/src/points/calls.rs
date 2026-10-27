//! Another mod's calls, read from its schema and written as the declaration
//! a caller compiles.

use std::fmt::Write as _;

use serde::Deserialize;

use super::{Error, Kind, Schema, doc, find, hex, named_type, reached_from, rust_type};

/// One call.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CallSchema {
    pub(super) name: String,
    pub(super) input: String,
    pub(super) doc: String,
    pub(super) major: u32,
    pub(super) output: String,
    pub(super) output_layout: String,
    pub(super) input_size: u32,
    pub(super) output_size: u32,
    pub(super) layout: Vec<String>,
}

/// The Rust declaration of the call `name` of `schema`, with every named
/// type it reaches.
///
/// # Errors
///
/// [`Error`] when the schema lacks the call or a type it names.
///
/// # Examples
///
/// ```
/// use cargo_enshwick::points::{Schema, rust_call};
///
/// let schema = Schema::parse("schema = 1\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
/// assert!(rust_call(&schema, "you.stalls:rent").is_err());
/// # Ok::<(), cargo_enshwick::points::Error>(())
/// ```
pub fn rust_call(schema: &Schema, name: &str) -> Result<String, Error> {
    let call = schema
        .call
        .iter()
        .find(|c| c.name == name)
        .ok_or_else(|| Error::NoCall {
            owner: schema.owner.clone(),
            name: name.to_owned(),
        })?;
    let name = call.name.as_str();
    let input = find(schema, name, &call.input)?;
    if input.kind != Kind::Struct || input.field.len() != call.layout.len() {
        return Err(Error::BadInput(name.to_owned()));
    }
    let reached = reached_from(
        schema,
        name,
        input
            .field
            .iter()
            .map(|f| f.ty.as_str())
            .chain([call.output.as_str()])
            .collect(),
    )?;
    let mut code = String::new();
    let _ = writeln!(
        code,
        "//! The call `{name}` v{} of {} {}.\n\
         //!\n\
         //! Written by `cargo enshwick call {name}`. Write it again for a newer\n\
         //! {} rather than edit it; the checks at the end hold it to the server's\n\
         //! layout. Declare this module `pub`.\n",
        call.major, schema.owner, schema.version, schema.owner
    );
    doc(&mut code, "", &call.doc, &format!("The input of `{name}`."));
    let _ = writeln!(
        code,
        "#[enshwick::call(name = {name:?}, output = {}, version = {})]\n\
         #[derive(Clone, Copy, Debug)]\npub struct {} {{",
        rust_type(&call.output),
        call.major,
        call.input
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
    checks(&mut code, call, &input.field)?;
    Ok(code)
}

/// The checks that hold the declaration of `call` to the server's layout
/// hashes and sizes.
fn checks(
    code: &mut String,
    call: &CallSchema,
    fields: &[super::FieldSchema],
) -> Result<(), Error> {
    let name = call.name.as_str();
    let hash = |text: &str| -> Result<u64, Error> {
        text.parse().map_err(|_| Error::BadHash(name.to_owned()))
    };
    code.push_str("\nconst _: () = {\n");
    for (i, (layout, field)) in call.layout.iter().zip(fields).enumerate() {
        let _ = writeln!(
            code,
            "    assert!(\n        <{} as enshwick::services::Call>::LAYOUT[{i}].hash == {},\n        \"`{}` differs from the server's\",\n    );",
            call.input,
            hex(hash(layout)?),
            field.name
        );
    }
    let _ = writeln!(
        code,
        "    assert!(\n        <{} as enshwick::services::Call>::OUTPUT[0].hash == {},\n        \"the answer differs from the server's\",\n    );",
        call.input,
        hex(hash(&call.output_layout)?)
    );
    let _ = writeln!(
        code,
        "    assert!(\n        size_of::<{}>() == {},\n        \"the input's size differs from the server's\"\n    );\n}};",
        call.input, call.input_size
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Schema;
    use super::rust_call;

    const SCHEMA: &str = r#"schema = 1
mod = "you.trading-post"
version = "1.2.0"

[[call]]
name = "you.trading-post:price-of"
input = "PriceOf"
doc = "What the trading post pays for an item."
major = 2
output = "u32"
output_layout = "17"
input_size = 8
output_size = 4
layout = ["1", "2"]

[[type]]
name = "PriceOf"
doc = "What the trading post pays for an item."
kind = "struct"

[[type.field]]
name = "item"
doc = "The item."
type = "u32"

[[type.field]]
name = "deal"
doc = ""
type = "Deal"

[[type]]
name = "Deal"
doc = "How the item changes hands."
kind = "enum"
repr = "u8"

[[type.variant]]
name = "Buy"
value = 0

[[type.variant]]
name = "Sell"
value = 1
"#;

    #[test]
    fn a_call_becomes_a_declaration_with_its_types_and_checks() {
        let schema = Schema::parse(SCHEMA).unwrap();
        assert_eq!(
            schema.calls().collect::<Vec<_>>(),
            ["you.trading-post:price-of"]
        );
        let code = rust_call(&schema, "you.trading-post:price-of").unwrap();
        insta::assert_snapshot!(code);
    }

    #[test]
    fn the_declaration_of_the_fixture_is_the_one_the_sdk_compiles() {
        let text = include_str!("../../tests/fixtures/stalls.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let code = rust_call(&schema, "you.stalls:rent").unwrap();
        assert_eq!(code, include_str!("../../tests/fixtures/stalls_rent.rs"));
    }

    #[test]
    fn the_luau_module_of_the_fixture_is_the_one_the_runtime_is_tested_with() {
        let text = include_str!("../../tests/fixtures/stalls.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let code = super::super::luau_call(&schema, "you.stalls:rent").unwrap();
        assert_eq!(code, include_str!("../../tests/fixtures/stalls_rent.luau"));
    }

    #[test]
    fn a_call_the_schema_lacks_is_named() {
        let schema = Schema::parse(SCHEMA).unwrap();
        let error = rust_call(&schema, "you.trading-post:rent").unwrap_err();
        assert_eq!(
            error.to_string(),
            "you.trading-post serves no call you.trading-post:rent"
        );
    }

    #[test]
    fn a_bad_call_name_is_refused() {
        let text = SCHEMA.replace("you.trading-post:price-of", "You.Trading:price-of");
        assert!(Schema::parse(&text).is_err());
    }
}
