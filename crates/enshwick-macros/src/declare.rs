//! `declare_mod!`: a mod's manifest, built at compile time from the
//! `[package.metadata.enshwick]` table of its crate's `Cargo.toml`.

use std::collections::BTreeSet;
use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Type};

mod deps;

pub(crate) use deps::expand_deps;

/// The words of the API's `schema_section::Kind`, which this crate cannot
/// depend on; an API test holds the two to each other.
pub(crate) const EXPORT_KINDS: [&str; 6] = ["point", "event", "call", "service", "content", "stat"];

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

/// What `declare_mod!` takes: the mod's type, and its metadata as TOML and
/// the path of its texts for a test.
pub(crate) struct Args {
    ty: Type,
    metadata: Option<LitStr>,
    texts: Option<LitStr>,
}

impl Parse for Args {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let ty: Type = input.parse()?;
        let mut args = Self {
            ty,
            metadata: None,
            texts: None,
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
            if key != "texts" {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `texts = \"..\"`: points, events and calls ship from their attributes",
                ));
            }
            input.parse::<Token![=]>()?;
            if args.texts.is_some() {
                return Err(syn::Error::new(key.span(), "`texts` is given once"));
            }
            args.texts = Some(input.parse()?);
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
    /// The content and stat kinds other mods may use, each
    /// `content:<name>` or `stat:<name>`.
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
    if let Some(texts) = &args.texts {
        return Err(syn::Error::new(
            texts.span(),
            "a mod's texts live in src/texts.toml; `texts = \"..\"` is for `enshwick::test::declare_mod!`",
        ));
    }
    let (metadata, shown) = cargo_metadata()?;
    let texts = crate::texts::File::read(crate::texts::DEFAULT)?;
    Ok(exports(args, &metadata, Some(&shown), texts.as_ref()))
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

/// Whose a declaration's name is, by the crate's `Cargo.toml`.
pub(crate) enum Owner {
    /// The crate's mod's own.
    Own,
    /// Another mod's.
    Other,
    /// Unknown: the crate's `Cargo.toml` does not read as a mod's.
    Unread(syn::Error),
}

impl Owner {
    /// Whose `full`, `<mod id>:<what>`, is by `metadata`.
    fn of(metadata: &Metadata, full: &str) -> Self {
        match full.split_once(':') {
            Some((id, _)) if id == metadata.id => Self::Own,
            _ => Self::Other,
        }
    }
}

/// The full name `<mod id>:<what>` of a call, point, event or service
/// declared with `name`: as written when it holds a `:`, else after the id
/// of the dependency `of` or, without `of`, of the crate's own mod. With
/// the name, the `Cargo.toml` it read, if any, for build tracking, and
/// whose the name is, from that one read.
pub(crate) fn full_name(
    name: &LitStr,
    of: Option<&Ident>,
) -> syn::Result<(LitStr, Option<String>, Owner)> {
    let text = name.value();
    if text.contains(':') {
        if let Some(of) = of {
            return Err(syn::Error::new(
                of.span(),
                "with `of`, the name is the dependency's own, without `<mod id>:`",
            ));
        }
        return Ok(match cargo_metadata() {
            Ok((metadata, shown)) => {
                let owner = Owner::of(&metadata, &text);
                (name.clone(), Some(shown), owner)
            }
            Err(e) => (name.clone(), None, Owner::Unread(e)),
        });
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
    let full = format!("{owner}:{text}");
    let whose = if of.is_some() {
        Owner::Other
    } else {
        Owner::of(&metadata, &full)
    };
    Ok((LitStr::new(&full, name.span()), Some(shown), whose))
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
    let texts = crate::texts::File::for_test(args.texts.as_ref())?;
    Ok(exports(args, &metadata, None, texts.as_ref()))
}

impl Metadata {
    fn read(table: &toml::Table) -> syn::Result<Self> {
        if let Some(key) = table.keys().find(|k| !KEYS.contains(&k.as_str())) {
            return Err(refuse(format!(
                "has no key `{key}`; it takes {}",
                KEYS.join(", ")
            )));
        }
        let id = string(table, "id")?
            .ok_or_else(|| refuse("needs the mod's `id`, `<author>.<name>`"))?;
        if !is_id(&id) {
            return Err(refuse(format!(
                "`id = {id:?}` is not <author>.<name>, each of [a-z0-9-], at most 64 characters"
            )));
        }
        let side = match string(table, "side")?.as_deref() {
            Some("server") => Side::Server,
            Some("client") => Side::Client,
            Some("both") => Side::Both,
            Some(other) => {
                return Err(refuse(format!(
                    "`side = {other:?}` is not \"server\", \"client\" or \"both\""
                )));
            }
            None => {
                return Err(refuse(
                    "needs the mod's `side`: \"server\", \"client\" or \"both\"",
                ));
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
                .ok_or_else(|| refuse(format!("`{key}` is a table of dependencies by alias")))?;
            for (alias, entry) in entries {
                if !is_alias(alias) || syn::parse_str::<Ident>(alias).is_err() {
                    return Err(refuse(format!(
                        "`{key}.{alias}`: an alias is a Rust name in snake case, [a-z][a-z0-9_]*, and no keyword"
                    )));
                }
                if alias == VANILLA {
                    return Err(refuse(format!(
                        "`{key}.{VANILLA}`: `deps::{VANILLA}` names the game's own content; choose another alias"
                    )));
                }
                if !aliases.insert(alias.clone()) {
                    return Err(refuse(format!(
                        "`{key}.{alias}`: the alias names another dependency already"
                    )));
                }
                dependencies.push(dependency(key, alias, entry, relation)?);
            }
        }
        if let Some(value) = table.get("replaces") {
            let ids = value
                .as_array()
                .ok_or_else(|| refuse("`replaces` is a list of mod ids"))?;
            for id in ids {
                let id = id
                    .as_str()
                    .ok_or_else(|| refuse("`replaces` is a list of mod ids"))?;
                if !is_old_or_new_id(id) {
                    return Err(refuse(format!(
                        "`replaces`: {id:?} is not <author>.<name> or [a-z0-9-]+"
                    )));
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
                .ok_or_else(|| {
                    refuse("`builds` is a list of tables, `[[package.metadata.enshwick.builds]]`")
                })?
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
fn string(table: &toml::Table, key: &str) -> syn::Result<Option<String>> {
    table
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| refuse(format!("`{key}` is a string")))
        })
        .transpose()
}

/// The entries of `exports`, each `content:<name>` or `stat:<name>` and
/// given once; a call, point, service or event is exported by `export` on
/// its declaration instead.
fn exports_of(table: &toml::Table) -> syn::Result<Vec<String>> {
    const TAKES: &str = "`exports` is a list of `content:<name>` and `stat:<name>`";
    let Some(value) = table.get("exports") else {
        return Ok(Vec::new());
    };
    let names = value.as_array().ok_or_else(|| refuse(TAKES))?;
    let mut seen = BTreeSet::new();
    names
        .iter()
        .map(|name| {
            let name = name.as_str().ok_or_else(|| refuse(TAKES))?;
            let local = match name.split_once(':') {
                Some((kind @ ("content" | "stat"), local)) if EXPORT_KINDS.contains(&kind) => local,
                Some((kind, _)) if EXPORT_KINDS.contains(&kind) => {
                    return Err(refuse(format!(
                        "`exports`: {name:?}: a point, call, event or service is exported by `export` on its declaration"
                    )));
                }
                _ => {
                    return Err(refuse(format!(
                        "`exports`: {name:?} is not `content:<name>` or `stat:<name>`"
                    )));
                }
            };
            if !is_content_name(local) {
                return Err(refuse(format!(
                    "`exports`: {name:?}: the name is 1 to 128 of [a-z0-9_./-], without the mod's id"
                )));
            }
            if !seen.insert(name) {
                return Err(refuse(format!("`exports`: {name:?} is given twice")));
            }
            Ok(name.to_owned())
        })
        .collect()
}

/// Whether `key` is `true`; `false` when left out.
fn flag(table: &toml::Table, key: &str) -> syn::Result<bool> {
    table.get(key).map_or(Ok(false), |value| {
        value
            .as_bool()
            .ok_or_else(|| refuse(format!("`{key}` is `true` or `false`")))
    })
}

/// The dependency `alias` of the table `key`: a mod id, or a table with its
/// `id` and a `version` range.
fn dependency(
    key: &str,
    alias: &str,
    entry: &toml::Value,
    relation: &'static str,
) -> syn::Result<Dependency> {
    let at = format!("`{key}.{alias}`");
    let (id, range) = match entry {
        toml::Value::String(id) => (id.clone(), String::new()),
        toml::Value::Table(fields) => {
            if let Some(field) = fields
                .keys()
                .find(|f| !matches!(f.as_str(), "id" | "version"))
            {
                return Err(refuse(format!(
                    "{at} has no key `{field}`; it takes `id` and `version`"
                )));
            }
            let id = string(fields, "id")
                .map_err(|e| refuse(format!("{at}: {e}")))?
                .ok_or_else(|| refuse(format!("{at} needs the mod's `id`")))?;
            let range = string(fields, "version").map_err(|e| refuse(format!("{at}: {e}")))?;
            (id, range.unwrap_or_default())
        }
        _ => {
            return Err(refuse(format!(
                "{at} is a mod id or a table with `id` and `version`"
            )));
        }
    };
    if !is_old_or_new_id(&id) {
        return Err(refuse(format!(
            "{at}: {id:?} is not <author>.<name> or [a-z0-9-]+"
        )));
    }
    if relation == "AFTER" && !range.is_empty() {
        return Err(refuse(format!(
            "{at}: `after` orders by id alone and takes no `version`"
        )));
    }
    Ok(Dependency {
        alias: alias.to_owned(),
        id,
        range,
        relation,
    })
}

/// One `[[package.metadata.enshwick.builds]]` row.
fn build(value: &toml::Value) -> syn::Result<Build> {
    let row = value.as_table().ok_or_else(|| {
        refuse("`builds` is a list of tables, `[[package.metadata.enshwick.builds]]`")
    })?;
    if let Some(key) = row.keys().find(|k| {
        !matches!(
            k.as_str(),
            "side" | "guid" | "age" | "symbols" | "constants"
        )
    }) {
        return Err(refuse(format!(
            "`builds` has no key `{key}`; a row takes side, guid, age, symbols and constants"
        )));
    }
    let side = match string(row, "side")
        .map_err(|e| refuse(format!("`builds`: {e}")))?
        .as_deref()
    {
        Some("server") => Side::Server,
        Some("client") => Side::Client,
        _ => {
            return Err(refuse(
                "a `builds` row needs `side = \"server\"` or `\"client\"`",
            ));
        }
    };
    let guid = string(row, "guid")
        .map_err(|e| refuse(format!("`builds`: {e}")))?
        .ok_or_else(|| refuse("a `builds` row needs the build's `guid`"))?;
    if guid.len() != 32
        || !guid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refuse(format!(
            "`builds`: `guid = {guid:?}` is not 32 lowercase hex digits"
        )));
    }
    let age = row
        .get("age")
        .and_then(toml::Value::as_integer)
        .and_then(|age| u32::try_from(age).ok())
        .ok_or_else(|| refuse("a `builds` row needs the build's `age`, a whole number"))?;
    Ok(Build {
        side,
        guid,
        age,
        symbols: entries(row, "symbols")?,
        constants: entries(row, "constants")?,
    })
}

/// The `"id" = value` entries of a row's table `key`.
fn entries(row: &toml::Table, key: &str) -> syn::Result<Vec<(String, u64)>> {
    let Some(value) = row.get(key) else {
        return Ok(Vec::new());
    };
    let table = value
        .as_table()
        .ok_or_else(|| refuse(format!("`builds.{key}` is a table of ids to numbers")))?;
    table
        .iter()
        .map(|(id, value)| {
            value
                .as_integer()
                .and_then(|v| u64::try_from(v).ok())
                .map(|v| (id.clone(), v))
                .ok_or_else(|| refuse(format!("`builds.{key}.{id:?}` is a whole number from 0 to TOML's largest, 0x7fff_ffff_ffff_ffff, such as 0x4_05cb")))
        })
        .collect()
}

/// An error in the `[package.metadata.enshwick]` table; the caller names the
/// file and points at the macro call.
fn refuse(message: impl core::fmt::Display) -> syn::Error {
    syn::Error::new(Span::call_site(), message)
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

/// The exports of `args.ty` with the manifest of `metadata` and the texts
/// `Load::texts` serves; `cargo` is the `Cargo.toml` it came from, a file
/// the build then depends on.
fn exports(
    args: &Args,
    metadata: &Metadata,
    cargo: Option<&str>,
    texts: Option<&crate::texts::File>,
) -> TokenStream {
    let ty = &args.ty;
    let manifest = manifest(metadata, cargo);
    let texts = crate::texts::File::option(texts);
    quote! {
        ::enshwick::__declare_exports!(#ty, #manifest, #texts);
    }
}

/// The block that builds the static manifest and evaluates to the
/// `Manifested` that completes it with the mod's schemas.
fn manifest(metadata: &Metadata, cargo: Option<&str>) -> TokenStream {
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
    let listed = format!(
        "exports = [{}]\n",
        metadata
            .exports
            .iter()
            .map(|e| crate::shape::toml_string(e))
            .collect::<Vec<_>>()
            .join(", ")
    );
    quote! {{
        #tracked
        static DEPENDENCIES: &[#abi::Dependency] = &[#(#dependencies),*];
        const AUTHOR_COUNT: usize = ::enshwick::__private::author_count(::core::env!("CARGO_PKG_AUTHORS"));
        static AUTHORS: [#abi::Str; AUTHOR_COUNT] =
            ::enshwick::__private::authors(::core::env!("CARGO_PKG_AUTHORS"));
        static BUILDS: &[#abi::BuildRow] = &[#(#builds),*];
        const VERSION: &str = ::core::env!("CARGO_PKG_VERSION");
        const HEAD: &str = ::enshwick::__private::concatcp!(
            "schema = ", ::enshwick::schema_section::FORMAT, "\nmod = \"", #id,
            "\"\nversion = \"", VERSION, "\"\n", #listed,
        );
        ::enshwick::__schema_entry!(HEAD);
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
            points: #abi::Str::EMPTY,
            exports: ::core::ptr::null(),
            export_count: 0,
        };
        ::enshwick::__private::Manifested::new(&MANIFEST, #id, HEAD)
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
        Metadata::read(&toml::from_str(text).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    #[test]
    fn the_table_reads_with_every_key() {
        let metadata = read(
            r#"
            id = "you.market-stalls"
            name = "Market Stalls"
            side = "both"
            join = true
            exports = ["content:stall", "stat:luck"]
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
        assert_eq!(metadata.exports, ["content:stall", "stat:luck"]);
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
            refused("id = \"you.a\"\nside = \"both\"\nexports = [\"sold\"]")
                .contains("is not `content:<name>` or `stat:<name>`")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\nexports = [\"event:sold\"]")
                .contains("by `export` on its declaration")
        );
        assert!(
            refused("id = \"you.a\"\nside = \"both\"\nexports = [\"content:you.a:sold\"]")
                .contains("without the mod's id")
        );
        assert!(
            refused(
                "id = \"you.a\"\nside = \"both\"\nexports = [\"content:sold\", \"content:sold\"]"
            )
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
