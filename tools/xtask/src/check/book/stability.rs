//! The rule that the book's stability chapter names every public item the
//! API compiles only with its `unstable` or `typed` feature. The developer
//! site reads stability from that chapter alone, so an item missing there
//! shows as stable while a mod built without the feature fails to compile.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use syn::{Attribute, ImplItem, Item, Visibility};

use super::SOURCE;
use crate::FACADE;
use crate::affected::Package;
use crate::check::domains::use_leaves;
use crate::check::domains::walk::{Cfg, cfgs, item_attrs, test_attrs};

/// The chapter, under the book's source.
const CHAPTER: &str = "stability.md";
/// The facade's features whose items carry no stability promise.
const GATES: [&str; 2] = ["unstable", "typed"];
/// The facade's feature only a mod's tests turn on, so what it alone turns
/// on, and `cfg(test)`, is off in a mod's build.
const TEST_FEATURE: &str = "test";

/// A public item that exists only with a gated feature.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Gated {
    /// Where it is defined, relative to the repository root, and its line.
    place: String,
    /// Its name as the chapter writes it: `map`, `Load::add_sound`.
    name: String,
    /// The public modules it sits in, outermost first.
    modules: Vec<String>,
}

/// The gated public items of the API crates under `root` that the chapter
/// does not name, one line each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let chapter_path = format!("{SOURCE}/{CHAPTER}");
    let chapter = match std::fs::read_to_string(root.join(&chapter_path)) {
        Ok(text) => text,
        Err(error) => return vec![format!("{chapter_path}: {error}")],
    };
    let named = chapter_names(&chapter);
    let (gates, tests) = match (
        reached_features(root, packages, &GATES),
        reached_features(root, packages, &[TEST_FEATURE]),
    ) {
        (Ok(gates), Ok(tests)) => (gates, tests),
        (Err(error), _) | (_, Err(error)) => return vec![error],
    };
    let mut problems = Vec::new();
    for (package, features) in &gates {
        let Some(dir) = packages.iter().find(|p| &p.name == package).map(|p| &p.dir) else {
            continue;
        };
        let lib = Path::new(dir).join("src").join("lib.rs");
        let test_only: BTreeSet<String> = tests
            .get(package)
            .map(|reached| reached.difference(features).cloned().collect())
            .unwrap_or_default();
        let mut found = Vec::new();
        if let Err(error) = gated_items(root, &lib, features, &test_only, &mut found) {
            problems.push(error);
            continue;
        }
        problems.extend(
            found
                .into_iter()
                .filter(|item| !is_named(item, &named))
                .map(|item| {
                    format!(
                        "{}: `{}` needs the `unstable` or `typed` feature, but {chapter_path} names neither it, its type nor its module; add it to the table of what `unstable` holds",
                        item.place, item.name
                    )
                }),
        );
    }
    problems.sort();
    problems.dedup();
    problems
}

/// Whether the developer site marks `item` unstable from the chapter: the
/// chapter names the item itself, bare or under the inner part of its
/// module path its re-exports give it (`interactions::Targets`), or its
/// outermost module. The site matches nothing else, so neither its type nor
/// an inner module counts.
fn is_named(item: &Gated, named: &BTreeSet<String>) -> bool {
    (0..=item.modules.len()).any(|from| {
        let mut path = item.modules[from..].to_vec();
        path.push(item.name.clone());
        named.contains(&path.join("::"))
    }) || item
        .modules
        .first()
        .is_some_and(|module| named.contains(module))
}

/// The names in backticks in the first column of the chapter's tables, as
/// the developer site reads them: a name the row gives after "of" only for
/// context, once it has named something, does not count.
fn chapter_names(chapter: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in chapter.lines() {
        let Some(part) = line
            .strip_prefix('|')
            .and_then(|rest| rest.split('|').next())
        else {
            continue;
        };
        let mut after_first = false;
        let mut rest = part;
        while let Some(open) = rest.find('`') {
            let before = &rest[..open];
            let after = &rest[open + 1..];
            let Some(close) = after.find('`') else { break };
            let span = &after[..close];
            let context = before
                .strip_suffix("of ")
                .is_some_and(|start| start.chars().last().is_none_or(|c| !c.is_alphanumeric()));
            if is_path(span) {
                if !context || !after_first {
                    names.insert(span.to_owned());
                }
                after_first = true;
            }
            rest = &after[close + 1..];
        }
    }
    names
}

/// Whether `text` is a Rust path such as `world` or `Knowledge::revoke`.
fn is_path(text: &str) -> bool {
    !text.is_empty()
        && text.split("::").all(|segment| {
            segment
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

/// Each workspace package that the facade's features `starts` reach, with
/// the features of its own they turn on: what its code puts behind them is
/// behind `starts` in the facade too. An optional dependency's feature
/// (`dep?/f`) counts, since it is on whenever that dependency is.
fn reached_features(
    root: &Path,
    packages: &[Package],
    starts: &[&str],
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let mut reached: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut queue: Vec<(String, String)> = starts
        .iter()
        .map(|start| (FACADE.to_owned(), (*start).to_owned()))
        .collect();
    while let Some((package, feature)) = queue.pop() {
        if !reached
            .entry(package.clone())
            .or_default()
            .insert(feature.clone())
        {
            continue;
        }
        let Some(dir) = packages.iter().find(|p| p.name == package).map(|p| &p.dir) else {
            continue;
        };
        let manifest_path = Path::new(dir).join("Cargo.toml");
        let text = std::fs::read_to_string(root.join(&manifest_path))
            .map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let manifest: toml::Table =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        let turned_on = manifest
            .get("features")
            .and_then(|features| features.get(&feature))
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str);
        for entry in turned_on {
            match entry.split_once('/') {
                Some((dependency, sub)) => {
                    let key = dependency.trim_end_matches('?');
                    // shortcut: a rename in [workspace.dependencies] is not
                    // read; read the root manifest once one appears there.
                    let package = manifest
                        .get("dependencies")
                        .and_then(|deps| deps.get(key))
                        .and_then(|dep| dep.get("package"))
                        .and_then(toml::Value::as_str)
                        .unwrap_or(key);
                    queue.push((package.to_owned(), sub.to_owned()));
                }
                None if !entry.starts_with("dep:") => {
                    queue.push((package.clone(), entry.to_owned()));
                }
                None => {}
            }
        }
    }
    reached.retain(|name, _| packages.iter().any(|p| &p.name == name));
    Ok(reached)
}

/// Where a module's own files and its child modules' files lie.
struct ModuleFile {
    /// The file, relative to the repository root.
    path: PathBuf,
    /// The directory its `mod x;` items find `x.rs` or `x/mod.rs` in.
    children: PathBuf,
}

impl ModuleFile {
    /// The module file at `path`: a crate root or `mod.rs` holds its
    /// children beside it, any other file in the directory of its name.
    fn new(path: PathBuf) -> Self {
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let children = if matches!(stem, "lib" | "main" | "mod") {
            parent
        } else {
            parent.join(stem)
        };
        Self { path, children }
    }
}

/// The walk over one crate's module tree.
struct Walk<'a> {
    root: &'a Path,
    /// The crate's gated features.
    features: &'a BTreeSet<String>,
    /// The crate's features only the facade's tests turn on.
    test_only: &'a BTreeSet<String>,
    found: &'a mut Vec<Gated>,
}

/// Collects into `found` every public item of the crate rooted at `lib`
/// that a mod's build has only with one of `features`, read from its
/// module tree; `test_only` and `cfg(test)` are off in that build.
fn gated_items(
    root: &Path,
    lib: &Path,
    features: &BTreeSet<String>,
    test_only: &BTreeSet<String>,
    found: &mut Vec<Gated>,
) -> Result<(), String> {
    let mut walk = Walk {
        root,
        features,
        test_only,
        found,
    };
    let context = Context {
        public: true,
        ..Context::default()
    };
    walk.file(&ModuleFile::new(lib.to_path_buf()), &context)
}

/// Where the walk stands: the public modules on the way, whether what it
/// reads is already gated, and whether a gated item above names it all.
#[derive(Clone, Default)]
struct Context {
    modules: Vec<String>,
    /// Every module on the way is public, so a module here can be named.
    public: bool,
    gated: bool,
    /// A gated public module above, which the chapter names in its place.
    covered: bool,
}

impl Walk<'_> {
    /// Whether `attrs` hold a `cfg` a mod's build meets only with one of
    /// the gated features.
    fn gates(&self, attrs: &[Attribute]) -> bool {
        let all: Vec<Cfg> = cfgs(attrs).collect();
        if all.is_empty() {
            return false;
        }
        let built = Cfg::All(all).with_off("test");
        let in_build = |feature: &str| !self.test_only.contains(feature);
        built.can_hold(&in_build)
            && !built.can_hold(&|feature| in_build(feature) && !self.features.contains(feature))
    }

    fn file(&mut self, module: &ModuleFile, context: &Context) -> Result<(), String> {
        let shown = module.path.display().to_string().replace('\\', "/");
        let text = std::fs::read_to_string(self.root.join(&module.path))
            .map_err(|e| format!("{shown}: {e}"))?;
        let file = syn::parse_file(&text).map_err(|e| format!("{shown}: {e}"))?;
        self.items(&file.items, &shown, &module.children, context)
    }

    fn items(
        &mut self,
        items: &[Item],
        file: &str,
        children: &Path,
        context: &Context,
    ) -> Result<(), String> {
        // shortcut: a hidden type is known only to impls in its own module;
        // collect hidden types per crate if an impl ever sits elsewhere.
        let hidden_types: BTreeSet<String> = items
            .iter()
            .filter(|item| hidden(item_attrs(item)))
            .filter_map(type_ident)
            .collect();
        let globbed = glob_modules(items);
        for item in items {
            let attrs = item_attrs(item);
            if test_attrs(attrs) || hidden(attrs) {
                continue;
            }
            let gated = context.gated || self.gates(attrs);
            match item {
                Item::Mod(declared) => {
                    let through_glob = globbed.contains(&declared.ident.to_string());
                    self.module(declared, gated, through_glob, file, children, context)?;
                }
                Item::Impl(block) if block.trait_.is_none() && !context.covered => {
                    if let Some(owner) = type_name(&block.self_ty)
                        && !hidden_types.contains(&owner)
                    {
                        self.members(block, &owner, gated, file, context);
                    }
                }
                Item::Enum(_) | Item::Struct(_) | Item::Trait(_)
                    if !gated && !context.covered && public_ident(item).is_some() =>
                {
                    self.parts(item, file, context);
                }
                // Below a private module only a type's members reach mods: a
                // free item there is public only through a re-export, read
                // where it is.
                _ if !context.public || !gated || context.covered => {}
                Item::Use(used) if is_pub(&used.vis) => {
                    for name in use_leaves(&used.tree)
                        .into_iter()
                        .filter_map(|(name, _)| name)
                    {
                        if !name.starts_with("__") {
                            let ident = proc_macro2::Ident::new(&name, used.use_token.span);
                            self.report(file, &ident, name, context);
                        }
                    }
                }
                _ => {
                    if let Some(ident) = public_ident(item) {
                        let name = ident.to_string();
                        self.report(file, &ident, name, context);
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads the module `declared` declares, inline or from its file, and
    /// reports it when it is public and gated here first; a private module
    /// its parent re-exports whole (`through_glob`) is read as the parent.
    fn module(
        &mut self,
        declared: &syn::ItemMod,
        gated: bool,
        through_glob: bool,
        file: &str,
        children: &Path,
        context: &Context,
    ) -> Result<(), String> {
        let name = declared.ident.to_string();
        if name.starts_with("__") {
            return Ok(());
        }
        let own = is_pub(&declared.vis);
        let public = context.public && (own || through_glob);
        let reported = gated && !context.gated && own;
        if reported && !context.covered {
            self.report(file, &declared.ident, name.clone(), context);
        }
        let mut inner = context.clone();
        inner.gated = gated;
        inner.covered = context.covered || (reported && public);
        inner.public = public;
        if public && own {
            inner.modules.push(name.clone());
        }
        let path = declared.attrs.iter().find_map(path_attr);
        if let Some((_, inline)) = &declared.content {
            let below = children.join(path.as_deref().unwrap_or(&name));
            return self.items(inline, file, &below, &inner);
        }
        let module = if let Some(path) = path {
            // Outside an inline module, `#[path]` starts at the declaring
            // file's own directory, not its children's.
            let base = Path::new(file).parent().unwrap_or(Path::new(""));
            ModuleFile {
                children: base.join(&path).with_extension(""),
                path: base.join(path),
            }
        } else {
            let flat = children.join(format!("{name}.rs"));
            if self.root.join(&flat).is_file() {
                ModuleFile::new(flat)
            } else {
                ModuleFile::new(children.join(&name).join("mod.rs"))
            }
        };
        self.file(&module, &inner)
    }

    /// Reports the public methods and constants of an inherent impl of
    /// `owner` that are gated, by the impl or by themselves.
    fn members(
        &mut self,
        block: &syn::ItemImpl,
        owner: &str,
        gated: bool,
        file: &str,
        context: &Context,
    ) {
        for member in &block.items {
            if let Some((ident, own)) = impl_member(member, |attrs| self.gates(attrs))
                && (gated || own)
            {
                let name = format!("{owner}::{ident}");
                self.report(file, &ident, name, context);
            }
        }
    }

    /// Reports the gated parts of an ungated public type: an enum's
    /// variants, a struct's public fields, a trait's methods and constants.
    fn parts(&mut self, item: &Item, file: &str, context: &Context) {
        let mut found = Vec::new();
        let (owner, parts): (_, Vec<(&proc_macro2::Ident, &[Attribute])>) = match item {
            Item::Enum(declared) => (
                &declared.ident,
                declared
                    .variants
                    .iter()
                    .map(|v| (&v.ident, v.attrs.as_slice()))
                    .collect(),
            ),
            Item::Struct(declared) => (
                &declared.ident,
                declared
                    .fields
                    .iter()
                    .filter(|f| is_pub(&f.vis))
                    .filter_map(|f| Some((f.ident.as_ref()?, f.attrs.as_slice())))
                    .collect(),
            ),
            Item::Trait(declared) => (
                &declared.ident,
                declared
                    .items
                    .iter()
                    .filter_map(|member| match member {
                        syn::TraitItem::Fn(m) => Some((&m.sig.ident, m.attrs.as_slice())),
                        syn::TraitItem::Const(c) => Some((&c.ident, c.attrs.as_slice())),
                        _ => None,
                    })
                    .collect(),
            ),
            _ => return,
        };
        for (ident, attrs) in parts {
            if self.gates(attrs) && !hidden(attrs) {
                found.push((ident.clone(), format!("{owner}::{ident}")));
            }
        }
        for (ident, name) in found {
            self.report(file, &ident, name, context);
        }
    }

    fn report(&mut self, file: &str, ident: &proc_macro2::Ident, name: String, context: &Context) {
        self.found.push(Gated {
            place: format!("{file}:{}", ident.span().start().line),
            name,
            modules: context.modules.clone(),
        });
    }
}

/// Whether `vis` is plain `pub`.
fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

/// Whether `attrs` hide the item from the docs.
fn hidden(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("doc")
            && attr
                .parse_args::<syn::Ident>()
                .is_ok_and(|arg| arg == "hidden")
    })
}

/// The file a `#[path = "..."]` attribute names.
fn path_attr(attr: &Attribute) -> Option<String> {
    let syn::Meta::NameValue(pair) = &attr.meta else {
        return None;
    };
    let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(path),
        ..
    }) = &pair.value
    else {
        return None;
    };
    pair.path.is_ident("path").then(|| path.value())
}

/// The last name of the type an inherent impl is for: `Load` for
/// `Load<'_>`.
fn type_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    path.path.segments.last().map(|s| s.ident.to_string())
}

/// The name of a public, documented impl member with whether `gates` finds
/// it gated; a method the impl declares without a body, as forwarding
/// macros take it, counts too.
fn impl_member(
    member: &ImplItem,
    gates: impl Fn(&[Attribute]) -> bool,
) -> Option<(proc_macro2::Ident, bool)> {
    let shown = |vis: &Visibility, ident: &proc_macro2::Ident, attrs: &[Attribute]| {
        (is_pub(vis) && !hidden(attrs) && !test_attrs(attrs)).then(|| (ident.clone(), gates(attrs)))
    };
    match member {
        ImplItem::Fn(method) => shown(&method.vis, &method.sig.ident, &method.attrs),
        ImplItem::Const(constant) => shown(&constant.vis, &constant.ident, &constant.attrs),
        ImplItem::Verbatim(tokens) => {
            let declared: syn::ForeignItemFn = syn::parse2(tokens.clone()).ok()?;
            shown(&declared.vis, &declared.sig.ident, &declared.attrs)
        }
        _ => None,
    }
}

/// The child modules `items` re-export whole: `pub use m::*` or
/// `pub use self::m::*`.
fn glob_modules(items: &[Item]) -> BTreeSet<String> {
    items
        .iter()
        .filter_map(|item| match item {
            Item::Use(used) if is_pub(&used.vis) => Some(&used.tree),
            _ => None,
        })
        .filter_map(|tree| {
            let syn::UseTree::Path(path) = tree else {
                return None;
            };
            let mut path = path;
            if path.ident == "self" {
                let syn::UseTree::Path(inner) = &*path.tree else {
                    return None;
                };
                path = inner;
            }
            matches!(*path.tree, syn::UseTree::Glob(_)).then(|| path.ident.to_string())
        })
        .collect()
}

/// The name of a struct or enum, whatever its visibility.
fn type_ident(item: &Item) -> Option<String> {
    match item {
        Item::Struct(item) => Some(item.ident.to_string()),
        Item::Enum(item) => Some(item.ident.to_string()),
        _ => None,
    }
}

/// The name of a public item a path can name, for the kinds not read
/// member by member.
fn public_ident(item: &Item) -> Option<proc_macro2::Ident> {
    let (vis, ident) = match item {
        Item::Const(item) => (&item.vis, &item.ident),
        Item::Enum(item) => (&item.vis, &item.ident),
        Item::Fn(item) => (&item.vis, &item.sig.ident),
        Item::Static(item) => (&item.vis, &item.ident),
        Item::Struct(item) => (&item.vis, &item.ident),
        Item::Trait(item) => (&item.vis, &item.ident),
        Item::Type(item) => (&item.vis, &item.ident),
        Item::Union(item) => (&item.vis, &item.ident),
        _ => return None,
    };
    (is_pub(vis) && !ident.to_string().starts_with("__")).then(|| ident.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "| Part | Why |\n| --- | --- |\n| `world`: `World`, its players | young |\n| `map` and `Load::fog_of_war`. Also the map markers of `ui`. | young |\n| `#[derive(Script)]` | binding |\n";

    #[test]
    fn the_chapter_names_each_path_in_its_first_column_but_one_given_after_of_for_context() {
        let names = chapter_names(TABLE);
        let expected: BTreeSet<String> = ["Load::fog_of_war", "World", "map", "world"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(names, expected);
    }

    fn walk(source: &str) -> Vec<String> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "xtask-stability-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join("src/inner")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), source).unwrap();
        std::fs::write(
            dir.join("src/inner.rs"),
            "impl Load<'_> { pub fn add_sound(&mut self); pub(crate) fn helper(&self) {} }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/errors.rs"),
            "pub enum WorldError { Gone, #[cfg(feature = \"unstable\")] NotAProp }\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("src/ids.rs"),
            "#[cfg(feature = \"unstable\")]\npub struct Free;\npub struct Event { pub a: u32, #[cfg(feature = \"unstable\")] pub killer: u32 }\npub trait Script { #[cfg(feature = \"unstable\")] fn tick(&self); }\n",
        )
        .unwrap();
        let features = BTreeSet::from(["unstable".to_owned()]);
        let test_only = BTreeSet::from(["fake".to_owned()]);
        let mut found = Vec::new();
        gated_items(
            &dir,
            Path::new("src/lib.rs"),
            &features,
            &test_only,
            &mut found,
        )
        .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        found.into_iter().map(|item| item.name).collect()
    }

    #[test]
    fn a_gated_private_module_gates_each_public_method_inside_it() {
        let found = walk("#[cfg(feature = \"unstable\")]\nmod inner;\n");
        assert_eq!(found, ["Load::add_sound"]);
    }

    #[test]
    fn a_gated_variant_of_an_enum_in_a_private_module_counts_by_its_type() {
        let found = walk("mod errors;\npub use errors::WorldError;\n");
        assert_eq!(found, ["WorldError::NotAProp"]);
    }

    #[test]
    fn a_gated_public_module_or_re_export_counts_once_by_its_own_name() {
        let found = walk(
            "#[cfg(feature = \"unstable\")]\npub mod inner;\n#[cfg(feature = \"unstable\")]\npub use other::{map, __hidden};\n",
        );
        assert_eq!(found, ["inner", "map"]);
    }

    #[test]
    fn an_item_another_feature_can_also_turn_on_the_docs_hide_or_a_private_module_holds_is_not_gated()
     {
        let found = walk(
            "#[cfg(any(feature = \"unstable\", feature = \"serde\"))]\npub fn both() {}\n#[cfg(feature = \"unstable\")]\n#[doc(hidden)]\npub fn hidden() {}\n#[cfg(not(feature = \"unstable\"))]\npub fn stable() {}\n#[cfg(feature = \"unstable\")]\npub(crate) mod private { pub struct Free; }\n#[cfg(feature = \"fake\")]\npub fn test_only() {}\n",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_item_only_tests_could_also_turn_on_is_gated_in_a_mod_build() {
        let found = walk(
            "#[cfg(any(feature = \"unstable\", test))]\npub fn with_test() {}\n#[cfg(any(feature = \"unstable\", feature = \"fake\"))]\npub fn with_fake() {}\n",
        );
        assert_eq!(found, ["with_test", "with_fake"]);
    }

    #[test]
    fn gated_fields_trait_methods_and_items_a_glob_re_exports_count() {
        let found = walk("mod ids;\npub use ids::*;\n");
        assert_eq!(found, ["Free", "Event::killer", "Script::tick"]);
    }

    #[test]
    fn a_glob_re_export_names_the_child_module_alone() {
        let parsed =
            syn::parse_file("pub use self::a::*;\npub use b::*;\npub use c::D;\nuse e::*;\n")
                .unwrap();
        assert_eq!(
            glob_modules(&parsed.items),
            BTreeSet::from(["a".to_owned(), "b".to_owned()])
        );
    }

    #[test]
    fn an_item_counts_as_named_by_itself_or_its_outermost_module_as_the_site_reads_it() {
        let named: BTreeSet<String> = ["world", "Damage", "interactions::Targets"]
            .into_iter()
            .map(String::from)
            .collect();
        let item = |name: &str, modules: &[&str]| Gated {
            place: String::new(),
            name: name.to_owned(),
            modules: modules.iter().map(|m| (*m).to_owned()).collect(),
        };
        assert!(is_named(&item("props", &["world"]), &named));
        assert!(is_named(&item("Damage", &[]), &named));
        assert!(is_named(&item("Targets", &["interactions"]), &named));
        assert!(!is_named(&item("Damage::kind", &[]), &named));
        assert!(!is_named(
            &item("Targets::target", &["interactions"]),
            &named
        ));
        assert!(!is_named(&item("Foo", &["content", "world"]), &named));
        assert!(!is_named(&item("Targets", &[]), &named));
        assert!(!is_named(&item("Load::add_sound", &["sound"]), &named));
    }
}
