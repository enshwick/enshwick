//! Derive macros for the Enshwick modding API; use them through
//! `enshwick_api`, which re-exports them.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Lit, Meta, parse_macro_input};

/// Implements `enshwick_api::config::Config` for a struct with named fields.
///
/// The settings file a player sees on first start is generated from the type:
/// the struct's doc comment heads the file, each field's doc comment precedes
/// its key, and each value comes from `Default`.
#[proc_macro_derive(Config)]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
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
    let header = comment(&input.attrs);
    let entries = fields.named.iter().filter_map(|field| {
        let ident = field.ident.as_ref()?;
        let key = ident.to_string();
        let doc = comment(&field.attrs);
        Some(quote! {
            text.push_str(#doc);
            text.push_str(&::enshwick_api::config::__entry(#key, &defaults.#ident));
        })
    });
    quote! {
        impl ::enshwick_api::config::Config for #name {
            fn template() -> ::std::string::String {
                let defaults = <Self as ::core::default::Default>::default();
                let mut text = ::std::string::String::from(#header);
                if !text.is_empty() {
                    text.push('\n');
                }
                #(#entries)*
                text
            }
        }
    }
    .into()
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
