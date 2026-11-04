//! The Rust writer: the API crate's `typed` modules, a module per reflected
//! namespace, an item per struct, enum and typedef, a role per covered type.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::names::{camel, snake, split};
use super::{Catalogue, Role, Ty};
use crate::reflect::Kind;

/// Typedefs deeper than this are taken for a loop in bad data.
const MAX_HOPS: usize = 32;

/// The runtime's names in `typed`, which no item of the root namespace may
/// take.
const RUNTIME: &[&str] = &[
    "Component",
    "ComponentType",
    "Constant",
    "Event",
    "EventType",
    "Flags",
    "Named",
    "Reflected",
    "Resource",
    "ResourceType",
    "ShapeError",
];

/// Modules of `typed` that are not namespaces.
const MODULES: &[&str] = &["root", "runtime"];

/// Lints the game's names trip in generated code, which a person cannot
/// rename.
const ALLOWED: &str = "clippy::doc_markdown, clippy::module_name_repetitions, \
    clippy::struct_field_names, clippy::struct_excessive_bools, \
    clippy::large_enum_variant, clippy::enum_variant_names, \
    clippy::upper_case_acronyms";

/// Why the generator cannot write a build's items.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::typed::Catalogue;
///
/// assert!(Catalogue::default().rust("test").is_ok());
/// ```
#[derive(Debug, thiserror::Error)]
#[error("types that share a Rust name in one module: {}", .0.join(", "))]
pub struct GenerateError(Vec<String>);

/// Where an item lives: its module below `typed` and its name there.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Place {
    module: Vec<String>,
    name: String,
}

struct Writer<'c> {
    catalogue: &'c Catalogue,
    root: String,
    places: BTreeMap<&'c str, Place>,
}

/// Whether a type of `kind` is an item of its own.
fn is_item(kind: Kind) -> bool {
    matches!(kind, Kind::Struct | Kind::Enum | Kind::Typedef)
}

/// `text` without the engine's root namespace `root`, for docs and names.
fn bare(text: &str, root: &str) -> String {
    if root.is_empty() {
        text.to_owned()
    } else {
        text.replace(&format!("{root}::"), "")
    }
}

impl Catalogue {
    /// The API crate's `typed` modules for this catalogue, by path below
    /// the crate's `src`, read from the reflection of `build`.
    ///
    /// # Errors
    ///
    /// [`GenerateError`] when two types take one Rust name in one module.
    ///
    /// # Examples
    ///
    /// See [`GenerateError`].
    pub fn rust(&self, build: &str) -> Result<BTreeMap<String, String>, GenerateError> {
        let writer = Writer::new(self)?;
        let mut modules: BTreeMap<Vec<String>, Vec<String>> = BTreeMap::new();
        for (full, place) in &writer.places {
            if let Some(ty) = self.types.get(*full) {
                let item = writer.item(ty, place);
                modules.entry(place.module.clone()).or_default().push(item);
            }
        }
        let mut files = BTreeMap::new();
        let top: BTreeSet<&String> = modules.keys().filter_map(|m| m.first()).collect();
        files.insert("typed.rs".to_owned(), root_file(&top, build));
        let root_items = modules.get(&Vec::new()).map_or(&[][..], Vec::as_slice);
        files.insert(
            "typed/root.rs".to_owned(),
            module_file(
                "the engine's root namespace",
                build,
                root_items,
                &modules,
                &[],
            ),
        );
        for name in top {
            let path = vec![name.clone()];
            let items = modules.get(&path).map_or(&[][..], Vec::as_slice);
            files.insert(
                format!("typed/{name}.rs"),
                module_file(&format!("`{name}`"), build, items, &modules, &path),
            );
        }
        Ok(files)
    }

    /// Every covered type's id and full name, sorted by id, for the
    /// loader's catalogue.
    pub(crate) fn ids(&self) -> Result<Vec<(String, String)>, GenerateError> {
        let writer = Writer::new(self)?;
        let mut ids: Vec<(String, String)> = writer
            .places
            .iter()
            .filter(|(full, _)| self.types.get(**full).is_some_and(|t| !t.roles.is_empty()))
            .map(|(full, place)| (id(place), (*full).to_owned()))
            .collect();
        ids.sort();
        Ok(ids)
    }
}

/// Enshwick's id for the item at `place`: its path below `typed`.
fn id(place: &Place) -> String {
    let mut parts = place.module.clone();
    parts.push(place.name.clone());
    parts.join("::")
}

impl<'c> Writer<'c> {
    fn new(catalogue: &'c Catalogue) -> Result<Self, GenerateError> {
        let root = catalogue
            .types
            .keys()
            .filter_map(|name| split(name).0.first().copied())
            .next()
            .unwrap_or_default()
            .to_owned();
        let mut places = BTreeMap::new();
        let mut taken: BTreeMap<Vec<String>, BTreeMap<String, &str>> = BTreeMap::new();
        let mut clashes = Vec::new();
        for (full, ty) in &catalogue.types {
            if !is_item(ty.kind) {
                continue;
            }
            let (spaces, last) = split(full);
            let spaces = match spaces.split_first() {
                Some((first, rest)) if *first == root => rest,
                _ => &spaces[..],
            };
            let module: Vec<String> = spaces.iter().map(|n| snake(n)).collect();
            let name = camel(&bare(last, &root));
            let reserved = if module.is_empty() {
                RUNTIME.contains(&name.as_str())
            } else {
                module.len() == 1 && MODULES.contains(&module[0].as_str())
            };
            let names = taken.entry(module.clone()).or_default();
            if reserved {
                clashes.push(format!("{full} (a name of the runtime)"));
            } else if let Some(other) = names.insert(name.clone(), full) {
                clashes.push(format!("{other} and {full}"));
            }
            places.insert(full.as_str(), Place { module, name });
        }
        if !clashes.is_empty() {
            return Err(GenerateError(clashes));
        }
        Ok(Self {
            catalogue,
            root,
            places,
        })
    }

    /// The Rust type of the game type `name`, as code in `at` names it.
    fn rust_type(&self, name: &str, at: &[String], hops: usize) -> String {
        const VALUE: &str = "crate::resources::Value";
        let Some(ty) = self.catalogue.types.get(name) else {
            return VALUE.to_owned();
        };
        let inner = |hops| {
            ty.inner
                .as_deref()
                .map_or_else(|| VALUE.to_owned(), |i| self.rust_type(i, at, hops))
        };
        match ty.kind {
            Kind::Bool => "bool".to_owned(),
            Kind::Uint8 => "u8".to_owned(),
            Kind::Sint8 => "i8".to_owned(),
            Kind::Uint16 => "u16".to_owned(),
            Kind::Sint16 => "i16".to_owned(),
            Kind::Uint32 => "u32".to_owned(),
            Kind::Sint32 => "i32".to_owned(),
            Kind::Uint64 => "u64".to_owned(),
            Kind::Sint64 => "i64".to_owned(),
            Kind::Float32 => "f32".to_owned(),
            Kind::Struct | Kind::Enum | Kind::Typedef => self.path(name, at),
            Kind::Bitmask8 | Kind::Bitmask16 | Kind::Bitmask32 | Kind::Bitmask64 => {
                match ty
                    .inner
                    .as_deref()
                    .and_then(|i| self.catalogue.types.get(i))
                {
                    Some(flag) if flag.kind == Kind::Enum => {
                        format!("crate::typed::Flags<{}>", self.path(&flag.name, at))
                    }
                    _ => VALUE.to_owned(),
                }
            }
            _ if hops > MAX_HOPS => VALUE.to_owned(),
            Kind::StaticArray | Kind::BlobArray | Kind::DsArray => {
                format!("Vec<{}>", inner(hops + 1))
            }
            Kind::BlobString | Kind::DsString => "String".to_owned(),
            Kind::BlobOptional | Kind::DsOptional => {
                let held = ty
                    .inner
                    .as_deref()
                    .and_then(|i| self.catalogue.types.get(i));
                if held.is_some_and(|h| h.kind == Kind::Struct) {
                    format!("Option<Box<{}>>", inner(hops + 1))
                } else {
                    format!("Option<{}>", inner(hops + 1))
                }
            }
            Kind::BlobVariant | Kind::DsVariant => "Option<crate::resources::Variant>".to_owned(),
            Kind::ObjectReference | Kind::Guid => "crate::content::Guid".to_owned(),
            _ => VALUE.to_owned(),
        }
    }

    /// The path code in the module `at` names the item `name` by: bare in
    /// its own module, else from the crate root.
    fn path(&self, name: &str, at: &[String]) -> String {
        let Some(place) = self.places.get(name) else {
            return "crate::resources::Value".to_owned();
        };
        if place.module == at {
            return place.name.clone();
        }
        let mut path = String::from("crate::typed::");
        for module in &place.module {
            path.push_str(module);
            path.push_str("::");
        }
        path.push_str(&place.name);
        path
    }

    /// The item for `ty` at `place`.
    fn item(&self, ty: &Ty, place: &Place) -> String {
        let shown = bare(&ty.name, &self.root);
        let mut doc = match ty.kind {
            Kind::Enum => format!("The game's enum `{shown}`, {} bytes.", ty.size),
            Kind::Typedef => format!(
                "The game's `{shown}`, another name for `{}`.",
                bare(ty.inner.as_deref().unwrap_or("?"), &self.root)
            ),
            _ => format!("The game's struct `{shown}`, {} bytes.", ty.size),
        };
        for role in &ty.roles {
            let _ = write!(doc, " {}", role_line(*role));
        }
        for side in ["client_only", "server_only"] {
            if ty.attributes.iter().any(|a| a == side) {
                let _ = write!(
                    doc,
                    " The game holds it on the {} only.",
                    side.trim_end_matches("_only")
                );
            }
        }
        let mut out = match ty.kind {
            Kind::Enum => enum_item(ty, place, &doc),
            Kind::Typedef => {
                let target = ty.inner.as_deref().map_or_else(
                    || "crate::resources::Value".to_owned(),
                    |inner| self.rust_type(inner, &place.module, 1),
                );
                format!("/// {doc}\npub type {} = {target};\n", place.name)
            }
            _ => self.struct_item(ty, place, &doc),
        };
        if !ty.roles.is_empty() && ty.kind != Kind::Typedef {
            let roles: Vec<&str> = ty.roles.iter().map(|r| role_trait(*r)).collect();
            let _ = writeln!(
                out,
                "crate::typed::runtime::named!({} = \"{}\": {});",
                place.name,
                id(place),
                roles.join(", ")
            );
        }
        out
    }

    fn struct_item(&self, ty: &Ty, place: &Place, doc: &str) -> String {
        let mut out = format!(
            "crate::typed::runtime::reflected_struct! {{\n    /// {doc}\n    {}",
            place.name
        );
        if let Some(base) = ty.base.as_deref() {
            let _ = write!(out, ": {}", self.rust_type(base, &place.module, 0));
        }
        out.push_str(" {\n");
        let mut taken = BTreeSet::from(["base".to_owned()]);
        for field in &ty.fields {
            let Some(of) = field.ty.as_deref() else {
                continue;
            };
            let rust = super::names::unique(snake(&field.name), &mut taken);
            let _ = write!(
                out,
                "        /// `{}`, of `{}`.\n        {rust}: {} = \"{}\",\n",
                field.name,
                bare(of, &self.root),
                self.rust_type(of, &place.module, 0),
                field.name
            );
        }
        out.push_str("    }\n}\n");
        out
    }
}

fn enum_item(ty: &Ty, place: &Place, doc: &str) -> String {
    let mut out = format!(
        "crate::typed::runtime::reflected_enum! {{\n    /// {doc}\n    {} {{\n",
        place.name
    );
    let mut taken = BTreeSet::from(["Other".to_owned()]);
    for (name, value) in &ty.values {
        let variant = super::names::unique(camel(name), &mut taken);
        let _ = write!(
            out,
            "        /// `{name}`, {value}.\n        {variant} = \"{name}\",\n"
        );
    }
    out.push_str("    }\n}\n");
    out
}

impl Catalogue {
    /// The loader's table of every covered type's id and full name, as the
    /// source of `typed/ids.rs` in this crate.
    ///
    /// # Errors
    ///
    /// [`GenerateError`] when two types take one Rust name in one module.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::Catalogue;
    ///
    /// assert!(Catalogue::default().ids_file("test")?.contains("IDS"));
    /// # Ok::<(), enshwick_gamedata::typed::GenerateError>(())
    /// ```
    pub fn ids_file(&self, build: &str) -> Result<String, GenerateError> {
        let mut out = String::from(
            "// Long file: generated, a row per type the typed API covers.\n\
             //! Enshwick's id of every type the typed API covers and the engine's\n\
             //! name for it, sorted by id.\n\n",
        );
        out.push_str(&header(build));
        out.push_str("\npub(super) const IDS: &[(&str, &str)] = &[\n");
        for (id, name) in self.ids()? {
            let _ = writeln!(out, "    (\"{id}\", \"{name}\"),");
        }
        out.push_str("];\n");
        Ok(out)
    }
}

fn role_line(role: Role) -> &'static str {
    match role {
        Role::Component => {
            "An ECS component: read and write it through [`crate::typed::ComponentType`]."
        }
        Role::Event => "A game event: handle it through [`crate::typed::EventType`].",
        Role::Resource => "A resource: read and serve it through [`crate::typed::ResourceType`].",
    }
}

fn role_trait(role: Role) -> &'static str {
    match role {
        Role::Component => "Component",
        Role::Event => "Event",
        Role::Resource => "Resource",
    }
}

/// The generated header of every file, below its module doc.
fn header(build: &str) -> String {
    format!(
        "// Generated by `cargo enshwick generate` from the reflection of {build};\n\
         // never edited by hand: regenerate it.\n"
    )
}

/// `typed.rs`: the module root, its docs, the runtime and the namespaces.
fn root_file(top: &BTreeSet<&String>, build: &str) -> String {
    let mut out = String::from(
        "//! Every component, event and resource the game reflects, as typed Rust\n\
         //! items generated from the game's own reflection: a module per\n\
         //! reflected namespace, read and written by field name, so a type whose\n\
         //! shape a build changes turns off alone.\n\
         //!\n\
         //! # Examples\n\
         //!\n\
         //! ```\n\
         //! use enshwick::typed::{ComponentType, ecs::Health};\n\
         //!\n\
         //! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {\n\
         //!     let health = ComponentType::<Health>::resolve(load)?;\n\
         //!     let world = load.world()?;\n\
         //!     load.on::<enshwick::events::PlayerJoined>(move |joined| {\n\
         //!         if let Ok(Some(value)) = health.read(&world, joined.entity) {\n\
         //!             enshwick::info!(\"{value:?}\");\n\
         //!         }\n\
         //!     })\n\
         //! }\n\
         //! ```\n\n",
    );
    out.push_str(&header(build));
    out.push_str("\nmod root;\npub(crate) mod runtime;\n\n");
    for name in top {
        let _ = writeln!(out, "pub mod {name};");
    }
    out.push_str(
        "\npub use root::*;\npub use runtime::{\n    Component, ComponentType, Constant, Event, EventType, Flags, Named, Reflected, Resource,\n    ResourceType, ShapeError,\n};\n",
    );
    out
}

/// A module file: its items and its nested namespaces inline.
fn module_file(
    what: &str,
    build: &str,
    items: &[String],
    modules: &BTreeMap<Vec<String>, Vec<String>>,
    path: &[String],
) -> String {
    let mut out = format!(
        "// Long file: generated, one file per reflected namespace.\n\
         //! The game's types of {what}.\n\n"
    );
    out.push_str(&header(build));
    let _ = writeln!(
        out,
        "\n#![allow({ALLOWED}, reason = \"the game's own names\")]"
    );
    write_items(&mut out, items, modules, path, 0);
    out
}

fn write_items(
    out: &mut String,
    items: &[String],
    modules: &BTreeMap<Vec<String>, Vec<String>>,
    path: &[String],
    depth: usize,
) {
    let indent = "    ".repeat(depth);
    for item in items {
        out.push('\n');
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
        .filter(|m| m.len() == path.len() + 1 && m.starts_with(path))
        .map(|m| &m[path.len()])
        .collect();
    for child in children {
        let mut inner = path.to_vec();
        inner.push(child.clone());
        let _ = write!(
            out,
            "\n{indent}/// The game's types of `{}`.\n{indent}pub mod {child} {{\n",
            inner.join("::")
        );
        let items = modules.get(&inner).map_or(&[][..], Vec::as_slice);
        write_items(out, items, modules, &inner, depth + 1);
        let _ = writeln!(out, "{indent}}}");
    }
}
