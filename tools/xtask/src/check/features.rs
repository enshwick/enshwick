//! A crate's code names the `unstable` feature only when its manifest
//! declares it: the workspace's check-cfg knows that value in every crate,
//! for the badge `derive(Script)` writes, so the compiler no longer refuses
//! a `cfg` that names it where it does not exist.

use std::path::Path;

use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Meta, Token};

use super::domains::Cfg;
use crate::affected::Package;

/// The feature the workspace's check-cfg declares for every crate.
const UNSTABLE: &str = "unstable";

/// The folders of a package that hold its own code.
const CODE: [&str; 4] = ["src", "tests", "examples", "benches"];

/// Each file of a package without the `unstable` feature whose `cfg`,
/// `cfg_attr`, `doc(cfg)` or `cfg!` names it, one line each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let mut problems = Vec::new();
    for package in packages.iter().filter(|package| !package.unstable) {
        for folder in CODE {
            let dir = root.join(&package.dir).join(folder);
            for file in rust_files(&dir) {
                let shown = file.strip_prefix(root).unwrap_or(&file).display();
                let parsed = std::fs::read_to_string(&file)
                    .map_err(|e| e.to_string())
                    .and_then(|text| syn::parse_file(&text).map_err(|e| e.to_string()));
                match parsed {
                    Ok(parsed) if names_unstable(&parsed) => problems.push(format!(
                        "{shown}: a cfg names the feature `{UNSTABLE}`, which {} does not declare: declare it, or drop the cfg",
                        package.name
                    )),
                    Ok(_) => {}
                    Err(e) => problems.push(format!("{shown}: {e}")),
                }
            }
        }
    }
    problems
}

/// The Rust files under `dir`, at any depth; none when it does not exist.
fn rust_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_dir() {
            files.extend(rust_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// Whether a `cfg` of `file` names the `unstable` feature; a macro's
/// tokens other than `cfg!` are not read, so a derive's template does not
/// count.
fn names_unstable(file: &syn::File) -> bool {
    /// Every `cfg` predicate of a file.
    #[derive(Default)]
    struct Cfgs(Vec<Cfg>);

    impl Cfgs {
        /// The predicates of `meta`: a `cfg`, the condition of a `cfg_attr`
        /// and the attributes it adds, or a `doc(cfg)`.
        fn meta(&mut self, meta: &Meta) {
            let Meta::List(list) = meta else { return };
            let Ok(args) = list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            else {
                return;
            };
            if list.path.is_ident("cfg") {
                self.0.extend(args.iter().map(Cfg::new));
            } else if list.path.is_ident("cfg_attr") {
                let mut args = args.iter();
                self.0.extend(args.next().map(Cfg::new));
                args.for_each(|arg| self.meta(arg));
            } else if list.path.is_ident("doc") {
                args.iter().for_each(|arg| self.meta(arg));
            }
        }
    }

    impl<'ast> Visit<'ast> for Cfgs {
        fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
            self.meta(&attr.meta);
        }

        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac.path.is_ident("cfg")
                && let Ok(meta) = mac.parse_body::<Meta>()
            {
                self.0.push(Cfg::new(&meta));
            }
            visit::visit_macro(self, mac);
        }
    }

    let mut cfgs = Cfgs::default();
    cfgs.visit_file(file);
    cfgs.0.iter().any(|cfg| cfg.features().contains(UNSTABLE))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(code: &str) -> bool {
        names_unstable(&syn::parse_file(code).unwrap())
    }

    #[test]
    fn a_cfg_cfg_attr_doc_cfg_or_cfg_macro_naming_unstable_is_seen() {
        assert!(names("#[cfg(feature = \"unstable\")] fn f() {}"));
        assert!(names(
            "#[cfg(all(windows, not(feature = \"unstable\")))] fn f() {}"
        ));
        assert!(names(
            "#![cfg_attr(feature = \"unstable\", allow(dead_code))]"
        ));
        assert!(names(
            "#[cfg_attr(docsrs, doc(cfg(feature = \"unstable\")))] impl T for A {}"
        ));
        assert!(names("fn f() -> bool { cfg!(feature = \"unstable\") }"));
    }

    #[test]
    fn another_feature_or_a_derive_s_template_is_not_seen() {
        assert!(!names("#[cfg(feature = \"test\")] fn f() {}"));
        assert!(!names(
            "fn badge() { quote!(#[cfg_attr(enshwick_docs, doc(cfg(feature = \"unstable\")))]); }"
        ));
        assert!(!names(
            "/// `cfg(feature = \"unstable\")` in a doc\nfn f() {}"
        ));
    }
}
