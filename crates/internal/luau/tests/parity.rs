//! The Luau API mirrors the Rust API: every public function of the
//! `enshwick` crate has a Luau function of the same module and name, or a row
//! of the book's table of differences, `book/src/luau.md`, that says why not.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::script;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};
    use enshwick_luau::{Limits, Scripts};
    use syn::{Attribute, FnArg, GenericArgument, ImplItem, Item, Meta, PathArguments, ReturnType};

    /// The heading of the book's table of differences.
    const DIFFERENCES: &str = "## Where Luau differs from Rust";

    /// Modules another change replaces, whose Luau calls arrive with that
    /// change; the change that lands second takes its module off this list.
    const REWORKED: &[&str] = &[];

    /// Logs every name of the `enshwick` table, and of each table in it, as
    /// `api <name>` and `api <table>.<name>`, then the methods of the point
    /// `define_point` gives as `api OwnedPoint:<name>`.
    const LIST_API: &str = r"
        for name, value in enshwick do
            enshwick.log.info(`api {name}`)
            if type(value) == 'table' then
                for inner in value do
                    enshwick.log.info(`api {name}.{inner}`)
                end
            end
        end
        for name in enshwick.points.define_point('probe') do
            enshwick.log.info(`api OwnedPoint:{name}`)
        end
    ";

    /// A pack that declares the point `LIST_API` defines.
    const MANIFEST: &str = r#"
        id = "you.luau"
        version = "1.0.0"

        [[point]]
        name = "probe"
        mode = "observe"
        output = "u32"
    "#;

    static CASE: Mutex<Option<Scripts>> = Mutex::new(None);

    struct Luau;

    impl Mod for Luau {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let scripts = CASE.lock().unwrap().take().unwrap();
            let manifest = enshwick::packs::PackManifest::parse(MANIFEST).unwrap();
            let _runtime = enshwick_luau::start_pack(load, &manifest, scripts, Limits::default())?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Luau,
        r#"
        id = "you.luau"
        side = "both"
        "#
    );

    /// A public function of the Rust API.
    #[derive(Debug)]
    struct RustFn {
        /// Its top-level module; empty for the crate root, where the items
        /// of private modules are re-exported.
        module: String,
        /// The type of its `impl` block; empty for a free function.
        owner: String,
        name: String,
        /// The type it returns, out of any `Result` or `Option`.
        returns: Option<String>,
        /// Whether it takes anything besides its receiver.
        takes: bool,
        /// Whether it takes `self`.
        receiver: bool,
    }

    impl RustFn {
        /// Its path in the book's table, such as `weather::Weather::at`.
        fn path(&self) -> String {
            [&self.module, &self.owner, &self.name]
                .into_iter()
                .filter(|part| !part.is_empty())
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("::")
        }

        /// Its name in Luau: `enshwick.<module>.<name>`, or `enshwick.<name>`
        /// at the crate root.
        fn luau(&self) -> String {
            if self.module.is_empty() {
                self.name.clone()
            } else {
                format!("{}.{}", self.module, self.name)
            }
        }
    }

    /// The public functions of the API crate and the types that cross into
    /// a script as values.
    #[derive(Debug, Default)]
    struct Walk {
        /// The public top-level modules.
        public: BTreeSet<String>,
        /// What the crate root re-exports from its private modules.
        root: BTreeSet<String>,
        /// The public types of the public modules.
        types: BTreeSet<String>,
        functions: Vec<RustFn>,
        values: BTreeSet<String>,
    }

    /// Every name `tree` brings in, as `use a::{B, c}` brings `B` and `c`.
    fn used(tree: &syn::UseTree, names: &mut BTreeSet<String>) {
        match tree {
            syn::UseTree::Path(path) => used(&path.tree, names),
            syn::UseTree::Name(name) => {
                names.insert(name.ident.to_string());
            }
            syn::UseTree::Rename(rename) => {
                names.insert(rename.rename.to_string());
            }
            syn::UseTree::Group(group) => group.items.iter().for_each(|t| used(t, names)),
            syn::UseTree::Glob(_) => {}
        }
    }

    fn source() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../enshwick/src")
    }

    /// The source of the API crate and of the internal crates it re-exports,
    /// whose public modules carry the API's own paths.
    fn sources() -> [PathBuf; 3] {
        let internal = |name: &str| source().join("../../internal").join(name).join("src");
        [source(), internal("values"), internal("typed")]
    }

    fn parse(file: &Path) -> syn::File {
        let text =
            std::fs::read_to_string(file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        syn::parse_file(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
    }

    /// The tokens of `#[name(...)]`, without spaces.
    fn list(attr: &Attribute, name: &str) -> Option<String> {
        match &attr.meta {
            Meta::List(list) if list.path.is_ident(name) => {
                Some(list.tokens.to_string().replace(' ', ""))
            }
            _ => None,
        }
    }

    /// Whether `attrs` leave the item out of the public API: a test or a
    /// hidden item.
    fn left_out(attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            list(attr, "cfg").is_some_and(|cfg| cfg == "test" || cfg.starts_with("all(test,"))
                || list(attr, "doc").is_some_and(|doc| doc == "hidden")
        })
    }

    fn is_public(vis: &syn::Visibility) -> bool {
        matches!(vis, syn::Visibility::Public(_))
    }

    /// The last name of `ty`'s path, such as `Load` for `Load<'_>`.
    fn last_name(ty: &syn::Type) -> Option<String> {
        match ty {
            syn::Type::Path(path) => path.path.segments.last().map(|s| s.ident.to_string()),
            syn::Type::Reference(r) => last_name(&r.elem),
            _ => None,
        }
    }

    /// The type `output` names, out of any `Result` or `Option`.
    fn returned(output: &ReturnType) -> Option<String> {
        match output {
            ReturnType::Type(_, ty) => unwrapped(ty),
            ReturnType::Default => None,
        }
    }

    fn unwrapped(ty: &syn::Type) -> Option<String> {
        let syn::Type::Path(path) = ty else {
            return last_name(ty);
        };
        let last = path.path.segments.last()?;
        if last.ident != "Result" && last.ident != "Option" {
            return Some(last.ident.to_string());
        }
        match &last.arguments {
            PathArguments::AngleBracketed(args) => match args.args.first() {
                Some(GenericArgument::Type(inner)) => unwrapped(inner),
                _ => None,
            },
            _ => None,
        }
    }

    impl Walk {
        fn crate_api() -> Self {
            let roots = sources().map(|root| {
                let lib = parse(&root.join("lib.rs"));
                (root, lib)
            });
            let mut walk = Self::default();
            for item in roots.iter().flat_map(|(_, lib)| &lib.items) {
                match item {
                    Item::Mod(m) if is_public(&m.vis) && !left_out(&m.attrs) => {
                        walk.public.insert(m.ident.to_string());
                    }
                    Item::Use(u) if is_public(&u.vis) => used(&u.tree, &mut walk.root),
                    _ => {}
                }
            }
            for (root, lib) in &roots {
                walk.items(&lib.items, root, "");
            }
            // A private module's item is public only as the root re-exports it.
            let (root, types) = (&walk.root, &walk.types);
            walk.functions.retain(|f| {
                !f.module.is_empty()
                    || if f.owner.is_empty() {
                        root.contains(&f.name)
                    } else {
                        root.contains(&f.owner) || types.contains(&f.owner)
                    }
            });
            walk
        }

        /// The module the items of the top-level module `top` belong to.
        fn module(&self, top: &str) -> String {
            if self.public.contains(top) {
                top.to_owned()
            } else {
                String::new()
            }
        }

        /// Walks `items` of a module whose child modules sit in `dir`, inside
        /// the top-level module `top`, empty at the crate root.
        fn items(&mut self, items: &[Item], dir: &Path, top: &str) {
            for item in items {
                match item {
                    Item::Mod(m) if !left_out(&m.attrs) => {
                        let name = m.ident.to_string();
                        let top = if top.is_empty() {
                            name.clone()
                        } else {
                            top.to_owned()
                        };
                        let inner = dir.join(&name);
                        if let Some((_, items)) = &m.content {
                            self.items(items, &inner, &top);
                        } else {
                            let flat = dir.join(format!("{name}.rs"));
                            let file = if flat.exists() {
                                flat
                            } else {
                                inner.join("mod.rs")
                            };
                            self.items(&parse(&file).items, &inner, &top);
                        }
                    }
                    Item::Fn(f) if is_public(&f.vis) && !left_out(&f.attrs) => {
                        self.functions.push(RustFn {
                            module: self.module(top),
                            owner: String::new(),
                            name: f.sig.ident.to_string(),
                            returns: returned(&f.sig.output),
                            takes: !f.sig.inputs.is_empty(),
                            receiver: false,
                        });
                    }
                    Item::Impl(i) if !left_out(&i.attrs) => {
                        let Some(owner) = last_name(&i.self_ty) else {
                            continue;
                        };
                        if let Some((path, _)) = &i.trait_ {
                            let crosses = ["Typed", "IntoScript", "FromScript"];
                            if path
                                .segments
                                .last()
                                .is_some_and(|s| crosses.iter().any(|c| s.ident == c))
                            {
                                self.values.insert(owner);
                            }
                            continue;
                        }
                        for entry in &i.items {
                            if let ImplItem::Fn(f) = entry
                                && is_public(&f.vis)
                                && !left_out(&f.attrs)
                            {
                                let receiver =
                                    matches!(f.sig.inputs.first(), Some(FnArg::Receiver(_)));
                                self.functions.push(RustFn {
                                    module: self.module(top),
                                    owner: owner.clone(),
                                    name: f.sig.ident.to_string(),
                                    returns: returned(&f.sig.output),
                                    takes: f.sig.inputs.len() > usize::from(receiver),
                                    receiver,
                                });
                            }
                        }
                    }
                    Item::Struct(s) => self.declare(top, &s.vis, &s.ident, &s.attrs),
                    Item::Enum(e) => self.declare(top, &e.vis, &e.ident, &e.attrs),
                    _ => {}
                }
            }
        }
    }

    impl Walk {
        /// Notes the type `ident` of the top-level module `top`: public, and
        /// a value in a script when it derives `Script`.
        fn declare(
            &mut self,
            top: &str,
            vis: &syn::Visibility,
            ident: &syn::Ident,
            attrs: &[Attribute],
        ) {
            if is_public(vis) && self.public.contains(top) {
                self.types.insert(ident.to_string());
            }
            if derives_script(attrs) {
                self.values.insert(ident.to_string());
            }
        }
    }

    fn derives_script(attrs: &[Attribute]) -> bool {
        attrs
            .iter()
            .filter_map(|attr| list(attr, "derive"))
            .any(|derives| derives.split(',').any(|d| d.ends_with("Script")))
    }

    /// Every name the `enshwick` table of a running Luau mod holds at its
    /// top, and every function of the namespaces in it, whether or not the
    /// fake host has what a namespace needs to open.
    fn luau_api() -> BTreeSet<String> {
        *CASE.lock().unwrap() = Some(Scripts::new().with("main.luau", LIST_API));
        let loader = FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Luau)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let mut names: BTreeSet<String> = loader
            .logs()
            .iter()
            .filter_map(|(_, line)| {
                line.split_once("api ")
                    .map(|(_, name)| name.trim().to_owned())
            })
            .collect();
        let mut signatures = Vec::new();
        for namespace in script::NAMESPACES {
            let prefix = if namespace.name().is_empty() {
                String::new()
            } else {
                names.insert(namespace.name().to_owned());
                format!("{}.", namespace.name())
            };
            for function in namespace.functions() {
                names.insert(format!("{prefix}{}", function.name()));
                signatures.push(function.signature());
            }
            for function in namespace.init_functions() {
                names.insert(format!("{prefix}{}", function.name()));
                signatures.push(function.signature());
            }
            if let Some(function) = namespace.request_function() {
                names.insert(format!("{prefix}{}", function.name()));
                signatures.push(function.signature());
            }
        }
        let mut classes = Vec::new();
        for signature in signatures {
            classes_in_signature(signature, &mut classes);
        }
        for class in classes {
            for method in class.methods() {
                names.insert(format!("{}:{}", class.name(), method.name()));
            }
        }
        names
    }

    /// Every object class `signature` reaches, through records, lists,
    /// functions and other classes' methods.
    fn classes_in_signature(
        signature: &script::Signature,
        classes: &mut Vec<&'static script::Class>,
    ) {
        for param in signature.params() {
            classes_in(param.ty(), classes);
        }
        classes_in(signature.ret(), classes);
    }

    fn classes_in(ty: &'static script::Ty, classes: &mut Vec<&'static script::Class>) {
        match *ty {
            script::Ty::Optional(inner) | script::Ty::List(inner) => classes_in(inner, classes),
            script::Ty::Record(record) => {
                for field in record.fields() {
                    classes_in(field.ty(), classes);
                }
            }
            script::Ty::Function(signature) => classes_in_signature(signature, classes),
            script::Ty::Union(union) => {
                for ty in union.variants().iter().filter_map(script::Variant::ty) {
                    classes_in(ty, classes);
                }
            }
            script::Ty::Object(class) if !classes.iter().any(|c| core::ptr::eq(*c, class)) => {
                classes.push(class);
                for method in class.methods() {
                    classes_in_signature(method.signature(), classes);
                }
            }
            _ => {}
        }
    }

    /// `name` in snake case, as `ZoneSizes` is `zone_sizes`.
    fn snake(name: &str) -> String {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        }
        out
    }

    #[test]
    fn a_type_name_reads_in_snake_case() {
        assert_eq!(snake("Hold"), "hold");
        assert_eq!(snake("ZoneSizes"), "zone_sizes");
    }

    /// A row of the table of differences: the Rust paths it covers, such as
    /// `test::*` or `Load::config`, and why Luau differs there.
    #[derive(Debug)]
    struct Difference {
        patterns: Vec<String>,
        why: String,
    }

    impl Difference {
        fn covers(&self, path: &str) -> bool {
            self.patterns.iter().any(|p| match p.strip_suffix("::*") {
                Some(prefix) => path.starts_with(&format!("{prefix}::")),
                None => p == path,
            })
        }
    }

    /// The rows of the book's table of differences.
    fn differences() -> Vec<Difference> {
        let book = source().join("../../../book/src/luau.md");
        let text = std::fs::read_to_string(&book).unwrap();
        let (_, section) = text
            .split_once(DIFFERENCES)
            .unwrap_or_else(|| panic!("{} has no {DIFFERENCES:?}", book.display()));
        let section = section.split("\n## ").next().unwrap_or_default();
        section
            .lines()
            .filter(|line| line.starts_with('|'))
            .skip(2)
            .map(|row| {
                let cells: Vec<&str> = row.trim_matches('|').split(" | ").map(str::trim).collect();
                let patterns = cells[0]
                    .split('`')
                    .skip(1)
                    .step_by(2)
                    .map(str::to_owned)
                    .collect();
                Difference {
                    patterns,
                    why: cells.get(1).copied().unwrap_or_default().to_owned(),
                }
            })
            .collect()
    }

    #[test]
    fn every_public_rust_function_has_its_luau_mirror_or_a_stated_difference() {
        let walk = Walk::crate_api();
        let luau = luau_api();
        let differences = differences();
        // A constructor, such as `Hold::new` or `WritableType::resolve`, is
        // the module's function named for its type: `state.hold`.
        let mirrored = |f: &RustFn| {
            let constructor = !f.receiver
                && !f.owner.is_empty()
                && (f.returns.as_deref() == Some(f.owner.as_str())
                    || f.returns.as_deref() == Some("Self"));
            luau.contains(&f.luau())
                || luau.contains(&format!("{}:{}", f.owner, f.name))
                // `load.caller()` reads the same in every module, as `enshwick.caller`.
                || (f.owner == "Load" && luau.contains(&f.name))
                || (constructor && luau.contains(&format!("{}.{}", f.module, snake(&f.owner))))
        };
        // A handle's getter, `load.weather()`, is the namespace itself.
        let handles: BTreeSet<&str> = walk
            .functions
            .iter()
            .filter(|f| !f.owner.is_empty() && mirrored(f))
            .map(|f| f.owner.as_str())
            .collect();
        let mut missing: Vec<String> = walk
            .functions
            .iter()
            .filter(|f| !mirrored(f))
            .filter(|f| !REWORKED.contains(&f.module.as_str()))
            .filter(|f| !walk.values.contains(&f.owner))
            .filter(|f| {
                !(f.owner == "Load"
                    && !f.takes
                    && f.returns.as_deref().is_some_and(|r| handles.contains(r)))
            })
            .map(RustFn::path)
            .filter(|path| !differences.iter().any(|d| d.covers(path)))
            .collect();
        missing.sort();
        missing.dedup();
        assert!(
            missing.is_empty(),
            "{} public Rust functions have no Luau mirror and no row in the book's {DIFFERENCES:?}:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    #[test]
    fn every_stated_difference_names_rust_functions_and_why() {
        let walk = Walk::crate_api();
        let paths: Vec<String> = walk.functions.iter().map(RustFn::path).collect();
        for difference in differences() {
            assert!(!difference.why.is_empty(), "{difference:?} says no why");
            for pattern in &difference.patterns {
                let one = Difference {
                    patterns: vec![pattern.clone()],
                    why: String::new(),
                };
                assert!(
                    paths.iter().any(|p| one.covers(p)),
                    "{pattern} names no public Rust function"
                );
            }
        }
    }

    #[test]
    fn no_two_functions_of_a_namespace_share_a_name() {
        let mut seen = BTreeSet::new();
        for namespace in script::NAMESPACES {
            let names = namespace
                .functions()
                .iter()
                .map(script::Declared::name)
                .chain(namespace.init_functions().map(script::InitFunction::name))
                .chain(namespace.request_function().map(|f| f.name()))
                .collect::<Vec<_>>();
            for name in names {
                assert!(
                    seen.insert((namespace.name(), name)),
                    "enshwick.{}.{name} is declared twice; the second hides the first",
                    namespace.name()
                );
            }
        }
    }
}
