//! The walk over a file's paths that every part of the domain rule reads:
//! each path the file names, by where it starts, tests left out.

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Ident, Spacing, TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    Arm, Attribute, Expr, Field, ImplItem, Item, ItemMod, Meta, Stmt, Token, TraitItem, UseTree,
    Variant,
};

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
    /// Each external crate the file names, with the `cfg`s around one
    /// place that names it.
    pub(super) conditions: BTreeSet<(String, Cfg)>,
}

/// The paths `file`, of module `module`, names, with those that start at
/// one of `externs` kept apart; `#[cfg(test)]` items left out, macro bodies
/// read as tokens.
pub(super) fn paths(file: &syn::File, module: &[String], externs: &[&str]) -> Named {
    paths_under(file, module, externs, &[])
}

/// The paths of [`paths`], for a file whose code sits under the `cfg`s of
/// `context`, which each [`Named::conditions`] entry starts with.
pub(super) fn paths_under(
    file: &syn::File,
    module: &[String],
    externs: &[&str],
    context: &[Cfg],
) -> Named {
    let mut walk = Walk::new(module, externs);
    walk.cfg = context.to_vec();
    walk.visit_file(file);
    walk.named
}

/// The paths of [`paths`] that no item under a `#[cfg(feature = "...")]`
/// holds, read through items, impl and trait items, statements, fields,
/// variants and match arms.
pub(super) fn ungated_paths(file: &syn::File, module: &[String], externs: &[&str]) -> Named {
    let mut walk = Walk::new(module, externs);
    walk.ungated = true;
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
    /// Whether the walk leaves out items under a `cfg(feature)`.
    ungated: bool,
    /// The `cfg`s of the items the walk stands in.
    cfg: Vec<Cfg>,
    pub(super) named: Named,
}

impl<'a> Walk<'a> {
    pub(super) fn new(module: &[String], externs: &'a [&'a str]) -> Self {
        Self {
            module: module.to_vec(),
            externs,
            ungated: false,
            cfg: Vec::new(),
            named: Named::default(),
        }
    }

    /// Runs `visit` with the `cfg`s of `attrs` on the walk's stack, unless
    /// the walk leaves out what they gate.
    fn within(&mut self, attrs: &[Attribute], visit: impl FnOnce(&mut Self)) {
        if self.left_out(attrs) {
            return;
        }
        let depth = self.cfg.len();
        self.cfg.extend(cfgs(attrs));
        visit(self);
        self.cfg.truncate(depth);
    }

    /// Records an external path and the `cfg`s it stands under.
    fn external_path(&mut self, segments: Vec<String>) {
        if let Some(first) = segments.first() {
            self.named
                .conditions
                .insert((first.clone(), Cfg::All(self.cfg.clone())));
        }
        self.named.external.insert(segments);
    }

    /// Whether the walk leaves out what `attrs` put under `cfg(test)`, or,
    /// when it reads only ungated code, under a `cfg(feature)`.
    fn left_out(&self, attrs: &[Attribute]) -> bool {
        test_attrs(attrs) || (self.ungated && feature_only(attrs))
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
            self.external_path(segments.to_vec());
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
        self.within(item_attrs(item), |walk| visit::visit_item(walk, item));
    }

    fn visit_impl_item(&mut self, item: &'ast ImplItem) {
        let attrs: &[Attribute] = match item {
            ImplItem::Const(item) => &item.attrs,
            ImplItem::Fn(item) => &item.attrs,
            ImplItem::Type(item) => &item.attrs,
            ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.within(attrs, |walk| visit::visit_impl_item(walk, item));
    }

    fn visit_trait_item(&mut self, item: &'ast TraitItem) {
        let attrs: &[Attribute] = match item {
            TraitItem::Const(item) => &item.attrs,
            TraitItem::Fn(item) => &item.attrs,
            TraitItem::Type(item) => &item.attrs,
            TraitItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.within(attrs, |walk| visit::visit_trait_item(walk, item));
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        let attrs = match stmt {
            Stmt::Local(local) => &local.attrs[..],
            Stmt::Macro(mac) => &mac.attrs,
            Stmt::Expr(expr, _) => expr_attrs(expr),
            Stmt::Item(_) => &[],
        };
        self.within(attrs, |walk| visit::visit_stmt(walk, stmt));
    }

    fn visit_field(&mut self, field: &'ast Field) {
        self.within(&field.attrs, |walk| visit::visit_field(walk, field));
    }

    fn visit_variant(&mut self, variant: &'ast Variant) {
        self.within(&variant.attrs, |walk| visit::visit_variant(walk, variant));
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.within(&arm.attrs, |walk| visit::visit_arm(walk, arm));
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
            self.external_path(segments);
        }
        visit::visit_path(self, path);
    }

    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        self.tree(tree);
    }

    /// Records the external paths in an attribute's arguments, such as a
    /// derive's; its other words are not paths of the crate.
    fn visit_meta_list(&mut self, list: &'ast syn::MetaList) {
        let mut inner = Walk::new(&self.module, self.externs);
        inner.cfg.clone_from(&self.cfg);
        inner.tokens(list.tokens.clone());
        self.named.external.extend(inner.named.external);
        self.named.conditions.extend(inner.named.conditions);
        visit::visit_meta_list(self, list);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit::visit_macro(self, mac);
    }
}

/// Whether `item` exists only in tests, under a plain `#[cfg(test)]`.
pub(super) fn test_only(item: &Item) -> bool {
    test_attrs(item_attrs(item))
}

/// Whether `attrs` hold a plain `#[cfg(test)]`; a compound `cfg` with
/// `test` in it counts as none.
fn test_attrs(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg") && attr.parse_args::<Ident>().is_ok_and(|arg| arg == "test")
    })
}

/// The attributes of `item`; none for a kind that never carries a `cfg`
/// here.
pub(super) fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
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
        _ => &[],
    }
}

/// Whether `attrs` hold a `cfg` only a cargo feature turns on: one that
/// cannot hold while every feature is off, `test` and the other options
/// either way.
pub(super) fn feature_only(attrs: &[Attribute]) -> bool {
    !Cfg::All(cfgs(attrs).collect()).can_hold(&|_| false)
}

/// The predicates of the `#[cfg]` attributes among `attrs`.
pub(super) fn cfgs(attrs: &[Attribute]) -> impl Iterator<Item = Cfg> + '_ {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("cfg"))
        .map(|attr| {
            attr.parse_args::<Meta>()
                .map_or_else(|_| Cfg::Other("cfg".to_owned()), |meta| Cfg::new(&meta))
        })
}

/// A `cfg` predicate, the cargo features it names kept apart from every
/// other option.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Cfg {
    /// `feature = "..."`.
    Feature(String),
    /// Any other option, such as `test` or `windows`, by its name; it may
    /// be on or off.
    Other(String),
    All(Vec<Cfg>),
    Any(Vec<Cfg>),
    Not(Box<Cfg>),
}

impl Cfg {
    pub(crate) fn new(meta: &Meta) -> Self {
        let name = |path: &syn::Path| {
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::")
        };
        match meta {
            Meta::NameValue(pair) if pair.path.is_ident("feature") => match &pair.value {
                Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(feature),
                    ..
                }) => Self::Feature(feature.value()),
                _ => Self::Other(name(&pair.path)),
            },
            Meta::List(list) => {
                let parts = list
                    .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                    .map(|parts| parts.iter().map(Self::new).collect::<Vec<_>>());
                match (list.path.get_ident().map(ToString::to_string), parts) {
                    (Some(all), Ok(parts)) if all == "all" => Self::All(parts),
                    (Some(any), Ok(parts)) if any == "any" => Self::Any(parts),
                    (Some(not), Ok(mut parts)) if not == "not" && parts.len() == 1 => {
                        Self::Not(Box::new(parts.remove(0)))
                    }
                    _ => Self::Other(name(&list.path)),
                }
            }
            Meta::Path(path) => Self::Other(name(path)),
            Meta::NameValue(pair) => Self::Other(name(&pair.path)),
        }
    }

    /// Whether the predicate can hold while only the features `allowed`
    /// takes may be on, every other option either way.
    pub(super) fn can_hold(&self, allowed: &dyn Fn(&str) -> bool) -> bool {
        self.can_be(true, allowed)
    }

    /// Whether the predicate can come out `value`.
    ///
    /// shortcut: an option named twice counts as two free ones, so a
    /// predicate that contradicts itself may count as one that can hold,
    /// never the reverse; read it as a set of clauses if that ever matters.
    fn can_be(&self, value: bool, allowed: &dyn Fn(&str) -> bool) -> bool {
        match self {
            Self::Feature(feature) => !value || allowed(feature),
            Self::Other(_) => true,
            Self::Not(inner) => inner.can_be(!value, allowed),
            Self::All(parts) if value => parts.iter().all(|part| part.can_be(true, allowed)),
            Self::All(parts) => parts.iter().any(|part| part.can_be(false, allowed)),
            Self::Any(parts) if value => parts.iter().any(|part| part.can_be(true, allowed)),
            Self::Any(parts) => parts.iter().all(|part| part.can_be(false, allowed)),
        }
    }

    /// The features the predicate names, at any depth.
    pub(crate) fn features(&self) -> BTreeSet<&str> {
        match self {
            Self::Feature(feature) => BTreeSet::from([feature.as_str()]),
            Self::Other(_) => BTreeSet::new(),
            Self::Not(inner) => inner.features(),
            Self::All(parts) | Self::Any(parts) => parts.iter().flat_map(Self::features).collect(),
        }
    }
}

impl std::fmt::Display for Cfg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |f: &mut std::fmt::Formatter<'_>, word: &str, parts: &[Self]| {
            let parts: Vec<String> = parts.iter().map(ToString::to_string).collect();
            write!(f, "{word}({})", parts.join(", "))
        };
        match self {
            Self::Feature(feature) => write!(f, "feature = \"{feature}\""),
            Self::Other(name) => f.write_str(name),
            Self::Not(inner) => write!(f, "not({inner})"),
            Self::All(parts) if parts.len() == 1 => parts[0].fmt(f),
            Self::All(parts) => list(f, "all", parts),
            Self::Any(parts) => list(f, "any", parts),
        }
    }
}

/// The attributes of an expression statement of the forms that carry a
/// `cfg` in practice.
// shortcut: reads blocks, calls, method calls, macros and assignments only, so a cfg on another expression statement counts as ungated; add its form if one appears.
fn expr_attrs(expr: &Expr) -> &[Attribute] {
    match expr {
        Expr::Assign(expr) => &expr.attrs,
        Expr::Block(expr) => &expr.attrs,
        Expr::Call(expr) => &expr.attrs,
        Expr::Macro(expr) => &expr.attrs,
        Expr::MethodCall(expr) => &expr.attrs,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(text: &str) -> Cfg {
        Cfg::new(&syn::parse_str::<Meta>(text).unwrap())
    }

    #[test]
    fn a_negated_all_or_any_holds_with_every_feature_off_and_a_feature_alone_does_not() {
        let off = |_: &str| false;
        assert!(cfg("not(all(feature = \"a\", feature = \"b\"))").can_hold(&off));
        assert!(cfg("not(any(feature = \"a\", feature = \"b\"))").can_hold(&off));
        assert!(!cfg("all(feature = \"a\", windows)").can_hold(&off));
        assert!(!cfg("any(feature = \"a\", feature = \"b\")").can_hold(&off));
        assert!(cfg("any(feature = \"a\", windows)").can_hold(&off));
        assert!(cfg("all(feature = \"a\", windows)").can_hold(&|f| f == "a"));
    }

    #[test]
    fn a_cfg_reads_back_as_written_and_a_lone_all_part_as_itself() {
        let shown = |text: &str| cfg(text).to_string();
        assert_eq!(
            shown("all(feature = \"a\", not(windows))"),
            "all(feature = \"a\", not(windows))"
        );
        assert_eq!(
            shown("any(test, feature = \"b\")"),
            "any(test, feature = \"b\")"
        );
        assert_eq!(Cfg::All(vec![cfg("windows")]).to_string(), "windows");
    }
}
