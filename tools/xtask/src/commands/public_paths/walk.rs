//! The public paths of a crate, walked through rustdoc's JSON of it and of
//! the crates it re-exports from, each documented with its private items so
//! an item a crate re-exports out of a private module is found by the path
//! another crate's re-export names.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use rustc_hash::{FxHashMap, FxHashSet};
use rustdoc_types::{Attribute, Crate, Id, Item, ItemEnum, StructKind, Type, Visibility};

/// What a path separator joins.
pub(crate) const SEPARATOR: &str = "::";

/// An item of one of the documented crates: its crate's library name and its
/// id in that crate's JSON.
type Key<'a> = (&'a str, Id);

/// What brings a member to its type: the trait of a documented crate whose
/// impl gives it, which a mod calls only where it can name that trait, or
/// `None` for an inherent member and for a trait the walk cannot gate: one
/// of an undocumented crate (`core`'s `Debug`, `From`) or one `resolve` does
/// not find, both listed as the type's own.
type Via<'a> = Option<Key<'a>>;

/// The members of a type, each with what brings it ([`Via`]).
type Members<'a> = BTreeMap<&'a str, Vec<Via<'a>>>;

/// What a walk from the root found.
#[derive(Default)]
struct Found<'a> {
    /// The paths a mod can write.
    paths: BTreeSet<String>,
    /// The paths of members only traits bring, each with those traits: one
    /// stands once the walk reaches one of them.
    gated: BTreeMap<String, Vec<Key<'a>>>,
    /// Every item a path reaches.
    reached: FxHashSet<Key<'a>>,
}

/// What a name of a module reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target<'a> {
    /// An item of a documented crate, whose members its path reaches.
    Item(Key<'a>),
    /// An item of a crate not documented here, such as `core`: the name
    /// stands, its members go unread.
    Outside,
}

/// The documented crates, by library name, with what the walk looks up in
/// them.
pub(crate) struct Surface<'a> {
    crates: &'a BTreeMap<String, Crate>,
    /// Each crate's own items by their path, the crate's name first: how a
    /// re-export in another crate, which names an item by that path, finds it.
    /// A path may name one item in each namespace, a macro and a module.
    by_path: FxHashMap<&'a str, FxHashMap<Vec<String>, Vec<Id>>>,
    /// The names `Type::name` reaches through impl blocks, wherever they are
    /// written: an inherent block's public items and a trait impl's items.
    impl_members: FxHashMap<Key<'a>, Members<'a>>,
}

impl<'a> Surface<'a> {
    pub(crate) fn new(crates: &'a BTreeMap<String, Crate>) -> Self {
        let by_path = crates
            .iter()
            .map(|(name, krate)| {
                let mut own: FxHashMap<Vec<String>, Vec<Id>> = FxHashMap::default();
                for (id, summary) in &krate.paths {
                    if summary.crate_id == 0 && krate.index.contains_key(id) {
                        own.entry(summary.path.clone()).or_default().push(*id);
                    }
                }
                // A proc-macro crate's `paths` gives its macros other ids than its index.
                declared(krate, krate.root, &mut vec![name.clone()], &mut own);
                (name.as_str(), own)
            })
            .collect();
        let mut surface = Self {
            crates,
            by_path,
            impl_members: FxHashMap::default(),
        };
        surface.impl_members = surface.impls();
        surface
    }

    /// Every public path of the crate `root`, without the crate's name, as
    /// a mod writes it after `root::`. A member a trait's impl brings counts
    /// only where the crate names the trait: a build without the trait's
    /// module leaves its methods out, so the list marks them as the trait.
    pub(crate) fn paths(&self, root: &'a str) -> Result<BTreeSet<String>, String> {
        let krate = self
            .crates
            .get(root)
            .ok_or_else(|| format!("no rustdoc JSON of {root}"))?;
        let mut found = Found::default();
        self.walk_module((root, krate.root), &[], &mut Vec::new(), &mut found);
        let Found {
            mut paths,
            gated,
            reached,
        } = found;
        paths.extend(
            gated
                .into_iter()
                .filter(|(_, traits)| traits.iter().any(|t| reached.contains(t)))
                .map(|(path, _)| path),
        );
        Ok(paths)
    }

    /// Records the paths below the module `module`, reached at `prefix`;
    /// `chain` keeps a re-export cycle finite.
    fn walk_module(
        &self,
        module: Key<'a>,
        prefix: &[&'a str],
        chain: &mut Vec<Key<'a>>,
        found: &mut Found<'a>,
    ) {
        if chain.contains(&module) {
            return;
        }
        chain.push(module);
        for (name, targets) in self.exports(module, &mut Vec::new()) {
            let mut path = prefix.to_vec();
            path.push(name);
            let joined = path
                .iter()
                .map(|part| written(part))
                .collect::<Vec<_>>()
                .join(SEPARATOR);
            for target in targets {
                let Target::Item(key) = target else {
                    continue;
                };
                found.reached.insert(key);
                for (member, vias) in self.members(key) {
                    let member = format!("{joined}{SEPARATOR}{}", written(member));
                    if vias.contains(&None) {
                        found.paths.insert(member);
                    } else {
                        found
                            .gated
                            .entry(member)
                            .or_default()
                            .extend(vias.into_iter().flatten());
                    }
                }
                if matches!(
                    self.item(key).map(|item| &item.inner),
                    Some(ItemEnum::Module(_))
                ) {
                    self.walk_module(key, &path, chain, found);
                }
            }
            found.paths.insert(joined);
        }
        chain.pop();
    }

    /// The names the module `module` shows, each with what it reaches: its
    /// public items and what its public uses bring, a name it declares
    /// itself shadowing the same name a glob brings. `globbing` keeps a cycle
    /// of globs finite.
    fn exports(
        &self,
        module: Key<'a>,
        globbing: &mut Vec<Key<'a>>,
    ) -> BTreeMap<&'a str, Vec<Target<'a>>> {
        let mut names: BTreeMap<&'a str, Vec<Target<'a>>> = BTreeMap::new();
        let Some(Item {
            inner: ItemEnum::Module(inner),
            ..
        }) = self.item(module)
        else {
            return names;
        };
        if globbing.contains(&module) {
            return names;
        }
        globbing.push(module);
        let (crate_name, _) = module;
        let mut globs = Vec::new();
        for id in &inner.items {
            let Some(item) = self.item((crate_name, *id)) else {
                continue;
            };
            if item.visibility != Visibility::Public || hidden(item) {
                continue;
            }
            match &item.inner {
                ItemEnum::Use(used) if used.is_glob => globs.push(used.id),
                ItemEnum::Use(used) => {
                    let target = match used.id {
                        Some(id) => self.resolve(crate_name, id),
                        None => Some(Target::Outside),
                    };
                    if let Some(target) = target {
                        add(&mut names, &used.name, target);
                    }
                }
                ItemEnum::Impl(_) => {}
                _ => {
                    if let Some(name) = &item.name {
                        add(&mut names, name, Target::Item((crate_name, *id)));
                    }
                }
            }
        }
        let declared: BTreeSet<&str> = names.keys().copied().collect();
        for id in globs.into_iter().flatten() {
            let Some(Target::Item(target)) = self.resolve(crate_name, id) else {
                continue;
            };
            let brought = match self.item(target).map(|item| &item.inner) {
                Some(ItemEnum::Module(_)) => self.exports(target, globbing),
                Some(ItemEnum::Enum(inner)) => {
                    let mut variants = BTreeMap::new();
                    for variant in &inner.variants {
                        let key = (target.0, *variant);
                        if let Some(name) = self
                            .item(key)
                            .filter(|item| !hidden(item))
                            .and_then(|item| item.name.as_deref())
                        {
                            add(&mut variants, name, Target::Item(key));
                        }
                    }
                    variants
                }
                _ => BTreeMap::new(),
            };
            for (name, targets) in brought {
                if !declared.contains(name) {
                    for target in targets {
                        add(&mut names, name, target);
                    }
                }
            }
        }
        globbing.pop();
        names
    }

    /// What `Item::member` reaches beside the item: a type's public fields,
    /// an enum's variants, a trait's items, and the impl blocks' items.
    fn members(&self, key: Key<'a>) -> Members<'a> {
        let Some(item) = self.item(key) else {
            return Members::new();
        };
        let declared: &[Id] = match &item.inner {
            ItemEnum::Struct(inner) => match &inner.kind {
                StructKind::Plain { fields, .. } => fields,
                StructKind::Unit | StructKind::Tuple(_) => &[],
            },
            ItemEnum::Union(inner) => &inner.fields,
            ItemEnum::Enum(inner) => &inner.variants,
            ItemEnum::Trait(inner) => &inner.items,
            _ => &[],
        };
        let mut members: Members<'a> = declared
            .iter()
            .filter_map(|id| self.item((key.0, *id)))
            .filter(|member| {
                !hidden(member)
                    && (!matches!(member.inner, ItemEnum::StructField(_))
                        || member.visibility == Visibility::Public)
            })
            .filter_map(|member| member.name.as_deref())
            .map(|name| (name, vec![None]))
            .collect();
        for (name, vias) in self.impl_members.get(&key).into_iter().flatten() {
            for via in vias {
                bring(&mut members, name, *via);
            }
        }
        members
    }

    /// The members every written impl block gives the type it is for: an
    /// inherent block's public items and a trait impl's methods and
    /// constants, with the provided methods of a trait of the documented
    /// crates; `Type::Assoc` does not compile, so associated types stay out,
    /// and a standard trait's provided methods, which move with the
    /// toolchain, too. Blanket and
    /// auto-trait impls give none, or every type would list `into` and
    /// `type_id`, and neither do `#[derive]`d ones, whose `clone`, `fmt` and
    /// `eq` no table names and which would move the list with each derive.
    /// A member of a documented crate's trait comes by that trait ([`Via`]).
    fn impls(&self) -> FxHashMap<Key<'a>, Members<'a>> {
        let mut members: FxHashMap<Key<'a>, Members<'a>> = FxHashMap::default();
        for (crate_name, krate) in self.crates {
            for item in krate.index.values() {
                let ItemEnum::Impl(block) = &item.inner else {
                    continue;
                };
                if block.is_synthetic
                    || block.is_negative
                    || block.blanket_impl.is_some()
                    || hidden(item)
                    || item.attrs.contains(&Attribute::AutomaticallyDerived)
                {
                    continue;
                }
                let Type::ResolvedPath(path) = &block.for_ else {
                    continue;
                };
                let Some(Target::Item(owner)) = self.resolve(crate_name, path.id) else {
                    continue;
                };
                let names = block
                    .items
                    .iter()
                    .filter_map(|id| krate.index.get(id))
                    .filter(|member| {
                        !hidden(member)
                            && !matches!(member.inner, ItemEnum::AssocType { .. })
                            && (block.trait_.is_some() || member.visibility == Visibility::Public)
                    })
                    .filter_map(|member| member.name.as_deref());
                let via = block.trait_.as_ref().and_then(|used| {
                    match self.resolve(crate_name, used.id) {
                        Some(Target::Item(key)) => Some(key),
                        _ => None,
                    }
                });
                let provided = via.map(|_| block.provided_trait_methods.iter().map(String::as_str));
                let listed = members.entry(owner).or_default();
                for name in names.chain(provided.into_iter().flatten()) {
                    bring(listed, name, via);
                }
            }
        }
        members
    }

    /// What the id `id` of the crate `crate_name` names: its own item, an
    /// item of another documented crate found by its path, or an item of a
    /// crate not documented here; `None` for an item rustdoc left out, such
    /// as a `#[doc(hidden)]` one.
    fn resolve(&self, crate_name: &'a str, id: Id) -> Option<Target<'a>> {
        let krate = self.crates.get(crate_name)?;
        if krate.index.contains_key(&id) {
            return Some(Target::Item((crate_name, id)));
        }
        let summary = krate.paths.get(&id)?;
        let owner = if summary.crate_id == 0 {
            crate_name
        } else {
            krate.external_crates.get(&summary.crate_id)?.name.as_str()
        };
        let Some((owner, own)) = self.by_path.get_key_value(owner) else {
            return Some(Target::Outside);
        };
        let krate = &self.crates[*owner];
        let found: Vec<&Id> = own
            .get(summary.path.as_slice())
            .into_iter()
            .flatten()
            .filter(|found| krate.index.contains_key(found))
            .collect();
        // The item of the namespace the reference names, where the path has several.
        found
            .iter()
            .find(|found| {
                krate
                    .paths
                    .get(found)
                    .is_some_and(|own| own.kind == summary.kind)
            })
            .or(found.first())
            .map(|found| Target::Item((owner, **found)))
    }

    fn item(&self, (crate_name, id): Key<'a>) -> Option<&'a Item> {
        self.crates.get(crate_name)?.index.get(&id)
    }
}

/// The keywords a name must be written raw as, `r#type`: Rust's strict and
/// reserved keywords but those no raw name may be.
const KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];

/// `name` as a path writes it: rustdoc gives a raw name without its `r#`.
fn written(name: &str) -> Cow<'_, str> {
    if KEYWORDS.contains(&name) {
        Cow::Owned(format!("r#{name}"))
    } else {
        Cow::Borrowed(name)
    }
}

/// How rustdoc's JSON writes `#[doc(hidden)]` (format 61).
const DOC_HIDDEN: &str = "#[doc(hidden)]";

/// Whether the item is marked `#[doc(hidden)]` itself, which leaves it out
/// of the documented surface; one only inside a hidden module stays when the
/// crate re-exports it.
fn hidden(item: &Item) -> bool {
    item.attrs
        .iter()
        .any(|attribute| matches!(attribute, Attribute::Other(text) if text == DOC_HIDDEN))
}

/// Adds each item `krate` declares below the module `module`, at `path`,
/// by the path it is declared at, private modules included.
fn declared(
    krate: &Crate,
    module: Id,
    path: &mut Vec<String>,
    found: &mut FxHashMap<Vec<String>, Vec<Id>>,
) {
    let Some(ItemEnum::Module(inner)) = krate.index.get(&module).map(|item| &item.inner) else {
        return;
    };
    for id in &inner.items {
        let Some(item) = krate.index.get(id) else {
            continue;
        };
        let Some(name) = item.name.as_ref() else {
            continue;
        };
        if matches!(item.inner, ItemEnum::Use(_) | ItemEnum::Impl(_)) {
            continue;
        }
        path.push(name.clone());
        let ids = found.entry(path.clone()).or_default();
        if !ids.contains(id) {
            ids.push(*id);
        }
        if matches!(item.inner, ItemEnum::Module(_)) {
            declared(krate, *id, path, found);
        }
        path.pop();
    }
}

/// Adds the member `name`, brought by `via`, beside what else brings it.
fn bring<'a>(members: &mut Members<'a>, name: &'a str, via: Via<'a>) {
    let vias = members.entry(name).or_default();
    if !vias.contains(&via) {
        vias.push(via);
    }
}

/// Adds `target` under `name`, beside what the name reaches in another
/// namespace.
fn add<'a>(names: &mut BTreeMap<&'a str, Vec<Target<'a>>>, name: &'a str, target: Target<'a>) {
    let targets = names.entry(name).or_default();
    if !targets.contains(&target) {
        targets.push(target);
    }
}
