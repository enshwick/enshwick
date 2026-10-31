//! `declare_mod!`: a mod's manifest, built at compile time from the
//! `[package.metadata.enshwick]` table of its crate's `Cargo.toml`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, LitStr, Token, Type, bracketed};

/// The keys of the table, the identity the platform reads.
const KEYS: [&str; 13] = [
    "id",
    "name",
    "side",
    "join",
    "reload",
    "developer",
    "exports",
    "replaces",
    "requires",
    "after",
    "recommends",
    "breaks",
    "builds",
];

/// The module of `deps!` that names the game's own content, which no
/// dependency's alias may take.
const VANILLA: &str = "vanilla";

/// The tables of dependencies by alias, with the ABI relation of each.
const RELATIONS: [(&str, &str); 4] = [
    ("requires", "REQUIRES"),
    ("after", "AFTER"),
    ("recommends", "RECOMMENDS"),
    ("breaks", "BREAKS"),
];

/// What `declare_mod!` takes: the mod's type, its metadata as TOML for a
/// test, and the Rust types of the points, events and calls it defines.
pub(crate) struct Args {
    ty: Type,
    metadata: Option<LitStr>,
    points: Vec<Type>,
    events: Vec<Type>,
    calls: Vec<Type>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let ty: Type = input.parse()?;
        let mut args = Self {
            ty,
            metadata: None,
            points: Vec::new(),
            events: Vec::new(),
            calls: Vec::new(),
        };
        while input.parse::<Option<Token![,]>>()?.is_some() {
            if input.is_empty() {
                break;
            }
            if input.peek(LitStr) {
                if args.metadata.is_some() {
                    return Err(input.error("the metadata is given once"));
                }
                args.metadata = Some(input.parse()?);
                continue;
            }
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let content;
            bracketed!(content in input);
            let types: Vec<Type> = Punctuated::<Type, Token![,]>::parse_terminated(&content)?
                .into_iter()
                .collect();
            let list = match key.to_string().as_str() {
                "points" => &mut args.points,
                "events" => &mut args.events,
                "calls" => &mut args.calls,
                _ => {
                    return Err(syn::Error::new(
                        key.span(),
                        "expected `points: [..]`, `events: [..]` or `calls: [..]`",
                    ));
                }
            };
            if !list.is_empty() {
                return Err(syn::Error::new(
                    key.span(),
                    format!("`{key}` is given once"),
                ));
            }
            *list = types;
        }
        Ok(args)
    }
}

/// Where a mod runs, or a build row's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Server,
    Client,
    Both,
}

impl Side {
    fn constant(self) -> Ident {
        let name = match self {
            Self::Server => "SERVER",
            Self::Client => "CLIENT",
            Self::Both => "BOTH",
        };
        Ident::new(name, Span::call_site())
    }
}

/// One relation to another mod.
struct Dependency {
    /// The name the mod's code knows it by; empty for `replaces`.
    alias: String,
    id: String,
    range: String,
    relation: &'static str,
}

/// One build row: symbols and constants the mod pins for a game build.
struct Build {
    side: Side,
    guid: String,
    age: u32,
    symbols: Vec<(String, u64)>,
    constants: Vec<(String, u64)>,
}

/// The table, read and checked.
struct Metadata {
    id: String,
    name: String,
    side: Side,
    join: bool,
    reload: bool,
    developer: bool,
    /// The local names other mods may use.
    exports: Vec<String>,
    dependencies: Vec<Dependency>,
    builds: Vec<Build>,
}

/// Expands `declare_mod!(Type, ..)`, reading the crate's `Cargo.toml`.
pub(crate) fn expand(args: &Args) -> syn::Result<TokenStream> {
    if let Some(metadata) = &args.metadata {
        return Err(syn::Error::new(
            metadata.span(),
            "a mod's metadata lives in `[package.metadata.enshwick]` of its Cargo.toml; \
             inline metadata is for `enshwick::test::declare_mod!`",
        ));
    }
    let (metadata, shown) = cargo_metadata()?;
    Ok(exports(args, &metadata, Some(&shown)))
}

/// Expands `content::id!("<name>")`: the content id `<mod>:<name>` of the
/// crate's mod, a constant `Id` whose build tracks the crate's `Cargo.toml`.
pub(crate) fn expand_id(name: &LitStr) -> syn::Result<TokenStream> {
    let local = name.value();
    if !is_content_name(&local) {
        return Err(syn::Error::new(
            name.span(),
            "a content name is 1 to 128 of [a-z0-9_./-], without the mod's id: \
             `id!` puts `<mod>:` in front",
        ));
    }
    let (metadata, shown) = cargo_metadata()?;
    let full = format!("{}:{local}", metadata.id);
    Ok(quote! {{
        const _: &[u8] = ::core::include_bytes!(#shown);
        ::enshwick::content::Id::new(#full)
    }})
}

/// Expands `deps!()`: a module per dependency the crate's `Cargo.toml`
/// names by alias in `requires`, `after` or `recommends`, with the
/// dependency's id and its content ids.
pub(crate) fn expand_deps() -> syn::Result<TokenStream> {
    let (metadata, shown) = cargo_metadata()?;
    let modules = metadata.dependencies.iter().filter_map(|d| {
        let relation = match d.relation {
            "REQUIRES" => "requires",
            "RECOMMENDS" => "recommends",
            "AFTER" => "loads after",
            _ => return None,
        };
        let alias = Ident::new(&d.alias, Span::call_site());
        let id = &d.id;
        let doc = format!("`{id}`, which this mod {relation}.");
        let content = if d.relation == "REQUIRES" {
            quote! {
                /// The dependency's content id `<dependency>:<name>`, once
                /// the loader lets this mod name it: the dependency's
                /// `exports` list `<name>` or the name it is a part of.
                /// Ask once, at init, and keep the id.
                ///
                /// # Errors
                ///
                /// [`Off`](::enshwick::Off) when the dependency does not
                /// export the name; the loader logs what it exports.
                ///
                /// # Panics
                ///
                /// When `name` is not 1 to 128 of `[a-z0-9_./-]`.
                pub fn id(
                    name: &'static str,
                ) -> ::core::result::Result<::enshwick::content::Id, ::enshwick::Off> {
                    ::enshwick::__private::dependency_id(::enshwick::content::Id::of(ID, name))
                }
            }
        } else {
            quote! {
                /// The dependency's content id `<dependency>:<name>`; `None`
                /// while the dependency does not run, and when it does not
                /// export the name, which the loader logs. Each call asks
                /// the loader which mods run: call it once, at init, and
                /// keep the id.
                ///
                /// # Panics
                ///
                /// When `name` is not 1 to 128 of `[a-z0-9_./-]`, whether
                /// the dependency runs or not.
                #[must_use]
                pub fn id(name: &'static str) -> ::core::option::Option<::enshwick::content::Id> {
                    let id = ::enshwick::content::Id::of(ID, name);
                    if !::enshwick::mods::is_running(ID) {
                        return ::core::option::Option::None;
                    }
                    ::enshwick::__private::dependency_id(id).ok()
                }
            }
        };
        Some(quote! {
            #[doc = #doc]
            pub mod #alias {
                /// The dependency's mod id.
                pub const ID: &str = #id;

                #content
            }
        })
    });
    Ok(quote! {
        /// The mods this one depends on, by the aliases its `Cargo.toml` gives
        /// them.
        pub mod deps {
            const _: &[u8] = ::core::include_bytes!(#shown);

            /// The game's own content, by the debug names its data gives it.
            pub mod vanilla {
                pub use ::enshwick::content::vanilla::{item, loot_table, recipe, station, workshop};
            }

            #(#modules)*
        }
    })
}

/// The full name `<mod id>:<what>` of a call, point or event declared with
/// `name`: as written when it holds a `:`, else after the id of the
/// dependency `of` or, without `of`, of the crate's own mod. With the name,
/// the `Cargo.toml` it read, if any, for build tracking.
pub(crate) fn full_name(
    name: &LitStr,
    of: Option<&Ident>,
) -> syn::Result<(LitStr, Option<String>)> {
    let text = name.value();
    if text.contains(':') {
        if let Some(of) = of {
            return Err(syn::Error::new(
                of.span(),
                "with `of`, the name is the dependency's own, without `<mod id>:`",
            ));
        }
        return Ok((name.clone(), None));
    }
    let (metadata, shown) = cargo_metadata()?;
    let owner = match of {
        None => metadata.id.clone(),
        Some(of) => {
            let alias = of.to_string();
            let names_code =
                |d: &&Dependency| matches!(d.relation, "REQUIRES" | "RECOMMENDS" | "AFTER");
            let found = metadata
                .dependencies
                .iter()
                .filter(names_code)
                .find(|d| d.alias == alias)
                .ok_or_else(|| {
                    let names: Vec<&str> = metadata
                        .dependencies
                        .iter()
                        .filter(names_code)
                        .map(|d| d.alias.as_str())
                        .collect();
                    let known = if names.is_empty() {
                        "it names none".to_owned()
                    } else {
                        format!("it names {}", names.join(", "))
                    };
                    syn::Error::new(
                        of.span(),
                        format!(
                            "`of = {alias}`: {shown} names no dependency `{alias}` in `requires`, `after` or `recommends`; {known}"
                        ),
                    )
                })?;
            found.id.clone()
        }
    };
    Ok((
        LitStr::new(&format!("{owner}:{text}"), name.span()),
        Some(shown),
    ))
}

/// Whether `name` is the name side of a content id: 1 to 128 of
/// `[a-z0-9_./-]`, as the API's `content::is_id` reads it.
fn is_content_name(name: &str) -> bool {
    (1..=128).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'.' | b'/' | b'-')
        })
}

/// The `[package.metadata.enshwick]` table of the crate being built, read,
/// with the path of its `Cargo.toml` for messages and build tracking.
fn cargo_metadata() -> syn::Result<(Metadata, String)> {
    let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
        syn::Error::new(
            Span::call_site(),
            "CARGO_MANIFEST_DIR is not set: build the mod with cargo",
        )
    })?;
    let path = PathBuf::from(dir).join("Cargo.toml");
    let shown = path.display().to_string();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| syn::Error::new(Span::call_site(), format!("{shown} does not read: {e}")))?;
    let cargo: toml::Table = toml::from_str(&text)
        .map_err(|e| syn::Error::new(Span::call_site(), format!("{shown}: {e}")))?;
    let table = cargo
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("enshwick"))
        .and_then(toml::Value::as_table)
        .ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                format!(
                    "{shown} has no `[package.metadata.enshwick]` table with the mod's id and side"
                ),
            )
        })?;
    let metadata = Metadata::read(table).map_err(|e| {
        syn::Error::new(
            Span::call_site(),
            format!("{shown}: [package.metadata.enshwick] {e}"),
        )
    })?;
    Ok((metadata, shown))
}

/// Expands `enshwick::test::declare_mod!(Type, "<toml>", ..)`, reading the
/// metadata from the literal.
pub(crate) fn expand_test(args: &Args) -> syn::Result<TokenStream> {
    let Some(literal) = &args.metadata else {
        return Err(syn::Error::new(
            Span::call_site(),
            "expected the mod's metadata after its type, as `[package.metadata.enshwick]` holds it",
        ));
    };
    let table: toml::Table = toml::from_str(&literal.value())
        .map_err(|e| syn::Error::new(literal.span(), e.to_string()))?;
    let metadata = Metadata::read(&table).map_err(|e| syn::Error::new(literal.span(), e))?;
    Ok(exports(args, &metadata, None))
}

impl Metadata {
    fn read(table: &toml::Table) -> Result<Self, String> {
        if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(format!("has no key `{key}`; it takes {}", KEYS.join(", ")));
        }
        let id = string(table, "id")?.ok_or("needs the mod's `id`, `<author>.<name>`")?;
        if !is_id(&id) {
            return Err(format!(
                "`id = {id:?}` is not <author>.<name>, each of [a-z0-9-], at most 64 characters"
            ));
        }
        let side = match string(table, "side")?.as_deref() {
            Some("server") => Side::Server,
            Some("client") => Side::Client,
            Some("both") => Side::Both,
            Some(other) => {
                return Err(format!(
                    "`side = {other:?}` is not \"server\", \"client\" or \"both\""
                ));
            }
            None => {
                return Err("needs the mod's `side`: \"server\", \"client\" or \"both\"".to_owned());
            }
        };
        let name = string(table, "name")?.unwrap_or_default();
        let mut dependencies = Vec::new();
        let mut aliases = BTreeSet::new();
        for (key, relation) in RELATIONS {
            let Some(value) = table.get(key) else {
                continue;
            };
            let entries = value
                .as_table()
                .ok_or_else(|| format!("`{key}` is a table of dependencies by alias"))?;
            for (alias, entry) in entries {
                if !is_alias(alias) || syn::parse_str::<Ident>(alias).is_err() {
                    return Err(format!(
                        "`{key}.{alias}`: an alias is a Rust name in snake case, [a-z][a-z0-9_]*, and no keyword"
                    ));
                }
                if alias == VANILLA {
                    return Err(format!(
                        "`{key}.{VANILLA}`: `deps::{VANILLA}` names the game's own content; choose another alias"
                    ));
                }
                if !aliases.insert(alias.clone()) {
                    return Err(format!(
                        "`{key}.{alias}`: the alias names another dependency already"
                    ));
                }
                dependencies.push(dependency(key, alias, entry, relation)?);
            }
        }
        if let Some(value) = table.get("replaces") {
            let ids = value.as_array().ok_or("`replaces` is a list of mod ids")?;
            for id in ids {
                let id = id.as_str().ok_or("`replaces` is a list of mod ids")?;
                if !is_old_or_new_id(id) {
                    return Err(format!(
                        "`replaces`: {id:?} is not <author>.<name> or [a-z0-9-]+"
                    ));
                }
                dependencies.push(Dependency {
                    alias: String::new(),
                    id: id.to_owned(),
                    range: String::new(),
                    relation: "REPLACES",
                });
            }
        }
        let builds = match table.get("builds") {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or("`builds` is a list of tables, `[[package.metadata.enshwick.builds]]`")?
                .iter()
                .map(build)
                .collect::<Result<_, _>>()?,
        };
        Ok(Self {
            id,
            name,
            side,
            join: flag(table, "join")?,
            reload: flag(table, "reload")?,
            developer: flag(table, "developer")?,
            exports: exports_of(table)?,
            dependencies,
            builds,
        })
    }
}

/// The text of `key`, if given.
fn string(table: &toml::Table, key: &str) -> Result<Option<String>, String> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("`{key}` is a string"))
        })
        .transpose()
}

/// The local names of `exports`, each a name once.
fn exports_of(table: &toml::Table) -> Result<Vec<String>, String> {
    let Some(value) = table.get("exports") else {
        return Ok(Vec::new());
    };
    let names = value
        .as_array()
        .ok_or("`exports` is a list of local names")?;
    let mut seen = BTreeSet::new();
    names
        .iter()
        .map(|name| {
            let name = name.as_str().ok_or("`exports` is a list of local names")?;
            if !is_content_name(name) {
                return Err(format!(
                    "`exports`: {name:?} is not a local name, 1 to 128 of [a-z0-9_./-] without the mod's id"
                ));
            }
            if !seen.insert(name) {
                return Err(format!("`exports`: {name:?} is given twice"));
            }
            Ok(name.to_owned())
        })
        .collect()
}

/// Whether `key` is `true`; `false` when left out.
fn flag(table: &toml::Table, key: &str) -> Result<bool, String> {
    table.get(key).map_or(Ok(false), |value| {
        value
            .as_bool()
            .ok_or_else(|| format!("`{key}` is `true` or `false`"))
    })
}

/// The dependency `alias` of the table `key`: a mod id, or a table with its
/// `id` and a `version` range.
fn dependency(
    key: &str,
    alias: &str,
    entry: &toml::Value,
    relation: &'static str,
) -> Result<Dependency, String> {
    let at = format!("`{key}.{alias}`");
    let (id, range) = match entry {
        toml::Value::String(id) => (id.clone(), String::new()),
        toml::Value::Table(fields) => {
            if let Some(field) = fields
                .keys()
                .find(|f| !matches!(f.as_str(), "id" | "version"))
            {
                return Err(format!(
                    "{at} has no key `{field}`; it takes `id` and `version`"
                ));
            }
            let id = string(fields, "id")
                .map_err(|e| format!("{at}: {e}"))?
                .ok_or_else(|| format!("{at} needs the mod's `id`"))?;
            let range = string(fields, "version").map_err(|e| format!("{at}: {e}"))?;
            (id, range.unwrap_or_default())
        }
        _ => {
            return Err(format!(
                "{at} is a mod id or a table with `id` and `version`"
            ));
        }
    };
    if !is_old_or_new_id(&id) {
        return Err(format!("{at}: {id:?} is not <author>.<name> or [a-z0-9-]+"));
    }
    if relation == "AFTER" && !range.is_empty() {
        return Err(format!(
            "{at}: `after` orders by id alone and takes no `version`"
        ));
    }
    Ok(Dependency {
        alias: alias.to_owned(),
        id,
        range,
        relation,
    })
}

/// One `[[package.metadata.enshwick.builds]]` row.
fn build(value: &toml::Value) -> Result<Build, String> {
    let row = value
        .as_table()
        .ok_or("`builds` is a list of tables, `[[package.metadata.enshwick.builds]]`")?;
    if let Some(key) = row.keys().find(|k| {
        !matches!(
            k.as_str(),
            "side" | "guid" | "age" | "symbols" | "constants"
        )
    }) {
        return Err(format!(
            "`builds` has no key `{key}`; a row takes side, guid, age, symbols and constants"
        ));
    }
    let side = match string(row, "side")
        .map_err(|e| format!("`builds`: {e}"))?
        .as_deref()
    {
        Some("server") => Side::Server,
        Some("client") => Side::Client,
        _ => return Err("a `builds` row needs `side = \"server\"` or `\"client\"`".to_owned()),
    };
    let guid = string(row, "guid")
        .map_err(|e| format!("`builds`: {e}"))?
        .ok_or("a `builds` row needs the build's `guid`")?;
    if guid.len() != 32
        || !guid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format!(
            "`builds`: `guid = {guid:?}` is not 32 lowercase hex digits"
        ));
    }
    let age = row
        .get("age")
        .and_then(toml::Value::as_integer)
        .and_then(|age| u32::try_from(age).ok())
        .ok_or("a `builds` row needs the build's `age`, a whole number")?;
    Ok(Build {
        side,
        guid,
        age,
        symbols: entries(row, "symbols")?,
        constants: entries(row, "constants")?,
    })
}

/// The `"id" = value` entries of a row's table `key`.
fn entries(row: &toml::Table, key: &str) -> Result<Vec<(String, u64)>, String> {
    let Some(value) = row.get(key) else {
        return Ok(Vec::new());
    };
    let table = value
        .as_table()
        .ok_or_else(|| format!("`builds.{key}` is a table of ids to numbers"))?;
    table
        .iter()
        .map(|(id, value)| {
            value
                .as_integer()
                .and_then(|v| u64::try_from(v).ok())
                .map(|v| (id.clone(), v))
                .ok_or_else(|| format!("`builds.{key}.{id:?}` is a whole number from 0 to TOML's largest, 0x7fff_ffff_ffff_ffff, such as 0x4_05cb"))
        })
        .collect()
}

/// Whether `part` is 1 or more of `[a-z0-9-]`.
fn is_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `id` is `<author>.<name>`, at most 64 characters; the API's
/// `abi::is_valid_id`, which this crate cannot depend on.
fn is_id(id: &str) -> bool {
    id.len() <= 64
        && id
            .split_once('.')
            .is_some_and(|(a, n)| is_part(a) && is_part(n))
}

/// Whether `id` is a mod id or one of the old form without an author that
/// `replaces` still takes; the API's `abi::is_valid_old_form_id`.
fn is_old_or_new_id(id: &str) -> bool {
    is_id(id) || (id.len() <= 64 && is_part(id))
}

/// Whether `alias` is a snake-case Rust name.
fn is_alias(alias: &str) -> bool {
    let mut bytes = alias.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// The exports of `args.ty` with the manifest of `metadata`; `cargo` is the
/// `Cargo.toml` it came from, a file the build then depends on.
fn exports(args: &Args, metadata: &Metadata, cargo: Option<&str>) -> TokenStream {
    let ty = &args.ty;
    let manifest = manifest(args, metadata, cargo);
    quote! {
        ::enshwick::__declare_exports!(#ty, #manifest);
    }
}

/// The block that builds the static manifest and evaluates to it.
fn manifest(args: &Args, metadata: &Metadata, cargo: Option<&str>) -> TokenStream {
    let abi = quote!(::enshwick::abi);
    let tracked = cargo.map(|path| {
        quote!(
            const _: &[u8] = ::core::include_bytes!(#path);
        )
    });
    let id = &metadata.id;
    let name = &metadata.name;
    let side = metadata.side.constant();
    let flags = flags_of(metadata);
    let dependencies = metadata.dependencies.iter().map(|d| {
        let (id, range) = (&d.id, &d.range);
        let relation = Ident::new(d.relation, Span::call_site());
        quote! {
            #abi::Dependency {
                id: #abi::Str::new(#id),
                range: #abi::Str::new(#range),
                relation: #abi::Relation::#relation,
            }
        }
    });
    let builds = metadata.builds.iter().map(|b| {
        let side = b.side.constant();
        let (guid, age) = (&b.guid, b.age);
        let pinned = symbols(&b.symbols);
        let constants = symbols(&b.constants);
        quote! {
            #abi::BuildRow {
                side: #abi::Side::#side,
                fingerprint: #abi::Fingerprint { guid: #abi::guid(#guid), age: #age },
                symbols: #pinned.as_ptr(),
                symbol_count: #pinned.len(),
                constants: #constants.as_ptr(),
                constant_count: #constants.len(),
            }
        }
    });
    let (points, events, calls) = (&args.points, &args.events, &args.calls);
    let exports = export_items(args, metadata);
    quote! {{
        #tracked
        #(const _: () = ::core::assert!(
            <#events as ::enshwick::services::ModEvent>::DESCRIBED,
            "an event's schema cannot describe a field's type: derive `Shape` for it",
        );)*
        #exports
        static DEPENDENCIES: &[#abi::Dependency] = &[#(#dependencies),*];
        const AUTHOR_COUNT: usize = ::enshwick::__private::author_count(::core::env!("CARGO_PKG_AUTHORS"));
        static AUTHORS: [#abi::Str; AUTHOR_COUNT] =
            ::enshwick::__private::authors(::core::env!("CARGO_PKG_AUTHORS"));
        static BUILDS: &[#abi::BuildRow] = &[#(#builds),*];
        const VERSION: &str = ::core::env!("CARGO_PKG_VERSION");
        const POINTS: &str = ::enshwick::__private::point_schemas(
            ::enshwick::__private::concatcp!(
                "schema = 1\nmod = \"", #id, "\"\nversion = \"", VERSION, "\"\n",
                #(<#points as ::enshwick::points::Point>::SCHEMA,)*
                #(<#events as ::enshwick::services::ModEvent>::SCHEMA,)*
                #(<#calls as ::enshwick::services::Call>::SCHEMA,)*
            ),
        );
        #[cfg_attr(windows, unsafe(link_section = ".enshpt"))]
        #[used]
        static POINT_SECTION: [u8; POINTS.len()] = ::enshwick::__private::bytes(POINTS);
        static MANIFEST: #abi::Manifest = #abi::Manifest {
            header: #abi::Header::of::<#abi::Manifest>(),
            id: #abi::Str::new(#id),
            version: #abi::Str::new(VERSION),
            side: #abi::Side::#side,
            flags: #flags,
            dependencies: DEPENDENCIES.as_ptr(),
            dependency_count: DEPENDENCIES.len(),
            builds: BUILDS.as_ptr(),
            build_count: BUILDS.len(),
            name: #abi::Str::new(#name),
            description: #abi::Str::new(::core::env!("CARGO_PKG_DESCRIPTION")),
            authors: AUTHORS.as_ptr(),
            author_count: AUTHOR_COUNT,
            license: #abi::Str::new(::core::env!("CARGO_PKG_LICENSE")),
            homepage: #abi::Str::new(::enshwick::__private::first_text(
                &[::core::env!("CARGO_PKG_HOMEPAGE"), ::core::env!("CARGO_PKG_REPOSITORY")],
            )),
            points: #abi::Str::new(POINTS),
            exports: EXPORTS.as_ptr(),
            export_count: EXPORTS.len(),
        };
        &MANIFEST
    }}
}

/// The manifest's flags expression.
fn flags_of(metadata: &Metadata) -> TokenStream {
    let abi = quote!(::enshwick::abi);
    let flags: Vec<TokenStream> = [
        (metadata.join, "JOIN"),
        (metadata.reload, "RELOAD"),
        (metadata.developer, "DEVELOPER"),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .map(|(_, flag)| {
        let flag = Ident::new(flag, Span::call_site());
        quote!(#abi::ManifestFlags::#flag.0)
    })
    .collect();
    if flags.is_empty() {
        quote!(#abi::ManifestFlags::NONE)
    } else {
        quote!(#abi::ManifestFlags(#(#flags)|*))
    }
}

/// The static `EXPORTS` of the manifest, and a check that every point,
/// event and call the mod lists, which are for other mods, is its own and
/// among them.
fn export_items(args: &Args, metadata: &Metadata) -> TokenStream {
    let abi = quote!(::enshwick::abi);
    let (id, exports) = (&metadata.id, &metadata.exports);
    let (points, events, calls) = (&args.points, &args.events, &args.calls);
    quote! {
        const EXPORT_NAMES: &[&str] = &[#(#exports),*];
        #(const _: () = ::core::assert!(
            ::enshwick::__private::is_own(#id, <#points as ::enshwick::points::Point>::NAME),
            "a point listed in `declare_mod!` has another mod's id: a mod lists only its own points, events and calls",
        );)*
        #(const _: () = ::core::assert!(
            ::enshwick::__private::is_own(#id, <#events as ::enshwick::services::ModEvent>::NAME),
            "an event listed in `declare_mod!` has another mod's id: a mod lists only its own points, events and calls",
        );)*
        #(const _: () = ::core::assert!(
            ::enshwick::__private::is_own(#id, <#calls as ::enshwick::services::Call>::NAME),
            "a call listed in `declare_mod!` has another mod's id: a mod lists only its own points, events and calls",
        );)*
        #(const _: () = ::core::assert!(
            !::enshwick::__private::is_own(#id, <#points as ::enshwick::points::Point>::NAME)
                || ::enshwick::__private::is_exported(#id, EXPORT_NAMES, <#points as ::enshwick::points::Point>::NAME),
            "a point listed in `declare_mod!` is for other mods: add its local name to `exports` in [package.metadata.enshwick]",
        );)*
        #(const _: () = ::core::assert!(
            !::enshwick::__private::is_own(#id, <#events as ::enshwick::services::ModEvent>::NAME)
                || ::enshwick::__private::is_exported(#id, EXPORT_NAMES, <#events as ::enshwick::services::ModEvent>::NAME),
            "an event listed in `declare_mod!` is for other mods: add its local name to `exports` in [package.metadata.enshwick]",
        );)*
        #(const _: () = ::core::assert!(
            !::enshwick::__private::is_own(#id, <#calls as ::enshwick::services::Call>::NAME)
                || ::enshwick::__private::is_exported(#id, EXPORT_NAMES, <#calls as ::enshwick::services::Call>::NAME),
            "a call listed in `declare_mod!` is for other mods: add its local name to `exports` in [package.metadata.enshwick]",
        );)*
        static EXPORTS: [#abi::Str; EXPORT_NAMES.len()] = [#(#abi::Str::new(#exports)),*];
    }
}

/// A row's symbols or constants as a static slice expression.
fn symbols(entries: &[(String, u64)]) -> TokenStream {
    let items = entries.iter().map(|(name, value)| {
        quote!(::enshwick::abi::Symbol { name: ::enshwick::abi::Str::new(#name), rva: #value })
    });
    quote!((&[#(#items),*] as &[::enshwick::abi::Symbol]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Metadata, String> {
        Metadata::read(&toml::from_str(text).map_err(|e| e.to_string())?)
    }

    #[test]
    fn the_table_reads_with_every_key() {
        let metadata = read(
            r#"
            id = "you.market-stalls"
            name = "Market Stalls"
            side = "both"
            join = true
            exports = ["price-of", "sold"]
            replaces = ["stalls"]

            [requires]
            lockers = { id = "you.lockers", version = ">=0.1, <1.0" }

            [after]
            lights = "you.lanterns"

            [breaks]
            old = { id = "old-stalls", version = "<0.3" }

            [[builds]]
            side = "server"
            guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
            age = 387

            [builds.symbols]
            "survivors.spawn_return" = 0x4_05cb
            "#,
        )
        .unwrap();
        assert_eq!(metadata.id, "you.market-stalls");
        assert_eq!(metadata.side, Side::Both);
        assert!(metadata.join && !metadata.reload && !metadata.developer);
        assert_eq!(metadata.exports, ["price-of", "sold"]);
        let relations: Vec<(&str, &str, &str)> = metadata
            .dependencies
            .iter()
            .map(|d| (d.relation, d.id.as_str(), d.range.as_str()))
            .collect();
        assert_eq!(
            relations,
            [
                ("REQUIRES", "you.lockers", ">=0.1, <1.0"),
                ("AFTER", "you.lanterns", ""),
                ("BREAKS", "old-stalls", "<0.3"),
                ("REPLACES", "stalls", ""),
            ]
        );
        assert_eq!(
            metadata.builds[0].symbols,
            [("survivors.spawn_return".to_owned(), 0x4_05cb)]
        );
        assert_eq!(metadata.builds[0].age, 387);
    }

    #[test]
    fn a_wrong_key_side_id_or_alias_is_refused_by_name() {
        let refused = |text: &str| read(text).err().unwrap_or_default();
        assert!(refused("id = \"you.a\"\nside = \"both\"\nversion = \"1\"").contains("`version`"));
        assert!(refused("id = \"you.a\"\nside = \"all\"").contains("\"all\""));
        assert!(refused("id = \"lanterns\"\nside = \"both\"").contains("<author>.<name>"));
        assert!(refused("side = \"both\"").contains("`id`"));
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\nexports = [\"you.a:sold\"]")
                .contains("not a local name")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\nexports = [\"sold\", \"sold\"]")
                .contains("given twice")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\n[requires]\nOther = \"you.b\"")
                .contains("`requires.Other`")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\n[requires]\ntype = \"you.b\"")
                .contains("no keyword")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\n[recommends]\nvanilla = \"you.b\"")
                .contains("the game's own content")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\n[requires]\nb = \"you.b\"\n[breaks]\nb = \"you.c\"")
                .contains("another dependency")
        );
        assert!(
            refused(
                "id = \"you.a\"\nside = \"both\"\n[after]\nb = { id = \"you.b\", version = \"1\" }"
            )
            .contains("no `version`")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\n[[builds]]\nside = \"both\"\nguid = \"00\"\nage = 1")
                .contains("`side = \"server\"`")
        );
    }

    #[test]
    fn a_content_name_is_the_name_side_of_a_content_id_only() {
        for name in ["locker", "stall/blacksmith.name", "bulk-bench_2"] {
            assert!(is_content_name(name), "{name}");
        }
        let long = "a".repeat(129);
        for name in ["", "you.lockers:locker", "Locker", "locker!", long.as_str()] {
            assert!(!is_content_name(name), "{name}");
        }
    }
}
