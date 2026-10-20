//! Macros for the Enshwick modding API; use them through `enshwick_loader_api`,
//! which re-exports them.

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, FnArg, GenericArgument, Ident, ItemFn, Lit, Meta,
    Pat, PathArguments, Type, parse_macro_input,
};

/// Implements `enshwick_loader_api::config::Config` for a struct with named fields.
///
/// The settings file a player sees on first start is generated from the type:
/// the struct's doc comment heads the file, each field's doc comment precedes
/// its key, and each value comes from `Default`. `#[config(validate = path)]`
/// names a `fn(&Self) -> Result<(), String>` that checks parsed values.
///
/// `#[config(sync)]` on a field marks a join value, one the server and its
/// players must agree on: the derive implements `synced` and `merge_synced`
/// over the marked fields, and a joined client runs with the server's values.
#[proc_macro_derive(Config, attributes(config))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let mut validate: Option<syn::Path> = None;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("config")) {
        let parsed = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("validate") {
                validate = Some(meta.value()?.parse()?);
                Ok(())
            } else {
                Err(meta.error("expected `validate = path`"))
            }
        });
        if let Err(e) = parsed {
            return e.to_compile_error().into();
        }
    }
    let validate = validate.map(|path| {
        quote! {
            fn validate(&self) -> ::core::result::Result<(), ::std::string::String> {
                #path(self)
            }
        }
    });
    let Data::Struct(data) = &input.data else {
        return syn::Error::new_spanned(name, "Config is derived for structs only")
            .to_compile_error()
            .into();
    };
    let Fields::Named(fields) = &data.fields else {
        return syn::Error::new_spanned(name, "Config needs named fields")
            .to_compile_error()
            .into();
    };
    let mut synced = Vec::new();
    for field in &fields.named {
        match is_synced(&field.attrs) {
            Ok(true) => synced.push(field),
            Ok(false) => {}
            Err(e) => return e.to_compile_error().into(),
        }
    }
    let sync = sync_methods(&synced);
    let header = comment(&input.attrs);
    let entries = fields.named.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let key = ident.to_string();
        let doc = comment(&field.attrs);
        Some(quote! {
            text.push_str(#doc);
            text.push_str(&::enshwick_loader_api::config::__entry(#key, &defaults.#ident));
        })
    });
    quote! {
        impl ::enshwick_loader_api::config::Config for #name {
            fn template() -> ::std::string::String {
                let defaults = <Self as ::core::default::Default>::default();
                let mut text = ::std::string::String::from(#header);
                if !text.is_empty() {
                    text.push('\n');
                }
                #(#entries)*
                text
            }

            #validate

            #sync
        }
    }
    .into()
}

/// Declares a hook from its body: the function stays, and
/// `enshwick_loader_api::detour!` declares a static `Hook` named after it in upper
/// case.
///
/// The first argument is `next: Next<F>` with `F` the target's
/// `extern "system" fn` type, the rest are the target's arguments by plain
/// names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. Examples are on `enshwick_loader_api::hook`.
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
    if !sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "a hook body takes no generics",
        ));
    }
    let mut inputs = sig.inputs.iter();
    let is_unsafe = inputs.next().and_then(next_is_unsafe).ok_or_else(|| {
        syn::Error::new_spanned(
            &sig.ident,
            "a hook body takes `next: Next<extern \"system\" fn(..)>` first",
        )
    })?;
    let unsafety = is_unsafe.then(|| quote!(unsafe));
    let mut args = Vec::new();
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
        let ident = &pat.ident;
        let ty = &typed.ty;
        args.push(quote!(#ident: #ty));
    }
    let output = &sig.output;
    let body = &sig.ident;
    let name = Ident::new(&body.to_string().to_uppercase(), body.span());
    let vis = &function.vis;
    let docs = function.attrs.iter().filter(|a| a.path().is_ident("doc"));
    let priority = priority.map(|p| quote!(, priority = #p));
    let flags = flags.map(|f| quote!(, flags = #f));
    Ok(quote! {
        #function

        ::enshwick_loader_api::detour! {
            #(#docs)*
            #vis static #name: #unsafety extern "system" fn(#(#args),*) #output
                = #body #priority #flags;
        }
    }
    .into())
}

/// Whether the `F` of `next: Next<F>` is an `unsafe fn`; `None` when the
/// argument has another shape.
fn next_is_unsafe(arg: &FnArg) -> Option<bool> {
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
        GenericArgument::Type(Type::FnPtr(f)) => Some(f.unsafety.is_some()),
        _ => None,
    }
}

/// Whether a field carries `#[config(sync)]`.
fn is_synced(attrs: &[Attribute]) -> syn::Result<bool> {
    let mut synced = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("sync") {
                synced = true;
                Ok(())
            } else {
                Err(meta.error("expected `sync`"))
            }
        })?;
    }
    Ok(synced)
}

/// `synced` and `merge_synced` over the join value fields, none when there
/// are none.
fn sync_methods(fields: &[&syn::Field]) -> Option<impl quote::ToTokens> {
    if fields.is_empty() {
        return None;
    }
    let idents: Vec<&Ident> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
    let keys: Vec<String> = idents.iter().map(ToString::to_string).collect();
    let types = fields.iter().map(|f| &f.ty);
    let locals: Vec<Ident> = idents
        .iter()
        .map(|ident| quote::format_ident!("__{}", ident))
        .collect();
    Some(quote! {
        fn synced(&self) -> ::core::option::Option<::std::string::String> {
            let mut table = ::enshwick_loader_api::config::__Synced::default();
            #(table.put(#keys, &self.#idents);)*
            ::core::option::Option::Some(table.finish())
        }

        fn merge_synced(
            &mut self,
            synced: &str,
        ) -> ::core::result::Result<(), ::std::string::String> {
            let table = ::enshwick_loader_api::config::__Synced::parse(synced)?;
            #(let #locals = table.take::<#types>(#keys)?;)*
            #(
                if let ::core::option::Option::Some(value) = #locals {
                    self.#idents = value;
                }
            )*
            ::core::result::Result::Ok(())
        }
    })
}

/// The doc comment of `attrs` as TOML comment lines.
fn comment(attrs: &[Attribute]) -> String {
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
        .fold(String::new(), |mut text, line| {
            text.push('#');
            text.push_str(line.trim_end());
            text.push('\n');
            text
        })
}
