//! `#[forward(field)]`: the bodies of an `impl` block's methods written
//! without one, each calling the method of the same name on `self.field`
//! with the same arguments and type arguments.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, FnArg, GenericParam, Ident, ImplItem, ItemImpl, Pat, ReceiverKind, Safety,
    Signature, Token, Visibility,
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

pub(crate) fn expand(field: &Ident, mut item: ItemImpl) -> syn::Result<TokenStream> {
    if let Some(hidden) = item.attrs.iter().find(|attr| is_doc_hidden(attr)) {
        return Err(syn::Error::new_spanned(
            hidden,
            "forwarders are the documented methods; a hidden block hides them all",
        ));
    }
    for entry in &mut item.items {
        let ImplItem::Verbatim(tokens) = entry else {
            continue;
        };
        let Bodiless {
            attrs,
            vis,
            mut sig,
        } = syn::parse2(tokens.clone())?;
        // The arguments only pass through, so a `mut` binding would be unused.
        for input in &mut sig.inputs {
            if let FnArg::Typed(arg) = input
                && let Pat::Ident(ident) = &mut *arg.pat
            {
                ident.mutability = None;
            }
        }
        let call = call(field, &sig)?;
        // An `unsafe fn` forwards to the method whose contract its caller
        // upholds: the forwarder carries that method's `# Safety` docs.
        let body = if matches!(sig.safety, Safety::Unsafe(_)) {
            quote!(unsafe { #call })
        } else {
            call
        };
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

/// Whether `attr` is `#[doc(hidden)]`.
fn is_doc_hidden(attr: &Attribute) -> bool {
    attr.path().is_ident("doc")
        && attr
            .parse_args::<Ident>()
            .is_ok_and(|word| word == "hidden")
}

/// The call on `self.field` that stands for `sig`.
fn call(field: &Ident, sig: &Signature) -> syn::Result<TokenStream> {
    if !sig
        .receiver()
        .is_some_and(|receiver| matches!(receiver.kind, ReceiverKind::Reference(..)))
    {
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
            FnArg::Typed(arg) => Some(arg),
        })
        .map(|arg| match &*arg.pat {
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
            /// A block's own docs and aliases are no hiding.
            #[doc(alias = "loader")]
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
        assert!(
            !out.contains("mut handler"),
            "a passed argument binds without mut: {out}"
        );
        Ok(())
    }

    #[test]
    fn an_unsafe_method_calls_the_field_s_method_in_an_unsafe_block() -> syn::Result<()> {
        let item: ItemImpl = syn::parse2(quote! {
            impl Load<'_> {
                pub unsafe fn hook(&mut self, target: u64) -> Result<(), Off>;
            }
        })?;
        let out = expand(&parse_quote!(base), item)?.to_string();
        assert!(
            out.contains("pub unsafe fn hook (& mut self , target : u64) -> Result < () , Off > { unsafe { self . base . hook (target) } }"),
            "{out}"
        );
        Ok(())
    }

    #[test]
    fn a_hidden_block_an_unborrowed_self_or_an_argument_pattern_is_refused() -> syn::Result<()> {
        let free: ItemImpl = syn::parse2(quote!(impl Load { fn new() -> Self; }))?;
        assert!(expand(&parse_quote!(base), free).is_err());
        let pattern: ItemImpl =
            syn::parse2(quote!(impl Load { fn pair(&self, (a, b): (u8, u8)); }))?;
        assert!(expand(&parse_quote!(base), pattern).is_err());
        let hidden: ItemImpl = syn::parse2(quote!(
            #[doc(hidden)]
            impl Load {
                fn f(&self);
            }
        ))?;
        assert!(expand(&parse_quote!(base), hidden).is_err());
        let owned: ItemImpl = syn::parse2(quote!(impl Load { fn into_base(self) -> Base; }))?;
        assert!(expand(&parse_quote!(base), owned).is_err());
        Ok(())
    }
}
