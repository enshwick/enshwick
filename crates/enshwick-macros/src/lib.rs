//! Macros for the Enshwick modding API; use them through `enshwick`,
//! which re-exports them.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::spanned::Spanned as _;
use syn::{
    Attribute, DeriveInput, Expr, Fields, FnArg, GenericArgument, Ident, ItemFn, Lit, Meta, Pat,
    PathArguments, Type, parse_macro_input,
};

use crate::config::expand_config;
use crate::shape::expand_layout;
use crate::shape::{expand_shape, toml_string};

/// Implements `enshwick::config::Config` for a struct with named fields.
///
/// The settings file a player sees on first start is generated from the type:
/// the struct's doc comment heads the file, each field's doc comment precedes
/// its key, and each value comes from `Default`. Struct attributes:
/// `#[config(validate = path)]` names a `fn(&Self) -> Result<(), E>`, `E`
/// any `Display` such as a `String`, that checks parsed values, `#[config(text = "key")]` the text key of the
/// set's translated title.
///
/// Field attributes: one stage of `startup`, `runtime` (the default), `restart`,
/// `client` and `sync`, or the pair `startup, sync`, applied when a world loads
/// and taken from the server by a joined client; `range = a..=b`, `a..` or
/// `..=b`; `choices = [..]`;
/// `text = "key"`, the text key of the field's translated label; `secret`,
/// a value never sent to a client and changed in the file only, so never
/// with `sync`; `group`, on a field that is a `Config` of its own, makes it
/// a table of the file. The derive implements `schema`, `values`, `merge_values` and
/// a `validate` that checks ranges and choices before the struct's own
/// check.
#[proc_macro_derive(Config, attributes(config))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_config(&input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// Declares a hook from its body: the function stays, and a static `Hook`
/// named after it in upper case joins it, as `enshwick::detour!`
/// declares one.
///
/// The first argument is `next: Next<F>` with `F` the target's
/// `extern "system" fn` type or an alias of it, the rest are the target's
/// arguments by plain names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. Examples are on `enshwick::hook`.
#[proc_macro_attribute]
pub fn hook(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut priority: Option<Expr> = None;
    let mut flags: Option<Expr> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("priority") {
            priority = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("flags") {
            flags = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `priority = <i32>` or `flags = <HookFlags>`"))
        }
    });
    parse_macro_input!(args with settings);
    let function = parse_macro_input!(item as ItemFn);
    expand_hook(&function, priority.as_ref(), flags.as_ref())
        .unwrap_or_else(|e| e.to_compile_error().into())
}

fn expand_hook(
    function: &ItemFn,
    priority: Option<&Expr>,
    flags: Option<&Expr>,
) -> syn::Result<TokenStream> {
    let sig = &function.sig;
    if let syn::Safety::Unsafe(unsafety) = &sig.safety {
        return Err(syn::Error::new_spanned(
            unsafety,
            "a hook body is a safe fn: put `unsafe` blocks inside it",
        ));
    }
    if let Some(asyncness) = &sig.asyncness {
        return Err(syn::Error::new_spanned(
            asyncness,
            "a hook body runs on the game's thread and is not `async`",
        ));
    }
    if !sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "a hook body takes no generics",
        ));
    }
    let mut inputs = sig.inputs.iter();
    let target = inputs.next().and_then(next_type).ok_or_else(|| {
        syn::Error::new_spanned(
            &sig.ident,
            "a hook body takes `next: Next<extern \"system\" fn(..)>` first",
        )
    })?;
    let mut idents = Vec::new();
    let mut arg_types = Vec::new();
    for input in inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "a hook body takes no `self`",
            ));
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "a hook body names each argument",
            ));
        };
        idents.push(&pat.ident);
        arg_types.push(&typed.ty);
    }
    let output = &sig.output;
    let body = &sig.ident;
    let name = Ident::new(&body.unraw().to_string().to_uppercase(), body.span());
    let vis = &function.vis;
    let kept = function
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc") || a.path().is_ident("cfg"));
    let priority = priority.map_or_else(|| quote!(0), |p| quote!(#p));
    let flags = flags.map_or_else(|| quote!(::enshwick::abi::HookFlags::NONE), |f| quote!(#f));
    let (entry, next, value) = (
        Ident::new("__enshwick_entry", Span::mixed_site()),
        Ident::new("next", Span::mixed_site()),
        Ident::new("value", Span::mixed_site()),
    );
    // The arguments go to the body and, after a panic, to `next` again.
    let copies = arg_types.iter().map(|ty| {
        quote::quote_spanned! {ty.span()=>
            ::enshwick::__private::hook_argument::<#ty>();
        }
    });
    // The entry point is a safe fn, which coerces to the target's type
    // whether that is `unsafe` or not, so `F` may be any name for it.
    Ok(quote! {
        #function

        #(#kept)*
        #vis static #name: ::enshwick::Hook<#target> = {
            const _: () = {
                #(#copies)*
            };
            extern "system" fn #entry(#(#idents: #arg_types),*) #output {
                let #next = #name.next();
                match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(
                    || #body(#next, #(#idents),*),
                )) {
                    ::core::result::Result::Ok(#value) => #value,
                    ::core::result::Result::Err(_) => {
                        ::enshwick::__private::fault_in(::core::stringify!(#name));
                        #[allow(unused_unsafe)]
                        // SAFETY: the rest of the chain takes the arguments
                        // the game passed.
                        unsafe { #next.get()(#(#idents),*) }
                    }
                }
            }
            ::enshwick::Hook::new(::core::stringify!(#name), #entry, #priority, #flags)
        };
    }
    .into())
}

/// Declares a service table: makes the struct `repr(C)` and implements
/// `enshwick::services::Service` with a layout hash per field.
///
/// `#[service(name = "<publisher id>:<what>", version = <major>)]`; the
/// version defaults to 1. Fields are `extern "system" fn` pointers or plain
/// data, appended over time. Examples are on `enshwick::service`.
#[proc_macro_attribute]
pub fn service(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let mut version: Option<syn::LitInt> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("version") {
            version = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `name = \"<id>:<what>\"` or `version = <major>`"))
        }
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    expand_layout(
        &item,
        name,
        Some(version.map_or_else(|| quote!(1), |v| quote!(#v))),
        &quote!(Service),
    )
    .unwrap_or_else(|e| e.to_compile_error().into())
}

/// Declares a mod event: makes the struct `repr(C)` and implements
/// `enshwick::services::ModEvent` with a layout hash per field.
///
/// `#[event(name = "<defining mod id>:<what>")]`; the struct is `Copy`
/// plain data, appended over time. Examples are on
/// `enshwick::event`.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `name = \"<id>:<what>\"`"))
        }
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    expand_layout(&item, name, None, &quote!(ModEvent))
        .unwrap_or_else(|e| e.to_compile_error().into())
}

/// Declares an extension point: makes the input struct `repr(C)` and
/// implements `enshwick::points::Point` with a layout hash per
/// field and one for the answer.
///
/// `#[point(name = "<owner id>:<point>", mode = Observe | Advise | Override,
/// output = <type>, refusable)]`; `refusable` lets a link refuse. Examples
/// are on `enshwick::point`.
#[proc_macro_attribute]
pub fn point(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let mut mode: Option<Ident> = None;
    let mut output: Option<Type> = None;
    let mut refusable = false;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("mode") {
            mode = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("output") {
            output = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("refusable") {
            refusable = true;
        } else {
            return Err(meta.error(
                "expected `name = \"<id>:<point>\"`, `mode = Override`, `output = <type>` or `refusable`",
            ));
        }
        Ok(())
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    let ident = item.ident.clone();
    let expanded = (|| {
        let mode = mode.ok_or_else(|| {
            syn::Error::new_spanned(
                &ident,
                "choose a mode: `mode = Observe`, `Advise` or `Override`",
            )
        })?;
        if !["Observe", "Advise", "Override"].contains(&mode.to_string().as_str()) {
            return Err(syn::Error::new_spanned(
                &mode,
                "the mode is `Observe`, `Advise` or `Override`",
            ));
        }
        let output = output
            .ok_or_else(|| syn::Error::new_spanned(&ident, "name the answer: `output = <type>`"))?;
        let schema = point_schema(&item, name.as_ref(), &mode, &output, refusable);
        let extra = quote! {
            const MODE: ::enshwick::points::Mode =
                ::enshwick::points::Mode::#mode;
            const REFUSE: bool = #refusable;
            const OUTPUT: &'static [::enshwick::abi::LayoutField] =
                &[::enshwick::abi::LayoutField {
                    name: ::enshwick::abi::Str::new("output"),
                    hash: ::enshwick::services::layout_hash(
                        "output",
                        <#output as ::enshwick::Shape>::SHAPE,
                        ::core::mem::size_of::<#output>(),
                        ::core::mem::align_of::<#output>(),
                        0,
                    ),
                }];
            type Output = #output;
            #schema
        };
        expand_declaration(&item, name, &extra, &quote!(points::Point))
    })();
    expanded.unwrap_or_else(|e| e.to_compile_error().into())
}

/// The `SCHEMA` of a point: its `[[point]]` table with the layout hashes,
/// then the `[[type]]` tables of its input and answer; and a check that a
/// schema can describe every field and the answer.
fn point_schema(
    item: &syn::ItemStruct,
    name: Option<&syn::LitStr>,
    mode: &Ident,
    output: &Type,
    refusable: bool,
) -> proc_macro2::TokenStream {
    let shape = quote!(::enshwick::Shape);
    let point = quote!(::enshwick::points::Point);
    let ident = &item.ident;
    let input = ident.unraw().to_string();
    let head = format!(
        "[[point]]\nname = {}\nmode = \"{}\"\nrefusable = {refusable}\ninput = {}\ndoc = {}\noutput = \"",
        toml_string(&name.map(syn::LitStr::value).unwrap_or_default()),
        mode.to_string().to_lowercase(),
        toml_string(&input),
        toml_string(&schema_doc(&item.attrs)),
    );
    let mut parts = vec![
        quote!(#head, <#output as #shape>::SCHEMA_NAME, "\"\noutput_layout = \"", <#ident as #point>::OUTPUT[0].hash, "\"\nlayout = ["),
    ];
    let mut checks = vec![quote! {
        assert!(
            !<#output as #shape>::SCHEMA_NAME.is_empty(),
            "a point schema cannot describe the answer's type: derive `Shape` for it",
        );
    }];
    let mut fields = Vec::new();
    let mut nested = Vec::new();
    for (i, field) in item.fields.iter().enumerate() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        let ty = &field.ty;
        let label = field_ident.unraw().to_string();
        let separator = if i == 0 { "\"" } else { ", \"" };
        parts.push(quote!(#separator, <#ident as #point>::LAYOUT[#i].hash, "\""));
        let message =
            format!("a point schema cannot describe the type of `{label}`: derive `Shape` for it");
        checks.push(quote!(assert!(!<#ty as #shape>::SCHEMA_NAME.is_empty(), #message);));
        let entry = format!(
            "[[type.field]]\nname = {}\ndoc = {}\ntype = \"",
            toml_string(&label),
            toml_string(&schema_doc(&field.attrs))
        );
        fields.push(quote!(#entry, <#ty as #shape>::SCHEMA_NAME, "\"\n"));
        nested.push(quote!(<#ty as #shape>::SCHEMA_TYPES));
    }
    let table = format!(
        "]\n[[type]]\nname = {}\ndoc = {}\nkind = \"struct\"\n",
        toml_string(&input),
        toml_string(&schema_doc(&item.attrs))
    );
    quote! {
        const SCHEMA: &'static str = {
            const _: () = { #(#checks)* };
            ::enshwick::__private::concatcp!(
                #(#parts,)* #table, #(#fields,)* #(#nested,)* <#output as #shape>::SCHEMA_TYPES,
            )
        };
    }
}

/// Implements `enshwick::Shape`: the structural hash the layout
/// checks of services, mod events and extension points reach into.
///
/// A struct is `repr(C)` or `repr(transparent)` and hashes its name, size,
/// alignment and every field with its shape and offset. An enum has an
/// integer `repr`, unit variants only, and hashes every variant with its
/// discriminant. `#[shape(values(A, B))]` on a newtype over an integer adds
/// the named constants `Self::A` and `Self::B` with their values. Examples
/// are on `enshwick::Shape`.
#[proc_macro_derive(Shape, attributes(shape))]
pub fn derive_shape(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_shape(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Describes a type for script languages; examples are on `enshwick::Script`.
///
/// A struct with named fields is a record of its public fields (fields of
/// type `Header` and fields named `_*` or `reserved*` left out), a newtype
/// its inner type, an enum of unit variants a choice of names.
#[proc_macro_derive(Script)]
pub fn derive_script(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    script::expand_derive(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Exports the methods of an `impl` block marked `#[script]` to scripts.
///
/// `#[script(<part>)]` adds the constant `SCRIPT_<PART>` of `Function`s.
/// Examples are on `enshwick::script`.
#[proc_macro_attribute]
pub fn script(args: TokenStream, item: TokenStream) -> TokenStream {
    let part = parse_macro_input!(args as Ident);
    let item = parse_macro_input!(item as syn::ItemImpl);
    script::expand_impl(&part, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Whether `id` is a mod id, `<author>.<name>`, or an id a renamed mod had
/// before ids carried an author, of `[a-z0-9-]`; at most 64 characters. The
/// API's `abi::is_valid_former_id`, which this crate cannot depend on.
fn is_mod_id(id: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    id.len() <= 64
        && match id.split_once('.') {
            Some((author, name)) => part(author) && part(name),
            None => part(id),
        }
}

/// The struct as `repr(C)` with the declaration trait at
/// `enshwick::<trait_path>`, named `<mod id>:<what>`, with
/// `extra` items in the impl.
fn expand_declaration(
    item: &syn::ItemStruct,
    name: Option<syn::LitStr>,
    extra: &proc_macro2::TokenStream,
    trait_path: &proc_macro2::TokenStream,
) -> syn::Result<TokenStream> {
    let ident = &item.ident;
    let name = name.ok_or_else(|| {
        syn::Error::new_spanned(ident, "name the declaration: `name = \"<id>:<what>\"`")
    })?;
    let text = name.value();
    let valid = text
        .split_once(':')
        .is_some_and(|(id, what)| is_mod_id(id) && !what.is_empty());
    if !valid {
        return Err(syn::Error::new_spanned(
            &name,
            "the name is `<mod id>:<what>`, the mod id `<author>.<name>` or a former id, of `[a-z0-9-]`",
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "a declaration crosses DLLs and takes no generics",
        ));
    }
    if let Some(repr) = item.attrs.iter().find(|a| a.path().is_ident("repr")) {
        return Err(syn::Error::new_spanned(
            repr,
            "the declaration is `repr(C)` already: remove `#[repr]`",
        ));
    }
    let Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "a declaration has named fields",
        ));
    };
    let entries = fields.named.iter().filter_map(|field| {
        let field_ident = field.ident.as_ref()?;
        let ty = &field.ty;
        let field_name = field_ident.unraw().to_string();
        Some(quote! {
            ::enshwick::abi::LayoutField {
                name: ::enshwick::abi::Str::new(#field_name),
                hash: ::enshwick::services::layout_hash(
                    #field_name,
                    <#ty as ::enshwick::Shape>::SHAPE,
                    ::core::mem::size_of::<#ty>(),
                    ::core::mem::align_of::<#ty>(),
                    ::core::mem::offset_of!(#ident, #field_ident),
                ),
            }
        })
    });
    Ok(quote! {
        #[repr(C)]
        #item

        // SAFETY: the layout lists every field of the `repr(C)` struct in
        // order, with its size, alignment and offset.
        unsafe impl ::enshwick::#trait_path for #ident {
            const NAME: &'static str = #name;
            #extra
            const LAYOUT: &'static [::enshwick::abi::LayoutField] = &[#(#entries),*];
        }
    }
    .into())
}

/// A name the generated code binds: hygienic against the user's locals,
/// and prefixed against the user's constants, which a `let` cannot shadow.
fn local(name: &str) -> Ident {
    Ident::new(&format!("__enshwick_{name}"), Span::mixed_site())
}

/// The `F` of `next: Next<F>`; `None` when the argument has another shape.
fn next_type(arg: &FnArg) -> Option<&Type> {
    let FnArg::Typed(typed) = arg else {
        return None;
    };
    let Type::Path(path) = &*typed.ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(generics) = &segment.arguments else {
        return None;
    };
    match generics.args.first()? {
        GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

/// The doc comment of `attrs` for a point schema: its prose before the
/// first heading, such as `# Examples`, whose code would not compile in the
/// mods that read the schema; lines joined by spaces.
fn schema_doc(attrs: &[Attribute]) -> String {
    doc_lines(attrs)
        .take_while(|line| !line.trim_start().starts_with('#'))
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The doc comment of `attrs` as plain text, lines joined by spaces.
fn doc_text(attrs: &[Attribute]) -> String {
    doc_lines(attrs)
        .map(|line| line.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The lines of the doc comment of `attrs`; a block comment or a
/// `#[doc = "..."]` may hold several.
fn doc_lines(attrs: &[Attribute]) -> impl Iterator<Item = String> + '_ {
    attrs
        .iter()
        .filter(|a| a.path().is_ident("doc"))
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(lit) => match &lit.lit {
                    Lit::Str(s) => Some(s.value()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        })
        .flat_map(|text| {
            if text.is_empty() {
                vec![text]
            } else {
                text.lines().map(str::to_owned).collect()
            }
        })
}

/// The doc comment of `attrs` as TOML comment lines.
fn comment(attrs: &[Attribute]) -> String {
    doc_lines(attrs).fold(String::new(), |mut text, line| {
        text.push('#');
        text.push_str(line.trim_end());
        text.push('\n');
        text
    })
}

mod config;
mod script;
mod shape;
