//! The schemas of points, events, calls and services, and the `.enshpt`
//! entries that ship them with their mod.

use proc_macro2::TokenStream;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::{Ident, Type};

use crate::shape::toml_string;
use crate::{declare, schema_doc};

/// The `.enshpt` entry of a declaration marked `export`, `declared` being
/// `<Type as Trait>`; none without the flag. The mod's own ships its
/// schema; an event or a service that a schema cannot describe fails the
/// build. Another mod's, a full name of another id, fails the
/// build, except in a test build, where a test binary declares mods of
/// other ids and an event a schema cannot describe ships nothing.
///
/// # Errors
///
/// When `export` comes with `of`, which names a dependency's declaration.
/// Outside a test build, `export` in a crate whose `Cargo.toml` does not
/// read as a mod's fails the build with that error.
pub(crate) fn schema_entry(
    name: Option<&syn::LitStr>,
    owner: &declare::Owner,
    of: Option<&Ident>,
    export: Option<&Ident>,
    declared: &TokenStream,
    kind: Kind,
) -> syn::Result<TokenStream> {
    let Some(export) = export else {
        return Ok(TokenStream::new());
    };
    if of.is_some() {
        return Err(syn::Error::new_spanned(
            export,
            "`export` marks the mod's own declaration; one with `of` is a dependency's",
        ));
    }
    let Some(name) = name else {
        return Err(syn::Error::new_spanned(
            export,
            "`export` needs the declaration's `name = \"<what>\"`",
        ));
    };
    let full = name.value();
    let local = full
        .split_once(':')
        .map_or(full.as_str(), |(_, local)| local);
    if !declare::is_content_name(local) {
        return Err(syn::Error::new_spanned(
            name,
            format!(
                "`export`: the local name {local:?} is not 1 to 128 of [a-z0-9_./-], which the loader refuses"
            ),
        ));
    }
    let schema = match kind {
        Kind::Event if !matches!(owner, declare::Owner::Own) => {
            quote!(if #declared::DESCRIBED { #declared::SCHEMA } else { "" })
        }
        Kind::Point | Kind::Call | Kind::Event | Kind::Service => quote!(#declared::SCHEMA),
    };
    match owner {
        // A test crate may declare other mods' names without a mod of its
        // own; outside a test the crate's metadata decides.
        declare::Owner::Unread(_) | declare::Owner::Other => {
            let message = match owner {
                declare::Owner::Unread(e) => e.to_string(),
                _ => format!(
                    "`export` on `{full}`: the crate's mod exports only its own names; name it locally"
                ),
            };
            Ok(quote! {
                #[cfg(not(test))]
                ::core::compile_error!(#message);
                #[cfg(test)]
                const _: () = {
                    ::enshwick::__schema_entry!(#schema);
                };
            })
        }
        declare::Owner::Own => {
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
    /// A service table.
    Service,
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

/// The `SCHEMA` of a service: its `[[service]]` table with the layout
/// hashes, then the `[[type]]` tables of the table and the types its
/// fields reach. An `extern "system"` function field is `fn(<args>) -> <answer>`,
/// with `unsafe ` before it for an unsafe one. Evaluating it, which an
/// exported service does, fails the build on a field a schema cannot
/// describe.
pub(crate) fn service_schema(item: &syn::ItemStruct, name: Option<&syn::LitStr>) -> TokenStream {
    let shape = quote!(::enshwick::Shape);
    let service = quote!(::enshwick::services::Service);
    let ident = &item.ident;
    let table = ident.unraw().to_string();
    let head = format!(
        "[[service]]\nname = {}\ntable = {}\ndoc = {}\nmajor = ",
        toml_string(&name.map(syn::LitStr::value).unwrap_or_default()),
        toml_string(&table),
        toml_string(&schema_doc(&item.attrs)),
    );
    let mut parts = vec![quote!(#head, <#ident as #service>::MAJOR, "\nlayout = [")];
    let mut checks = Vec::new();
    let mut fields = Vec::new();
    let mut nested = Vec::new();
    for (i, field) in item.fields.iter().enumerate() {
        let Some(field_ident) = &field.ident else {
            continue;
        };
        let label = field_ident.unraw().to_string();
        let separator = if i == 0 { "\"" } else { ", \"" };
        parts.push(quote!(#separator, <#ident as #service>::LAYOUT[#i].hash, "\""));
        let entry = format!(
            "[[type.field]]\nname = {}\ndoc = {}\ntype = \"",
            toml_string(&label),
            toml_string(&schema_doc(&field.attrs))
        );
        let mut described = |ty: &Type| {
            let message = format!(
                "an exported service's schema cannot describe a type in `{label}`: derive `Shape` for it"
            );
            checks.push(quote!(assert!(!<#ty as #shape>::SCHEMA_NAME.is_empty(), #message);));
            nested.push(quote!(<#ty as #shape>::SCHEMA_TYPES));
            quote!(<#ty as #shape>::SCHEMA_NAME)
        };
        let text = match &field.ty {
            Type::FnPtr(function) if is_system(function) => {
                let unsafety = if function.unsafety.is_some() {
                    "unsafe fn("
                } else {
                    "fn("
                };
                let mut text = vec![quote!(#unsafety)];
                for (j, input) in function.inputs.iter().enumerate() {
                    if j > 0 {
                        text.push(quote!(", "));
                    }
                    text.push(described(&input.ty));
                }
                text.push(quote!(")"));
                if let syn::ReturnType::Type(_, output) = &function.output
                    && !matches!(&**output, Type::Tuple(unit) if unit.elems.is_empty())
                {
                    let output = described(output);
                    text.push(quote!(" -> ", #output));
                }
                quote!(#(#text),*)
            }
            ty => described(ty),
        };
        fields.push(quote!(#entry, #text, "\"\n"));
    }
    let table = format!(
        "]\n[[type]]\nname = {}\ndoc = {}\nkind = \"struct\"\n",
        toml_string(&table),
        toml_string(&schema_doc(&item.attrs))
    );
    quote! {
        const SCHEMA: &'static str = {
            #(#checks)*
            ::enshwick::__private::concatcp!(#(#parts,)* #table, #(#fields,)* #(#nested,)*)
        };
    }
}

/// Whether `function` is an `extern "system"` function, the only kind a
/// service schema describes; another kind fails the check of its type.
fn is_system(function: &syn::TypeFnPtr) -> bool {
    function.variadic.is_none()
        && function.lifetimes.is_none()
        && function
            .abi
            .as_ref()
            .and_then(|abi| abi.name.as_ref())
            .is_some_and(|name| name.value() == "system")
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
