use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::ext::IdentExt as _;
use syn::parse::Parser as _;
use syn::punctuated::Punctuated;
use syn::{Attribute, Data, DeriveInput, Expr, Fields, Ident, Lit, Meta, Token};

use crate::{comment, doc_text, local};

/// What a field's `#[config(...)]` says.
#[derive(Default)]
pub(super) struct FieldConfig {
    pub(super) min: Option<Expr>,
    pub(super) max: Option<Expr>,
    pub(super) choices: Option<syn::ExprArray>,
    pub(super) text: Option<syn::LitStr>,
    pub(super) secret: bool,
    /// `group`: the field is a `Config` of its own, a table of the file.
    pub(super) group: bool,
    /// `first = n`: the number of a list's first element on the Mods screen.
    pub(super) first: Option<syn::LitInt>,
}

pub(super) fn field_config(attrs: &[Attribute]) -> syn::Result<FieldConfig> {
    let mut config = FieldConfig::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("range") {
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
                    "expected `range = a..=b`, `choices = [..]`, `text = \"key\"`, `secret`, `first = n` or `group`; settings have no stages: a mod gets them as each world loads",
                ))
            }
        })?;
    }
    let grouped_with_more = config.group
        && (config.min.is_some()
            || config.max.is_some()
            || config.choices.is_some()
            || config.text.is_some()
            || config.secret);
    if grouped_with_more {
        return Err(syn::Error::new(
            Span::call_site(),
            "a `group` field takes its ranges and texts from its own fields",
        ));
    }
    Ok(config)
}

/// What a settings struct's `#[config(...)]` says.
pub(super) struct StructConfig {
    /// `validate = path`.
    pub(super) validate: Option<syn::Path>,
    /// The text key of its title, `config` by default.
    pub(super) title: String,
    /// `file = "name"`, `settings` by default.
    pub(super) file: String,
    /// `group`: a table of another settings type, which writes no file.
    pub(super) group: bool,
}

/// Whether `name` may name a settings file: lowercase ASCII letters,
/// digits, `-` and `_`, as every file system and the loader take it.
fn file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The struct's `validate` path, the text key of its title and its file.
pub(super) fn struct_config(attrs: &[Attribute]) -> syn::Result<StructConfig> {
    let mut validate = None;
    let mut title = None;
    let mut file = None;
    let mut group = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("config")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("group") {
                group = true;
                Ok(())
            } else if meta.path.is_ident("file") {
                if file.is_some() {
                    return Err(meta.error("`file` is given twice"));
                }
                let name: syn::LitStr = meta.value()?.parse()?;
                if !file_name(&name.value()) {
                    return Err(syn::Error::new_spanned(
                        &name,
                        "a settings file is named by lowercase letters, digits, `-` and `_`, without `.toml`",
                    ));
                }
                file = Some(name.value());
                Ok(())
            } else if meta.path.is_ident("validate") {
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
                Err(meta.error(
                    "expected `file = \"name\"`, `group`, `validate = path` or `text = \"key\"`; every field has its default",
                ))
            }
        })?;
    }
    if group && file.is_some() {
        return Err(syn::Error::new(
            Span::call_site(),
            "a group is a table of another settings type's file and names no file of its own",
        ));
    }
    Ok(StructConfig {
        validate,
        title: title.unwrap_or_else(|| String::from("config")),
        file: file.unwrap_or_else(|| String::from("settings")),
        group,
    })
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

/// The struct `input` under `#[enshwick::config(args)]`: serde's derives
/// through the API's serde, unknown keys refused, every missing key at the
/// struct's `Default`, the arguments as the `Config` derive's struct
/// attributes.
pub(super) fn expand_attribute(
    args: proc_macro2::TokenStream,
    input: &DeriveInput,
) -> syn::Result<proc_macro2::TokenStream> {
    let forwarded: Vec<Meta> = Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(args)?
        .into_iter()
        .collect();
    let forwarded = (!forwarded.is_empty()).then(|| quote!(#[config(#(#forwarded),*)]));
    Ok(quote! {
        #[derive(
            ::enshwick::__private::serde::Deserialize,
            ::enshwick::__private::serde::Serialize,
            ::enshwick::Config,
        )]
        #[serde(crate = "::enshwick::__private::serde", default, deny_unknown_fields)]
        #forwarded
        #input
    })
}

/// Refuses a settings struct that would read past an unknown key: serde
/// ignores one unless the struct says `deny_unknown_fields`.
pub(super) fn refuses_unknown_keys(input: &DeriveInput) -> syn::Result<()> {
    let refused = input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("serde"))
        .filter_map(|a| match &a.meta {
            Meta::List(list) => Some(list.tokens.clone()),
            _ => None,
        })
        .flatten()
        .any(|token| {
            matches!(token, proc_macro2::TokenTree::Ident(word) if word == "deny_unknown_fields")
        });
    if refused {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            &input.ident,
            "a settings file must not hide a misspelt key: declare the struct with `#[enshwick::config]`",
        ))
    }
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
    refuses_unknown_keys(input)?;
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
    let StructConfig {
        validate,
        title,
        file,
        group,
    } = struct_config(&input.attrs)?;
    let claim = if group {
        quote!(impl ::enshwick::config::__Group for #name {})
    } else {
        file_claim(name, &file)
    };
    let own_check = validate
        .map(|path| quote!(#path(self).map_err(::core::convert::Into::<::enshwick::config::ConfigError>::into)?;));
    let mut entries = Vec::new();
    let mut described = Vec::new();
    let mut checks = Vec::new();
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
        let key = ident.unraw().to_string();
        let doc = comment(&field.attrs);
        entries.push(quote! {
            #text.push_str(#doc);
            #text.push_str(&::enshwick::config::__entry(#key, &#defaults.#ident));
        });
        checks.push(quote! {
            ::enshwick::config::__check_field(&#schema, #key, &self.#ident)?;
        });
        described.push(described_field(&key, ident, config, field, &defaults));
    }
    let header = comment(&input.attrs);
    let header_text = doc_text(&input.attrs);
    let (path, fields_of, groups_of) = (local("path"), local("fields"), local("groups"));
    let config = quote!(::enshwick::config::Config);
    let GroupTokens {
        tables: group_tables,
        schemas: group_schemas,
        checks: group_checks,
    } = group_tokens(&groups, &text, &path, &fields_of, &groups_of);
    Ok(quote! {
        impl ::enshwick::config::Config for #name {
            const FILE: &'static str = #file;

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
        }

        #claim
    }
    .into())
}

/// A macro named after the settings file `file`, exported at the crate's
/// root under the span of the type `name`, so a second type of the crate
/// that writes the same file fails the build pointing at both types. `_`
/// and `-` are spelt apart, so `a-b` and `a_b` never meet.
fn file_claim(name: &Ident, file: &str) -> proc_macro2::TokenStream {
    let spelt: String = file
        .chars()
        .map(|c| match c {
            '_' => "_u".to_owned(),
            '-' => "_d".to_owned(),
            c => c.to_string(),
        })
        .collect();
    let claim = Ident::new(
        &format!("__enshwick_two_settings_types_write_{spelt}_toml"),
        name.span(),
    );
    quote::quote_spanned! {name.span()=>
        // Off in test builds, whose many small types share one crate.
        #[cfg(not(test))]
        #[doc(hidden)]
        #[macro_export]
        macro_rules! #claim {
            () => {};
        }
    }
}

/// The schema's `Field` for a plain field, its default read from the
/// derive's local `defaults`.
pub(super) fn described_field(
    key: &str,
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
    let first = config.first.map(|n| quote!(.with_first(#n)));
    quote! {
        ::enshwick::config::Field::new(#key, &#defaults.#ident)
        .with_range(#min, #max)
        #choices
        .with_doc(#plain, #label)
        .with_secret(#secret)
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
            quote! {
                ::enshwick::config::__group::<#ty>();
                <#ty as #config>::validate(&self.#ident)?;
            }
        })
        .collect();
    GroupTokens {
        tables,
        schemas,
        checks,
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{DeriveInput, parse_quote};

    use super::{expand_attribute, refuses_unknown_keys};

    #[test]
    fn the_derive_alone_refuses_a_struct_that_reads_past_unknown_keys() {
        let open: DeriveInput = parse_quote!(
            struct Settings {
                limit: u32,
            }
        );
        let closed: DeriveInput = parse_quote!(
            #[serde(default, deny_unknown_fields)]
            struct Settings {
                limit: u32,
            }
        );
        assert!(refuses_unknown_keys(&open).is_err());
        assert!(refuses_unknown_keys(&closed).is_ok());
    }

    #[test]
    fn the_attribute_refuses_unknown_keys_and_forwards_the_struct_attributes() {
        let input: DeriveInput = parse_quote!(
            struct Settings {
                limit: u32,
            }
        );
        let expanded = expand_attribute(quote!(file = "altars", validate = check), &input).unwrap();
        let text = expanded.to_string();
        assert!(text.contains("default , deny_unknown_fields"), "{text}");
        assert!(
            text.contains("# [config (file = \"altars\" , validate = check)]"),
            "{text}"
        );
        let declared: DeriveInput = syn::parse2(expanded).unwrap();
        assert!(refuses_unknown_keys(&declared).is_ok());
    }

    #[test]
    fn a_file_is_named_once_by_lowercase_letters_digits_dashes_and_underscores() {
        use super::struct_config;

        let config = |attr: syn::Attribute| struct_config(&[attr]);
        assert_eq!(
            config(parse_quote!(#[config(file = "altar-zones_2")]))
                .unwrap()
                .file,
            "altar-zones_2"
        );
        assert_eq!(
            config(parse_quote!(#[config(text = "t")])).unwrap().file,
            "settings"
        );
        for name in ["", "Altars", "altars.toml", "a/b", "a b"] {
            assert!(
                config(parse_quote!(#[config(file = #name)])).is_err(),
                "{name}"
            );
        }
        assert!(config(parse_quote!(#[config(file = "a", file = "b")])).is_err());
        assert!(config(parse_quote!(#[config(default)])).is_err());
    }

    #[test]
    fn a_stage_attribute_is_refused_with_why() {
        let field: syn::Field = parse_quote!(
            #[config(startup)]
            limit: u32
        );
        let error = super::field_config(&field.attrs).err().unwrap().to_string();
        assert!(error.contains("no stages"), "{error}");
    }
}
