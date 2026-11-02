//! The schemas of points, events and calls, and the `.enshpt` entries that
//! ship them with their mod.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::{Ident, Type};

use crate::shape::toml_string;
use crate::{declare, schema_doc};

/// The `.enshpt` entry of a declaration, `declared` being `<Type as
/// Trait>`. The mod's own, marked `export`, ships its schema, or a service
/// its name; an event that a schema cannot describe fails the build. The
/// mod's own without the flag ships `[[own]]` with its name and kind, so
/// the manifest exports no other declaration of that local name. Another
/// mod's ships nothing, and `export` on it fails the build, except in a
/// test build, where a test binary declares mods of other ids and an event
/// a schema cannot describe ships nothing.
///
/// # Errors
///
/// When `export` comes with `of`, which names a dependency's declaration,
/// when the crate's `Cargo.toml` does not read, or when its `exports` lists
/// the local name of a declaration without `export`.
pub(crate) fn schema_entry(
    name: Option<&syn::LitStr>,
    owner: &declare::Owner,
    of: Option<&Ident>,
    export: Option<&Ident>,
    declared: &TokenStream,
    kind: Kind,
) -> syn::Result<TokenStream> {
    let Some(name) = name else {
        return Ok(TokenStream::new());
    };
    let full = name.value();
    let Some(export) = export else {
        return match owner {
            declare::Owner::Own { listed: true } => Err(syn::Error::new_spanned(
                name,
                format!(
                    "`exports` in Cargo.toml lists the local name of `{full}`, a {}: mark the declaration `export` and take the name out of `exports`, which lists content and stat kinds alone",
                    kind.label(),
                ),
            )),
            declare::Owner::Own { listed: false } => {
                let text = format!(
                    "[[own]]\nname = {}\nkind = \"{}\"\n",
                    toml_string(&full),
                    kind.label()
                );
                Ok(quote!(::enshwick::__schema_entry!(#text);))
            }
            declare::Owner::Other | declare::Owner::Unread(_) => Ok(TokenStream::new()),
        };
    };
    if of.is_some() {
        return Err(syn::Error::new_spanned(
            export,
            "`export` marks the mod's own declaration; one with `of` is a dependency's",
        ));
    }
    let schema = match kind {
        Kind::Service => {
            let text = format!("[[service]]\nname = {}\n", toml_string(&full));
            quote!(#text)
        }
        Kind::Event if !matches!(owner, declare::Owner::Own { .. }) => {
            quote!(if #declared::DESCRIBED { #declared::SCHEMA } else { "" })
        }
        Kind::Point | Kind::Call | Kind::Event => quote!(#declared::SCHEMA),
    };
    match owner {
        declare::Owner::Unread(e) => Err(syn::Error::new_spanned(export, e.to_string())),
        declare::Owner::Other => {
            let message = format!(
                "`export` on `{full}`: the crate's mod exports only its own names; name it locally"
            );
            Ok(quote! {
                #[cfg(not(test))]
                ::core::compile_error!(#message);
                #[cfg(test)]
                const _: () = {
                    ::enshwick::__schema_entry!(#schema);
                };
            })
        }
        declare::Owner::Own { .. } => {
            let check = matches!(kind, Kind::Event).then(|| {
                quote! {
                    const _: () = ::core::assert!(
                        #declared::DESCRIBED,
                        "an exported event's schema cannot describe a field's type: derive `Shape` for it",
                    );
                }
            });
            Ok(quote!(#check ::enshwick::__schema_entry!(#schema);))
        }
    }
}

/// What a declaration is, for its entry.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    /// An extension point.
    Point,
    /// A call.
    Call,
    /// A mod event.
    Event,
    /// A service table, whose entry is its name alone.
    Service,
}

impl Kind {
    /// The kind's name in a schema's `[[own]]` entry and in messages.
    const fn label(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Call => "call",
            Self::Event => "event",
            Self::Service => "service",
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
