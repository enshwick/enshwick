//! The Luau definitions, `game.d.luau`: an exported record type per struct,
//! a union of string literals per enum and an alias per typedef, types only.
//! Luau has no namespaces, so a type keeps its short name where that is
//! unique, and takes as many of its namespaces as it needs where not.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use enshwick_gamedata::reflect::Kind;

use super::names::{camel, is_luau_name, split, unique};
use super::{Catalogue, Ty};

/// The Luau definitions of the `chosen` items of `catalogue`, read from
/// `source`.
pub(crate) fn render(catalogue: &Catalogue, chosen: &BTreeSet<String>, source: &str) -> String {
    let names = luau_names(chosen);
    let mut out = format!(
        "-- The game's own types, written by `cargo enshwick types` from the\n\
         -- reflection data of `{source}`. Write it again for another game build.\n"
    );
    for (full, name) in &names {
        let Some(ty) = catalogue.get(*full) else {
            continue;
        };
        let _ = write!(out, "\n-- `{full}`\nexport type {name} = ");
        match ty.kind {
            Kind::Enum => {
                let values: Vec<String> = ty.values.iter().map(|(v, _)| format!("{v:?}")).collect();
                if values.is_empty() {
                    out.push_str("number");
                } else {
                    out.push_str(&values.join(" | "));
                }
            }
            Kind::Typedef => {
                out.push_str(&luau_type(catalogue, &names, ty.inner.as_deref(), 0));
            }
            _ => out.push_str(&record(catalogue, &names, ty)),
        }
        out.push('\n');
    }
    out
}

/// The record of a struct's fields, joined with its base's.
fn record(catalogue: &Catalogue, names: &BTreeMap<&str, String>, ty: &Ty) -> String {
    let mut fields: Vec<_> = ty.fields.iter().collect();
    fields.sort_by_key(|f| f.offset);
    let base = ty.base.as_deref().and_then(|base| names.get(base));
    if fields.is_empty() {
        return base.cloned().unwrap_or_else(|| "{}".to_owned());
    }
    let mut text = String::from("{\n");
    for field in fields {
        let key = if is_luau_name(&field.name) {
            field.name.clone()
        } else {
            format!("[{:?}]", field.name)
        };
        let _ = writeln!(
            text,
            "    {key}: {},",
            luau_type(catalogue, names, field.ty.as_deref(), 0)
        );
    }
    text.push('}');
    match base {
        Some(base) => format!("{base} & {text}"),
        None => text,
    }
}

/// The Luau type for the game type `name`.
fn luau_type(
    catalogue: &Catalogue,
    names: &BTreeMap<&str, String>,
    name: Option<&str>,
    hops: usize,
) -> String {
    let unknown = || "unknown".to_owned();
    let Some((full, ty)) = name.and_then(|n| catalogue.get_key_value(n)) else {
        return unknown();
    };
    if hops > 32 {
        return unknown();
    }
    let element = || luau_type(catalogue, names, ty.inner.as_deref(), hops + 1);
    match ty.kind {
        Kind::Bool => "boolean".to_owned(),
        Kind::Uint8
        | Kind::Sint8
        | Kind::Uint16
        | Kind::Sint16
        | Kind::Uint32
        | Kind::Sint32
        | Kind::Uint64
        | Kind::Sint64
        | Kind::Float32
        | Kind::Bitmask8
        | Kind::Bitmask16
        | Kind::Bitmask32
        | Kind::Bitmask64 => "number".to_owned(),
        Kind::Guid | Kind::BlobString | Kind::DsString => "string".to_owned(),
        Kind::Struct | Kind::Enum | Kind::Typedef => {
            names.get(full.as_str()).cloned().unwrap_or_else(unknown)
        }
        Kind::StaticArray | Kind::BlobArray | Kind::DsArray => format!("{{{}}}", element()),
        Kind::BlobOptional | Kind::DsOptional => format!("{}?", element()),
        _ => unknown(),
    }
}

/// The Luau name of every chosen item: its short name, with as many of its
/// namespaces as keep it apart from the others.
fn luau_names(chosen: &BTreeSet<String>) -> BTreeMap<&str, String> {
    let parts: Vec<(&str, Vec<String>)> = chosen
        .iter()
        .map(|full| {
            let (namespaces, last) = split(full);
            let mut words: Vec<String> = namespaces.iter().map(|n| camel(n)).collect();
            words.push(camel(last));
            (full.as_str(), words)
        })
        .collect();
    let name_of = |words: &[String], depth: usize| words[words.len() - depth..].join("_");
    let mut taken = BTreeSet::new();
    parts
        .iter()
        .map(|(full, words)| {
            let mut depth = 1;
            while depth < words.len()
                && parts
                    .iter()
                    .filter(|(_, other)| {
                        other.len() >= depth && name_of(other, depth) == name_of(words, depth)
                    })
                    .count()
                    > 1
            {
                depth += 1;
            }
            (*full, unique(name_of(words, depth), &mut taken))
        })
        .collect()
}
