//! `#[forward(field)]`: the bodies of an `impl` block's methods written
//! without one, each calling the method of the same name on `self.field`
//! with the same arguments and type arguments.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, FnArg, GenericParam, Ident, ImplItem, ItemImpl, Pat, Signature, Token, Visibility,
};

/// A method written without a body: `fn f(&self);`.
struct Bodiless {
    attrs: Vec<Attribute>,
    vis: Visibility,
    sig: Signature,
}

impl Parse for Bodiless {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let sig = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self { attrs, vis, sig })
    }
}

/// `item` with each bodiless method given its forwarding body.
pub(crate) fn expand(field: &Ident, mut item: ItemImpl) -> syn::Result<TokenStream> {
    for entry in &mut item.items {
        let ImplItem::Verbatim(tokens) = entry else {
            continue;
        };
        let Bodiless { attrs, vis, sig } = syn::parse2(tokens.clone())?;
        let body = call(field, &sig)?;
        *entry = ImplItem::Verbatim(quote! {
            #(#attrs)*
            #[inline]
            #vis #sig {
                #body
            }
        });
    }
    Ok(quote!(#item))
}

/// The call on `self.field` that stands for `sig`.
fn call(field: &Ident, sig: &Signature) -> syn::Result<TokenStream> {
    if sig.receiver().is_none() {
        return Err(syn::Error::new_spanned(
            &sig.ident,
            "a forwarded method takes `self` by reference",
        ));
    }
    let name = &sig.ident;
    let types: Vec<&Ident> = sig
        .generics
        .params
        .iter()
        .filter_map(|param| match param {
            GenericParam::Type(ty) => Some(&ty.ident),
            GenericParam::Const(constant) => Some(&constant.ident),
            GenericParam::Lifetime(_) => None,
        })
        .collect();
    let turbofish = if types.is_empty() {
        quote!()
    } else {
        quote!(::<#(#types),*>)
    };
    let args = sig
        .inputs
        .iter()
        .filter_map(|input| match input {
            FnArg::Receiver(_) => None,
            FnArg::Typed(typed) => Some(typed),
        })
        .map(|typed| match &*typed.pat {
            Pat::Ident(ident) => Ok(&ident.ident),
            other => Err(syn::Error::new_spanned(
                other,
                "a forwarded method names each argument",
            )),
        })
        .collect::<syn::Result<Vec<_>>>()?;
    Ok(quote!(self.#field.#name #turbofish (#(#args),*)))
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    #[test]
    fn a_bodiless_method_calls_the_field_s_method_with_its_arguments_and_types() -> syn::Result<()>
    {
        let item: TokenStream = quote! {
            impl Load<'_> {
                /// Docs stay.
                #[script]
                pub fn on<E: Kind, const N: usize>(&mut self, mut handler: impl Fn(&E::Data)) -> Result<(), Off>;

                pub fn kept(&self) -> u32 {
                    1
                }
            }
        };
        let out = expand(&parse_quote!(base), syn::parse2(item)?)?.to_string();
        assert!(
            out.contains("# [doc = r\" Docs stay.\"] # [script] # [inline] pub fn on"),
            "{out}"
        );
        assert!(
            out.contains("self . base . on :: < E , N > (handler)"),
            "{out}"
        );
        assert!(out.contains("pub fn kept (& self) -> u32 { 1 }"), "{out}");
        Ok(())
    }

    #[test]
    fn a_forwarded_method_without_self_or_with_a_pattern_is_refused() -> syn::Result<()> {
        let free: ItemImpl = syn::parse2(quote!(impl Load { fn new() -> Self; }))?;
        assert!(expand(&parse_quote!(base), free).is_err());
        let pattern: ItemImpl =
            syn::parse2(quote!(impl Load { fn pair(&self, (a, b): (u8, u8)); }))?;
        assert!(expand(&parse_quote!(base), pattern).is_err());
        Ok(())
    }
}
