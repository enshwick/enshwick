//! `#[derive(ErrorKind)]`: the kind a script reads from an error, the name
//! of its variant or of its type.

use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::ext::IdentExt;
use syn::{Data, DeriveInput, LitStr};

/// The kind `ident` names: its name without a raw `r#`.
fn kind(ident: &Ident) -> LitStr {
    LitStr::new(&ident.unraw().to_string(), ident.span())
}

/// The `ErrorKind` impl of `input`, under the API crate at `krate`.
pub(crate) fn expand(input: &DeriveInput, krate: &TokenStream) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let body = match &input.data {
        Data::Enum(data) => {
            let arms = data.variants.iter().map(|variant| {
                let ident = &variant.ident;
                let kind = kind(ident);
                quote!(Self::#ident { .. } => #kind)
            });
            // On the place, not the reference: an enum without variants
            // matches no arm only as a value.
            quote!(match *self { #(#arms,)* })
        }
        Data::Struct(_) => {
            let kind = kind(name);
            quote!(#kind)
        }
        Data::Union(_) => {
            return Err(syn::Error::new(
                name.span(),
                "an error's kind is derived for an enum or a struct",
            ));
        }
    };
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #krate::script::ErrorKind for #name #ty_generics #where_clause {
            fn kind(&self) -> &'static str {
                #body
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expanded(input: &DeriveInput) -> syn::Result<String> {
        Ok(expand(input, &quote!(::enshwick))?.to_string())
    }

    #[test]
    fn every_variant_shape_answers_its_own_name() -> syn::Result<()> {
        let code = expanded(&syn::parse_quote! {
            enum PlantError { NoSeed, TooCold(f32), Blocked { by: u32 } }
        })?;
        for arm in [
            quote!(Self::NoSeed { .. } => "NoSeed"),
            quote!(Self::TooCold { .. } => "TooCold"),
            quote!(Self::Blocked { .. } => "Blocked"),
        ] {
            assert!(code.contains(&arm.to_string()), "{arm} missing from {code}");
        }
        Ok(())
    }

    #[test]
    fn a_struct_answers_its_type_name_and_keeps_its_generics() -> syn::Result<()> {
        let code = expanded(&syn::parse_quote! {
            struct Refused<T: Copy> { why: T }
        })?;
        let head = quote!(impl<T: Copy> ::enshwick::script::ErrorKind for Refused<T>);
        assert!(code.starts_with(&head.to_string()), "{code}");
        assert!(code.contains("\"Refused\""), "{code}");
        Ok(())
    }

    #[test]
    fn a_union_has_no_kind() {
        let union: DeriveInput = syn::parse_quote!(union Bits { a: u32, b: f32 });
        assert!(expand(&union, &quote!(::enshwick)).is_err());
    }
}
