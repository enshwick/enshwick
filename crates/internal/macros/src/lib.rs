//! Macros for the Enshwick modding API; use them through `enshwick_loader_api`,
//! which re-exports them.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::spanned::Spanned as _;
use syn::{
    Attribute, Data, DeriveInput, Expr, Fields, FnArg, GenericArgument, Ident, ItemFn, Lit, Meta,
    Pat, PathArguments, Type, parse_macro_input,
};

/// Implements `enshwick_loader_api::config::Config` for a struct with named fields.
///
/// The settings file a player sees on first start is generated from the type:
/// the struct's doc comment heads the file, each field's doc comment precedes
/// its key, and each value comes from `Default`. Struct attributes:
/// `#[config(validate = path)]` names a `fn(&Self) -> Result<(), String>`
/// that checks parsed values, `#[config(text = "key")]` the text key of the
/// set's translated title.
///
/// Field attributes: one stage of `startup`, `runtime` (the default),
/// `client` and `sync`; `range = a..=b`, `a..` or `..=b`; `choices = [..]`;
/// `text = "key"`, the text key of the field's translated label; `secret`,
/// a value never sent to a client and changed in the file only. The derive
/// implements `schema`, `values`, `merge_values` and a `validate` that checks
/// ranges and choices before the struct's own check.
#[proc_macro_derive(Config, attributes(config))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_config(&input).unwrap_or_else(|e| e.to_compile_error().into())
}

/// What a field's `#[config(...)]` says.
#[derive(Default)]
struct FieldConfig {
    stage: Option<Ident>,
    min: Option<Expr>,
    max: Option<Expr>,
    choices: Option<syn::ExprArray>,
    text: Option<syn::LitStr>,
    secret: bool,
}

/// The stages a field may name, as `enshwick_loader_api::config::Stage` variants.
const STAGES: [(&str, &str); 4] = [
    ("startup", "Startup"),
    ("runtime", "Runtime"),
    ("client", "Client"),
    ("sync", "Sync"),
];

fn field_config(attrs: &[Attribute]) -> syn::Result<FieldConfig> {
    let mut config = FieldConfig::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if let Some((_, variant)) = STAGES.iter().find(|(name, _)| meta.path.is_ident(name)) {
                if config.stage.is_some() {
                    return Err(meta.error("a field has one stage"));
                }
                config.stage = Some(Ident::new(variant, meta.path.span()));
                Ok(())
            } else if meta.path.is_ident("range") {
                let range: syn::ExprRange = meta.value()?.parse()?;
                if matches!(range.limits, syn::RangeLimits::HalfOpen(_)) && range.end.is_some() {
                    return Err(meta.error("a range includes its end: write `a..=b`"));
                }
                config.min = range.start.map(|e| *e);
                config.max = range.end.map(|e| *e);
                Ok(())
            } else if meta.path.is_ident("choices") {
                config.choices = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("text") {
                config.text = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("secret") {
                config.secret = true;
                Ok(())
            } else {
                Err(meta.error(
                    "expected `startup`, `runtime`, `client`, `sync`, `range = a..=b`, `choices = [..]`, `text = \"key\"` or `secret`",
                ))
            }
        })?;
    }
    Ok(config)
}

/// The struct's `validate` path and the text key of its title.
fn struct_config(attrs: &[Attribute]) -> syn::Result<(Option<syn::Path>, String)> {
    let mut validate = None;
    let mut title = String::from("config");
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("validate") {
                validate = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("text") {
                title = meta.value()?.parse::<syn::LitStr>()?.value();
                Ok(())
            } else {
                Err(meta.error("expected `validate = path` or `text = \"key\"`"))
            }
        })?;
    }
    Ok((validate, title))
}

fn expand_config(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (validate, title) = struct_config(&input.attrs)?;
    let own_check = validate.map(|path| quote!(#path(self)?;));
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            name,
            "Config is derived for structs only",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(name, "Config needs named fields"));
    };
    let mut entries = Vec::new();
    let mut described = Vec::new();
    let mut staged: Vec<(Ident, &syn::Field)> = Vec::new();
    for field in &fields.named {
        let Some(ident) = field.ident.as_ref() else {
            continue;
        };
        let config = field_config(&field.attrs)?;
        let stage = config
            .stage
            .unwrap_or_else(|| Ident::new("Runtime", Span::call_site()));
        let key = ident.to_string();
        let doc = comment(&field.attrs);
        let note = if stage == "Startup" {
            "# Takes effect when the world loads.\n"
        } else {
            ""
        };
        entries.push(quote! {
            text.push_str(#doc);
            text.push_str(#note);
            text.push_str(&::enshwick_loader_api::config::__entry(#key, &defaults.#ident));
        });
        let bound = |e: &Option<Expr>| {
            e.as_ref().map_or_else(
                || quote!(::core::option::Option::None),
                |e| quote!(::core::option::Option::Some((#e) as f64)),
            )
        };
        let (min, max) = (bound(&config.min), bound(&config.max));
        let choices = config.choices.map(|c| quote!(.with_choices(&#c)));
        let plain = doc_text(&field.attrs);
        let text = config.text.map(|t| t.value()).unwrap_or_default();
        let secret = config.secret;
        described.push(quote! {
            ::enshwick_loader_api::config::Field::new(
                #key,
                ::enshwick_loader_api::config::Stage::#stage,
                &defaults.#ident,
            )
            .with_range(#min, #max)
            #choices
            .with_doc(#plain, #text)
            .with_secret(#secret)
        });
        staged.push((stage, field));
    }
    let header = comment(&input.attrs);
    let header_text = doc_text(&input.attrs);
    let values = stage_methods(&staged);
    Ok(quote! {
        impl ::enshwick_loader_api::config::Config for #name {
            fn template() -> ::std::string::String {
                let defaults = <Self as ::core::default::Default>::default();
                let mut text = ::std::string::String::from(#header);
                if !text.is_empty() {
                    text.push('\n');
                }
                #(#entries)*
                text
            }

            fn schema() -> ::enshwick_loader_api::config::Schema {
                let defaults = <Self as ::core::default::Default>::default();
                ::enshwick_loader_api::config::Schema::new(
                    #header_text,
                    #title,
                    ::std::vec![#(#described),*],
                )
            }

            fn validate(&self) -> ::core::result::Result<(), ::std::string::String> {
                ::enshwick_loader_api::config::__check(
                    &<Self as ::enshwick_loader_api::config::Config>::schema(),
                    self,
                )?;
                #own_check
                ::core::result::Result::Ok(())
            }

            #values
        }
    }
    .into())
}

/// `values` and `merge_values` over the fields of each stage that has any.
fn stage_methods(staged: &[(Ident, &syn::Field)]) -> impl quote::ToTokens {
    let mut value_arms = Vec::new();
    let mut merge_arms = Vec::new();
    for (_, variant) in STAGES {
        let fields: Vec<&syn::Field> = staged
            .iter()
            .filter(|(stage, _)| stage == variant)
            .map(|(_, field)| *field)
            .collect();
        if fields.is_empty() {
            continue;
        }
        let variant = Ident::new(variant, Span::call_site());
        let idents: Vec<&Ident> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();
        let keys: Vec<String> = idents.iter().map(ToString::to_string).collect();
        let types: Vec<&Type> = fields.iter().map(|f| &f.ty).collect();
        let locals: Vec<Ident> = idents
            .iter()
            .map(|ident| quote::format_ident!("__{}", ident))
            .collect();
        value_arms.push(quote! {
            ::enshwick_loader_api::config::Stage::#variant => {
                let mut table = ::enshwick_loader_api::config::__Values::default();
                #(table.put(#keys, &self.#idents);)*
                ::core::option::Option::Some(table.finish())
            }
        });
        merge_arms.push(quote! {
            ::enshwick_loader_api::config::Stage::#variant => {
                let table = ::enshwick_loader_api::config::__Values::parse(values)?;
                #(let #locals = table.take::<#types>(#keys)?;)*
                #(
                    if let ::core::option::Option::Some(value) = #locals {
                        self.#idents = value;
                    }
                )*
                ::core::result::Result::Ok(())
            }
        });
    }
    quote! {
        fn values(
            &self,
            stage: ::enshwick_loader_api::config::Stage,
        ) -> ::core::option::Option<::std::string::String> {
            match stage {
                #(#value_arms)*
                _ => ::core::option::Option::None,
            }
        }

        fn merge_values(
            &mut self,
            stage: ::enshwick_loader_api::config::Stage,
            values: &str,
        ) -> ::core::result::Result<(), ::std::string::String> {
            let _ = values;
            match stage {
                #(#merge_arms)*
                _ => ::core::result::Result::Ok(()),
            }
        }
    }
}

/// Declares a hook from its body: the function stays, and
/// `enshwick_loader_api::detour!` declares a static `Hook` named after it in upper
/// case.
///
/// The first argument is `next: Next<F>` with `F` the target's
/// `extern "system" fn` type, the rest are the target's arguments by plain
/// names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
/// order and flags. Examples are on `enshwick_loader_api::hook`.
#[proc_macro_attribute]
pub fn hook(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut priority: Option<Expr> = None;
    let mut flags: Option<Expr> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("priority") {
            priority = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("flags") {
            flags = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `priority = <i32>` or `flags = <HookFlags>`"))
        }
    });
    parse_macro_input!(args with settings);
    let function = parse_macro_input!(item as ItemFn);
    expand_hook(&function, priority.as_ref(), flags.as_ref())
        .unwrap_or_else(|e| e.to_compile_error().into())
}

fn expand_hook(
    function: &ItemFn,
    priority: Option<&Expr>,
    flags: Option<&Expr>,
) -> syn::Result<TokenStream> {
    let sig = &function.sig;
    if !sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig.generics,
            "a hook body takes no generics",
        ));
    }
    let mut inputs = sig.inputs.iter();
    let is_unsafe = inputs.next().and_then(next_is_unsafe).ok_or_else(|| {
        syn::Error::new_spanned(
            &sig.ident,
            "a hook body takes `next: Next<extern \"system\" fn(..)>` first",
        )
    })?;
    let unsafety = is_unsafe.then(|| quote!(unsafe));
    let mut args = Vec::new();
    for input in inputs {
        let FnArg::Typed(typed) = input else {
            return Err(syn::Error::new_spanned(
                input,
                "a hook body takes no `self`",
            ));
        };
        let Pat::Ident(pat) = &*typed.pat else {
            return Err(syn::Error::new_spanned(
                &typed.pat,
                "a hook body names each argument",
            ));
        };
        let ident = &pat.ident;
        let ty = &typed.ty;
        args.push(quote!(#ident: #ty));
    }
    let output = &sig.output;
    let body = &sig.ident;
    let name = Ident::new(&body.to_string().to_uppercase(), body.span());
    let vis = &function.vis;
    let docs = function.attrs.iter().filter(|a| a.path().is_ident("doc"));
    let priority = priority.map(|p| quote!(, priority = #p));
    let flags = flags.map(|f| quote!(, flags = #f));
    Ok(quote! {
        #function

        ::enshwick_loader_api::detour! {
            #(#docs)*
            #vis static #name: #unsafety extern "system" fn(#(#args),*) #output
                = #body #priority #flags;
        }
    }
    .into())
}

/// Whether the `F` of `next: Next<F>` is an `unsafe fn`; `None` when the
/// argument has another shape.
fn next_is_unsafe(arg: &FnArg) -> Option<bool> {
    let FnArg::Typed(typed) = arg else {
        return None;
    };
    let Type::Path(path) = &*typed.ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(generics) = &segment.arguments else {
        return None;
    };
    match generics.args.first()? {
        GenericArgument::Type(Type::FnPtr(f)) => Some(f.unsafety.is_some()),
        _ => None,
    }
}

/// The doc comment of `attrs` as plain text, lines joined by spaces.
fn doc_text(attrs: &[Attribute]) -> String {
    doc_lines(attrs)
        .map(|line| line.trim().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The lines of the doc comment of `attrs`.
fn doc_lines(attrs: &[Attribute]) -> impl Iterator<Item = String> + '_ {
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
}

/// The doc comment of `attrs` as TOML comment lines.
fn comment(attrs: &[Attribute]) -> String {
    doc_lines(attrs).fold(String::new(), |mut text, line| {
        text.push('#');
        text.push_str(line.trim_end());
        text.push('\n');
        text
    })
}
