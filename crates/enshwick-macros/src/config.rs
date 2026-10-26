use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::spanned::Spanned as _;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit, Meta, Type};

use crate::{comment, doc_text, local};

/// What a field's `#[config(...)]` says.
#[derive(Default)]
pub(super) struct FieldConfig {
    pub(super) stage: Option<Ident>,
    pub(super) min: Option<Expr>,
    pub(super) max: Option<Expr>,
    pub(super) choices: Option<syn::ExprArray>,
    pub(super) text: Option<syn::LitStr>,
    pub(super) secret: bool,
    /// `startup, sync`: a startup field a joined client takes from the server.
    pub(super) synced: bool,
    /// `group`: the field is a `Config` of its own, a table of the file.
    pub(super) group: bool,
    /// `first = n`: the number of a list's first element on the Mods screen.
    pub(super) first: Option<syn::LitInt>,
}

/// The stages a field may name, as `enshwick::config::Stage` variants.
pub(super) const STAGES: [(&str, &str); 5] = [
    ("startup", "Startup"),
    ("runtime", "Runtime"),
    ("client", "Client"),
    ("sync", "Sync"),
    ("restart", "Restart"),
];

/// The template's note above a `startup, sync` field.
pub(super) const SYNCED_NOTE: &str =
    "# Takes effect when the world loads; joined players run with the server's value.\n";

pub(super) fn field_config(attrs: &[Attribute]) -> syn::Result<FieldConfig> {
    let mut config = FieldConfig::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if let Some((_, variant)) = STAGES.iter().find(|(name, _)| meta.path.is_ident(name)) {
                let pair = |a: &Ident, b: &str| {
                    (a == "Startup" && b == "Sync") || (a == "Sync" && b == "Startup")
                };
                match config.stage.as_ref() {
                    Some(stage) if !config.synced && pair(stage, variant) => {
                        config.stage = Some(Ident::new("Startup", stage.span()));
                        config.synced = true;
                        Ok(())
                    }
                    Some(_) => Err(meta.error("a field has one stage, or `startup, sync`")),
                    None => {
                        config.stage = Some(Ident::new(variant, meta.path.span()));
                        Ok(())
                    }
                }
            } else if meta.path.is_ident("range") {
                if config.min.is_some() || config.max.is_some() {
                    return Err(meta.error("`range` is given twice"));
                }
                let range: syn::ExprRange = meta.value()?.parse()?;
                if matches!(range.limits, syn::RangeLimits::HalfOpen(_)) && range.end.is_some() {
                    return Err(meta.error("a range includes its end: write `a..=b`"));
                }
                if range.start.is_none() && range.end.is_none() {
                    return Err(meta.error("a range names at least one end"));
                }
                if let (Some(min), Some(max)) = (
                    range.start.as_deref().and_then(literal_number),
                    range.end.as_deref().and_then(literal_number),
                ) && min > max
                {
                    return Err(meta.error("a range starts at or below its end"));
                }
                config.min = range.start.map(|e| *e);
                config.max = range.end.map(|e| *e);
                Ok(())
            } else if meta.path.is_ident("choices") {
                if config.choices.is_some() {
                    return Err(meta.error("`choices` is given twice"));
                }
                let choices: syn::ExprArray = meta.value()?.parse()?;
                if choices.elems.is_empty() {
                    return Err(meta.error("name at least one choice"));
                }
                config.choices = Some(choices);
                Ok(())
            } else if meta.path.is_ident("text") {
                if config.text.is_some() {
                    return Err(meta.error("`text` is given twice"));
                }
                config.text = Some(text_key(&meta)?);
                Ok(())
            } else if meta.path.is_ident("first") {
                config.first = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("group") {
                config.group = true;
                Ok(())
            } else if meta.path.is_ident("secret") {
                if config.secret {
                    return Err(meta.error("`secret` is given twice"));
                }
                config.secret = true;
                Ok(())
            } else {
                Err(meta.error(
                    "expected `startup`, `runtime`, `client`, `sync`, `restart`, `range = a..=b`, `choices = [..]`, `text = \"key\"`, `secret`, `first = n` or `group`",
                ))
            }
        })?;
    }
    let grouped_with_more = config.group
        && (config.stage.is_some()
            || config.min.is_some()
            || config.max.is_some()
            || config.choices.is_some()
            || config.text.is_some()
            || config.secret);
    if grouped_with_more {
        return Err(syn::Error::new(
            Span::call_site(),
            "a `group` field takes its stages, ranges and texts from its own fields",
        ));
    }
    if let Some(stage) = config
        .stage
        .as_ref()
        .filter(|s| config.secret && (*s == "Sync" || config.synced))
    {
        return Err(syn::Error::new(
            stage.span(),
            "a secret never reaches a client, so it cannot be `sync`",
        ));
    }
    Ok(config)
}

/// The struct's `validate` path and the text key of its title.
pub(super) fn struct_config(attrs: &[Attribute]) -> syn::Result<(Option<syn::Path>, String)> {
    let mut validate = None;
    let mut title = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("validate") {
                if validate.is_some() {
                    return Err(meta.error("`validate` is given twice"));
                }
                validate = Some(meta.value()?.parse()?);
                Ok(())
            } else if meta.path.is_ident("text") {
                if title.is_some() {
                    return Err(meta.error("`text` is given twice"));
                }
                title = Some(text_key(&meta)?.value());
                Ok(())
            } else {
                Err(meta.error("expected `validate = path` or `text = \"key\"`"))
            }
        })?;
    }
    Ok((validate, title.unwrap_or_else(|| String::from("config"))))
}

/// The key of `text = "key"`, refused when empty.
pub(super) fn text_key(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<syn::LitStr> {
    let key: syn::LitStr = meta.value()?.parse()?;
    if key.value().is_empty() {
        return Err(syn::Error::new_spanned(&key, "a text key is not empty"));
    }
    Ok(key)
}

/// The value of a numeric literal, negated or not; `None` for any other
/// expression.
pub(super) fn literal_number(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Int(int) => int.base10_parse::<f64>().ok(),
            Lit::Float(float) => float.base10_parse::<f64>().ok(),
            _ => None,
        },
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
            literal_number(&unary.expr).map(|n| -n)
        }
        Expr::Group(group) => literal_number(&group.expr),
        Expr::Paren(paren) => literal_number(&paren.expr),
        _ => None,
    }
}

/// Refuses the serde attributes that make serde read other keys than the
/// fields' names, which the template, the schema and the checks use.
pub(super) fn same_keys_as_serde(attrs: &[Attribute]) -> syn::Result<()> {
    const RENAMING: [&str; 14] = [
        "rename",
        "rename_all",
        "alias",
        "flatten",
        "skip",
        "skip_serializing",
        "skip_deserializing",
        "with",
        "serialize_with",
        "deserialize_with",
        "transparent",
        "from",
        "try_from",
        "into",
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
                    "Config writes and checks each field under its own name and type: drop this serde attribute",
                ));
            }
        }
    }
    Ok(())
}

/// The named fields of a settings struct without generics.
pub(super) fn settings_fields(input: &DeriveInput) -> syn::Result<&syn::FieldsNamed> {
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

pub(super) fn expand_config(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let fields = settings_fields(input)?;
    let (defaults, text, schema) = (local("defaults"), local("text"), local("schema"));
    let (validate, title) = struct_config(&input.attrs)?;
    let own_check = validate
        .map(|path| quote!(#path(self).map_err(::enshwick::config::ConfigError::invalid)?;));
    let mut entries = Vec::new();
    let mut described = Vec::new();
    let mut checks = Vec::new();
    let mut staged: Vec<(Ident, bool, &syn::Field)> = Vec::new();
    let mut groups: Vec<(&Ident, String, &syn::Field)> = Vec::new();
    for field in &fields.named {
        let Some(ident) = field.ident.as_ref() else {
            continue;
        };
        same_keys_as_serde(&field.attrs)?;
        let config = field_config(&field.attrs)?;
        if config.group {
            groups.push((ident, ident.unraw().to_string(), field));
            continue;
        }
        let stage = config
            .stage
            .clone()
            .unwrap_or_else(|| Ident::new("Runtime", Span::call_site()));
        let key = ident.unraw().to_string();
        let doc = comment(&field.attrs);
        let note = template_note(&stage, config.synced);
        entries.push(quote! {
            #text.push_str(#doc);
            #text.push_str(#note);
            #text.push_str(&::enshwick::config::__entry(#key, &#defaults.#ident));
        });
        checks.push(quote! {
            ::enshwick::config::__check_field(&#schema, #key, &self.#ident)?;
        });
        let synced = config.synced;
        described.push(described_field(
            &key, &stage, ident, config, field, &defaults,
        ));
        staged.push((stage, synced, field));
    }
    let header = comment(&input.attrs);
    let header_text = doc_text(&input.attrs);
    let values = stage_methods(&staged, &groups);
    let (path, fields_of, groups_of) = (local("path"), local("fields"), local("groups"));
    let config = quote!(::enshwick::config::Config);
    let GroupTokens {
        tables: group_tables,
        schemas: group_schemas,
        checks: group_checks,
    } = group_tokens(&groups, &text, &path, &fields_of, &groups_of);
    Ok(quote! {
        impl ::enshwick::config::Config for #name {
            fn template() -> ::std::string::String {
                let mut #text = ::std::string::String::from(#header);
                if !#text.is_empty() {
                    #text.push('\n');
                }
                #text.push_str(&<Self as #config>::__template_body(""));
                #text
            }

            fn __template_body(#path: &str) -> ::std::string::String {
                let #defaults = <Self as ::core::default::Default>::default();
                let mut #text = ::std::string::String::new();
                #(#entries)*
                #(#group_tables)*
                let _ = (&#defaults, #path);
                #text
            }

            fn schema() -> ::enshwick::config::Schema {
                let #defaults = <Self as ::core::default::Default>::default();
                let mut #fields_of: ::std::vec::Vec<::enshwick::config::Field> =
                    ::std::vec![#(#described),*];
                let mut #groups_of: ::std::vec::Vec<::enshwick::config::Group> =
                    ::std::vec::Vec::new();
                #(#group_schemas)*
                let _ = &#defaults;
                ::enshwick::config::Schema::new(#header_text, #title, #fields_of)
                    .with_groups(#groups_of)
            }

            fn validate(
                &self,
            ) -> ::core::result::Result<(), ::enshwick::config::ConfigError> {
                let #schema = <Self as ::enshwick::config::Config>::schema();
                #(#checks)*
                #(#group_checks)*
                #own_check
                let _ = &#schema;
                ::core::result::Result::Ok(())
            }

            #values
        }
    }
    .into())
}

/// The template's note above a field at `stage`, `synced` for
/// `startup, sync`.
pub(super) fn template_note(stage: &Ident, synced: bool) -> &'static str {
    if synced {
        SYNCED_NOTE
    } else if stage == "Startup" {
        "# Takes effect when the world loads.\n"
    } else if stage == "Restart" {
        "# Takes effect when the game starts again.\n"
    } else {
        ""
    }
}

/// The schema's `Field` for a plain field, its default read from the
/// derive's local `defaults`.
pub(super) fn described_field(
    key: &str,
    stage: &Ident,
    ident: &Ident,
    config: FieldConfig,
    field: &syn::Field,
    defaults: &Ident,
) -> proc_macro2::TokenStream {
    let bound = |e: &Option<Expr>| {
        e.as_ref().map_or_else(
            || quote!(::core::option::Option::None),
            |e| quote!(::core::option::Option::Some((#e) as f64)),
        )
    };
    let (min, max) = (bound(&config.min), bound(&config.max));
    let choices = config.choices.map(|c| quote!(.with_choices(&#c)));
    let plain = doc_text(&field.attrs);
    let label = config.text.map(|t| t.value()).unwrap_or_default();
    let secret = config.secret;
    let synced = config.synced;
    let first = config.first.map(|n| quote!(.with_first(#n)));
    quote! {
        ::enshwick::config::Field::new(
            #key,
            ::enshwick::config::Stage::#stage,
            &#defaults.#ident,
        )
        .with_range(#min, #max)
        #choices
        .with_doc(#plain, #label)
        .with_secret(#secret)
        .with_synced(#synced)
        #first
    }
}

/// What `#[derive(Config)]` writes for its `group` fields.
pub(super) struct GroupTokens {
    /// Each group's table in the template.
    pub(super) tables: Vec<proc_macro2::TokenStream>,
    /// Each group's fields and groups in the schema.
    pub(super) schemas: Vec<proc_macro2::TokenStream>,
    /// Each group's own check.
    pub(super) checks: Vec<proc_macro2::TokenStream>,
}

/// The template, schema and check parts of the `group` fields `groups`,
/// over the derive's locals `text`, `path`, `fields_of` and `groups_of`.
pub(super) fn group_tokens(
    groups: &[(&Ident, String, &syn::Field)],
    text: &Ident,
    path: &Ident,
    fields_of: &Ident,
    groups_of: &Ident,
) -> GroupTokens {
    let config = quote!(::enshwick::config::Config);
    let tables = groups
        .iter()
        .map(|(_, key, field)| {
            let ty = &field.ty;
            let doc = comment(&field.attrs);
            quote! {
                #text.push_str(#doc);
                #text.push_str(&::std::format!("[{}{}]\n", #path, #key));
                #text.push_str(&<#ty as #config>::__template_body(
                    &::std::format!("{}{}.", #path, #key),
                ));
            }
        })
        .collect();
    let schemas = groups.iter().map(|(_, key, field)| {
        let ty = &field.ty;
        let doc = doc_text(&field.attrs);
        quote! {
            let inner = <#ty as #config>::schema();
            let doc = if #doc.is_empty() { inner.doc.clone() } else { ::std::string::String::from(#doc) };
            #groups_of.push(::enshwick::config::Group::new(#key, &doc));
            #groups_of.extend(inner.groups.into_iter().map(|g| g.__in_group(#key)));
            #fields_of.extend(inner.fields.into_iter().map(|f| f.__in_group(#key)));
        }
    }).collect();
    let checks = groups
        .iter()
        .map(|(ident, _, field)| {
            let ty = &field.ty;
            quote!(<#ty as #config>::validate(&self.#ident)?;)
        })
        .collect();
    GroupTokens {
        tables,
        schemas,
        checks,
    }
}

/// `values` and `merge_values` over the fields of each stage that has any,
/// and over every group's own; a `startup, sync` field counts at both.
pub(super) fn stage_methods(
    staged: &[(Ident, bool, &syn::Field)],
    groups: &[(&Ident, String, &syn::Field)],
) -> impl quote::ToTokens {
    let (table, value, stage, values, any) = (
        local("table"),
        local("value"),
        local("stage"),
        local("values"),
        local("any"),
    );
    let mut value_arms = Vec::new();
    let mut merge_arms = Vec::new();
    for (_, variant) in STAGES {
        let fields: Vec<&syn::Field> = staged
            .iter()
            .filter(|(stage, synced, _)| stage == variant || (*synced && variant == "Sync"))
            .map(|(_, _, field)| *field)
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
            .map(|ident| local(&format!("field_{}", ident.unraw())))
            .collect();
        value_arms.push(quote! {
            ::enshwick::config::Stage::#variant => {
                #any = true;
                #(#table.put(#keys, &self.#idents);)*
            }
        });
        merge_arms.push(quote! {
            ::enshwick::config::Stage::#variant => {
                #(let #locals = #table.take::<#types>(#keys)?;)*
                #(
                    if let ::core::option::Option::Some(#value) = #locals {
                        self.#idents = #value;
                    }
                )*
            }
        });
    }
    let (group_values, group_merges) = group_stage_tokens(groups, &table, &value, &stage, &any);
    let reads = !value_arms.is_empty() || !groups.is_empty();
    let merge_body = if reads {
        quote! {
            let #table = ::enshwick::config::__Values::parse(#values)?;
            match #stage {
                #(#merge_arms)*
                _ => {}
            }
            #(#group_merges)*
            ::core::result::Result::Ok(())
        }
    } else {
        quote! {
            let _ = (#values, #stage);
            ::core::result::Result::Ok(())
        }
    };
    quote! {
        fn values(
            &self,
            #stage: ::enshwick::config::Stage,
        ) -> ::core::option::Option<::std::string::String> {
            let mut #table = ::enshwick::config::__Values::default();
            let mut #any = false;
            match #stage {
                #(#value_arms)*
                _ => {}
            }
            #(#group_values)*
            #any.then(|| #table.finish())
        }

        fn merge_values(
            &mut self,
            #stage: ::enshwick::config::Stage,
            #values: &str,
        ) -> ::core::result::Result<(), ::enshwick::config::ConfigError> {
            #merge_body
        }
    }
}

/// The parts of `values` and `merge_values` for the `group` fields
/// `groups`, over the derive's locals.
pub(super) fn group_stage_tokens(
    groups: &[(&Ident, String, &syn::Field)],
    table: &Ident,
    value: &Ident,
    stage: &Ident,
    any: &Ident,
) -> (Vec<proc_macro2::TokenStream>, Vec<proc_macro2::TokenStream>) {
    let config = quote!(::enshwick::config::Config);
    let values = groups.iter().map(|(ident, key, field)| {
        let ty = &field.ty;
        quote! {
            if let ::core::option::Option::Some(#value) = <#ty as #config>::values(&self.#ident, #stage) {
                if #table.put_group(#key, &#value).is_ok() {
                    #any = true;
                }
            }
        }
    }).collect();
    let merges = groups
        .iter()
        .map(|(ident, key, field)| {
            let ty = &field.ty;
            quote! {
                if let ::core::option::Option::Some(#value) = #table.group(#key) {
                    <#ty as #config>::merge_values(&mut self.#ident, #stage, &#value)?;
                }
            }
        })
        .collect();
    (values, merges)
}
