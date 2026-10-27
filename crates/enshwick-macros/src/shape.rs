use std::fmt::Write as _;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::{Attribute, Data, DeriveInput, Fields, Ident};

use crate::{expand_declaration, schema_doc};

/// The integer `repr`s an enum may have; 128-bit ones are left out, since
/// the hash takes a discriminant as an `i128`.
pub(super) const PRIMITIVE_REPRS: [&str; 10] = [
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize",
];

/// What `#[derive(Shape)]` reads from a type's attributes.
pub(super) struct ShapeAttributes {
    /// The `repr`s.
    pub(super) reprs: Vec<String>,
    /// The constants `#[shape(values(..))]` names.
    pub(super) values: Vec<Ident>,
    /// The name `#[shape(api = "..")]` gives a type of the API crate in a
    /// point schema.
    pub(super) api: Option<String>,
}

/// The `repr`s, the `#[shape(values(..))]` constants and the
/// `#[shape(api = "..")]` name of a type.
pub(super) fn shape_attributes(attrs: &[Attribute]) -> syn::Result<ShapeAttributes> {
    let mut reprs = Vec::new();
    let mut values = Vec::new();
    let mut api = None;
    for attr in attrs {
        if attr.path().is_ident("repr") {
            attr.parse_nested_meta(|meta| {
                if let Some(repr) = meta.path.get_ident() {
                    reprs.push(repr.to_string());
                }
                if meta.input.peek(syn::token::Paren) {
                    let _skipped: proc_macro2::TokenStream = meta.input.parse()?;
                }
                Ok(())
            })?;
        } else if attr.path().is_ident("shape") {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("values") {
                    meta.parse_nested_meta(|value| {
                        values.push(value.path.require_ident()?.clone());
                        Ok(())
                    })
                } else if meta.path.is_ident("api") {
                    api = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                    Ok(())
                } else {
                    Err(meta.error("expected `values(<CONSTANT>, ...)` or `api = \"<name>\"`"))
                }
            })?;
        }
    }
    Ok(ShapeAttributes { reprs, values, api })
}

/// `text` as a TOML basic string, quotes included.
pub(super) fn toml_string(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            c if c.is_control() => {
                let _written = write!(quoted, "\\u{:04X}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

pub(super) fn expand_shape(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a shape crosses DLLs and takes no generics",
        ));
    }
    let ShapeAttributes { reprs, values, api } = shape_attributes(&input.attrs)?;
    let name = ident.unraw().to_string();
    let shape = quote!(::enshwick::Shape);
    let schema = shape_schema(input, &name, &reprs, &values, api.as_deref());
    let parts = match &input.data {
        Data::Struct(data) => {
            if !reprs.iter().any(|r| r == "C" || r == "transparent") {
                return Err(syn::Error::new_spanned(
                    ident,
                    "a shared struct is `#[repr(C)]` or `#[repr(transparent)]`",
                ));
            }
            if reprs.iter().any(|r| r == "packed" || r == "align") {
                return Err(syn::Error::new_spanned(
                    ident,
                    "a shared struct is plain `#[repr(C)]`: other languages lay it out from its fields, so `packed` and `align` are refused",
                ));
            }
            let newtype = matches!(&data.fields, Fields::Unnamed(f) if f.unnamed.len() == 1);
            if !values.is_empty() && !newtype {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`values` names the constants of a newtype over an integer",
                ));
            }
            let fields = data.fields.iter().enumerate().map(|(i, field)| {
                let ty = &field.ty;
                let (label, member) = if let Some(f) = &field.ident {
                    (f.unraw().to_string(), quote!(#f))
                } else {
                    let index = syn::Index::from(i);
                    (i.to_string(), quote!(#index))
                };
                quote! {
                    .text(#label)
                    .number(<#ty as #shape>::SHAPE)
                    .offset(::core::mem::offset_of!(Self, #member))
                }
            });
            let named = values.iter().map(|value| {
                let label = value.to_string();
                quote!(.text(#label).value(Self::#value.0 as i128))
            });
            quote!(.text("struct") #(#fields)* #(#named)*)
        }
        Data::Enum(data) => {
            if !values.is_empty() {
                return Err(syn::Error::new_spanned(
                    ident,
                    "`values` names the constants of a newtype; an enum lists its variants",
                ));
            }
            let Some(repr) = reprs.iter().find(|r| PRIMITIVE_REPRS.contains(&r.as_str())) else {
                return Err(syn::Error::new_spanned(
                    ident,
                    "a shared enum has an integer `repr`, such as `#[repr(u8)]`",
                ));
            };
            let mut variants = Vec::new();
            for variant in &data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "a shared enum has unit variants only",
                    ));
                }
                let v = &variant.ident;
                let label = v.unraw().to_string();
                variants.push(quote!(.text(#label).value(Self::#v as i128)));
            }
            quote!(.text("enum").text(#repr) #(#variants)*)
        }
        Data::Union(data) => {
            return Err(syn::Error::new_spanned(
                data.union_token,
                "a union has no shape: wrap it in a struct and implement `Shape` by hand",
            ));
        }
    };
    Ok(quote! {
        // SAFETY: the hash covers the type's name, size and alignment, and
        // every field with its shape and offset, every variant with its
        // discriminant, or every named value.
        unsafe impl #shape for #ident {
            const SHAPE: u64 = ::enshwick::shape::ShapeHasher::new(#name)
                .layout::<Self>()
                #parts
                .finish();
            #schema
        }
    })
}

/// The `SCHEMA_NAME` and `SCHEMA_TYPES` of a derived [`Shape`]: the type's
/// own `[[type]]` table, then those of the named types inside it. Called
/// after `expand_shape` has checked the type.
pub(super) fn shape_schema(
    input: &DeriveInput,
    name: &str,
    reprs: &[String],
    values: &[Ident],
    api: Option<&str>,
) -> proc_macro2::TokenStream {
    if let Some(api) = api {
        let api = format!("enshwick.{api}");
        return quote!(const SCHEMA_NAME: &'static str = #api;);
    }
    let shape = quote!(::enshwick::Shape);
    let ident = &input.ident;
    let head = format!(
        "[[type]]\nname = {}\ndoc = {}\n",
        toml_string(name),
        toml_string(&schema_doc(&input.attrs))
    );
    let mut parts = vec![quote!(#head)];
    let mut nested = Vec::new();
    match &input.data {
        Data::Struct(data) => {
            let newtype = matches!(&data.fields, Fields::Unnamed(f) if f.unnamed.len() == 1);
            if newtype && !values.is_empty() {
                let inner = &data.fields.iter().next().map(|f| f.ty.clone());
                parts.push(
                    quote!("kind = \"values\"\nrepr = \"", <#inner as #shape>::SCHEMA_NAME, "\"\n"),
                );
                for value in values {
                    let label = format!(
                        "[[type.value]]\nname = {}\nvalue = ",
                        toml_string(&value.to_string())
                    );
                    parts.push(quote!(#label, #ident::#value.0, "\n"));
                }
            } else {
                let kind = if newtype { "newtype" } else { "struct" };
                parts.push(quote!(concat!("kind = \"", #kind, "\"\n")));
                for (i, field) in data.fields.iter().enumerate() {
                    let ty = &field.ty;
                    let label = field
                        .ident
                        .as_ref()
                        .map_or_else(|| i.to_string(), |f| f.unraw().to_string());
                    let entry = format!(
                        "[[type.field]]\nname = {}\ndoc = {}\ntype = \"",
                        toml_string(&label),
                        toml_string(&schema_doc(&field.attrs))
                    );
                    parts.push(quote!(#entry, <#ty as #shape>::SCHEMA_NAME, "\"\n"));
                    nested.push(quote!(<#ty as #shape>::SCHEMA_TYPES));
                }
            }
        }
        Data::Enum(data) => {
            let repr = reprs
                .iter()
                .find(|r| PRIMITIVE_REPRS.contains(&r.as_str()))
                .cloned()
                .unwrap_or_default();
            let kind = format!("kind = \"enum\"\nrepr = \"{repr}\"\n");
            parts.push(quote!(#kind));
            let repr = Ident::new(&repr, Span::call_site());
            for variant in &data.variants {
                let v = &variant.ident;
                let entry = format!(
                    "[[type.variant]]\nname = {}\ndoc = {}\nvalue = ",
                    toml_string(&v.unraw().to_string()),
                    toml_string(&schema_doc(&variant.attrs))
                );
                parts.push(quote!(#entry, #ident::#v as #repr, "\n"));
            }
        }
        Data::Union(_) => return quote!(),
    }
    quote! {
        const SCHEMA_NAME: &'static str = #name;
        const SCHEMA_TYPES: &'static str =
            ::enshwick::__private::concatcp!(#(#parts,)* #(#nested,)*);
    }
}

/// The struct as `repr(C)` with its layout trait: `major` is `Some` for a
/// service.
pub(super) fn expand_layout(
    item: &syn::ItemStruct,
    name: Option<syn::LitStr>,
    major: Option<proc_macro2::TokenStream>,
    trait_name: &proc_macro2::TokenStream,
) -> syn::Result<TokenStream> {
    let major = major.map(|major| quote!(const MAJOR: u32 = #major;));
    expand_declaration(
        item,
        name,
        &major.unwrap_or_default(),
        &quote!(services::#trait_name),
    )
}
