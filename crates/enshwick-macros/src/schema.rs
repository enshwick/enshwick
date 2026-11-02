//! The schemas of points, events and calls, and the `.enshpt` entries that
//! ship them with their mod.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::{Ident, Type};

use crate::shape::toml_string;
use crate::{declare, schema_doc};

/// The `.enshpt` entry of a declaration's schema, `declared` being
/// `<Type as Trait>`: none with `of`. The mod's own exported declaration
/// ships it, an exported event that a schema cannot describe failing the
/// build; an internal one ships nothing. Another mod's ships it in a test
/// build only, where a test binary declares mods of other ids; there an
/// event a schema cannot describe ships nothing.
pub(crate) fn schema_entry(
    name: Option<&syn::LitStr>,
    of: Option<&Ident>,
    declared: &TokenStream,
    event: bool,
) -> TokenStream {
    let Some(name) = name.filter(|_| of.is_none()) else {
        return TokenStream::new();
    };
    let (shipping, cargo) = declare::shipping(&name.value());
    let tracked = cargo.map(|path| {
        quote!(
            const _: &[u8] = ::core::include_bytes!(#path);
        )
    });
    match shipping {
        declare::Shipping::Exported => {
            let check = event.then(|| {
                quote! {
                    const _: () = ::core::assert!(
                        #declared::DESCRIBED,
                        "an exported event's schema cannot describe a field's type: derive `Shape` for it",
                    );
                }
            });
            quote!(#tracked #check ::enshwick::__schema_entry!(#declared::SCHEMA);)
        }
        declare::Shipping::Internal => quote!(#tracked),
        declare::Shipping::Foreign => {
            let schema = if event {
                quote!(if #declared::DESCRIBED { #declared::SCHEMA } else { "" })
            } else {
                quote!(#declared::SCHEMA)
            };
            quote! {
                #tracked
                #[cfg(test)]
                const _: () = {
                    ::enshwick::__schema_entry!(#schema);
                };
            }
        }
    }
}

/// The `SCHEMA` of a mod event: its `[[event]]` table with the layout
/// hashes, then the `[[type]]` tables of its data; `DESCRIBED` says whether
/// a schema describes every field's type.
pub(crate) fn event_schema(item: &syn::ItemStruct, name: Option<&syn::LitStr>) -> TokenStream {
    let shape = quote!(::enshwick::Shape);
    let event = quote!(::enshwick::services::ModEvent);
    let ident = &item.ident;
    let data = ident.unraw().to_string();
    let head = format!(
        "[[event]]\nname = {}\ndata = {}\ndoc = {}\nlayout = [",
        toml_string(&name.map(syn::LitStr::value).unwrap_or_default()),
        toml_string(&data),
        toml_string(&schema_doc(&item.attrs)),
    );
    let mut parts = vec![quote!(#head)];
    let mut fields = Vec::new();
    let mut nested = Vec::new();
    let mut described = vec![quote!(true)];
    for (i, field) in item.fields.iter().enumerate() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        let ty = &field.ty;
        let label = field_ident.unraw().to_string();
        let separator = if i == 0 { "\"" } else { ", \"" };
        parts.push(quote!(#separator, <#ident as #event>::LAYOUT[#i].hash, "\""));
        described.push(quote!(!<#ty as #shape>::SCHEMA_NAME.is_empty()));
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
        toml_string(&data),
        toml_string(&schema_doc(&item.attrs))
    );
    quote! {
        const DESCRIBED: bool = #(#described)&&*;
        const SCHEMA: &'static str = ::enshwick::__private::concatcp!(
            #(#parts,)* #table, #(#fields,)* #(#nested,)*
        );
    }
}

/// The `SCHEMA` of a call: its `[[call]]` table with the layout hashes, then
/// the `[[type]]` tables of its input and answer; and a check that a schema
/// can describe every field and the answer.
pub(crate) fn call_schema(
    item: &syn::ItemStruct,
    name: Option<&syn::LitStr>,
    output: &Type,
) -> TokenStream {
    let shape = quote!(::enshwick::Shape);
    let call = quote!(::enshwick::services::Call);
    let ident = &item.ident;
    let input = ident.unraw().to_string();
    let head = format!(
        "[[call]]\nname = {}\ninput = {}\ndoc = {}\nmajor = ",
        toml_string(&name.map(syn::LitStr::value).unwrap_or_default()),
        toml_string(&input),
        toml_string(&schema_doc(&item.attrs)),
    );
    let mut parts = vec![quote!(
        #head, <#ident as #call>::MAJOR, "\noutput = \"", <#output as #shape>::SCHEMA_NAME,
        "\"\noutput_layout = \"", <#ident as #call>::OUTPUT[0].hash,
        "\"\ninput_size = ", ::core::mem::size_of::<#ident>(),
        "\noutput_size = ", ::core::mem::size_of::<#output>(),
        "\nlayout = ["
    )];
    let mut checks = vec![quote! {
        assert!(
            !<#output as #shape>::SCHEMA_NAME.is_empty(),
            "a call schema cannot describe the answer's type: derive `Shape` for it",
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
        parts.push(quote!(#separator, <#ident as #call>::LAYOUT[#i].hash, "\""));
        let message =
            format!("a call schema cannot describe the type of `{label}`: derive `Shape` for it");
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

/// The `SCHEMA` of a point: its `[[point]]` table with the layout hashes,
/// then the `[[type]]` tables of its input and answer; and a check that a
/// schema can describe every field and the answer.
pub(crate) fn point_schema(
    item: &syn::ItemStruct,
    name: Option<&syn::LitStr>,
    mode: &Ident,
    output: &Type,
    refusable: bool,
) -> TokenStream {
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
