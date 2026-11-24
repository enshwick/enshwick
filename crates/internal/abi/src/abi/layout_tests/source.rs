//! What the layout tests read from the ABI source itself: its structs with
//! their fields, and the value of every constant.

use std::collections::BTreeMap;
use std::path::Path;

/// The parsed files of the ABI source: the ABI module and the value types'
/// ABI module it re-exports, each with the submodules rustc builds outside
/// tests, a domain's folder (`events/knowledge.rs`) included.
fn abi_sources() -> Vec<syn::File> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let values = root.join("../../values/src");
    let mut files = Vec::new();
    module_files(&root.join("abi.rs"), &root.join("abi"), &mut files);
    module_files(&values.join("abi.rs"), &values.join("abi"), &mut files);
    files
}

/// Adds the file at `path` and the files of the modules it declares, which
/// sit in `dir`, and of theirs to `out`, leaving out `#[cfg(test)]` modules.
fn module_files(path: &Path, dir: &Path, out: &mut Vec<syn::File>) {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let file = syn::parse_file(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    for item in &file.items {
        if let syn::Item::Mod(module) = item
            && module.content.is_none()
            && !is_test(&module.attrs)
        {
            let name = module.ident.to_string();
            module_files(&dir.join(format!("{name}.rs")), &dir.join(name), out);
        }
    }
    out.push(file);
}

/// Whether `attrs` hold `#[cfg(test)]`.
fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

/// The constants the version names itself by, which change on every bump
/// rather than with the contract.
const VERSION_CONSTANTS: [&str; 2] = ["ABI_MAJOR", "ABI_MINOR"];

/// The value of every public scalar constant of the ABI source, by
/// `Type::NAME` for one in an `impl` block and by its bare name at module
/// level; a constant of a whole struct, such as an empty spec, is a value of
/// fields the layout already holds and is left out.
pub(super) fn constants_in_source() -> BTreeMap<String, String> {
    constants_in(abi_sources())
}

/// The constants of `files`, as [`constants_in_source`] gives them; two of
/// one name fail, since the golden file names each by its name alone.
fn constants_in(files: Vec<syn::File>) -> BTreeMap<String, String> {
    let mut raw: BTreeMap<String, (Option<String>, syn::Expr)> = BTreeMap::new();
    let public = |vis: &syn::Visibility| matches!(vis, syn::Visibility::Public(_));
    let mut add = |name: String, owner: Option<String>, expr: syn::Expr| {
        let earlier = raw.insert(name.clone(), (owner, expr));
        assert!(earlier.is_none(), "two ABI constants are named {name}");
    };
    for file in files {
        for item in file.items {
            match item {
                syn::Item::Const(item) if public(&item.vis) => {
                    let name = item.ident.to_string();
                    if !VERSION_CONSTANTS.contains(&name.as_str()) {
                        add(name, None, *item.expr);
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
                            add(format!("{owner}::{}", c.ident), Some(owner.clone()), c.expr);
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

/// The `repr` structs of `files`, with their field names in order; two of
/// one name fail, since the golden file names each by its name alone.
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
            let name = item.ident.to_string();
            let earlier = found.insert(name.clone(), fields);
            assert!(earlier.is_none(), "two ABI structs are named {name}");
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A fresh folder of one test's own, removed when dropped, also after a
    /// failed assert.
    struct Folder(PathBuf);

    impl Folder {
        /// Makes the folder `purpose` names and writes each `(path, text)` of
        /// `files` in it; `purpose` differs per test, since tests run as
        /// threads of one process under `cargo test`.
        fn with(purpose: &str, files: &[(&str, &str)]) -> Self {
            let dir =
                std::env::temp_dir().join(format!("enshwick-abi-{purpose}-{}", std::process::id()));
            if dir.exists() {
                std::fs::remove_dir_all(&dir).unwrap();
            }
            for (path, text) in files {
                let path = dir.join(path);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            Self(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let removed = std::fs::remove_dir_all(&self.0);
            // A folder that stays fails the test, unless it already fails.
            assert!(
                removed.is_ok() || std::thread::panicking(),
                "{}: {removed:?}",
                self.0.display()
            );
        }
    }

    /// The parsed file of `text`.
    fn parsed(text: &str) -> syn::File {
        syn::parse_file(text).unwrap()
    }

    #[test]
    fn a_domain_folder_is_read_and_a_test_module_or_a_stray_file_is_not() {
        let fixture = "#[repr(C)]\npub struct Fixture {\n    pub value: u32,\n}\n\
                       pub const LIMIT: u32 = 3;\n";
        let folder = Folder::with(
            "source-modules",
            &[
                ("abi.rs", "mod events;\n#[cfg(test)]\nmod tests;\n"),
                ("abi/events.rs", "mod knowledge;\n"),
                (
                    "abi/events/knowledge.rs",
                    "#[repr(C)]\npub struct Deep {\n    pub value: u32,\n}\n\
                     pub const DEPTH: u32 = 2;\n",
                ),
                ("abi/tests.rs", fixture),
                ("abi/stray.rs", fixture),
            ],
        );
        let read = || {
            let mut files = Vec::new();
            module_files(&folder.0.join("abi.rs"), &folder.0.join("abi"), &mut files);
            files
        };
        let structs = structs_in(read());
        assert_eq!(
            structs.into_iter().collect::<Vec<_>>(),
            [("Deep".to_owned(), vec!["value".to_owned()])]
        );
        let constants = constants_in(read());
        assert_eq!(
            constants.into_iter().collect::<Vec<_>>(),
            [("DEPTH".to_owned(), "2".to_owned())]
        );
    }

    #[test]
    #[should_panic(expected = "two ABI structs are named Twin")]
    fn two_structs_of_one_name_fail_the_scan() {
        let twin = "#[repr(C)]\npub struct Twin {\n    pub value: u32,\n}\n";
        structs_in(vec![parsed(twin), parsed(twin)]);
    }

    #[test]
    #[should_panic(expected = "two ABI constants are named LIMIT")]
    fn two_constants_of_one_name_fail_the_scan() {
        let twin = "pub const LIMIT: u32 = 3;\n";
        constants_in(vec![parsed(twin), parsed(twin)]);
    }
}
