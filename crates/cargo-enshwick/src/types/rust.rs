//! The Rust module: a module per namespace, a `repr(C)` struct per struct
//! with every field at its reflected offset, a newtype with constants per
//! enum and an alias per typedef. Every type reads from game memory through
//! `enshwick::raw`, so a `bool` or an enum is its integer.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use enshwick_gamedata::reflect::Kind;

use super::names::{camel, shout, snake, split, unique};
use super::{Catalogue, Ty};

/// Typedefs deeper than this are taken for a loop in bad data.
const MAX_HOPS: usize = 32;

/// A Rust type for a game type: its text, size and alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Repr {
    text: String,
    size: u64,
    align: u64,
}

impl Repr {
    fn new(text: impl Into<String>, size: u64, align: u64) -> Self {
        Self {
            text: text.into(),
            size,
            align,
        }
    }

    fn bytes(size: u64) -> Self {
        Self::new(format!("[u8; {size}]"), size, 1)
    }
}

/// A field of a struct as it is written.
struct Member {
    offset: u64,
    game: String,
    rust: String,
    doc: String,
    repr: Option<Repr>,
}

/// Where an item lives: its module path and its name there.
#[derive(Clone, Debug)]
struct Place {
    module: Vec<String>,
    name: String,
}

struct Writer<'c> {
    catalogue: &'c Catalogue,
    places: BTreeMap<&'c str, Place>,
}

/// The Rust module of the `chosen` items of `catalogue`, read from `source`.
pub(crate) fn render(catalogue: &Catalogue, chosen: &BTreeSet<String>, source: &str) -> String {
    let mut taken: BTreeMap<Vec<String>, BTreeSet<String>> = BTreeMap::new();
    let places = chosen
        .iter()
        .map(|full| {
            let (namespaces, last) = split(full);
            let module: Vec<String> = namespaces.iter().map(|n| snake(n)).collect();
            let name = unique(camel(last), taken.entry(module.clone()).or_default());
            (full.as_str(), Place { module, name })
        })
        .collect();
    let writer = Writer { catalogue, places };
    let mut modules: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
    for (full, place) in &writer.places {
        let Some(ty) = catalogue.get(*full) else {
            continue;
        };
        let item = writer.item(full, ty, &place.module);
        modules.entry(place.module.clone()).or_default().push(item);
    }
    let mut out = format!(
        "//! The game's own types, written by `cargo enshwick types` from the\n\
         //! reflection data of `{source}`. Offsets and sizes change between\n\
         //! game builds: write it again for another build.\n"
    );
    write_modules(&mut out, &modules, &[]);
    out
}

/// Writes the module at `path` of `modules` and the modules inside it.
fn write_modules(out: &mut String, modules: &BTreeMap<Vec<String>, Vec<String>>, path: &[String]) {
    let indent = "    ".repeat(path.len());
    for (i, item) in modules.get(path).into_iter().flatten().enumerate() {
        if i > 0 || path.is_empty() {
            out.push('\n');
        }
        for line in item.lines() {
            if line.is_empty() {
                out.push('\n');
            } else {
                let _ = writeln!(out, "{indent}{line}");
            }
        }
    }
    let children: BTreeSet<&String> = modules
        .keys()
        .filter(|module| module.len() > path.len() && module.starts_with(path))
        .map(|module| &module[path.len()])
        .collect();
    for child in children {
        let mut inner = path.to_vec();
        inner.push(child.clone());
        let full = inner.join("::");
        let _ = write!(
            out,
            "\n{indent}/// The game's namespace `{full}`.\n{indent}pub mod {child} {{\n"
        );
        let opened = out.len();
        write_modules(out, modules, &inner);
        // No blank line right inside the brace, as rustfmt writes it.
        if out[opened..].starts_with('\n') {
            out.remove(opened);
        }
        let _ = writeln!(out, "{indent}}}");
    }
}

impl Writer<'_> {
    /// The Rust type for the game type `name`, as code in the module `at`
    /// names it; `None` for a type the reflection does not describe.
    fn repr(&self, name: &str, at: &[String], hops: usize) -> Option<Repr> {
        let ty = self.catalogue.get(name)?;
        let size = u64::from(ty.size);
        let primitive = |text: &str| Some(Repr::new(text, size, size.max(1)));
        match ty.kind {
            Kind::Bool | Kind::Uint8 | Kind::Bitmask8 => primitive("u8"),
            Kind::Sint8 => primitive("i8"),
            Kind::Uint16 | Kind::Bitmask16 => primitive("u16"),
            Kind::Sint16 => primitive("i16"),
            Kind::Uint32 | Kind::Bitmask32 => primitive("u32"),
            Kind::Sint32 => primitive("i32"),
            Kind::Uint64 | Kind::Bitmask64 => primitive("u64"),
            Kind::Sint64 => primitive("i64"),
            Kind::Float32 => primitive("f32"),
            Kind::Struct | Kind::Enum | Kind::Typedef => {
                let place = self.places.get(name)?;
                let text = path_from(at, place);
                Some(Repr::new(text, size, self.align(ty, hops)?))
            }
            Kind::StaticArray => {
                let element = self.repr(ty.inner.as_deref()?, at, hops + 1);
                let count = u64::from(ty.count?);
                match element {
                    Some(element) if element.size.checked_mul(count) == Some(size) => Some(
                        Repr::new(format!("[{}; {count}]", element.text), size, element.align),
                    ),
                    _ => Some(Repr::bytes(size)),
                }
            }
            _ => Some(Repr::bytes(size)),
        }
    }

    /// The alignment of the item for `ty`, as its Rust type has it.
    fn align(&self, ty: &Ty, hops: usize) -> Option<u64> {
        if hops > MAX_HOPS {
            return None;
        }
        match ty.kind {
            Kind::Typedef => self
                .repr(ty.inner.as_deref()?, &[], hops + 1)
                .map(|r| r.align),
            Kind::Enum => Some(enum_int(ty.size).map_or(1, |_| u64::from(ty.size))),
            _ => Some(struct_align(ty)),
        }
    }

    /// The item for the game type `full`, in the module `at`.
    fn item(&self, full: &str, ty: &Ty, at: &[String]) -> String {
        let name = &self.places[full].name;
        match ty.kind {
            Kind::Enum => enum_item(full, name, ty),
            Kind::Typedef => {
                let target = ty
                    .inner
                    .as_deref()
                    .and_then(|inner| self.repr(inner, at, 1))
                    .unwrap_or_else(|| Repr::bytes(u64::from(ty.size)));
                format!(
                    "/// `{full}`, another name for `{}`.\npub type {name} = {};\n",
                    ty.inner.as_deref().unwrap_or("?"),
                    target.text
                )
            }
            _ => self.struct_item(full, name, ty, at),
        }
    }

    /// A struct with each field at its reflected offset and explicit
    /// padding; a field that would land elsewhere in Rust is left in the
    /// padding and named in the struct's documentation.
    fn struct_item(&self, full: &str, name: &str, ty: &Ty, at: &[String]) -> String {
        let size = u64::from(ty.size);
        let align = struct_align(ty);
        let mut body = String::new();
        let mut left_out = Vec::new();
        let mut taken = BTreeSet::new();
        let mut cursor = 0u64;
        let mut members: Vec<Member> = Vec::new();
        if let Some(base) = ty.base.as_deref() {
            members.push(Member {
                offset: 0,
                game: "base".to_owned(),
                rust: "base".to_owned(),
                doc: format!("The game's `{base}`, which this type derives from."),
                repr: self.repr(base, at, 0),
            });
        }
        let mut fields: Vec<_> = ty.fields.iter().collect();
        fields.sort_by_key(|f| f.offset);
        for field in fields {
            let of = field
                .ty
                .as_deref()
                .unwrap_or("a type the game does not reflect");
            members.push(Member {
                offset: field.offset,
                game: field.name.clone(),
                rust: snake(&field.name),
                doc: format!("`{}`, of `{of}`, at {:#x}.", field.name, field.offset),
                repr: field.ty.as_deref().and_then(|t| self.repr(t, at, 0)),
            });
        }
        for member in members {
            let Some(mut repr) = member.repr else {
                left_out.push(member.game);
                continue;
            };
            if repr.align > align || member.offset % repr.align != 0 {
                repr = Repr::bytes(repr.size);
            }
            let offset = member.offset;
            if offset < cursor || offset.saturating_add(repr.size) > size {
                left_out.push(member.game);
                continue;
            }
            if offset > cursor {
                let _ = writeln!(body, "    _pad_{cursor:x}: [u8; {}],", offset - cursor);
            }
            let field_name = unique(member.rust, &mut taken);
            let _ = writeln!(
                body,
                "    /// {}\n    pub {field_name}: {},",
                member.doc, repr.text
            );
            cursor = offset + repr.size;
        }
        if cursor < size {
            let _ = writeln!(body, "    _pad_{cursor:x}: [u8; {}],", size - cursor);
        }
        let mut doc = format!("`{full}`: {size} bytes, aligned to {align}.");
        if !left_out.is_empty() {
            let _ = write!(
                doc,
                " Left in padding, as Rust cannot place them there: `{}`.",
                left_out.join("`, `")
            );
        }
        format!(
            "/// {doc}\n#[repr(C, align({align}))]\n#[derive(Clone, Copy, Debug)]\npub struct {name} {{\n{body}}}\n{}",
            any_bit_pattern(name, size, align)
        )
    }
}

/// The shortest path from the module `at` to the item at `place`.
fn path_from(at: &[String], place: &Place) -> String {
    let common = at
        .iter()
        .zip(&place.module)
        .take_while(|(a, b)| a == b)
        .count();
    let mut text = "super::".repeat(at.len() - common);
    for module in &place.module[common..] {
        text.push_str(module);
        text.push_str("::");
    }
    text.push_str(&place.name);
    text
}

/// The alignment a struct is written with: the reflected one, when it is a
/// power of two that divides the size, else 1.
fn struct_align(ty: &Ty) -> u64 {
    let align = u64::from(ty.align);
    if align.is_power_of_two() && u64::from(ty.size) % align == 0 {
        align
    } else {
        1
    }
}

/// The integer an enum of `size` bytes is stored as.
fn enum_int(size: u32) -> Option<&'static str> {
    match size {
        1 => Some("u8"),
        2 => Some("u16"),
        4 => Some("u32"),
        8 => Some("u64"),
        _ => None,
    }
}

/// A newtype over the enum's integer, with a constant per enum value.
fn enum_item(full: &str, name: &str, ty: &Ty) -> String {
    let size = u64::from(ty.size);
    let Some(int) = enum_int(ty.size) else {
        return format!(
            "/// `{full}`: an enum of {size} bytes.\n#[repr(C)]\n#[derive(Clone, Copy, Debug)]\npub struct {name}(pub [u8; {size}]);\n{}",
            any_bit_pattern(name, size, 1)
        );
    };
    let mask = u64::MAX >> (64 - 8 * size);
    let mut constants = String::new();
    let mut taken = BTreeSet::new();
    for (value_name, value) in &ty.values {
        let constant = unique(shout(value_name), &mut taken);
        let _ = write!(
            constants,
            "    /// `{value_name}`.\n    pub const {constant}: Self = Self({});\n",
            literal(value & mask)
        );
    }
    let constants = if constants.is_empty() {
        String::new()
    } else {
        format!("\nimpl {name} {{\n{constants}}}\n")
    };
    format!(
        "/// `{full}`: an enum stored as `{int}`.\n#[repr(transparent)]\n#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]\npub struct {name}(pub {int});\n{constants}{}",
        any_bit_pattern(name, size, size)
    )
}

/// `value` as a literal clippy reads without complaint.
fn literal(value: u64) -> String {
    if value < 10_000 {
        return value.to_string();
    }
    let hex = format!("{value:x}");
    let mut grouped = String::new();
    for (i, digit) in hex.chars().enumerate() {
        if i > 0 && (hex.len() - i) % 4 == 0 {
            grouped.push('_');
        }
        grouped.push(digit);
    }
    format!("0x{grouped}")
}

/// The traits that let `enshwick::raw` read the type, and the layout checks.
fn any_bit_pattern(name: &str, size: u64, align: u64) -> String {
    format!(
        "\n// SAFETY: every field is an integer, a float or an array of them, so\n\
         // all bytes zero are a valid value.\n\
         unsafe impl enshwick::raw::Zeroable for {name} {{}}\n\
         // SAFETY: every field is an integer, a float or an array of them, so\n\
         // any bytes are a valid value; the type is `Copy` and `'static`.\n\
         unsafe impl enshwick::raw::AnyBitPattern for {name} {{}}\n\
         const _: () = {{\n    assert!(size_of::<{name}>() == {size});\n    assert!(align_of::<{name}>() == {align});\n}};\n"
    )
}
