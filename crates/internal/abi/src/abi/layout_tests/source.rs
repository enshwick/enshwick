//! What the layout tests read from the ABI source itself: its structs with
//! their fields, and the value of every constant.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The parsed files of the ABI source.
fn abi_sources() -> Vec<syn::File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // The ABI types the ABI module re-exports from the value types' crate.
    let values = root.join("../../values/src/abi.rs");
    let mut files = vec![root.join("abi.rs"), values];
    rust_files(&root.join("abi"), &mut files);
    files.iter().map(|file| parse(file)).collect()
}

/// Adds the Rust files under `dir`, a domain's folder (`events/knowledge.rs`)
/// included, to `out`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The Rust file at `path`, parsed.
fn parse(path: &Path) -> syn::File {
    syn::parse_file(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The constants the version names itself by, which change on every bump
/// rather than with the contract.
const VERSION_CONSTANTS: [&str; 2] = ["ABI_MAJOR", "ABI_MINOR"];

/// The value of every public scalar constant of the ABI source, by
/// `Type::NAME` for one in an `impl` block and by its bare name at module
/// level; a constant of a whole struct, such as an empty spec, is a value of
/// fields the layout already holds and is left out.
pub(super) fn constants_in_source() -> BTreeMap<String, String> {
    let mut raw: BTreeMap<String, (Option<String>, syn::Expr)> = BTreeMap::new();
    let public = |vis: &syn::Visibility| matches!(vis, syn::Visibility::Public(_));
    for file in abi_sources() {
        for item in file.items {
            match item {
                syn::Item::Const(item) if public(&item.vis) => {
                    let name = item.ident.to_string();
                    if !VERSION_CONSTANTS.contains(&name.as_str()) {
                        raw.insert(name, (None, *item.expr));
                    }
                }
                syn::Item::Impl(item) if item.trait_.is_none() => {
                    let syn::Type::Path(ty) = &*item.self_ty else {
                        continue;
                    };
                    let owner = ty.path.segments.last().unwrap().ident.to_string();
                    for inner in item.items {
                        if let syn::ImplItem::Const(c) = inner
                            && public(&c.vis)
                        {
                            let name = format!("{owner}::{}", c.ident);
                            raw.insert(name, (Some(owner.clone()), c.expr));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    raw.iter()
        .filter_map(|(name, (owner, expr))| {
            Some((name.clone(), evaluate(expr, owner.as_deref(), &raw, 0)?))
        })
        .collect()
}

/// The value of a constant's expression as decimal text: integer literals,
/// `Self(..)`, shifts, `|`, `&`, `+`, `-`, a negation, `MAX` of an integer
/// type and other constants of the ABI. `None` for a struct, which
/// [`constants_in_source`] leaves out; any other form fails the test, so a
/// new form is taught here rather than dropped from the golden file.
fn evaluate(
    expr: &syn::Expr,
    owner: Option<&str>,
    all: &BTreeMap<String, (Option<String>, syn::Expr)>,
    depth: u32,
) -> Option<String> {
    use syn::{BinOp, Expr, Lit, UnOp};
    assert!(depth < 16, "constants refer to each other in a loop");
    let int = |expr: &Expr| -> i128 {
        evaluate(expr, owner, all, depth + 1)
            .and_then(|text| text.parse().ok())
            .unwrap_or_else(|| panic!("a constant's operand is no integer: {expr:?}"))
    };
    let text = match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(value) => value.base10_digits().to_owned(),
            Lit::Float(value) => value.base10_digits().to_owned(),
            Lit::Bool(value) => value.value.to_string(),
            Lit::Str(value) => format!("{:?}", value.value()),
            other => panic!("a constant of an unread literal: {other:?}"),
        },
        Expr::Paren(inner) => return evaluate(&inner.expr, owner, all, depth + 1),
        Expr::Group(inner) => return evaluate(&inner.expr, owner, all, depth + 1),
        Expr::Call(call) if call.args.len() == 1 => {
            return evaluate(&call.args[0], owner, all, depth + 1);
        }
        Expr::Struct(_) => return None,
        Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => (-int(&unary.expr)).to_string(),
        Expr::Binary(binary) => {
            let (left, right) = (int(&binary.left), int(&binary.right));
            match binary.op {
                BinOp::Shl(_) => (left << right).to_string(),
                BinOp::BitOr(_) => (left | right).to_string(),
                BinOp::BitAnd(_) => (left & right).to_string(),
                BinOp::Add(_) => (left + right).to_string(),
                BinOp::Sub(_) => (left - right).to_string(),
                BinOp::Mul(_) => (left * right).to_string(),
                BinOp::Div(_) => (left / right).to_string(),
                ref op => panic!("a constant of an unread operator: {op:?}"),
            }
        }
        Expr::Field(field) => return evaluate(&field.base, owner, all, depth + 1),
        Expr::Path(path) => {
            let words: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect();
            let max = |ty: &str| -> Option<String> {
                Some(match ty {
                    "u8" => u8::MAX.to_string(),
                    "u16" => u16::MAX.to_string(),
                    "u32" => u32::MAX.to_string(),
                    "u64" | "usize" => u64::MAX.to_string(),
                    "i64" => i64::MAX.to_string(),
                    _ => return None,
                })
            };
            let name = match &words[..] {
                [ty, last] if last == "MAX" && max(ty).is_some() => return max(ty),
                [this, last] if this == "Self" => format!("{}::{last}", owner?),
                [one] if all.contains_key(&format!("{}::{one}", owner.unwrap_or(""))) => {
                    format!("{}::{one}", owner.unwrap_or(""))
                }
                [.., ty, last] => format!("{ty}::{last}"),
                [one] => one.clone(),
                [] => unreachable!("a path has a segment"),
            };
            let (next_owner, next) = all
                .get(&name)
                .unwrap_or_else(|| panic!("a constant names {name}, which is no ABI constant"));
            return evaluate(next, next_owner.as_deref(), all, depth + 1);
        }
        other => panic!("a constant of an unread form: {other:?}"),
    };
    Some(text)
}

/// The structs of the ABI source, with their field names in order.
pub(super) fn structs_in_source() -> BTreeMap<String, Vec<String>> {
    structs_in(abi_sources())
}

/// The `repr` structs of `files`, with their field names in order.
fn structs_in(files: Vec<syn::File>) -> BTreeMap<String, Vec<String>> {
    let mut found = BTreeMap::new();
    for parsed in files {
        for item in parsed.items {
            let syn::Item::Struct(item) = item else {
                continue;
            };
            let has_repr = item.attrs.iter().any(|attr| attr.path().is_ident("repr"));
            if !has_repr {
                continue;
            }
            let fields = item
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    field
                        .ident
                        .as_ref()
                        .map_or_else(|| index.to_string(), ToString::to_string)
                })
                .collect();
            found.insert(item.ident.to_string(), fields);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_struct_in_a_domain_folder_is_listed() {
        let dir = std::env::temp_dir().join(format!("enshwick-abi-source-{}", std::process::id()));
        let domain = dir.join("events");
        std::fs::create_dir_all(&domain).unwrap();
        std::fs::write(dir.join("events.rs"), "mod knowledge;\n").unwrap();
        std::fs::write(
            domain.join("knowledge.rs"),
            "#[repr(C)]\npub struct Deep {\n    pub value: u32,\n}\n",
        )
        .unwrap();
        let mut files = Vec::new();
        rust_files(&dir, &mut files);
        let parsed = files.iter().map(|file| parse(file)).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            structs_in(parsed).get("Deep"),
            Some(&vec!["value".to_owned()])
        );
    }
}
