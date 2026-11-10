//! The Rust writer: the API crate's `typed` modules, a module per reflected
//! namespace, an item per struct, enum and typedef, a role per covered type.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::names::{camel, snake, split};
use super::{Catalogue, Renames, Role, Ty};
use crate::reflect::Kind;

/// The game's enum of its screens, which `ui/screens.rs` is generated from.
const SCREEN_TYPE: &str = "keen::HudScreenType";
/// The member of [`SCREEN_TYPE`] that names no screen.
const NO_SCREEN: &str = "Invalid";
/// Where the screens go, below the enshwick repository root.
const SCREENS_FILE: &str = "crates/enshwick/src/ui/screens.rs";

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
/// rename, and the paths that stay whole where a game type shadows a
/// prelude name.
const ALLOWED: &[&str] = &[
    "clippy::doc_markdown",
    "clippy::module_name_repetitions",
    "clippy::struct_field_names",
    "clippy::struct_excessive_bools",
    "clippy::large_enum_variant",
    "clippy::enum_variant_names",
    "clippy::upper_case_acronyms",
    "unused_qualifications",
];

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

/// The engine's root namespace, which no module repeats.
fn root_of(catalogue: &Catalogue) -> String {
    catalogue
        .types
        .keys()
        .find_map(|name| split(name).0.first().copied())
        .unwrap_or_default()
        .to_owned()
}

/// Where the item for the full name `full` lives below `typed`.
fn place_of(full: &str, root: &str) -> Place {
    let (spaces, last) = split(full);
    let spaces = match spaces.split_first() {
        Some((first, rest)) if *first == root => rest,
        _ => &spaces[..],
    };
    Place {
        module: spaces.iter().map(|n| snake(n)).collect(),
        name: camel(&bare(last, root)),
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
        let mut root = root_file(&top, build);
        root.push_str("\nruntime::named_table! {\n");
        for (id, _) in self.ids()? {
            let _ = writeln!(root, "    \"{id}\" => {id},");
        }
        root.push_str("}\n");
        files.insert("typed.rs".to_owned(), root);
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
            .filter(|(full, _)| {
                self.types
                    .get(**full)
                    .is_some_and(|t| !t.roles.is_empty() && t.kind != Kind::Typedef)
            })
            .map(|(full, place)| (id(place), (*full).to_owned()))
            .collect();
        ids.sort();
        Ok(ids)
    }

    /// The ids of renamed types as older mods know them, each with the new
    /// full name: the loader resolves an old id under its new name.
    fn aliases(&self, renames: &Renames) -> Vec<(String, String)> {
        // A type renamed again resolves to its newest name.
        fn newest<'r>(renames: &'r Renames, mut to: &'r str) -> &'r str {
            for _ in 0..MAX_HOPS {
                match renames.types().find(|(from, _)| *from == to) {
                    Some((_, next)) => to = next,
                    None => break,
                }
            }
            to
        }
        let root = root_of(self);
        let mut aliases: Vec<(String, String)> = renames
            .types()
            .map(|(from, to)| (from, newest(renames, to)))
            .filter(|(_, to)| self.types.get(*to).is_some_and(|t| !t.roles.is_empty()))
            .map(|(from, to)| (id(&place_of(from, &root)), to.to_owned()))
            .collect();
        aliases.sort();
        aliases
    }
}

/// Enshwick's id for the item at `place`: its path below `typed`.
fn id(place: &Place) -> String {
    let mut parts = place.module.clone();
    parts.push(place.name.clone());
    parts.join("::")
}

/// The Rust type the generated root module names the game type `name` by.
#[cfg(test)]
pub(super) fn type_of(catalogue: &Catalogue, name: &str) -> String {
    Writer::new(catalogue).map_or_else(|e| e.to_string(), |w| w.rust_type(name, &[], 0))
}

impl<'c> Writer<'c> {
    fn new(catalogue: &'c Catalogue) -> Result<Self, GenerateError> {
        let root = root_of(catalogue);
        let mut places = BTreeMap::new();
        let mut taken: BTreeMap<Vec<String>, BTreeMap<String, &str>> = BTreeMap::new();
        let mut clashes = Vec::new();
        for (full, ty) in &catalogue.types {
            if !is_item(ty.kind) {
                continue;
            }
            let Place { module, name } = place_of(full, &root);
            let reserved = if module.is_empty() {
                RUNTIME.contains(&name.as_str())
            } else {
                MODULES.contains(&module[0].as_str())
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
                format!("::std::vec::Vec<{}>", inner(hops + 1))
            }
            Kind::BlobString | Kind::DsString => "::std::string::String".to_owned(),
            Kind::BlobOptional | Kind::DsOptional => {
                let held = ty
                    .inner
                    .as_deref()
                    .and_then(|i| self.catalogue.types.get(i));
                if held.is_some_and(|h| h.kind == Kind::Struct) {
                    format!(
                        "::core::option::Option<::std::boxed::Box<{}>>",
                        inner(hops + 1)
                    )
                } else {
                    format!("::core::option::Option<{}>", inner(hops + 1))
                }
            }
            Kind::BlobVariant | Kind::DsVariant => {
                "::core::option::Option<crate::resources::Variant>".to_owned()
            }
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
        let bytes = if ty.size == 1 { "byte" } else { "bytes" };
        let mut doc = match ty.kind {
            Kind::Enum => format!("The game's enum `{shown}`, {} {bytes}.", ty.size),
            Kind::Typedef => format!(
                "The game's `{shown}`, another name for `{}`.",
                bare(ty.inner.as_deref().unwrap_or("?"), &self.root)
            ),
            _ => format!("The game's struct `{shown}`, {} {bytes}.", ty.size),
        };
        let mut more: Vec<String> = ty
            .roles
            .iter()
            .map(|r| role_line(*r, place.module.len()))
            .collect();
        for side in ["client_only", "server_only"] {
            if ty.attributes.iter().any(|a| a == side) {
                more.push(format!(
                    "The game holds it on the {} only.",
                    side.trim_end_matches("_only")
                ));
            }
        }
        if !more.is_empty() {
            let _ = write!(doc, "\n///\n/// {}", more.join(" "));
        }
        let mut out = match ty.kind {
            Kind::Enum => enum_item(ty, place, &doc),
            Kind::Typedef => {
                let target = ty.inner.as_deref().map_or_else(
                    || "crate::resources::Value".to_owned(),
                    |inner| self.rust_type(inner, &place.module, 1),
                );
                let line = format!("pub type {} = {target};", place.name);
                // As rustfmt wraps a line past its width of 100.
                let line = if line.len() > 100 {
                    format!("pub type {} =\n    {target};", place.name)
                } else {
                    line
                };
                format!("/// {doc}\n{line}\n")
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
        let doc = doc.replace('\n', "\n    ");
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
    let doc = doc.replace('\n', "\n    ");
    let mut out = format!(
        "crate::typed::runtime::reflected_enum! {{\n    /// {doc}\n    {} {{\n",
        place.name
    );
    let mut taken = BTreeSet::from(["Other".to_owned()]);
    for (name, value) in &ty.values {
        let variant = variant(name, ty.values.len(), &mut taken);
        let _ = write!(
            out,
            "        /// `{name}`, {value}.\n        {variant} = \"{name}\",\n"
        );
    }
    out.push_str("    }\n}\n");
    out
}

/// The Rust variant of the enum constant `name`, numbered past the names
/// `taken` holds, which it then holds too; `count` is how many constants the
/// enum has.
fn variant(name: &str, count: usize, taken: &mut BTreeSet<String>) -> String {
    let base = camel(name);
    // One suffix more than there are constants always finds a free name.
    let variant = (1..=count + 1)
        .map(|n| {
            if n == 1 {
                base.clone()
            } else {
                format!("{base}{n}")
            }
        })
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_default();
    taken.insert(variant.clone());
    variant
}

impl Catalogue {
    /// The loader's table of every covered type's id and full name, and of
    /// the old ids of the types `renames` records, as the source of
    /// `typed/ids.rs` in this crate.
    ///
    /// # Errors
    ///
    /// [`GenerateError`] when two types take one Rust name in one module.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::{Catalogue, Renames};
    ///
    /// let table = Catalogue::default().ids_file("test", &Renames::default())?;
    /// assert!(table.contains("id_table!"));
    /// # Ok::<(), enshwick_gamedata::typed::GenerateError>(())
    /// ```
    pub fn ids_file(&self, build: &str, renames: &Renames) -> Result<String, GenerateError> {
        let mut out = String::from(
            "// Long file: generated, a row per type the typed API covers.\n\
             //! Enshwick's id of every type the typed API covers and the engine's\n\
             //! name for it, sorted by id, with the old ids of renamed types.\n\n",
        );
        out.push_str(&header(build));
        let mut rows: BTreeMap<String, String> = self.aliases(renames).into_iter().collect();
        rows.extend(self.ids()?);
        out.push_str("\nsuper::id_table! {\n");
        for (id, name) in rows {
            let _ = writeln!(out, "    \"{id}\" => \"{name}\",");
        }
        out.push_str("}\n");
        Ok(out)
    }

    /// Every file the typed API is generated into, by path below the
    /// enshwick repository root: the `typed` modules of the generated
    /// types' crate, the API crate's game screens, and the loader's id table.
    ///
    /// # Errors
    ///
    /// [`GenerateError`] when two types take one Rust name in one module.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::typed::{Catalogue, Renames};
    ///
    /// let files = Catalogue::default().files("test", &Renames::default())?;
    /// assert!(files.contains_key("crates/internal/typed/src/typed.rs"));
    /// # Ok::<(), enshwick_gamedata::typed::GenerateError>(())
    /// ```
    pub fn files(
        &self,
        build: &str,
        renames: &Renames,
    ) -> Result<BTreeMap<String, String>, GenerateError> {
        let mut files: BTreeMap<String, String> = self
            .rust(build)?
            .into_iter()
            .map(|(path, text)| (format!("crates/internal/typed/src/{path}"), text))
            .collect();
        files.insert(
            "crates/enshwick-gamedata/src/typed/ids.rs".to_owned(),
            self.ids_file(build, renames)?,
        );
        if let Some(screens) = self.screens_file(build) {
            files.insert(SCREENS_FILE.to_owned(), screens);
        }
        Ok(files)
    }

    /// `ui/screens.rs` of the API crate: the game's screens, `HudScreenType`
    /// without its `Invalid`, as the Rust enum mods extend screens by and
    /// the names the fake host checks them against; `None` for a catalogue
    /// without the type.
    fn screens_file(&self, build: &str) -> Option<String> {
        let screens: Vec<&str> = self
            .types
            .get(SCREEN_TYPE)?
            .values
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| *name != NO_SCREEN)
            .collect();
        let mut taken = BTreeSet::new();
        let variants: Vec<String> = screens
            .iter()
            .map(|name| variant(name, screens.len(), &mut taken))
            .collect();
        let mut out = String::from(
            "//! The game's screens, which mods extend and the fake host checks\n\
             //! extensions against as the loader does.\n\n",
        );
        out.push_str(&header(build));
        let _ = write!(
            out,
            "\n/// One of the game's screens, which [`Load::extend_screen`](crate::Load::extend_screen) adds a panel to.\n\
             ///\n\
             /// The members and their names are the game's `HudScreenType`, from the\n\
             /// reflection of {build}; a screen the running build lacks is refused at\n\
             /// registration.\n\
             ///\n\
             /// # Examples\n\
             ///\n\
             /// ```\n\
             /// use enshwick::ui::HudScreenType;\n\
             ///\n\
             /// assert_eq!(HudScreenType::StorageChest.name(), \"StorageChest\");\n\
             /// ```\n\
             #[cfg(feature = \"unstable\")]\n\
             #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]\n\
             #[non_exhaustive]\n\
             pub enum HudScreenType {{\n"
        );
        for (name, variant) in screens.iter().zip(&variants) {
            let _ = writeln!(out, "    /// `{name}`.\n    {variant},");
        }
        out.push_str(
            "}\n\n\
             #[cfg(feature = \"unstable\")]\n\
             impl HudScreenType {\n    \
             /// Every screen, in the game's order.\n    \
             ///\n    \
             /// # Examples\n    \
             ///\n    \
             /// ```\n    \
             /// use enshwick::ui::HudScreenType;\n    \
             ///\n    \
             /// assert!(HudScreenType::ALL.contains(&HudScreenType::Map));\n    \
             /// ```\n    \
             pub const ALL: [Self; NAMES.len()] = [\n",
        );
        for variant in &variants {
            let _ = writeln!(out, "        Self::{variant},");
        }
        let _ = write!(
            out,
            "    ];\n}}\n\n\
             /// The game's name of each `HudScreenType` member, in the game's order.\n\
             pub(crate) const NAMES: [&str; {}] = [\n",
            screens.len()
        );
        for name in &screens {
            let _ = writeln!(out, "    \"{name}\",");
        }
        out.push_str("];\n");
        Some(out)
    }
}

/// The doc line of a type with `role`, `depth` modules below `typed`. The
/// handle it names lies in the API crate, above the generated types' own, so
/// the link is the handle's page there, as a page of the type reaches it.
fn role_line(role: Role, depth: usize) -> String {
    let (what, handle) = match role {
        Role::Component => ("An ECS component: read and write it", "ComponentType"),
        Role::Event => ("A game event: handle it", "EventType"),
        Role::Resource => ("A resource: read and serve it", "ResourceType"),
    };
    format!(
        "{what} through [`crate::typed::{handle}`]({}struct.{handle}.html).",
        "../".repeat(depth)
    )
}

fn role_trait(role: Role) -> &'static str {
    match role {
        Role::Component => "Component",
        Role::Event => "Event",
        Role::Resource => "Resource",
    }
}

/// The generated header of every file, below its module doc.
pub(super) fn header(build: &str) -> String {
    format!(
        "// Generated by `cargo enshwick generate` from the reflection of {build};\n\
         // never edited by hand: regenerate it.\n"
    )
}

/// `typed.rs`: the module root, its docs, the runtime and the namespaces.
fn root_file(top: &BTreeSet<&String>, build: &str) -> String {
    let mut out = String::from(
        "// Long file: generated, a row per covered type in the table at its end.\n\
         //! Every component, event and resource the game reflects, as typed Rust\n\
         //! items generated from the game's own reflection; `enshwick::typed`\n\
         //! re-exports them beside the handles that read and write them.\n\n",
    );
    out.push_str(&header(build));
    out.push_str("\nmod root;\n#[doc(hidden)]\npub mod runtime;\n\n");
    for name in top {
        let _ = writeln!(out, "pub mod {name};");
    }
    out.push_str(
        "\npub use root::*;\npub use runtime::{Component, Constant, Event, Flags, Named, Reflected, Resource, ShapeError};\n#[doc(hidden)]\npub use runtime::{Row, check};\n",
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
    out.push_str("\n#![allow(\n");
    for lint in ALLOWED {
        let _ = writeln!(out, "    {lint},");
    }
    out.push_str("    reason = \"the game's own names\"\n)]\n");
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
    // The root namespace's children are files of their own; a child that
    // holds no types itself still holds the namespaces below it.
    let children: BTreeSet<&String> = modules
        .keys()
        .filter(|m| !path.is_empty() && m.len() > path.len() && m.starts_with(path))
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
