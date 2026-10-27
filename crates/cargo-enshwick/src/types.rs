//! `cargo enshwick types`: Rust and Luau definitions of the game's own types,
//! written from the reflection data of the game build at hand, for the
//! structs the API does not wrap.

mod luau;
mod names;
mod rust;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use enshwick_gamedata::reflect::{Image, Kind, Layout, Reflection};

use crate::install::GAME_EXECUTABLES;

/// The namespace of the designer data, the editor's twins of the runtime
/// types, written only when named.
const DESIGNER_DATA: &str = "keen::ds::";

/// A reflected type, as the writers read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ty {
    pub(crate) kind: Kind,
    pub(crate) size: u32,
    pub(crate) align: u16,
    pub(crate) base: Option<String>,
    pub(crate) inner: Option<String>,
    pub(crate) count: Option<u32>,
    pub(crate) fields: Vec<Field>,
    pub(crate) values: Vec<(String, u64)>,
}

/// A field of a reflected struct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) offset: u64,
    pub(crate) ty: Option<String>,
}

/// Every type one name names alone, by full name.
pub(crate) type Catalogue = BTreeMap<String, Ty>;

/// Whether a type of `kind` becomes an item of its own: a struct, an enum
/// or a typedef. The rest is written in place where a field uses it.
pub(crate) fn is_item(kind: Kind) -> bool {
    matches!(kind, Kind::Struct | Kind::Enum | Kind::Typedef)
}

/// The catalogue of `reflection`; a name more than one type carries is left
/// out, as lookups leave it.
fn catalogue(reflection: &Reflection<'_>) -> Catalogue {
    reflection
        .type_names()
        .filter_map(|name| {
            let info = reflection.type_info(name).ok()?;
            let ty = Ty {
                kind: info.kind(),
                size: info.size(),
                align: info.align(),
                base: info.base().map(str::to_owned),
                inner: info.inner().map(str::to_owned),
                count: info.element_count(),
                fields: info
                    .fields()
                    .iter()
                    .map(|f| Field {
                        name: f.name().to_owned(),
                        offset: f.offset(),
                        ty: f.type_name().map(str::to_owned),
                    })
                    .collect(),
                values: info
                    .enum_values()
                    .iter()
                    .map(|v| (v.name().to_owned(), v.value()))
                    .collect(),
            };
            Some((name.to_owned(), ty))
        })
        .collect()
}

/// The items to write: those `wanted` names, each a full name or a
/// namespace ending in `::`, and every item they reach through bases,
/// fields and inner types; every runtime item when `wanted` is empty.
///
/// # Errors
///
/// Names no type carries and namespaces that hold no item.
pub(crate) fn select(catalogue: &Catalogue, wanted: &[String]) -> anyhow::Result<BTreeSet<String>> {
    let mut roots: Vec<&str> = Vec::new();
    let mut missing = Vec::new();
    if wanted.is_empty() {
        roots.extend(
            catalogue
                .iter()
                .filter(|(name, ty)| is_item(ty.kind) && !name.starts_with(DESIGNER_DATA))
                .map(|(name, _)| name.as_str()),
        );
    }
    for name in wanted {
        let before = roots.len();
        if name.ends_with("::") {
            roots.extend(
                catalogue
                    .iter()
                    .filter(|(full, ty)| is_item(ty.kind) && full.starts_with(name.as_str()))
                    .map(|(full, _)| full.as_str()),
            );
        } else if let Some((full, _)) = catalogue.get_key_value(name) {
            roots.push(full);
        }
        if roots.len() == before {
            missing.push(name.as_str());
        }
    }
    if !missing.is_empty() {
        bail!("no reflected type is named {}", missing.join(", "));
    }
    let mut chosen = BTreeSet::new();
    let mut seen = BTreeSet::new();
    while let Some(name) = roots.pop() {
        let Some((name, ty)) = catalogue.get_key_value(name) else {
            continue;
        };
        if !seen.insert(name.as_str()) {
            continue;
        }
        if is_item(ty.kind) {
            chosen.insert(name.clone());
        }
        roots.extend(ty.base.as_deref());
        roots.extend(ty.inner.as_deref());
        roots.extend(ty.fields.iter().filter_map(|f| f.ty.as_deref()));
    }
    Ok(chosen)
}

/// The executable `from` names: the file itself, or the game's executable
/// in the folder.
fn executable(from: &Path) -> anyhow::Result<PathBuf> {
    if from.is_file() {
        return Ok(from.to_path_buf());
    }
    GAME_EXECUTABLES
        .iter()
        .map(|exe| from.join(exe))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            anyhow!(
                "{} holds none of {}",
                from.display(),
                GAME_EXECUTABLES.join(", ")
            )
        })
}

/// The catalogue of the game executable `from` names, and the executable's
/// name and build fingerprint, for the written files' header.
fn read(from: &Path) -> anyhow::Result<(Catalogue, String)> {
    let exe = executable(from)?;
    let bytes = std::fs::read(&exe).with_context(|| exe.display().to_string())?;
    let image =
        Image::parse(&bytes, Layout::File, None).with_context(|| exe.display().to_string())?;
    let catalogue = catalogue(&Reflection::scan(&image));
    if catalogue.is_empty() {
        bail!("{} holds no reflection data", exe.display());
    }
    let mut source = exe
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if let Some(fingerprint) = image.fingerprint() {
        let guid = fingerprint
            .guid()
            .iter()
            .fold(String::new(), |mut hex, byte| {
                let _ = write!(hex, "{byte:02x}");
                hex
            });
        let _ = write!(source, " {guid}-{}", fingerprint.age());
    }
    Ok((catalogue, source))
}

/// The Luau definitions of every runtime type of the game executable
/// `from` names, as `cargo enshwick types --luau` writes them.
///
/// # Errors
///
/// When the executable does not read or holds no reflection data.
pub(crate) fn luau_definitions(from: &Path) -> anyhow::Result<String> {
    let (catalogue, source) = read(from)?;
    let chosen = select(&catalogue, &[])?;
    Ok(luau::render(&catalogue, &chosen, &source))
}

/// Writes the definitions of the types `wanted` names, read from the game
/// executable `from` names, to `rust` and `luau`; the Rust module is the
/// result when neither is given.
///
/// # Errors
///
/// When the executable does not read, holds no reflection data, or names
/// none of the types `wanted` asks for.
pub(crate) fn run(
    from: &Path,
    wanted: &[String],
    rust: Option<&Path>,
    luau: Option<&Path>,
) -> anyhow::Result<String> {
    let (catalogue, source) = read(from)?;
    let chosen = select(&catalogue, wanted)?;
    if rust.is_none() && luau.is_none() {
        return Ok(rust::render(&catalogue, &chosen, &source));
    }
    let mut written = Vec::new();
    if let Some(path) = rust {
        std::fs::write(path, rust::render(&catalogue, &chosen, &source))
            .with_context(|| path.display().to_string())?;
        written.push(path.display().to_string());
    }
    if let Some(path) = luau {
        std::fs::write(path, luau::render(&catalogue, &chosen, &source))
            .with_context(|| path.display().to_string())?;
        written.push(path.display().to_string());
    }
    Ok(format!(
        "wrote {} types of {source} to {}",
        chosen.len(),
        written.join(" and ")
    ))
}

#[cfg(test)]
mod tests;
