//! A point's definition for a Luau mod: the types it reaches, and the
//! description the runtime reads to link a script to the point, with each
//! field's place in the owner's `repr(C)` layout.

use std::fmt::Write as _;

use super::{
    Error, Kind, PointSchema, Schema, TypeSchema, built_in, find, is_named, local_name, prose,
    reached, reached_from,
};

/// A type's size and alignment in a `repr(C)` layout.
#[derive(Clone, Copy)]
struct Shape {
    size: u64,
    align: u64,
}

/// The size and alignment of a primitive or built-in type.
fn primitive(name: &str) -> Option<Shape> {
    if let Some(b) = built_in(name) {
        return Some(Shape {
            size: b.size,
            align: b.align,
        });
    }
    let (size, align) = match name {
        "u8" | "i8" | "bool" => (1, 1),
        "u16" | "i16" => (2, 2),
        "u32" | "i32" | "f32" => (4, 4),
        "u64" | "i64" | "f64" | "usize" | "isize" => (8, 8),
        _ => return None,
    };
    Some(Shape { size, align })
}

/// `offset` rounded up to `align`.
const fn aligned(offset: u64, align: u64) -> u64 {
    offset.div_ceil(align) * align
}

/// Reads types from a schema and writes them for Luau.
struct Writer<'a> {
    schema: &'a Schema,
    point: &'a str,
}

impl Writer<'_> {
    /// The shape of the type `name`.
    fn shape(&self, name: &str, depth: u32) -> Result<Shape, Error> {
        if let Some(shape) = primitive(name) {
            return Ok(shape);
        }
        if depth > 16 {
            return Err(Error::BadInput(self.point.to_owned()));
        }
        let ty = find(self.schema, self.point, name)?;
        match ty.kind {
            Kind::Values | Kind::Enum => {
                primitive(&ty.repr).ok_or_else(|| Error::BadInput(self.point.to_owned()))
            }
            Kind::Newtype => {
                let inner = ty
                    .field
                    .first()
                    .ok_or_else(|| Error::EmptyNewtype(ty.name.clone()))?;
                self.shape(&inner.ty, depth + 1)
            }
            Kind::Struct => {
                let (mut offset, mut align) = (0, 1);
                for field in &ty.field {
                    let shape = self.shape(&field.ty, depth + 1)?;
                    offset = aligned(offset, shape.align) + shape.size;
                    align = align.max(shape.align);
                }
                Ok(Shape {
                    size: aligned(offset, align),
                    align,
                })
            }
        }
    }

    /// How the runtime reads a value of the type `name`: a primitive's
    /// name, or a struct's table of fields.
    fn read(&self, name: &str, depth: u32) -> Result<String, Error> {
        if primitive(name).is_some() {
            let read = match name {
                _ if let Some(b) = built_in(name) => b.read,
                "usize" => "u64",
                "isize" => "i64",
                other => other,
            };
            return Ok(format!("{read:?}"));
        }
        if depth > 16 {
            return Err(Error::BadInput(self.point.to_owned()));
        }
        let ty = find(self.schema, self.point, name)?;
        match ty.kind {
            Kind::Values => self.read(&ty.repr, depth + 1),
            Kind::Enum => {
                let variants: Vec<String> =
                    ty.variant.iter().map(|v| v.value.to_string()).collect();
                Ok(format!(
                    "{{ repr = {}, variants = {{ {} }} }}",
                    self.read(&ty.repr, depth + 1)?,
                    variants.join(", ")
                ))
            }
            Kind::Newtype => {
                let inner = ty
                    .field
                    .first()
                    .ok_or_else(|| Error::EmptyNewtype(ty.name.clone()))?;
                self.read(&inner.ty, depth + 1)
            }
            Kind::Struct => {
                let shape = self.shape(name, depth)?;
                let fields = self.fields(ty, None, depth + 1)?;
                Ok(format!(
                    "{{ size = {}, fields = {{ {} }} }}",
                    shape.size,
                    fields.join(", ")
                ))
            }
        }
    }

    /// The fields of the struct `ty` with their offsets and, for the
    /// point's input, the layout hashes of `hashes`.
    fn fields(
        &self,
        ty: &TypeSchema,
        hashes: Option<&[String]>,
        depth: u32,
    ) -> Result<Vec<String>, Error> {
        let mut offset = 0;
        let mut fields = Vec::new();
        for (i, field) in ty.field.iter().enumerate() {
            let shape = self.shape(&field.ty, depth)?;
            offset = aligned(offset, shape.align);
            let hash = match hashes {
                Some(hashes) => {
                    let hash: u64 = hashes
                        .get(i)
                        .and_then(|h| h.parse().ok())
                        .ok_or_else(|| Error::BadHash(self.point.to_owned()))?;
                    format!(", hash = \"{hash}\"")
                }
                None => String::new(),
            };
            fields.push(format!(
                "{{ name = {:?}, offset = {offset}, read = {}{hash} }}",
                field.name,
                self.read(&field.ty, depth)?
            ));
            offset += shape.size;
        }
        Ok(fields)
    }
}

/// The Luau type a value of the schema type `name` takes.
fn luau_type(schema: &Schema, name: &str) -> String {
    match name {
        _ if let Some(b) = built_in(name) => b.luau.to_owned(),
        "bool" => "boolean".to_owned(),
        "u64" | "i64" | "usize" | "isize" => "string".to_owned(),
        _ if !is_named(name) => "number".to_owned(),
        named => match schema.types.iter().find(|t| t.name == named) {
            Some(ty) if ty.kind == Kind::Struct => named.to_owned(),
            Some(ty) if ty.kind == Kind::Newtype => ty
                .field
                .first()
                .map_or_else(|| "number".to_owned(), |f| luau_type(schema, &f.ty)),
            _ => "number".to_owned(),
        },
    }
}

/// `text` as Luau doc comment lines at `indent`.
fn comment(out: &mut String, indent: &str, text: &str) {
    let text = prose(text);
    if !text.is_empty() {
        let _ = writeln!(out, "{indent}--- {text}");
    }
}

/// The Luau modules of everything `schema` offers other mods, for a Luau
/// mod whose `pack.toml` names the owner `alias`: each point, call and mod
/// event, by its local name.
///
/// # Errors
///
/// [`Error`] when a name is not of the schema's owner or makes no file
/// name, the schema lacks a type it names, or a declaration's input is no
/// struct of its layout's fields.
///
/// # Examples
///
/// ```
/// use cargo_enshwick::points::{Schema, luau_deps};
///
/// let schema = Schema::parse("schema = 2\nmod = \"you.stalls\"\nversion = \"1.0.0\"")?;
/// let deps = luau_deps(&schema, "stalls")?;
/// assert!(deps.modules.is_empty() && deps.skipped.is_empty());
/// # Ok::<(), cargo_enshwick::points::Error>(())
/// ```
pub fn luau_deps(schema: &Schema, alias: &str) -> Result<LuauDeps, Error> {
    let mut deps = LuauDeps {
        modules: Vec::new(),
        skipped: Vec::new(),
    };
    let mut add = |name: &str, text: String| -> Result<(), Error> {
        let local = local_name(schema, name)?;
        if !is_file_name(local) {
            return Err(Error::Invalid {
                what: "name for a file",
                value: name.to_owned(),
            });
        }
        if deps.modules.iter().any(|(known, _)| known == local) {
            return Err(Error::Invalid {
                what: "name, taken twice,",
                value: name.to_owned(),
            });
        }
        deps.modules.push((local.to_owned(), text));
        Ok(())
    };
    for point in schema.points() {
        add(point, luau(schema, point, alias)?)?;
    }
    for call in schema.calls() {
        match luau_call(schema, call, alias) {
            Ok(text) => add(call, text)?,
            Err(error @ Error::ScriptInput { .. }) => deps.skipped.push(error),
            Err(error) => return Err(error),
        }
    }
    for event in schema.events() {
        add(event, luau_event(schema, event, alias)?)?;
    }
    Ok(deps)
}

/// The Luau modules [`luau_deps`] writes for one dependency.
///
/// # Examples
///
/// See [`luau_deps`].
#[derive(Debug)]
#[non_exhaustive]
pub struct LuauDeps {
    /// Each module by the local name of what it describes, its file name
    /// without `.luau`.
    pub modules: Vec<(String, String)>,
    /// The calls left out, each with why: a script cannot fill their input.
    pub skipped: Vec<Error>,
}

/// Whether `local` names a module file on every system: 1 to 128 of
/// `[a-z0-9_.-]`, not starting with a dot.
fn is_file_name(local: &str) -> bool {
    (1..=128).contains(&local.len())
        && !local.starts_with('.')
        && local.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'-')
        })
}

/// The Luau definition of the point `name` of `schema`: a module that
/// exports the types the point reaches, the named values, and returns the
/// description `enshwick.extend` links.
pub(super) fn luau(schema: &Schema, name: &str, alias: &str) -> Result<String, Error> {
    let point: &PointSchema = schema
        .point
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| Error::NoPoint {
            owner: schema.owner.clone(),
            name: name.to_owned(),
        })?;
    let name = point.name.as_str();
    let input = find(schema, name, &point.input)?;
    if input.kind != Kind::Struct || input.field.len() != point.layout.len() {
        return Err(Error::BadInput(name.to_owned()));
    }
    if !matches!(point.mode.as_str(), "observe" | "advise" | "override") {
        return Err(Error::UnknownMode {
            point: name.to_owned(),
            mode: point.mode.clone(),
        });
    }
    let writer = Writer {
        schema,
        point: name,
    };
    let mut out = format!(
        "--!strict\n-- The extension point `{name}` of {} {}.\n-- Written by `cargo enshwick deps {alias} --luau`. Write it again for a newer\n-- {} rather than edit it: the runtime links by the layout below.\n",
        schema.owner, schema.version, schema.owner
    );
    let mut types = reached(schema, point, input)?;
    types.push(input);
    let values = types_and_values(schema, &types, &mut out);
    let input_shape = writer.shape(&point.input, 0)?;
    let output_shape = writer.shape(&point.output, 0)?;
    let output_hash: u64 = point
        .output_layout
        .parse()
        .map_err(|_| Error::BadHash(name.to_owned()))?;
    let fields = writer.fields(input, Some(&point.layout), 1)?;
    out.push('\n');
    comment(&mut out, "", &point.doc);
    let _ = writeln!(
        out,
        "return table.freeze({{\n\tname = {name:?},\n\tmode = {:?},\n\trefusable = {},\n\tinput = {{ size = {}, fields = {{\n\t\t{},\n\t}} }},\n\toutput = {{ size = {}, hash = \"{output_hash}\", read = {} }},\n{values}}})",
        point.mode,
        point.refusable,
        input_shape.size,
        fields.join(",\n\t\t"),
        output_shape.size,
        writer.read(&point.output, 0)?,
    );
    Ok(out)
}

/// The `export type` of every struct in `types` and the `table.freeze` line
/// of every named value set, for a module's head and its returned table.
fn types_and_values(schema: &Schema, types: &[&TypeSchema], out: &mut String) -> String {
    for ty in types.iter().filter(|t| t.kind == Kind::Struct) {
        out.push('\n');
        comment(out, "", &ty.doc);
        let _ = writeln!(out, "export type {} = {{", ty.name);
        for field in &ty.field {
            comment(out, "\t", &field.doc);
            let _ = writeln!(out, "\t{}: {},", field.name, luau_type(schema, &field.ty));
        }
        out.push_str("}\n");
    }
    let mut values = String::new();
    for ty in types
        .iter()
        .filter(|t| matches!(t.kind, Kind::Values | Kind::Enum))
    {
        let entries: Vec<String> = ty
            .value
            .iter()
            .chain(&ty.variant)
            .map(|v| format!("{} = {}", v.name, v.value))
            .collect();
        let _ = writeln!(
            values,
            "\t{} = table.freeze({{ {} }}),",
            ty.name,
            entries.join(", ")
        );
    }
    values
}

/// The Luau definition of the mod event `name` of `schema`: a module that
/// exports the types its data reaches and the named values, and returns the
/// description `enshwick.on_mod_event` subscribes with.
pub(super) fn luau_event(schema: &Schema, name: &str, alias: &str) -> Result<String, Error> {
    let event = schema
        .event
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| Error::NoPoint {
            owner: schema.owner.clone(),
            name: name.to_owned(),
        })?;
    let name = event.name.as_str();
    let data = find(schema, name, &event.data)?;
    if data.kind != Kind::Struct || data.field.len() != event.layout.len() {
        return Err(Error::BadInput(name.to_owned()));
    }
    let writer = Writer {
        schema,
        point: name,
    };
    let mut out = format!(
        "--!strict\n-- The mod event `{name}` of {} {}.\n-- Written by `cargo enshwick deps {alias} --luau`. Write it again for a newer\n-- {} rather than edit it: the runtime subscribes by the layout below.\n",
        schema.owner, schema.version, schema.owner
    );
    let mut types = reached_from(
        schema,
        name,
        data.field.iter().map(|f| f.ty.as_str()).collect(),
    )?;
    types.push(data);
    let values = types_and_values(schema, &types, &mut out);
    let shape = writer.shape(&event.data, 0)?;
    let fields = writer.fields(data, Some(&event.layout), 1)?;
    out.push('\n');
    comment(&mut out, "", &event.doc);
    let _ = writeln!(
        out,
        "return table.freeze({{\n\tname = {name:?},\n\tsize = {},\n\tfields = {{\n\t\t{},\n\t}},\n{values}}})",
        shape.size,
        fields.join(",\n\t\t"),
    );
    Ok(out)
}

/// Whether a call's input field of the type `ty` may be filled by a
/// script: its hash comes from the field alone and every value a script can
/// write is valid. A value type's or an enum's hash would come from the
/// module's own text, which a script can forge.
fn is_script_input(ty: &str) -> bool {
    super::PRIMITIVES.contains(&ty) || built_in(ty).is_some()
}

/// The Luau definition of the call `name` of `schema`: a module that exports
/// the types its input and answer reach and the named values, and returns
/// the description `enshwick.caller` asks for the call with; an error when
/// an input field is of a type other than a number, a bool, an entity or a
/// GUID.
pub(super) fn luau_call(schema: &Schema, name: &str, alias: &str) -> Result<String, Error> {
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
    if let Some(field) = input.field.iter().find(|f| !is_script_input(&f.ty)) {
        return Err(Error::ScriptInput {
            call: name.to_owned(),
            field: field.name.clone(),
            ty: field.ty.clone(),
        });
    }
    let writer = Writer {
        schema,
        point: name,
    };
    let mut out = format!(
        "--!strict\n-- The call `{name}` v{} of {} {}.\n-- Written by `cargo enshwick deps {alias} --luau`. Write it again for a newer\n-- {} rather than edit it: the runtime calls by the layout below.\n",
        call.major, schema.owner, schema.version, schema.owner
    );
    let mut types = reached_from(
        schema,
        name,
        input
            .field
            .iter()
            .map(|f| f.ty.as_str())
            .chain([call.output.as_str()])
            .collect(),
    )?;
    types.push(input);
    let values = types_and_values(schema, &types, &mut out);
    let output_hash: u64 = call
        .output_layout
        .parse()
        .map_err(|_| Error::BadHash(name.to_owned()))?;
    let fields = writer.fields(input, Some(&call.layout), 1)?;
    out.push('\n');
    comment(&mut out, "", &call.doc);
    let _ = writeln!(
        out,
        "return table.freeze({{\n\tname = {name:?},\n\tmajor = {},\n\tinput = {{ size = {}, fields = {{\n\t\t{},\n\t}} }},\n\toutput = {{ size = {}, hash = \"{output_hash}\", read = {} }},\n{values}}})",
        call.major,
        call.input_size,
        fields.join(",\n\t\t"),
        call.output_size,
        writer.read(&call.output, 0)?,
    );
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOLD: &str = "schema = 2\nmod = \"you.post\"\nversion = \"1.0.0\"\n[[event]]\nname = \"you.post:sold\"\ndata = \"Sold\"\ndoc = \"A sale.\"\nlayout = [\"1\", \"2\"]\n[[type]]\nname = \"Sold\"\ndoc = \"A sale.\"\nkind = \"struct\"\n[[type.field]]\nname = \"item\"\ndoc = \"The item.\"\ntype = \"u32\"\n[[type.field]]\nname = \"player\"\ndoc = \"The buyer.\"\ntype = \"u64\"\n";

    #[test]
    fn a_point_becomes_a_luau_description_with_its_layout() {
        let schema =
            Schema::parse(include_str!("../../tests/fixtures/lockers.points.toml")).unwrap();
        let text = luau(&schema, "you.lockers:price", "lockers").unwrap();
        assert_eq!(
            text,
            include_str!("../../tests/fixtures/lockers_price.luau")
        );
    }

    #[test]
    fn an_event_becomes_a_luau_description_with_its_layout() {
        let schema = Schema::parse(SOLD).unwrap();
        let module = luau_event(&schema, "you.post:sold", "post").unwrap();
        assert!(module.contains("export type Sold = {"), "{module}");
        assert!(module.contains("size = 16,"), "{module}");
        assert!(
            module.contains("{ name = \"player\", offset = 8, read = \"u64\", hash = \"2\" }"),
            "{module}"
        );
    }

    #[test]
    fn the_modules_of_a_dependency_are_named_by_their_local_names() {
        let schema = Schema::parse(include_str!(
            "../../../../examples/trading-post/points.toml"
        ))
        .unwrap();
        let deps = luau_deps(&schema, "trading_post").unwrap();
        let [(price, text), (sold, _)] = &deps.modules[..] else {
            panic!("{:?}", deps.modules);
        };
        assert_eq!([price.as_str(), sold.as_str()], ["price", "sold"]);
        insta::assert_snapshot!(text);
    }

    #[test]
    fn a_name_that_makes_no_file_name_is_refused() {
        for bad in ["../../main", "sold/again", ".hidden", "Sold"] {
            let text = SOLD.replace("you.post:sold", &format!("you.post:{bad}"));
            let schema = Schema::parse(&text).unwrap();
            assert!(
                matches!(
                    luau_deps(&schema, "post"),
                    Err(Error::Invalid {
                        what: "name for a file",
                        ..
                    })
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_pointer_sized_enum_reads_as_its_64_bit_integer() {
        for (repr, read) in [("usize", "u64"), ("isize", "i64")] {
            let text = format!(
                "schema = 2\nmod = \"you.post\"\nversion = \"1.0.0\"\n[[event]]\nname = \"you.post:sold\"\ndata = \"Sold\"\ndoc = \"A sale.\"\nlayout = [\"1\"]\n[[type]]\nname = \"Sold\"\ndoc = \"A sale.\"\nkind = \"struct\"\n[[type.field]]\nname = \"tier\"\ndoc = \"The tier.\"\ntype = \"Tier\"\n[[type]]\nname = \"Tier\"\ndoc = \"A tier.\"\nkind = \"enum\"\nrepr = \"{repr}\"\n[[type.variant]]\nname = \"Low\"\ndoc = \"Low.\"\nvalue = 1\n[[type.variant]]\nname = \"High\"\ndoc = \"High.\"\nvalue = 2\n"
            );
            let schema = Schema::parse(&text).unwrap();
            let module = luau_event(&schema, "you.post:sold", "post").unwrap();
            assert!(module.contains("size = 8,"), "{module}");
            assert!(
                module.contains(&format!(
                    "read = {{ repr = \"{read}\", variants = {{ 1, 2 }} }}"
                )),
                "{module}"
            );
        }
    }

    #[test]
    fn the_trading_post_s_price_lays_out_as_rust_does() {
        let schema = Schema::parse(include_str!(
            "../../../../examples/trading-post/points.toml"
        ))
        .unwrap();
        let text = luau(&schema, "you.trading-post:price", "trading_post").unwrap();
        assert!(text.contains("input = { size = 24, fields = {"), "{text}");
        assert!(
            text.contains("{ name = \"player\", offset = 8, read = \"player_key\""),
            "{text}"
        );
        assert!(
            text.contains("{ name = \"deal\", offset = 20, read = \"u32\""),
            "{text}"
        );
    }
}
