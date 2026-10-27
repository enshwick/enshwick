use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ext::IdentExt as _;
use syn::spanned::Spanned as _;
use syn::{
    Data, DeriveInput, Fields, FieldsNamed, FnArg, GenericArgument, Ident, ImplItem, ImplItemFn,
    ItemImpl, Pat, PatType, PathArguments, ReturnType, Type, TypeParamBound, Visibility,
};

use crate::schema_doc;

/// Whether `field` is left out of a script's view: private, named with a
/// leading underscore or `reserved`, or an ABI struct's `header`.
fn hidden(field: &syn::Field) -> bool {
    let private = !matches!(field.vis, Visibility::Public(_));
    let underscored = field.ident.as_ref().is_some_and(|i| {
        let name = i.to_string();
        name.starts_with('_') || name.starts_with("reserved")
    });
    let header = matches!(&field.ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Header"));
    private || underscored || header
}

pub(crate) fn expand_derive(input: &DeriveInput) -> syn::Result<TokenStream> {
    let ident = &input.ident;
    match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => Ok(derive_record(input, named)),
            Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => {
                Ok(derive_newtype(ident, &unnamed.unnamed[0].ty))
            }
            _ => Err(syn::Error::new_spanned(
                ident,
                "`Script` takes a struct with named fields or a newtype",
            )),
        },
        Data::Enum(data) => derive_choice(input, data),
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "`Script` takes a struct or an enum",
        )),
    }
}

fn derive_record(input: &DeriveInput, named: &FieldsNamed) -> TokenStream {
    let script = quote!(::enshwick::script);
    let ident = &input.ident;
    let name = ident.to_string();
    let doc = schema_doc(&input.attrs);
    let shown: Vec<&syn::Field> = named.named.iter().filter(|f| !hidden(f)).collect();
    let entries = shown.iter().map(|f| {
        let field = f
            .ident
            .as_ref()
            .map(|i| i.unraw().to_string())
            .unwrap_or_default();
        let doc = schema_doc(&f.attrs);
        let ty = &f.ty;
        quote!(#script::Field::new(#field, #doc, <#ty as #script::Typed>::TY))
    });
    let values = shown.iter().map(|f| {
        let field = &f.ident;
        quote!(#script::IntoScript::into_value(self.#field))
    });
    let from = (shown.len() == named.named.len()).then(|| {
        let count = shown.len();
        let fields = shown.iter().map(|f| {
            let field = &f.ident;
            let label = field.as_ref().map(|i| i.unraw().to_string()).unwrap_or_default();
            quote!(#field: ::enshwick::__private::argument(&mut fields, #label)?)
        });
        quote! {
            impl #script::FromScript for #ident {
                fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                    let mut fields = ::enshwick::__private::record_fields(value, #count)?;
                    ::core::result::Result::Ok(Self { #(#fields,)* })
                }
            }
        }
    });
    quote! {
        impl #script::Typed for #ident {
            const TY: &'static #script::Ty = &#script::Ty::Record(&#script::Record::new(
                #name,
                #doc,
                &[#(#entries),*],
            ));
        }

        impl #script::IntoScript for #ident {
            fn into_value(self) -> #script::Value {
                #script::Value::Record(::std::vec![#(#values),*])
            }
        }

        #from
    }
}

fn derive_newtype(ident: &Ident, inner: &Type) -> TokenStream {
    let script = quote!(::enshwick::script);
    quote! {
        impl #script::Typed for #ident {
            const TY: &'static #script::Ty = <#inner as #script::Typed>::TY;
        }

        impl #script::IntoScript for #ident {
            fn into_value(self) -> #script::Value {
                #script::IntoScript::into_value(self.0)
            }
        }

        impl #script::FromScript for #ident {
            fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                <#inner as #script::FromScript>::from_value(value).map(Self)
            }
        }
    }
}

fn derive_choice(input: &DeriveInput, data: &syn::DataEnum) -> syn::Result<TokenStream> {
    let script = quote!(::enshwick::script);
    let ident = &input.ident;
    let name = ident.to_string();
    let doc = schema_doc(&input.attrs);
    let mut values = Vec::new();
    let mut variants = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                variant,
                "`Script` takes an enum of unit variants",
            ));
        }
        let label = variant.ident.to_string();
        let variant_doc = schema_doc(&variant.attrs);
        values.push(quote!((#label, #variant_doc)));
        variants.push((&variant.ident, label));
    }
    let into = variants.iter().map(|(v, label)| quote!(Self::#v => #label));
    let from = variants.iter().map(|(v, label)| quote!(#label => Self::#v));
    let names = variants
        .iter()
        .map(|(_, label)| format!("{label:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(quote! {
        impl #script::Typed for #ident {
            const TY: &'static #script::Ty = &#script::Ty::Choice(&#script::Choice::new(
                #name,
                #doc,
                &[#(#values),*],
            ));
        }

        impl #script::IntoScript for #ident {
            fn into_value(self) -> #script::Value {
                #script::Value::String(::std::string::String::from(match self { #(#into,)* }))
            }
        }

        impl #script::FromScript for #ident {
            fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                let text = <::std::string::String as #script::FromScript>::from_value(value)?;
                ::core::result::Result::Ok(match text.as_str() {
                    #(#from,)*
                    _ => return ::core::result::Result::Err(#script::Error::new(
                        ::std::format!("{text:?} is not one of {}", #names),
                    )),
                })
            }
        }
    })
}

/// The `Fn`, `FnMut` or `FnOnce` bound of an `impl Trait` parameter: its
/// argument types and its output, `()` when it has none.
fn closure_bound(ty: &Type) -> Option<(Vec<Type>, Type)> {
    let Type::ImplTrait(imp) = ty else {
        return None;
    };
    imp.bounds.iter().find_map(|bound| {
        let TypeParamBound::Trait(t) = bound else {
            return None;
        };
        let last = t.path.segments.last()?;
        if !matches!(last.ident.to_string().as_str(), "Fn" | "FnMut" | "FnOnce") {
            return None;
        }
        let PathArguments::Parenthesized(args) = &last.arguments else {
            return None;
        };
        let output = match &args.output {
            ReturnType::Default => syn::parse_quote!(()),
            ReturnType::Type(_, ty) => (**ty).clone(),
        };
        Some((args.inputs.iter().map(|a| a.ty.clone()).collect(), output))
    })
}

/// The `T` of `Result<T, E>`, or `None` for a type that is not a `Result`.
fn ok_type(ty: &Type) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let last = p.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    args.args.iter().find_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    })
}

/// One parameter of a `#[script]` method: its description, the statement
/// that reads it from the arguments as `local`, and the expression passed to
/// the method.
struct Parameter {
    describe: TokenStream,
    read: TokenStream,
    pass: TokenStream,
}

fn parameter(index: usize, typed: &PatType) -> syn::Result<Parameter> {
    let script = quote!(::enshwick::script);
    let param = match &*typed.pat {
        Pat::Ident(p) => p.ident.to_string(),
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "a `#[script]` method's parameters are plain names",
            ));
        }
    };
    let local = format_ident!("arg{index}");
    let ty = &*typed.ty;
    if let Some((inputs, output)) = closure_bound(ty) {
        let names: Vec<Ident> = (0..inputs.len()).map(|n| format_ident!("p{n}")).collect();
        // A closure that lends its argument, `&T`, hands the script a copy.
        let owned: Vec<Type> = inputs
            .iter()
            .map(|input| match input {
                Type::Reference(r) => (*r.elem).clone(),
                other => other.clone(),
            })
            .collect();
        let values: Vec<TokenStream> = inputs
            .iter()
            .zip(&names)
            .map(|(input, name)| {
                let owned = if matches!(input, Type::Reference(_)) {
                    quote!(::core::clone::Clone::clone(#name))
                } else {
                    quote!(#name)
                };
                quote!(#script::IntoScript::into_value(#owned))
            })
            .collect();
        return Ok(Parameter {
            describe: quote! {
                #script::Param::new(#param, &#script::Ty::Function(&#script::Signature::new(
                    &[#(#script::Param::new("", <#owned as #script::Typed>::TY)),*],
                    <#output as #script::Answer>::TY,
                )))
            },
            read: quote! {
                let #local = {
                    let callback: #script::Callback = ::enshwick::__private::argument(&mut args, #param)?;
                    move |#(#names: #inputs),*| -> #output {
                        <#output as #script::Answer>::from_answer(
                            callback.call(::std::vec![#(#values),*]),
                        )
                    }
                };
            },
            pass: quote!(#local),
        });
    }
    let (owned, pass): (Type, TokenStream) = match ty {
        Type::Reference(r) if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")) => {
            (syn::parse_quote!(::std::string::String), quote!(&#local))
        }
        Type::Reference(r) => ((*r.elem).clone(), quote!(&#local)),
        other => (other.clone(), quote!(#local)),
    };
    Ok(Parameter {
        describe: quote!(#script::Param::new(#param, <#owned as #script::Typed>::TY)),
        read: quote!(let #local: #owned = ::enshwick::__private::argument(&mut args, #param)?;),
        pass,
    })
}

/// The answer type's description and the body that calls the method and
/// converts what it returns.
fn answer(method: &ImplItemFn, passes: &[TokenStream]) -> syn::Result<(TokenStream, TokenStream)> {
    let script = quote!(::enshwick::script);
    let name = &method.sig.ident;
    let call = quote!(this.#name(#(#passes),*));
    Ok(match &method.sig.output {
        ReturnType::Default => (
            quote!(&#script::Ty::Nil),
            quote! {
                #call;
                ::core::result::Result::Ok(#script::Value::Nil)
            },
        ),
        ReturnType::Type(_, ty) => match ok_type(ty) {
            Some(ok) => (
                quote!(<#ok as #script::Typed>::TY),
                quote! {
                    match #call {
                        ::core::result::Result::Ok(value) => ::core::result::Result::Ok(#script::IntoScript::into_value(value)),
                        ::core::result::Result::Err(error) => ::core::result::Result::Err(#script::Error::new(::std::string::ToString::to_string(&error))),
                    }
                },
            ),
            None if matches!(&**ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Result")) =>
            {
                return Err(syn::Error::new(
                    ty.span(),
                    "a `#[script]` method's `Result` names its `Ok` type",
                ));
            }
            None => (
                quote!(<#ty as #script::Typed>::TY),
                quote!(::core::result::Result::Ok(#script::IntoScript::into_value(#call))),
            ),
        },
    })
}

/// The `Function` of one `#[script]` method of `self_ty`.
fn function(method: &ImplItemFn, self_ty: &Type) -> syn::Result<TokenStream> {
    let script = quote!(::enshwick::script);
    let label = method.sig.ident.to_string();
    let doc = schema_doc(&method.attrs);
    if doc.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.ident,
            "a `#[script]` method needs a doc comment: scripts' editors show it",
        ));
    }
    if !matches!(method.sig.inputs.first(), Some(FnArg::Receiver(_))) {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "a `#[script]` method takes `self` or `&self`",
        ));
    }
    let parameters = method
        .sig
        .inputs
        .iter()
        .skip(1)
        .filter_map(|input| match input {
            FnArg::Typed(typed) => Some(typed),
            FnArg::Receiver(_) => None,
        })
        .enumerate()
        .map(|(i, typed)| parameter(i, typed))
        .collect::<syn::Result<Vec<_>>>()?;
    let describes = parameters.iter().map(|p| &p.describe);
    let reads = parameters.iter().map(|p| &p.read);
    let passes: Vec<TokenStream> = parameters.iter().map(|p| p.pass.clone()).collect();
    let (ret, body) = answer(method, &passes)?;
    let take = if parameters.is_empty() {
        quote!(let _no_arguments = args;)
    } else {
        quote!(let mut args = args.into_iter();)
    };
    Ok(quote! {
        #script::Function::new(
            #label,
            #doc,
            #script::Signature::new(&[#(#describes),*], #ret),
            |this: &#self_ty, args: ::std::vec::Vec<#script::Value>| {
                #take
                #(#reads)*
                #body
            },
        )
    })
}

pub(crate) fn expand_impl(part: &Ident, mut item: ItemImpl) -> syn::Result<TokenStream> {
    let script = quote!(::enshwick::script);
    let self_ty = item.self_ty.clone();
    let constant = format_ident!("SCRIPT_{}", part.to_string().to_uppercase());
    let mut functions = Vec::new();
    for entry in &mut item.items {
        let ImplItem::Fn(method) = entry else {
            continue;
        };
        let before = method.attrs.len();
        method.attrs.retain(|a| !a.path().is_ident("script"));
        if method.attrs.len() != before {
            functions.push(function(method, &self_ty)?);
        }
    }
    if functions.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "mark the methods scripts may call with `#[script]`",
        ));
    }
    let generics = item.generics.clone();
    Ok(quote! {
        #item

        impl #generics #self_ty {
            #[doc(hidden)]
            pub const #constant: &'static [#script::Function<Self>] = &[#(#functions),*];
        }
    })
}
