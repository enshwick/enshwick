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
        Data::Enum(data)
            if data
                .variants
                .iter()
                .all(|v| matches!(v.fields, Fields::Unit)) =>
        {
            derive_choice(input, data)
        }
        Data::Enum(data) => derive_union(input, data),
        Data::Union(_) => Err(syn::Error::new_spanned(
            ident,
            "`Script` takes a struct or an enum",
        )),
    }
}

/// The type's name in a script: `#[script(name = "...")]` on the type,
/// for a type whose Rust name another module's type has too, else its
/// Rust name.
fn type_name(input: &DeriveInput) -> String {
    let mut name = input.ident.to_string();
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("script")) {
        // Other arguments, such as `context`, are read where they act.
        let _read = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                name = meta.value()?.parse::<syn::LitStr>()?.value();
                Ok(())
            } else {
                crate::krate::skip(&meta)
            }
        });
    }
    name
}

fn derive_record(input: &DeriveInput, named: &FieldsNamed) -> TokenStream {
    let script = quote!(#krate::script);
    let ident = &input.ident;
    let name = type_name(input);
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
        let new = if defaulted(f) {
            quote!(defaulted)
        } else {
            quote!(new)
        };
        quote!(#script::Field::#new(#field, #doc, <#ty as #script::Typed>::TY))
    });
    let values = shown.iter().map(|f| {
        let field = &f.ident;
        quote!(#script::IntoScript::into_value(self.#field))
    });
    // `#[script(context)]`: a hook's `&mut` argument a handler gets as a
    // value and changes by returning the changed value.
    let context = has_word(&input.attrs, "context");
    let context = context.then(|| {
        quote! {
            impl #script::Context for #ident {
                const VIEW: &'static #script::Ty = <Self as #script::Typed>::TY;
                const CHANGE: &'static #script::Ty = <Self as #script::Typed>::TY;

                fn view(&self) -> #script::Value {
                    #script::IntoScript::into_value(::core::clone::Clone::clone(self))
                }

                fn apply(&mut self, change: #script::Value) -> ::core::result::Result<(), #script::Error> {
                    *self = #script::FromScript::from_value(change)?;
                    ::core::result::Result::Ok(())
                }
            }
        }
    });
    let from = (shown.len() == named.named.len()).then(|| {
        let count = shown.len();
        let fields = shown.iter().map(|f| {
            let field = &f.ident;
            let label = field.as_ref().map(|i| i.unraw().to_string()).unwrap_or_default();
            let read = if defaulted(f) {
                quote!(argument_or_default)
            } else {
                quote!(argument)
            };
            quote!(#field: #krate::__private::#read(&mut fields, #label)?)
        });
        quote! {
            impl #script::FromScript for #ident {
                fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                    let mut fields = #krate::__private::record_fields(value, #count)?;
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

        #context
    }
}

/// Whether `#[script(default)]` marks `field`: a script may leave it out.
fn defaulted(field: &syn::Field) -> bool {
    has_word(&field.attrs, "default")
}

/// Whether a `#[script(..)]` of `attrs` holds the bare `word`, beside any
/// other arguments.
fn has_word(attrs: &[syn::Attribute], word: &str) -> bool {
    let mut found = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("script")) {
        let _read = attr.parse_nested_meta(|meta| {
            found |= meta.path.is_ident(word);
            crate::krate::skip(&meta)
        });
    }
    found
}

fn derive_newtype(ident: &Ident, inner: &Type) -> TokenStream {
    let script = quote!(#krate::script);
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
    let script = quote!(#krate::script);
    let ident = &input.ident;
    let name = type_name(input);
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
        let cfg: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();
        values.push(quote!(#(#cfg)* (#label, #variant_doc)));
        variants.push((&variant.ident, label, cfg));
    }
    let into = variants
        .iter()
        .map(|(v, label, cfg)| quote!(#(#cfg)* Self::#v => #label));
    let from = variants
        .iter()
        .map(|(v, label, cfg)| quote!(#(#cfg)* #label => Self::#v));
    let names = variants
        .iter()
        .map(|(_, label, _)| format!("{label:?}"))
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
                #script::Value::Name(match self { #(#into,)* })
            }
        }

        impl #script::FromScript for #ident {
            fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                let owned;
                let text: &str = match &value {
                    #script::Value::Name(name) => name,
                    #script::Value::String(text) => text,
                    _ => {
                        owned = <::std::string::String as #script::FromScript>::from_value(value)?;
                        &owned
                    }
                };
                ::core::result::Result::Ok(match text {
                    #(#from,)*
                    _ => return ::core::result::Result::Err(#script::Error::new(
                        ::std::format!("{text:?} is not one of {}", #names),
                    )),
                })
            }
        }
    })
}

/// An enum whose variants carry one value or none, as a `Union`: a unit
/// variant crosses as its name, one with a value as a table of one key.
#[expect(
    clippy::too_many_lines,
    reason = "one pass over the variants builds the description and both conversions together"
)]
fn derive_union(input: &DeriveInput, data: &syn::DataEnum) -> syn::Result<TokenStream> {
    let script = quote!(#krate::script);
    let ident = &input.ident;
    let name = type_name(input);
    let doc = schema_doc(&input.attrs);
    let mut described = Vec::new();
    let mut into = Vec::new();
    let mut from_name = Vec::new();
    let mut from_table = Vec::new();
    for variant in &data.variants {
        let v = &variant.ident;
        let label = v.to_string();
        let variant_doc = schema_doc(&variant.attrs);
        let cfg: Vec<&syn::Attribute> = variant
            .attrs
            .iter()
            .filter(|a| a.path().is_ident("cfg"))
            .collect();
        match &variant.fields {
            Fields::Unit => {
                described.push(quote!(#(#cfg)* #script::Variant::new(#label, #variant_doc, ::core::option::Option::None)));
                into.push(quote!(#(#cfg)* Self::#v => #script::Value::Name(#label)));
                from_name.push(quote!(#(#cfg)* #label => ::core::result::Result::Ok(Self::#v)));
            }
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let ty = &fields.unnamed[0].ty;
                described.push(quote!(#(#cfg)* #script::Variant::new(
                    #label,
                    #variant_doc,
                    ::core::option::Option::Some(<#ty as #script::Typed>::TY),
                )));
                into.push(
                    quote!(#(#cfg)* Self::#v(value) => #script::Value::Map(::std::vec![(
                        ::std::string::String::from(#label),
                        #script::IntoScript::into_value(value),
                    )])),
                );
                from_table.push(quote!(#(#cfg)* #label => ::core::result::Result::Ok(Self::#v(
                    #script::FromScript::from_value(value).map_err(|e| #script::Error::new(::std::format!("{}: {e}", #label)))?,
                ))));
            }
            Fields::Named(fields) => {
                let record = format!("{name}{label}");
                let names: Vec<&Ident> = fields
                    .named
                    .iter()
                    .filter_map(|f| f.ident.as_ref())
                    .collect();
                let labels: Vec<String> = names.iter().map(|i| i.unraw().to_string()).collect();
                let docs = fields.named.iter().map(|f| schema_doc(&f.attrs));
                let tys = fields.named.iter().map(|f| &f.ty);
                let count = names.len();
                described.push(quote!(#(#cfg)* #script::Variant::new(
                    #label,
                    #variant_doc,
                    ::core::option::Option::Some(&#script::Ty::Record(&#script::Record::new(
                        #record,
                        #variant_doc,
                        &[#(#script::Field::new(#labels, #docs, <#tys as #script::Typed>::TY)),*],
                    ))),
                )));
                into.push(quote!(#(#cfg)* Self::#v { #(#names),* } => #script::Value::Map(::std::vec![(
                    ::std::string::String::from(#label),
                    #script::Value::Record(::std::vec![#(#script::IntoScript::into_value(#names)),*]),
                )])));
                from_table.push(quote!(#(#cfg)* #label => {
                    let mut fields = #krate::__private::record_fields(value, #count)
                        .map_err(|e| #script::Error::new(::std::format!("{}: {e}", #label)))?;
                    ::core::result::Result::Ok(Self::#v {
                        #(#names: #krate::__private::argument(&mut fields, #labels)?,)*
                    })
                }));
            }
            Fields::Unnamed(_) => {
                return Err(syn::Error::new_spanned(
                    variant,
                    "`Script` takes an enum whose variants carry one value, named fields or none",
                ));
            }
        }
    }
    Ok(quote! {
        impl #script::Typed for #ident {
            const TY: &'static #script::Ty = &#script::Ty::Union(&#script::Union::new(
                #name,
                #doc,
                &[#(#described),*],
            ));
        }

        impl #script::IntoScript for #ident {
            fn into_value(self) -> #script::Value {
                match self { #(#into,)* }
            }
        }

        impl #script::FromScript for #ident {
            fn from_value(value: #script::Value) -> ::core::result::Result<Self, #script::Error> {
                let named = match &value {
                    #script::Value::String(text) => ::core::option::Option::Some(text.as_str()),
                    #script::Value::Name(text) => ::core::option::Option::Some(*text),
                    _ => ::core::option::Option::None,
                };
                if let ::core::option::Option::Some(text) = named {
                    return match text {
                        #(#from_name,)*
                        _ => ::core::result::Result::Err(#script::Error::new(::std::format!(
                            "{text:?} is no variant of {} without a value", #name
                        ))),
                    };
                }
                match value {
                    #script::Value::Map(mut entries) if entries.len() == 1 => {
                        let (key, value) = entries.remove(0);
                        match key.as_str() {
                            #(#from_table,)*
                            _ => ::core::result::Result::Err(#script::Error::new(::std::format!(
                                "{key:?} is no variant of {} with a value", #name
                            ))),
                        }
                    }
                    _ => ::core::result::Result::Err(#script::Error::new(::std::format!(
                        "expected a {}: a variant's name, or a table of one variant and its value", #name
                    ))),
                }
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

fn parameter(index: usize, typed: &PatType, generics: &syn::Generics) -> syn::Result<Parameter> {
    let script = quote!(#krate::script);
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
    if let Some((inputs, output)) = closure_bound(ty).or_else(|| generic_closure(ty, generics)) {
        return closure(&param, &local, &inputs, &output);
    }
    let (owned, pass): (Type, TokenStream) = match ty {
        Type::Reference(r) if matches!(&*r.elem, Type::Path(p) if p.path.is_ident("str")) => {
            (syn::parse_quote!(::std::string::String), quote!(&#local))
        }
        Type::Reference(r) if matches!(&*r.elem, Type::Slice(_)) => {
            let Type::Slice(slice) = &*r.elem else {
                unreachable!("matched as a slice")
            };
            let elem = &slice.elem;
            (syn::parse_quote!(::std::vec::Vec<#elem>), quote!(&#local))
        }
        Type::Reference(r) => ((*r.elem).clone(), quote!(&#local)),
        other => (other.clone(), quote!(#local)),
    };
    Ok(Parameter {
        describe: quote!(#script::Param::new(#param, <#owned as #script::Typed>::TY)),
        read: quote!(let #local: #owned = #krate::__private::argument(&mut args, #param)?;),
        pass,
    })
}

/// The closure bound of `ty` when it names a type parameter of `generics`
/// bounded by `Fn`, inline or in the `where` clause.
fn generic_closure(ty: &Type, generics: &syn::Generics) -> Option<(Vec<Type>, Type)> {
    let Type::Path(path) = ty else { return None };
    let ident = path.path.get_ident()?;
    let inline = generics
        .type_params()
        .filter(|p| p.ident == *ident)
        .flat_map(|p| p.bounds.iter());
    let clause = generics
        .where_clause
        .iter()
        .flat_map(|w| w.predicates.iter())
        .filter_map(|p| match p {
            syn::WherePredicate::Type(t) if matches!(&t.bounded_ty, Type::Path(b) if b.path.is_ident(ident)) => {
                Some(t.bounds.iter())
            }
            _ => None,
        })
        .flatten();
    let bounds = inline.chain(clause).cloned().collect();
    closure_bound(&Type::ImplTrait(syn::TypeImplTrait {
        attrs: Vec::new(),
        impl_token: syn::token::Impl::default(),
        bounds,
    }))
}

/// A closure parameter: the script passes a function, called with each
/// argument as a value. A `&T` argument hands the script a copy; a `&mut T`
/// argument hands it `T`'s [`Context`] view, and what the function returns
/// besides nil is applied back to it.
fn closure(param: &str, local: &Ident, inputs: &[Type], output: &Type) -> syn::Result<Parameter> {
    let script = quote!(#krate::script);
    let names: Vec<Ident> = (0..inputs.len()).map(|n| format_ident!("p{n}")).collect();
    let context = inputs
        .iter()
        .position(|input| matches!(input, Type::Reference(r) if r.mutability.is_some()));
    let described: Vec<TokenStream> = inputs
        .iter()
        .map(|input| match input {
            Type::Reference(r) if r.mutability.is_some() => {
                let elem = &r.elem;
                quote!(<#elem as #script::Context>::VIEW)
            }
            Type::Reference(r) => {
                let elem = &r.elem;
                quote!(<#elem as #script::Typed>::TY)
            }
            other => quote!(<#other as #script::Typed>::TY),
        })
        .collect();
    let values: Vec<TokenStream> = inputs
        .iter()
        .zip(&names)
        .map(|(input, name)| match input {
            Type::Reference(r) if r.mutability.is_some() => quote!(#script::Context::view(&*#name)),
            Type::Reference(_) => {
                quote!(#script::IntoScript::into_value(::core::clone::Clone::clone(#name)))
            }
            _ => quote!(#script::IntoScript::into_value(#name)),
        })
        .collect();
    let (ret, body) = match context {
        Some(at) => {
            if !matches!(output, Type::Tuple(t) if t.elems.is_empty()) {
                return Err(syn::Error::new_spanned(
                    output,
                    "a `#[script]` closure with a `&mut` argument returns nothing: the script's answer changes the argument",
                ));
            }
            let Type::Reference(r) = &inputs[at] else {
                unreachable!("the position is of a reference")
            };
            let elem = &r.elem;
            let name = &names[at];
            (
                quote!(&#script::Ty::Optional(<#elem as #script::Context>::CHANGE)),
                quote! {
                    match callback.call(&mut [#(#values),*]) {
                        ::core::result::Result::Ok(#script::Value::Nil) | ::core::result::Result::Err(_) => {}
                        ::core::result::Result::Ok(change) => {
                            if let ::core::result::Result::Err(error) = #script::Context::apply(#name, change) {
                                callback.fault(&error);
                            }
                        }
                    }
                },
            )
        }
        None => (
            quote!(<#output as #script::Answer>::TY),
            quote! {
                <#output as #script::Answer>::from_answer(
                    callback.call(&mut [#(#values),*]),
                )
            },
        ),
    };
    Ok(Parameter {
        describe: quote! {
            #script::Param::new(#param, &#script::Ty::Function(&#script::Signature::new(
                &[#(#script::Param::new("", #described)),*],
                #ret,
            )))
        },
        read: quote! {
            let #local = {
                let callback: #script::Callback = #krate::__private::argument(&mut args, #param)?;
                move |#(#names: #inputs),*| -> #output { #body }
            };
        },
        pass: quote!(#local),
    })
}

/// The answer type's description and the body that calls the method and
/// converts what it returns.
fn answer(method: &ImplItemFn, passes: &[TokenStream]) -> syn::Result<(TokenStream, TokenStream)> {
    let script = quote!(#krate::script);
    let name = &method.sig.ident;
    // A method that takes `self` by value runs on a copy of the script's
    // object, as a builder does: `tx:place_prop(...)` returns the next one.
    let by_value = matches!(method.sig.inputs.first(), Some(FnArg::Receiver(r)) if matches!(r.kind, syn::ReceiverKind::Value));
    let call = if by_value {
        quote!(#krate::__private::owned(this).#name(#(#passes),*))
    } else {
        quote!(this.#name(#(#passes),*))
    };
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
            // A borrowed answer reaches the script as its own copy.
            None => match &**ty {
                Type::Reference(r) => match &*r.elem {
                    Type::Slice(slice) => {
                        let elem = &slice.elem;
                        (
                            quote!(<::std::vec::Vec<#elem> as #script::Typed>::TY),
                            quote!(::core::result::Result::Ok(#script::IntoScript::into_value(<[#elem]>::to_vec(#call)))),
                        )
                    }
                    elem => (
                        quote!(<#elem as #script::Typed>::TY),
                        quote!(::core::result::Result::Ok(#script::IntoScript::into_value(::core::clone::Clone::clone(#call)))),
                    ),
                },
                _ => (
                    quote!(<#ty as #script::Typed>::TY),
                    quote!(::core::result::Result::Ok(#script::IntoScript::into_value(#call))),
                ),
            },
        },
    })
}

/// The `Function` of one `#[script]` method of `self_ty`, or its
/// `InitFunction` when `self_ty` is `Load`.
fn function(
    method: &ImplItemFn,
    self_ty: &Type,
    starting: bool,
    name: Option<String>,
) -> syn::Result<TokenStream> {
    let script = quote!(#krate::script);
    let label = name.unwrap_or_else(|| method.sig.ident.to_string());
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
        .map(|(i, typed)| parameter(i, typed, &method.sig.generics))
        .collect::<syn::Result<Vec<_>>>()?;
    let describes = parameters.iter().map(|p| &p.describe);
    let reads = parameters.iter().map(|p| &p.read);
    let passes: Vec<TokenStream> = parameters.iter().map(|p| p.pass.clone()).collect();
    let (ret, body) = answer(method, &passes)?;
    let take = if parameters.is_empty() {
        quote!(let _no_arguments = args;)
    } else {
        quote!(let mut args = args.iter_mut().map(|value| ::core::mem::replace(value, #script::Value::Nil));)
    };
    let (kind, this) = if starting {
        (quote!(InitFunction), quote!(&mut #krate::Load<'_>))
    } else {
        (quote!(Function), quote!(&#self_ty))
    };
    Ok(quote! {
        #script::#kind::new(
            #label,
            #doc,
            #script::Signature::new(&[#(#describes),*], #ret),
            |this: #this, args: &mut [#script::Value]| {
                #take
                #(#reads)*
                #body
            },
        )
    })
}

pub(crate) fn expand_impl(part: &Ident, mut item: ItemImpl) -> syn::Result<TokenStream> {
    let script = quote!(#krate::script);
    let self_ty = item.self_ty.clone();
    // `impl Load` methods run while the mod starts, as in a Rust mod's init.
    let starting = matches!(&*self_ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Load"));
    let constant = format_ident!("SCRIPT_{}", part.to_string().to_uppercase());
    let mut functions = Vec::new();
    for entry in &mut item.items {
        let ImplItem::Fn(method) = entry else {
            continue;
        };
        let Some(at) = method
            .attrs
            .iter()
            .position(|a| a.path().is_ident("script"))
        else {
            continue;
        };
        let attr = method.attrs.remove(at);
        // `#[script(name = "...")]` gives a crate's adapter the name of the
        // Rust method it stands for.
        let name = match &attr.meta {
            syn::Meta::List(_) => {
                let pair: syn::MetaNameValue = attr.parse_args()?;
                let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(name),
                    ..
                }) = pair.value
                else {
                    return Err(syn::Error::new_spanned(
                        pair,
                        "`#[script(name = \"...\")]` takes a string",
                    ));
                };
                Some(name.value())
            }
            _ => None,
        };
        // A method behind a `cfg` keeps its entry behind the same one.
        let cfgs = method.attrs.iter().filter(|a| a.path().is_ident("cfg"));
        let entry = function(method, &self_ty, starting, name)?;
        functions.push(quote!(#(#cfgs)* #entry));
    }
    if functions.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "mark the methods scripts may call with `#[script]`",
        ));
    }
    let generics = item.generics.clone();
    let ty = if starting {
        quote!(#script::InitFunction)
    } else {
        quote!(#script::Function<Self>)
    };
    Ok(quote! {
        #item

        impl #generics #self_ty {
            #[doc(hidden)]
            pub const #constant: &'static [#ty] = &[#(#functions),*];
        }
    })
}

#[cfg(test)]
mod tests {
    use syn::parse_quote;

    use super::*;

    #[test]
    fn the_crate_argument_keeps_name_context_and_default_beside_it() -> syn::Result<()> {
        let input: DeriveInput = parse_quote! {
            #[script(crate = "::enshwick_core", name = "Renamed")]
            #[script(context, crate = "::enshwick_core")]
            struct Kept {
                #[script(default, crate = "::enshwick_core")]
                pub level: u32,
            }
        };
        assert_eq!(type_name(&input), "Renamed");
        let out = expand_derive(&input)?.to_string();
        assert!(out.contains("Context for Kept"), "{out}");
        assert!(out.contains("argument_or_default"), "{out}");
        Ok(())
    }
}
