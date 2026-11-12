//! `cargo xtask peel-load <from>=<into>...`: the step of a peel that moves
//! `Load`'s methods off `enshwick-core`. Each `impl Load` block of the
//! module file `from`, already moved into the crate below, goes onto the
//! base handle, and its public methods' docs, examples and `#[script]` go
//! to a forwarder block on `Load` appended to `into` in core, whose bodies
//! `#[forward(base)]` writes. In `enshwick-base`, the crate of `BaseLoad`,
//! a block becomes an `impl BaseLoad` block whose public methods carry one
//! doc line. Above it Rust allows no inherent method on `BaseLoad` (E0116),
//! so a module's blocks become one hidden trait, `LoadMethods`, and its
//! `impl` for `BaseLoad`; the crate's `src/__load.rs` re-exports each such
//! trait as `_`, and the forwarder file imports them all with a glob, as
//! does a peeled module that calls a method another module of the run moved;
//! a peeled module loses its import of core's `Load`.

use std::fmt::Write as _;
use std::path::Path;

use proc_macro2::{LineColumn, Span};
use syn::{Attribute, ImplItem, ImplItemFn, Item, ItemImpl, Type, Visibility};

/// The base handle's type, which the moved blocks implement.
const BASE: &str = "BaseLoad";
/// The field of `Load` that holds the base handle.
const FIELD: &str = "base";
/// The hidden trait that holds a module's methods above `enshwick-base`.
const TRAIT: &str = "LoadMethods";
/// The crate-private trait that holds a module's other moved methods.
const HELPERS: &str = "LoadHelpers";
/// The module of a domain crate that re-exports its modules' traits.
const REEXPORTS: &str = "__load";

/// How a module's blocks land on the base handle.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Form {
    /// Inherent `impl BaseLoad` blocks, in `enshwick-base`.
    Base,
    /// One `LoadMethods` trait per module, in the crate `krate` (its Rust
    /// name), whose root reaches the module at `module` and re-exports its
    /// trait in the file `reexports`, a path from the workspace root.
    Trait {
        krate: String,
        module: String,
        reexports: String,
    },
}

impl Form {
    /// The form for a module file at `from`, a path from the workspace root
    /// or under it: inherent blocks in `crates/internal/base`, the trait
    /// form in any other crate of `crates/internal`.
    ///
    /// # Errors
    ///
    /// When `from` is no module file of a crate in `crates/internal`.
    fn of(root: &Path, from: &str) -> Result<Self, String> {
        // A backslash separates only on Windows; the argument may come from either.
        let from_slashes = from.replace('\\', "/");
        let path = Path::new(&from_slashes);
        let relative = path.strip_prefix(root).unwrap_or(path);
        let parts: Vec<String> = relative
            .components()
            .filter_map(|part| match part {
                std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        match parts.as_slice() {
            ["crates", "internal", "base", "src", _, ..] => Ok(Self::Base),
            ["crates", "internal", dir, "src", rest @ ..] if !rest.is_empty() => {
                let mut module: Vec<&str> = rest.to_vec();
                if let Some(last) = module.last_mut() {
                    *last = last.strip_suffix(".rs").unwrap_or(last);
                }
                if module.last() == Some(&"mod") || module.as_slice() == ["lib"] {
                    module.pop();
                }
                Ok(Self::Trait {
                    krate: format!("enshwick_{}", dir.replace('-', "_")),
                    module: module.join("::"),
                    reexports: format!("crates/internal/{dir}/src/{REEXPORTS}.rs"),
                })
            }
            _ => Err(format!(
                "{from}: not a module file of a crate in crates/internal"
            )),
        }
    }
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool, String> {
    if args.is_empty() {
        return Err("usage: cargo xtask peel-load <from>=<into>...".to_owned());
    }
    let mut traits = Vec::new();
    for arg in args {
        let (from, into) = arg
            .split_once('=')
            .ok_or_else(|| format!("{arg}: not <from>=<into>"))?;
        let form = Form::of(root, from)?;
        let (from, into) = (root.join(from), root.join(into));
        let text =
            std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        let mut peeled = peel(&text, &form).map_err(|e| format!("{}: {e}", from.display()))?;
        if let Form::Trait {
            module, reexports, ..
        } = &form
        {
            for block in &mut peeled.forwarders {
                *block = rebase_links(block, module);
            }
            for name in &peeled.traits {
                reexport(&root.join(reexports), module, name)?;
            }
            traits.push((from.clone(), root.join(reexports)));
        }
        // The forwarders, which keep the docs, are written before `from`
        // loses them.
        if !peeled.forwarders.is_empty() {
            write_forwarders(&into, &peeled)?;
            crate::say(&format!(
                "{}: {} forwarder blocks into {}",
                from.display(),
                peeled.forwarders.len(),
                into.display()
            ));
        }
        std::fs::write(&from, &peeled.base).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    // Once every module of the run is peeled, each that calls a method
    // another module moved imports the crate's traits.
    for (file, reexports) in traits {
        let others = moved_methods(&reexports, &file)?;
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        if let Some(out) =
            with_load_glob(&text, &others).map_err(|e| format!("{}: {e}", file.display()))?
        {
            std::fs::write(&file, out).map_err(|e| format!("{}: {e}", file.display()))?;
        }
    }
    Ok(true)
}

/// `block`, a forwarder block whose docs come from `module` of a domain
/// crate, with its doc links relative to that module made paths from the
/// crate root, which core re-exports at the same paths: `self` is the
/// module, `super` its parent.
fn rebase_links(block: &str, module: &str) -> String {
    let path = |module: &str| {
        if module.is_empty() {
            "crate".to_owned()
        } else {
            format!("crate::{module}")
        }
    };
    let parent = module.rsplit_once("::").map_or("", |(parent, _)| parent);
    let (own, parent) = (path(module), path(parent));
    // A link's target follows `](` inline, or `]: ` in a reference line.
    let swaps = [
        ("](self)".to_owned(), format!("]({own})")),
        ("](self::".to_owned(), format!("]({own}::")),
        ("](super::".to_owned(), format!("]({parent}::")),
        ("]: self::".to_owned(), format!("]: {own}::")),
        ("]: super::".to_owned(), format!("]: {parent}::")),
    ];
    let mut out = String::with_capacity(block.len());
    for line in block.split_inclusive('\n') {
        if !line.trim_start().starts_with("///") {
            out.push_str(line);
            continue;
        }
        let mut rebased = line.to_owned();
        for (from, to) in &swaps {
            rebased = rebased.replace(from.as_str(), to);
        }
        let body = rebased.trim_end_matches(['\r', '\n']);
        if let Some(head) = body.strip_suffix("]: self") {
            rebased = format!("{head}]: {own}{}", &rebased[body.len()..]);
        }
        out.push_str(&rebased);
    }
    out
}

/// The methods the traits re-exported in `reexports`, a crate's
/// `__load.rs`, hold, but those of `own`, a module file of the crate.
fn moved_methods(reexports: &Path, own: &Path) -> Result<Vec<String>, String> {
    let listed =
        std::fs::read_to_string(reexports).map_err(|e| format!("{}: {e}", reexports.display()))?;
    let src = reexports.parent().unwrap_or(reexports);
    let mut found = Vec::new();
    for line in listed.lines() {
        let Some(path) = line
            .split_once("use crate::")
            .and_then(|(_, rest)| rest.strip_suffix(" as _;"))
        else {
            continue;
        };
        let (module, name) = path.rsplit_once("::").unwrap_or(("", path));
        let file = if module.is_empty() {
            src.join("lib.rs")
        } else {
            let stem = src.join(module.replace("::", "/"));
            let flat = stem.with_extension("rs");
            if flat.is_file() {
                flat
            } else {
                stem.join("mod.rs")
            }
        };
        if file == own {
            continue;
        }
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let parsed = syn::parse_file(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        for item in parsed.items {
            if let Item::Trait(item) = item
                && item.ident == name
            {
                found.extend(item.items.iter().filter_map(|entry| match entry {
                    syn::TraitItem::Fn(method) => Some(method.sig.ident.to_string()),
                    _ => None,
                }));
            }
        }
    }
    Ok(found)
}

/// `text`, a peeled module file, with the glob of its crate's method
/// traits after its last import, when it calls one of `others`, the
/// methods other modules moved, and lacks it; `None` when it needs none.
fn with_load_glob(text: &str, others: &[String]) -> Result<Option<String>, String> {
    let line = format!("use crate::{REEXPORTS}::*;");
    if text.contains(&line)
        || !others
            .iter()
            .any(|name| text.contains(&format!(".{name}(")))
    {
        return Ok(None);
    }
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let at = Offsets::new(text);
    let last_use = file.items.iter().rev().find_map(|item| match item {
        Item::Use(item) => Some(at.byte(item.semi_token.span.end())),
        _ => None,
    });
    let mut out = text.to_owned();
    if let Some(end) = last_use {
        out.insert_str(end, &format!("\n{line}"));
    } else {
        out.insert_str(after_inner_docs(text), &format!("{line}\n\n"));
    }
    Ok(Some(out))
}

/// Where a file's text after its inner doc comments, inner attributes and
/// the blank lines after them starts.
fn after_inner_docs(text: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with("//!") || trimmed.starts_with("#![") || trimmed.is_empty() {
            at += line.len();
        } else {
            break;
        }
    }
    at
}

/// Appends the forwarders of `peeled` to `into`, a new file when it is
/// missing.
fn write_forwarders(into: &Path, peeled: &Peeled) -> Result<(), String> {
    {
        let mut out = match std::fs::read_to_string(into) {
            Ok(kept) => with_glob(kept, peeled.krate.as_deref()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                header(&peeled.uses, peeled.krate.as_deref())
            }
            Err(e) => return Err(format!("{}: {e}", into.display())),
        };
        for block in &peeled.forwarders {
            out.push('\n');
            out.push_str(block);
            out.push('\n');
        }
        if let Some(dir) = into.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(into, out).map_err(|e| format!("{}: {e}", into.display()))?;
    }
    Ok(())
}

fn with_glob(kept: String, krate: Option<&str>) -> String {
    let Some(krate) = krate else {
        return kept;
    };
    let line = format!("use {krate}::{REEXPORTS}::*;\n");
    if kept.contains(&line) {
        return kept;
    }
    let at = after_inner_docs(&kept);
    let mut out = kept;
    out.insert_str(at, &line);
    out
}

/// Appends the re-export of `module`'s method trait `name` to `file`, the
/// crate's `__load.rs`, made when missing: public for `LoadMethods`, which
/// core imports, and for the crate alone for `LoadHelpers`.
fn reexport(file: &Path, module: &str, name: &str) -> Result<(), String> {
    let mut out = match std::fs::read_to_string(file) {
        Ok(kept) => kept,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::from(
            "//! The traits that hold `Load`'s methods of each module on the base\n//! handle; core's forwarders import them all.\n\n",
        ),
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let vis = if name == HELPERS { "pub(crate)" } else { "pub" };
    let path = if module.is_empty() {
        format!("crate::{name}")
    } else {
        format!("crate::{module}::{name}")
    };
    let line = format!("{vis} use {path} as _;\n");
    if !out.contains(&line) {
        out.push_str(&line);
    }
    std::fs::write(file, out).map_err(|e| format!("{}: {e}", file.display()))
}

/// The opening of a new forwarder file: its doc line, `uses`, the glob of
/// `krate`'s method traits when the module is in a domain crate, and `Load`
/// and `Off`.
fn header(uses: &[String], krate: Option<&str>) -> String {
    let mut out = String::from(
        "//! `Load`'s methods of a module below core, forwarded to the base handle.\n\n",
    );
    for line in uses {
        out.push_str(line);
        out.push('\n');
    }
    if let Some(krate) = krate {
        let _written = writeln!(out, "use {krate}::{REEXPORTS}::*;");
    }
    // `Load` and `Off`, each unless a copied import names it already.
    let named = |name: &str| {
        uses.iter().any(|line| {
            line.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == name)
        })
    };
    for name in ["Load", "Off"].into_iter().filter(|name| !named(name)) {
        let _written = writeln!(out, "use crate::{name};");
    }
    out
}

/// A module file after its peel.
#[derive(Debug, PartialEq, Eq)]
struct Peeled {
    /// The file with each `impl Load` block on the base handle.
    base: String,
    /// One `Load` block of forwarders per block with a public method.
    forwarders: Vec<String>,
    /// The file's `use` items, but those of `super`, for a new forwarder file.
    uses: Vec<String>,
    /// The method traits the file now holds, to re-export.
    traits: Vec<&'static str>,
    /// The domain crate's Rust name in the trait form, whose method traits
    /// the forwarder file imports.
    krate: Option<String>,
}

/// The trait form of a module's `impl Load` blocks: the public and
/// scripted methods on `LoadMethods`, which core's forwarders call, and the
/// rest on the crate's own `LoadHelpers`, whose signatures may name the
/// crate's private types.
#[derive(Debug, Default)]
struct Methods {
    public: Part,
    helpers: Part,
}

/// One trait's signatures and the implementing methods, in order.
#[derive(Debug, Default)]
struct Part {
    signatures: Vec<String>,
    bodies: Vec<String>,
}

impl Part {
    /// The trait `name`, declared with `head`, and its implementation for
    /// the base handle; nothing when it holds no method.
    fn render(&self, head: &str, name: &str) -> Option<String> {
        (!self.bodies.is_empty()).then(|| {
            format!(
                "{head}{name} {{\n{}\n}}\n\nimpl {name} for crate::{BASE} {{\n{}\n}}",
                self.signatures.join("\n\n"),
                self.bodies.join("\n\n")
            )
        })
    }
}

impl Methods {
    /// Adds the methods of `item` and returns its forwarder block, if it has
    /// a public method.
    fn add(
        &mut self,
        item: &ItemImpl,
        text: &str,
        at: &Offsets<'_>,
    ) -> Result<Option<String>, String> {
        // A `cfg` of the block holds for each of its methods, on both sides.
        let block_cfgs: Vec<&str> = item
            .attrs
            .iter()
            .filter(|a| is_cfg(a))
            .map(|a| attr_text(a, text, at))
            .collect();
        let mut bodiless = Vec::new();
        for entry in &item.items {
            let ImplItem::Fn(method) = entry else {
                return Err(
                    "a constant or type in an `impl Load` block above enshwick-base; move it by hand"
                        .to_owned(),
                );
            };
            let sig_end = at.byte(method.block.brace_token.span.open().start());
            let end = at.byte(method.block.brace_token.span.close().end());
            let after_vis = after_visibility(method, at);
            // A script adapter names the scripts' function on `Load`,
            // whatever its visibility.
            let forwarded = matches!(method.vis, Visibility::Public(_))
                || method.attrs.iter().any(|a| a.path().is_ident("script"));
            let mut signature = String::new();
            if forwarded {
                let _written = writeln!(signature, "    /// As `Load::{}`.", method.sig.ident);
                if matches!(method.sig.safety, syn::Safety::Unsafe(_)) {
                    for line in safety_section(&method.attrs, text, at) {
                        let _written = writeln!(signature, "    {line}");
                    }
                }
                signature.push_str("    #[doc(hidden)]\n");
            } else {
                // A forwarded method's docs are `Load`'s; a helper keeps its
                // own, often the invariant it holds.
                for attr in method.attrs.iter().filter(|a| a.path().is_ident("doc")) {
                    let _written = writeln!(signature, "    {}", attr_text(attr, text, at));
                }
            }
            let mut body = String::new();
            for cfg in &block_cfgs {
                let _written = writeln!(signature, "    {cfg}");
                let _written = writeln!(body, "    {cfg}");
            }
            for attr in method.attrs.iter().filter(|a| !a.path().is_ident("doc")) {
                let written = attr_text(attr, text, at);
                if is_cfg(attr) {
                    let _written = writeln!(signature, "    {written}");
                    let _written = writeln!(body, "    {written}");
                } else if on_signature(attr) {
                    let _written = writeln!(signature, "    {written}");
                } else if !attr.path().is_ident("script") {
                    let _written = writeln!(body, "    {written}");
                }
            }
            let sig = unbound(method, text, at, after_vis..sig_end);
            let sig = sig.trim_end();
            let _written = write!(signature, "    {};", sig.strip_suffix(',').unwrap_or(sig));
            body.push_str("    ");
            body.push_str(&text[after_vis..end]);
            let part = if forwarded {
                &mut self.public
            } else {
                &mut self.helpers
            };
            part.signatures.push(signature);
            part.bodies.push(body);
            if forwarded {
                bodiless.push(bodiless_forwarder(method, text, at));
            }
        }
        Ok(forwarder(item, &bodiless, text, at))
    }

    /// The traits and their implementations for the base handle.
    fn render(&self) -> String {
        let public = self.public.render(
            "/// `Load`'s methods of this module on the base handle, which core's\n/// forwarders call.\n#[doc(hidden)]\npub trait ",
            TRAIT,
        );
        let helpers = self.helpers.render(
            "/// The base handle's helpers of this module for the crate's own code.\npub(crate) trait ",
            HELPERS,
        );
        public
            .into_iter()
            .chain(helpers)
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Whether the module holds no moved method.
    fn is_empty(&self) -> bool {
        self.public.bodies.is_empty() && self.helpers.bodies.is_empty()
    }
}

/// Whether `attr` is a `cfg` or `cfg_attr`, which holds for a method's
/// signature and its body alike.
fn is_cfg(attr: &Attribute) -> bool {
    attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
}

/// Whether `attr` belongs on a trait method's signature rather than its
/// implementation, where it would have no effect.
fn on_signature(attr: &Attribute) -> bool {
    attr.path().is_ident("must_use") || attr.path().is_ident("deprecated")
}

/// Where a method's text after its visibility starts.
fn after_visibility(method: &ImplItemFn, at: &Offsets<'_>) -> usize {
    let start = match &method.vis {
        Visibility::Public(token) => at.byte(token.span.end()),
        Visibility::Restricted(r) => at.byte(r.paren_token.span.close().end()),
        Visibility::Inherited => return sig_start(method, at),
    };
    let rest = &at.text[start..];
    start + (rest.len() - rest.trim_start().len())
}

/// The text of `method`'s signature in `range` without the `mut` of its
/// arguments, which a trait's bodiless signature may not bind.
fn unbound(
    method: &ImplItemFn,
    text: &str,
    at: &Offsets<'_>,
    range: std::ops::Range<usize>,
) -> String {
    let mut cuts: Vec<usize> = method
        .sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            syn::FnArg::Typed(arg) => match &*arg.pat {
                syn::Pat::Ident(ident) => {
                    ident.mutability.as_ref().map(|m| at.byte(m.span.start()))
                }
                _ => None,
            },
            syn::FnArg::Receiver(_) => None,
        })
        .collect();
    cuts.sort_unstable();
    let mut out = String::new();
    let mut from = range.start;
    for cut in cuts {
        out.push_str(&text[from..cut]);
        from = cut + "mut".len();
        from += text[from..].len() - text[from..].trim_start().len();
    }
    out.push_str(&text[from..range.end]);
    out
}

/// Peels `text`, a module file, into `form`.
fn peel(text: &str, form: &Form) -> Result<Peeled, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let at = Offsets::new(text);
    let mut base = text.to_owned();
    let mut forwarders = Vec::new();
    let mut uses = Vec::new();
    let mut cuts = Vec::new();
    let mut imports = Vec::new();
    let mut methods = Methods::default();
    let children: Vec<String> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Mod(module) => Some(module.ident.to_string()),
            _ => None,
        })
        .collect();
    for item in &file.items {
        match item {
            Item::Use(item) => {
                let start = at.byte(item.use_token.span.start());
                let end = at.byte(item.semi_token.span.end());
                let start = attrs_start(&item.attrs, &at).unwrap_or(start);
                if imports_from_above(item, &children) {
                    uses.push(text[start..end].to_owned());
                }
                // `Load` is core's: it names nothing in a crate below core.
                if matches!(form, Form::Trait { .. }) && names_load(&item.tree) {
                    let kept = without_load(&text[start..end]);
                    let end = if kept.is_empty() && text[end..].starts_with('\n') {
                        end + 1
                    } else {
                        end
                    };
                    imports.push((start, end, kept));
                }
            }
            Item::Impl(item) if is_load(item) => {
                let line = item.impl_token.span.start().line;
                if item.items.iter().any(|entry| {
                    !matches!(
                        entry,
                        ImplItem::Fn(_) | ImplItem::Const(_) | ImplItem::Type(_)
                    )
                }) {
                    return Err(format!(
                        "line {line}: an `impl Load` block with an item other than a method, constant or type; move it by hand"
                    ));
                }
                let start =
                    attrs_start(&item.attrs, &at).unwrap_or(at.byte(item.impl_token.span.start()));
                let end = at.byte(item.brace_token.span.close().end());
                let forwarder = match form {
                    Form::Base => {
                        let (moved, forwarder) = block(item, text, &at);
                        cuts.push((start, end, moved));
                        forwarder
                    }
                    Form::Trait { .. } => {
                        cuts.push((start, end, String::new()));
                        methods
                            .add(item, text, &at)
                            .map_err(|e| format!("line {line}: {e}"))?
                    }
                };
                forwarders.extend(forwarder);
            }
            _ => {}
        }
    }
    // In the trait form the first block's place takes the trait and its
    // impl, and the others are cut.
    if let Some(first) = cuts.first_mut()
        && !methods.is_empty()
    {
        first.2 = methods.render();
    }
    cuts.extend(imports);
    cuts.sort_by_key(|cut| cut.0);
    for (start, end, moved) in cuts.into_iter().rev() {
        base.replace_range(start..end, &moved);
    }
    let krate = match form {
        Form::Trait { krate, .. } => Some(krate.clone()),
        Form::Base => None,
    };
    Ok(Peeled {
        base,
        forwarders,
        uses,
        traits: [
            (TRAIT, !methods.public.bodies.is_empty()),
            (HELPERS, !methods.helpers.bodies.is_empty()),
        ]
        .into_iter()
        .filter_map(|(name, held)| held.then_some(name))
        .collect(),
        krate,
    })
}

/// Whether `item` is an inherent `impl Load` block.
fn is_load(item: &ItemImpl) -> bool {
    item.trait_.is_none()
        && matches!(&*item.self_ty, Type::Path(path)
            if path.path.segments.last().is_some_and(|s| s.ident == "Load"))
}

/// Whether `tree` imports `Load` by that name.
fn names_load(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Path(path) => names_load(&path.tree),
        syn::UseTree::Name(name) => name.ident == "Load",
        syn::UseTree::Group(group) => group.items.iter().any(names_load),
        syn::UseTree::Rename(_) | syn::UseTree::Glob(_) => false,
    }
}

/// `item`, a `use` item's text, without its `Load`: empty when it imports
/// nothing else.
fn without_load(item: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = item.to_owned();
    let found = out.match_indices("Load").map(|(at, _)| at).find(|&at| {
        !out[..at].ends_with(word) && !out[at + 4..].starts_with(|c: char| word(c) || c == ':')
    });
    let Some(at) = found else {
        return out;
    };
    let after = &out[at + 4..];
    let trailing = after.len() - after.trim_start_matches([',', ' ', '\n']).len();
    if after.starts_with(',') {
        out.replace_range(at..at + 4 + trailing, "");
        return out;
    }
    let before = out[..at].trim_end_matches([' ', '\n']);
    if let Some(comma) = before.strip_suffix(',') {
        let cut = comma.len();
        out.replace_range(cut..at + 4, "");
        return out;
    }
    // `Load` alone, in braces or not.
    String::new()
}

/// Whether `item` is a private import that a forwarder file in core can
/// copy: of the crate or another crate, not of the module's own `super`,
/// `self` or `children`; a re-export stays with its module.
fn imports_from_above(item: &syn::ItemUse, children: &[String]) -> bool {
    let first = match &item.tree {
        syn::UseTree::Path(path) => path.ident.to_string(),
        syn::UseTree::Name(name) => name.ident.to_string(),
        syn::UseTree::Rename(rename) => rename.ident.to_string(),
        _ => return false,
    };
    matches!(item.vis, Visibility::Inherited)
        && first != "super"
        && first != "self"
        && !children.contains(&first)
}

/// The base block that replaces `item` and its forwarder block, if it has a
/// public method.
fn block(item: &ItemImpl, text: &str, at: &Offsets<'_>) -> (String, Option<String>) {
    let mut moved = Vec::new();
    let mut bodiless = Vec::new();
    for entry in &item.items {
        let ImplItem::Fn(method) = entry else {
            moved.push(format!("    {}", slice_of(entry, text, at)));
            continue;
        };
        let start = item_start(method, at);
        let end = at.byte(method.block.brace_token.span.close().end());
        let public = matches!(method.vis, Visibility::Public(_));
        // A script adapter names the scripts' function on `Load`, whatever
        // its visibility.
        let scripted = method.attrs.iter().any(|a| a.path().is_ident("script"));
        if !public && !scripted {
            moved.push(format!("    {}", &text[start..end]));
            continue;
        }
        // Hidden: its docs and `# Errors` are `Load`'s; an `unsafe fn` keeps
        // its `# Safety`, the contract of any caller through the base.
        let mut lines = format!("    /// As `Load::{}`.\n", method.sig.ident);
        if matches!(method.sig.safety, syn::Safety::Unsafe(_)) {
            for line in safety_section(&method.attrs, text, at) {
                let _written = writeln!(lines, "    {line}");
            }
        }
        lines.push_str("    #[doc(hidden)]\n");
        for attr in method
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("doc") && !a.path().is_ident("script"))
        {
            let _written = writeln!(lines, "    {}", attr_text(attr, text, at));
        }
        lines.push_str("    ");
        let signature_start = sig_start(method, at);
        match &method.vis {
            Visibility::Public(_) => lines.push_str(&text[signature_start..end]),
            Visibility::Restricted(r) => {
                lines.push_str("pub");
                lines.push_str(&text[at.byte(r.paren_token.span.close().end())..end]);
            }
            Visibility::Inherited => {
                lines.push_str("pub ");
                lines.push_str(&text[signature_start..end]);
            }
        }
        moved.push(lines);
        bodiless.push(bodiless_forwarder(method, text, at));
    }
    let moved = format!(
        "{}impl {BASE} {{\n{}\n}}",
        block_attrs(item, text, at),
        moved.join("\n\n")
    );
    (moved, forwarder(item, &bodiless, text, at))
}

/// Whether `attr` is the block's `#[script]`, which names `Load`'s
/// functions and goes with the forwarders.
fn is_script(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "script")
}

/// The block's own attributes but its `#[script]`, such as a `cfg`, which
/// hold for the moved block and the forwarders alike.
fn block_attrs(item: &ItemImpl, text: &str, at: &Offsets<'_>) -> String {
    let mut kept = String::new();
    for attr in item.attrs.iter().filter(|a| !is_script(a)) {
        kept.push_str(attr_text(attr, text, at));
        kept.push('\n');
    }
    kept
}

/// The forwarder block of `item` on `Load`, from its `bodiless` public
/// methods, if it has any.
fn forwarder(item: &ItemImpl, bodiless: &[String], text: &str, at: &Offsets<'_>) -> Option<String> {
    if bodiless.is_empty() {
        return None;
    }
    let mut head = format!(
        "{}#[enshwick_macros::forward({FIELD})]\n",
        block_attrs(item, text, at)
    );
    for attr in item.attrs.iter().filter(|a| is_script(a)) {
        head.push_str(attr_text(attr, text, at));
        head.push('\n');
    }
    Some(format!(
        "{head}impl Load<'_> {{\n{}\n}}",
        bodiless.join("\n\n")
    ))
}

/// A forwarded method's bodiless signature on `Load`, with its attributes
/// but those that set a lint level: the lint they meet is in the body,
/// which moved, and on the forwarder an `#[expect]` would go unmet.
// shortcut: a lint on the signature alone, such as `too_many_arguments`,
// fires on the forwarder too; it takes its `#[expect]` by hand.
fn bodiless_forwarder(method: &ImplItemFn, text: &str, at: &Offsets<'_>) -> String {
    let mut out = String::from("    ");
    for attr in method.attrs.iter().filter(|a| !is_lint_level(a)) {
        out.push_str(attr_text(attr, text, at));
        out.push_str("\n    ");
    }
    let sig_end = at.byte(method.block.brace_token.span.open().start());
    let sig = text[sig_start(method, at)..sig_end].trim_end();
    let _written = write!(out, "{};", sig.strip_suffix(',').unwrap_or(sig));
    out
}

/// Whether `attr` sets a lint level.
fn is_lint_level(attr: &Attribute) -> bool {
    ["expect", "allow", "warn", "deny", "forbid"]
        .iter()
        .any(|level| attr.path().is_ident(level))
}

/// The `# Safety` section of a method's doc comment, a blank doc line
/// first, up to the next section or the end of its docs.
fn safety_section<'t>(attrs: &[Attribute], text: &'t str, at: &Offsets<'_>) -> Vec<&'t str> {
    let docs: Vec<&str> = attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .map(|a| attr_text(a, text, at).trim())
        .collect();
    let Some(start) = docs.iter().position(|line| *line == "/// # Safety") else {
        return Vec::new();
    };
    let mut section: Vec<&str> = std::iter::once("///")
        .chain(
            docs[start..]
                .iter()
                .copied()
                .enumerate()
                .take_while(|(n, line)| *n == 0 || !line.starts_with("/// # "))
                .map(|(_, line)| line),
        )
        .collect();
    while section.last() == Some(&"///") {
        section.pop();
    }
    section
}

/// The text of an impl item other than a method, from its first attribute.
fn slice_of<'t>(entry: &ImplItem, text: &'t str, at: &Offsets<'_>) -> &'t str {
    let (attrs, first, last): (&[Attribute], Span, Span) = match entry {
        ImplItem::Const(c) => (&c.attrs, c.const_token.span, c.semi_token.span),
        ImplItem::Type(t) => (&t.attrs, t.type_token.span, t.semi_token.span),
        _ => return "",
    };
    let start = attrs_start(attrs, at).unwrap_or(at.byte(first.start()));
    &text[start..at.byte(last.end())]
}

/// Where a method starts: its first attribute, else its signature.
fn item_start(method: &ImplItemFn, at: &Offsets<'_>) -> usize {
    attrs_start(&method.attrs, at).unwrap_or_else(|| sig_start(method, at))
}

/// Where a method's visibility or signature starts.
fn sig_start(method: &ImplItemFn, at: &Offsets<'_>) -> usize {
    let sig = &method.sig;
    let spans = [
        match &method.vis {
            Visibility::Public(token) => Some(token.span),
            Visibility::Restricted(r) => Some(r.pub_token.span),
            Visibility::Inherited => None,
        },
        sig.constness.as_ref().map(|t| t.span),
        sig.asyncness.as_ref().map(|t| t.span),
        match &sig.safety {
            syn::Safety::Unsafe(token) => Some(token.span),
            syn::Safety::Safe(token) => Some(token.span),
            syn::Safety::Default => None,
        },
        sig.abi.as_ref().map(|a| a.extern_token.span),
        Some(sig.fn_token.span),
    ];
    spans
        .into_iter()
        .flatten()
        .map(|span| at.byte(span.start()))
        .min()
        .unwrap_or_default()
}

/// Where the first of `attrs` starts.
fn attrs_start(attrs: &[Attribute], at: &Offsets<'_>) -> Option<usize> {
    attrs
        .first()
        .map(|attr| at.byte(attr.pound_token.span.start()))
}

/// An attribute as written, a doc comment included.
fn attr_text<'t>(attr: &Attribute, text: &'t str, at: &Offsets<'_>) -> &'t str {
    let start = at.byte(attr.pound_token.span.start());
    let end = at
        .byte(attr.bracket_token.span.close().end())
        .max(at.byte(attr.pound_token.span.end()));
    &text[start..end]
}

/// Byte offsets of a text's lines, to place spans, whose columns count
/// characters.
struct Offsets<'t> {
    text: &'t str,
    lines: Vec<usize>,
}

impl<'t> Offsets<'t> {
    fn new(text: &'t str) -> Self {
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, lines }
    }

    /// The byte offset of `place`, its line counted from 1.
    fn byte(&self, place: LineColumn) -> usize {
        let line = self
            .lines
            .get(place.line.saturating_sub(1))
            .copied()
            .unwrap_or(self.text.len());
        let rest = &self.text[line..];
        line + rest
            .char_indices()
            .nth(place.column)
            .map_or(rest.len(), |(at, _)| at)
    }
}

#[cfg(test)]
mod tests;
