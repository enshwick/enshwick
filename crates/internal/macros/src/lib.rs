//! Macros for the Enshwick modding API; use them through `enshwick_loader_api`,
//! which re-exports them.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt as _;
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
/// `#[config(validate = path)]` names a `fn(&Self) -> Result<(), E>`, `E`
/// any `Display` such as a `String`, that checks parsed values, `#[config(text = "key")]` the text key of the
/// set's translated title.
///
/// Field attributes: one stage of `startup`, `runtime` (the default), `restart`,
/// `client` and `sync`; `range = a..=b`, `a..` or `..=b`; `choices = [..]`;
/// `text = "key"`, the text key of the field's translated label; `secret`,
/// a value never sent to a client and changed in the file only, so never
/// with `sync`. The derive implements `schema`, `values`, `merge_values` and
/// a `validate` that checks ranges and choices before the struct's own
/// check.
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
const STAGES: [(&str, &str); 5] = [
    ("startup", "Startup"),
    ("runtime", "Runtime"),
    ("client", "Client"),
    ("sync", "Sync"),
    ("restart", "Restart"),
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
                let choices: syn::ExprArray = meta.value()?.parse()?;
                if choices.elems.is_empty() {
                    return Err(meta.error("name at least one choice"));
                }
                config.choices = Some(choices);
                Ok(())
            } else if meta.path.is_ident("text") {
                config.text = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("secret") {
                config.secret = true;
                Ok(())
            } else {
                Err(meta.error(
                    "expected `startup`, `runtime`, `client`, `sync`, `restart`, `range = a..=b`, `choices = [..]`, `text = \"key\"` or `secret`",
                ))
            }
        })?;
    }
    if let Some(stage) = config
        .stage
        .as_ref()
        .filter(|s| config.secret && *s == "Sync")
    {
        return Err(syn::Error::new(
            stage.span(),
            "a secret never reaches a client, so it cannot be `sync`",
        ));
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

/// Refuses the serde attributes that make serde read other keys than the
/// fields' names, which the template, the schema and the checks use.
fn same_keys_as_serde(attrs: &[Attribute]) -> syn::Result<()> {
    const RENAMING: [&str; 6] = [
        "rename",
        "rename_all",
        "flatten",
        "skip",
        "skip_serializing",
        "skip_deserializing",
    ];
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let Meta::List(list) = &attr.meta else {
            continue;
        };
        for token in list.tokens.clone() {
            if let proc_macro2::TokenTree::Ident(word) = token
                && RENAMING.iter().any(|name| word == name)
            {
                return Err(syn::Error::new_spanned(
                    word,
                    "Config writes and checks each field under its own name:                      drop this serde attribute",
                ));
            }
        }
    }
    Ok(())
}

/// The named fields of a settings struct without generics.
fn settings_fields(input: &DeriveInput) -> syn::Result<&syn::FieldsNamed> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "a settings type takes no generics",
        ));
    }
    same_keys_as_serde(&input.attrs)?;
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Config is derived for structs only",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "Config needs named fields",
        ));
    };
    Ok(fields)
}

fn expand_config(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = settings_fields(input)?;
    let (validate, title) = struct_config(&input.attrs)?;
    let own_check = validate.map(
        |path| quote!(#path(self).map_err(::enshwick_loader_api::config::ConfigError::invalid)?;),
    );
    let mut entries = Vec::new();
    let mut described = Vec::new();
    let mut checks = Vec::new();
    let mut staged: Vec<(Ident, &syn::Field)> = Vec::new();
    for field in &fields.named {
        let Some(ident) = field.ident.as_ref() else {
            continue;
        };
        same_keys_as_serde(&field.attrs)?;
        let config = field_config(&field.attrs)?;
        let stage = config
            .stage
            .unwrap_or_else(|| Ident::new("Runtime", Span::call_site()));
        let key = ident.unraw().to_string();
        let doc = comment(&field.attrs);
        let note = if stage == "Startup" {
            "# Takes effect when the world loads.\n"
        } else if stage == "Restart" {
            "# Takes effect when the game starts again.\n"
        } else {
            ""
        };
        entries.push(quote! {
            text.push_str(#doc);
            text.push_str(#note);
            text.push_str(&::enshwick_loader_api::config::__entry(#key, &defaults.#ident));
        });
        checks.push(quote! {
            ::enshwick_loader_api::config::__check_field(&__schema, #key, &self.#ident)?;
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

            fn validate(
                &self,
            ) -> ::core::result::Result<(), ::enshwick_loader_api::config::ConfigError> {
                let __schema = <Self as ::enshwick_loader_api::config::Config>::schema();
                #(#checks)*
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
        let keys: Vec<String> = idents.iter().map(|i| i.unraw().to_string()).collect();
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
        ) -> ::core::result::Result<(), ::enshwick_loader_api::config::ConfigError> {
            let _ = values;
            match stage {
                #(#merge_arms)*
                _ => ::core::result::Result::Ok(()),
            }
        }
    }
}

/// Declares a hook from its body: the function stays, and a static `Hook`
/// named after it in upper case joins it, as `enshwick_loader_api::detour!`
/// declares one.
///
/// The first argument is `next: Next<F>` with `F` the target's
/// `extern "system" fn` type or an alias of it, the rest are the target's
/// arguments by plain names. `#[hook(priority = <i32>, flags = <HookFlags>)]` sets the link's
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
    let target = inputs.next().and_then(next_type).ok_or_else(|| {
        syn::Error::new_spanned(
            &sig.ident,
            "a hook body takes `next: Next<extern \"system\" fn(..)>` first",
        )
    })?;
    let mut idents = Vec::new();
    let mut arg_types = Vec::new();
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
        idents.push(&pat.ident);
        arg_types.push(&typed.ty);
    }
    let output = &sig.output;
    let body = &sig.ident;
    let name = Ident::new(&body.unraw().to_string().to_uppercase(), body.span());
    let vis = &function.vis;
    let kept = function
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("doc") || a.path().is_ident("cfg"));
    let priority = priority.map_or_else(|| quote!(0), |p| quote!(#p));
    let flags = flags.map_or_else(
        || quote!(::enshwick_loader_api::abi::HookFlags::NONE),
        |f| quote!(#f),
    );
    let (entry, next, value) = (
        Ident::new("entry", Span::mixed_site()),
        Ident::new("next", Span::mixed_site()),
        Ident::new("value", Span::mixed_site()),
    );
    // The entry point is a safe fn, which coerces to the target's type
    // whether that is `unsafe` or not, so `F` may be any name for it.
    Ok(quote! {
        #function

        #(#kept)*
        #vis static #name: ::enshwick_loader_api::Hook<#target> = {
            extern "system" fn #entry(#(#idents: #arg_types),*) #output {
                let #next = #name.next();
                match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(
                    || #body(#next, #(#idents),*),
                )) {
                    ::core::result::Result::Ok(#value) => #value,
                    ::core::result::Result::Err(_) => {
                        ::enshwick_loader_api::__private::fault_in(::core::stringify!(#name));
                        #[allow(unused_unsafe)]
                        // SAFETY: the rest of the chain takes the arguments
                        // the game passed.
                        unsafe { #next.get()(#(#idents),*) }
                    }
                }
            }
            ::enshwick_loader_api::Hook::new(::core::stringify!(#name), #entry, #priority, #flags)
        };
    }
    .into())
}

/// Declares a service table: makes the struct `repr(C)` and implements
/// `enshwick_loader_api::services::Service` with a layout hash per field.
///
/// `#[service(name = "<publisher id>.<what>", version = <major>)]`; the
/// version defaults to 1. Fields are `extern "system" fn` pointers or plain
/// data, appended over time. Examples are on `enshwick_loader_api::service`.
#[proc_macro_attribute]
pub fn service(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let mut version: Option<syn::LitInt> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
            Ok(())
        } else if meta.path.is_ident("version") {
            version = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `name = \"<id>.<what>\"` or `version = <major>`"))
        }
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    expand_layout(
        &item,
        name,
        Some(version.map_or_else(|| quote!(1), |v| quote!(#v))),
        &quote!(Service),
    )
    .unwrap_or_else(|e| e.to_compile_error().into())
}

/// Declares a mod event: makes the struct `repr(C)` and implements
/// `enshwick_loader_api::services::ModEvent` with a layout hash per field.
///
/// `#[event(name = "<defining mod id>.<what>")]`; the struct is `Copy`
/// plain data, appended over time. Examples are on
/// `enshwick_loader_api::event`.
#[proc_macro_attribute]
pub fn event(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `name = \"<id>.<what>\"`"))
        }
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    expand_layout(&item, name, None, &quote!(ModEvent))
        .unwrap_or_else(|e| e.to_compile_error().into())
}

/// Declares an extension point: makes the input struct `repr(C)` and
/// implements `enshwick_loader_api::points::Point` with a layout hash per
/// field and one for the answer.
///
/// `#[point(name = "<owner id>:<point>", mode = Observe | Advise | Override,
/// output = <type>, refusable)]`; `refusable` lets a link refuse. Examples
/// are on `enshwick_loader_api::point`.
#[proc_macro_attribute]
pub fn point(args: TokenStream, item: TokenStream) -> TokenStream {
    let mut name: Option<syn::LitStr> = None;
    let mut mode: Option<Ident> = None;
    let mut output: Option<Type> = None;
    let mut refusable = false;
    let settings = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("mode") {
            mode = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("output") {
            output = Some(meta.value()?.parse()?);
        } else if meta.path.is_ident("refusable") {
            refusable = true;
        } else {
            return Err(meta.error(
                "expected `name = \"<id>:<point>\"`, `mode = Override`, `output = <type>` or `refusable`",
            ));
        }
        Ok(())
    });
    parse_macro_input!(args with settings);
    let item = parse_macro_input!(item as syn::ItemStruct);
    let ident = item.ident.clone();
    let expanded = (|| {
        let mode = mode.ok_or_else(|| {
            syn::Error::new_spanned(
                &ident,
                "choose a mode: `mode = Observe`, `Advise` or `Override`",
            )
        })?;
        if !["Observe", "Advise", "Override"].contains(&mode.to_string().as_str()) {
            return Err(syn::Error::new_spanned(
                &mode,
                "the mode is `Observe`, `Advise` or `Override`",
            ));
        }
        let output = output
            .ok_or_else(|| syn::Error::new_spanned(&ident, "name the answer: `output = <type>`"))?;
        let written = format!("output: {}", quote!(#output));
        let extra = quote! {
            const MODE: ::enshwick_loader_api::points::Mode =
                ::enshwick_loader_api::points::Mode::#mode;
            const REFUSE: bool = #refusable;
            const OUTPUT: &'static [::enshwick_loader_api::abi::LayoutField] =
                &[::enshwick_loader_api::abi::LayoutField {
                    name: ::enshwick_loader_api::abi::Str::new("output"),
                    hash: ::enshwick_loader_api::services::layout_hash(
                        #written,
                        ::core::mem::size_of::<#output>(),
                        ::core::mem::align_of::<#output>(),
                        0,
                    ),
                }];
            type Output = #output;
        };
        expand_declaration(&item, name, ':', &extra, &quote!(points::Point))
    })();
    expanded.unwrap_or_else(|e| e.to_compile_error().into())
}

/// The struct as `repr(C)` with its layout trait: `major` is `Some` for a
/// service.
fn expand_layout(
    item: &syn::ItemStruct,
    name: Option<syn::LitStr>,
    major: Option<proc_macro2::TokenStream>,
    trait_name: &proc_macro2::TokenStream,
) -> syn::Result<TokenStream> {
    let major = major.map(|major| quote!(const MAJOR: u32 = #major;));
    expand_declaration(
        item,
        name,
        '.',
        &major.unwrap_or_default(),
        &quote!(services::#trait_name),
    )
}

/// The struct as `repr(C)` with the declaration trait at
/// `enshwick_loader_api::<trait_path>`, named `<mod id><separator><what>`,
/// with `extra` items in the impl.
fn expand_declaration(
    item: &syn::ItemStruct,
    name: Option<syn::LitStr>,
    separator: char,
    extra: &proc_macro2::TokenStream,
    trait_path: &proc_macro2::TokenStream,
) -> syn::Result<TokenStream> {
    let ident = &item.ident;
    let name = name.ok_or_else(|| {
        syn::Error::new_spanned(
            ident,
            format!("name the declaration: `name = \"<id>{separator}<what>\"`"),
        )
    })?;
    let text = name.value();
    let valid = text.split_once(separator).is_some_and(|(id, what)| {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !what.is_empty()
    });
    if !valid {
        return Err(syn::Error::new_spanned(
            &name,
            format!("the name is `<mod id>{separator}<what>`, the mod id in `[a-z0-9-]`"),
        ));
    }
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "a declaration crosses DLLs and takes no generics",
        ));
    }
    if let Some(repr) = item.attrs.iter().find(|a| a.path().is_ident("repr")) {
        return Err(syn::Error::new_spanned(
            repr,
            "the declaration is `repr(C)` already: remove `#[repr]`",
        ));
    }
    let Fields::Named(fields) = &item.fields else {
        return Err(syn::Error::new_spanned(
            ident,
            "a declaration has named fields",
        ));
    };
    let entries = fields.named.iter().filter_map(|field| {
        let field_ident = field.ident.as_ref()?;
        let ty = &field.ty;
        let field_name = field_ident.to_string();
        let written = format!("{field_name}: {}", quote!(#ty));
        Some(quote! {
            ::enshwick_loader_api::abi::LayoutField {
                name: ::enshwick_loader_api::abi::Str::new(#field_name),
                hash: ::enshwick_loader_api::services::layout_hash(
                    #written,
                    ::core::mem::size_of::<#ty>(),
                    ::core::mem::align_of::<#ty>(),
                    ::core::mem::offset_of!(#ident, #field_ident),
                ),
            }
        })
    });
    Ok(quote! {
        #[repr(C)]
        #item

        // SAFETY: the layout lists every field of the `repr(C)` struct in
        // order, with its size, alignment and offset.
        unsafe impl ::enshwick_loader_api::#trait_path for #ident {
            const NAME: &'static str = #name;
            #extra
            const LAYOUT: &'static [::enshwick_loader_api::abi::LayoutField] = &[#(#entries),*];
        }
    }
    .into())
}

/// The `F` of `next: Next<F>`; `None` when the argument has another shape.
fn next_type(arg: &FnArg) -> Option<&Type> {
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
        GenericArgument::Type(ty) => Some(ty),
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

/// The lines of the doc comment of `attrs`; a block comment or a
/// `#[doc = "..."]` may hold several.
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
        .flat_map(|text| {
            if text.is_empty() {
                vec![text]
            } else {
                text.lines().map(str::to_owned).collect()
            }
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
