//! The walk over a file's paths that every part of the domain rule reads:
//! each path the file names, by where it starts, tests left out.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Ident, Spacing, TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{Item, ItemMod, UseTree};

use super::use_leaves;

/// The paths a file names, by where they start.
#[derive(Debug, Default)]
pub(super) struct Named {
    /// Those under its crate's root, absolute.
    pub(super) found: BTreeSet<Vec<String>>,
    /// Those that start at one of the crates the walk counts as external.
    pub(super) external: BTreeSet<Vec<String>>,
    /// The rest, as written: a name the file imports or defines, a local
    /// or a prelude item.
    pub(super) relative: BTreeSet<Vec<String>>,
}

/// The paths `file`, of module `module`, names, with those that start at
/// one of `externs` kept apart; `#[cfg(test)]` items left out, macro bodies
/// read as tokens.
pub(super) fn paths(file: &syn::File, module: &[String], externs: &[&str]) -> Named {
    let mut walk = Walk::new(module, externs);
    walk.visit_file(file);
    walk.named
}

/// The name an item defines, for those a path can name.
pub(super) fn ident(item: &Item) -> Option<String> {
    let ident = match item {
        Item::Const(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Fn(item) => &item.sig.ident,
        Item::Mod(item) => &item.ident,
        Item::Static(item) => &item.ident,
        Item::Struct(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Union(item) => &item.ident,
        Item::Macro(item) => item.ident.as_ref()?,
        _ => return None,
    };
    Some(ident.to_string())
}

/// The names the items of `file` define, tests left out.
pub(super) fn defined(file: &syn::File) -> BTreeSet<String> {
    file.items
        .iter()
        .filter(|item| !test_only(item))
        .filter_map(ident)
        .collect()
}

/// What the `use` items of `file`, of module `module`, bring in, at any
/// depth (a function's own `use` too): each name with the path it stands
/// for, under the crate's root as `crate::…`, at another crate as written,
/// and the modules its globs take whole.
///
/// shortcut: a `use` inside a function, impl or inline module counts as the
/// file's; one that imports a name the file uses otherwise would be misread,
/// so split it into a file of its own.
pub(super) fn imports(
    file: &syn::File,
    module: &[String],
) -> (BTreeMap<String, Vec<String>>, Vec<Vec<String>>) {
    /// Every `use` item of a file outside its tests.
    struct Uses<'ast>(Vec<&'ast syn::ItemUse>);
    impl<'ast> Visit<'ast> for Uses<'ast> {
        fn visit_item(&mut self, item: &'ast Item) {
            if !test_only(item) {
                visit::visit_item(self, item);
            }
        }

        fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
            self.0.push(item);
        }
    }

    let local = defined(file);
    let walk = Walk::new(module, &[]);
    let mut uses = Uses(Vec::new());
    uses.visit_file(file);
    let mut names = BTreeMap::new();
    let mut globs = Vec::new();
    for used in uses.0 {
        for (name, full) in use_leaves(&used.tree) {
            let absolute = match walk.resolve(&full) {
                Some(abs) => std::iter::once("crate".to_owned()).chain(abs).collect(),
                // A plain path starts at an item of the module, else at a
                // crate.
                None if full.first().is_some_and(|first| local.contains(first)) => {
                    std::iter::once("crate".to_owned())
                        .chain(module.iter().cloned())
                        .chain(full)
                        .collect()
                }
                None => full,
            };
            match name {
                Some(name) => {
                    names.insert(name, absolute);
                }
                None => globs.push(absolute),
            }
        }
    }
    (names, globs)
}

/// The walk of [`paths`] over one file.
pub(super) struct Walk<'a> {
    /// The module the walk stands in, inline modules counted.
    module: Vec<String>,
    /// The crates whose paths count as external.
    externs: &'a [&'a str],
    pub(super) named: Named,
}

impl<'a> Walk<'a> {
    pub(super) fn new(module: &[String], externs: &'a [&'a str]) -> Self {
        Self {
            module: module.to_vec(),
            externs,
            named: Named::default(),
        }
    }

    /// Whether `segments` start at one of the external crates.
    fn external(&self, segments: &[String]) -> bool {
        segments
            .first()
            .is_some_and(|first| self.externs.contains(&first.as_str()))
    }

    /// The absolute path `segments` stands for when it starts at `crate`,
    /// `self` or a chain of `super` inside the crate.
    pub(super) fn resolve(&self, segments: &[String]) -> Option<Vec<String>> {
        let supers = segments.iter().take_while(|s| *s == "super").count();
        let (base, rest) = match segments.first().map(String::as_str) {
            Some("crate") => (&[][..], &segments[1..]),
            Some("self") => (&self.module[..], &segments[1..]),
            Some("super") if supers <= self.module.len() => (
                &self.module[..self.module.len() - supers],
                &segments[supers..],
            ),
            _ => return None,
        };
        Some(base.iter().chain(rest).cloned().collect())
    }

    /// Records the path of `segments` by where it starts.
    fn path(&mut self, segments: &[String]) {
        if let Some(absolute) = self.resolve(segments) {
            self.named.found.insert(absolute);
        } else if self.external(segments) {
            self.named.external.insert(segments.to_vec());
        } else if !segments.is_empty() {
            self.named.relative.insert(segments.to_vec());
        }
    }

    /// Records each path of a `use` tree.
    fn tree(&mut self, tree: &UseTree) {
        for (_, full) in use_leaves(tree) {
            self.path(&full);
        }
    }

    /// Records each `a::b` path in a macro body, `$crate::` included.
    fn tokens(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        let mut at = 0;
        while at < trees.len() {
            match &trees[at] {
                TokenTree::Group(group) => {
                    self.tokens(group.stream());
                    at += 1;
                }
                TokenTree::Ident(first) => {
                    let mut segments = vec![first.to_string()];
                    at += 1;
                    while let [
                        TokenTree::Punct(a),
                        TokenTree::Punct(b),
                        TokenTree::Ident(next),
                        ..,
                    ] = &trees[at..]
                        && a.as_char() == ':'
                        && a.spacing() == Spacing::Joint
                        && b.as_char() == ':'
                    {
                        segments.push(next.to_string());
                        at += 3;
                    }
                    self.path(&segments);
                }
                _ => at += 1,
            }
        }
    }
}

impl<'ast> Visit<'ast> for Walk<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if !test_only(item) {
            visit::visit_item(self, item);
        }
    }

    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        let inline = item.content.is_some();
        if inline {
            self.module.push(item.ident.to_string());
        }
        visit::visit_item_mod(self, item);
        if inline {
            self.module.pop();
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path.segments.iter().map(|s| s.ident.to_string()).collect();
        if path.leading_colon.is_none() {
            self.path(&segments);
        } else if self.external(&segments) {
            self.named.external.insert(segments);
        }
        visit::visit_path(self, path);
    }

    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        self.tree(tree);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

/// Whether `item` exists only in tests, under a plain `#[cfg(test)]`.
pub(super) fn test_only(item: &Item) -> bool {
    let attrs = match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => return false,
    };
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.parse_args::<Ident>().is_ok_and(|arg| arg == "test")
    })
}
