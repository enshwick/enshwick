//! `crate = "path"`, the hidden argument that names the API crate in an
//! expansion, as serde's `crate = "..."`: `::enshwick` unless an internal
//! crate of the API, which mods never see, names itself.

use proc_macro2::{Group, TokenStream};
use quote::ToTokens;
use syn::{Attribute, Path};

/// The argument's word.
pub(crate) const KEY: &str = "crate";

/// The path of `crate = "path"` at `meta`.
pub(crate) fn value(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<Path> {
    meta.value()?.parse::<syn::LitStr>()?.parse()
}

/// The path of a `crate = "path"` inside any `#[<helper>(...)]` of `attrs`;
/// the other arguments there are read where they act.
pub(crate) fn in_attrs(attrs: &[Attribute], helper: &str) -> syn::Result<Option<Path>> {
    let mut found = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident(helper)) {
        if !matches!(attr.meta, syn::Meta::List(_)) {
            continue;
        }
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident(KEY) {
                found = Some(value(&meta)?);
                Ok(())
            } else {
                skip(&meta)
            }
        })?;
    }
    Ok(found)
}

/// Reads past the `= value` or `(..)` of the argument at `meta`, so a reader
/// that wants other arguments goes on to the next one.
pub(crate) fn skip(meta: &syn::meta::ParseNestedMeta<'_>) -> syn::Result<()> {
    if meta.input.peek(syn::Token![=]) {
        let _skipped: syn::Expr = meta.value()?.parse()?;
    } else if meta.input.peek(syn::token::Paren) {
        let _skipped: Group = meta.input.parse()?;
    }
    Ok(())
}

/// The path an expansion names the API crate by: the one `crate = "path"`
/// gave, or `::enshwick`.
pub(crate) fn path(krate: Option<&Path>) -> TokenStream {
    krate.map_or_else(|| quote::quote!(::enshwick), ToTokens::to_token_stream)
}

/// `krate` as the text serde's `crate = "..."` takes.
pub(crate) fn text(krate: &TokenStream) -> String {
    krate
        .to_string()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;

    #[test]
    fn the_path_is_the_named_crate_or_the_api_crate() {
        let krate: Path = syn::parse_quote!(::enshwick_core);
        assert_eq!(text(&path(Some(&krate))), "::enshwick_core");
        assert_eq!(text(&path(None)), "::enshwick");
        assert_eq!(path(None).to_string(), quote!(::enshwick).to_string());
    }

    #[test]
    fn a_hook_body_keeps_its_own_enshwick_paths_under_another_crate() -> syn::Result<()> {
        let function: syn::ItemFn = syn::parse_quote! {
            fn count(next: Next<'_, Target>, a: u32) -> u32 {
                ::enshwick::info!("counted");
                next.call(a)
            }
        };
        let krate = path(Some(&syn::parse_quote!(::enshwick_core)));
        let out = crate::expand_hook(&function, None, None, &krate)?.to_string();
        assert!(
            out.contains(&quote!(::enshwick::info!).to_string()),
            "{out}"
        );
        assert!(
            out.contains(&quote!(::enshwick_core::Hook<Target>).to_string()),
            "{out}"
        );
        assert_eq!(
            out.matches(&quote!(::enshwick::).to_string()).count(),
            1,
            "{out}"
        );
        Ok(())
    }

    #[test]
    fn in_attrs_finds_the_crate_beside_other_arguments() -> syn::Result<()> {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[config(validate = check, crate = "::enshwick_core")]),
            syn::parse_quote!(#[shape(values(A, B))]),
            syn::parse_quote!(#[config]),
        ];
        let found = in_attrs(&attrs, "config")?.map(|p| text(&path(Some(&p))));
        assert_eq!(found.as_deref(), Some("::enshwick_core"));
        assert!(in_attrs(&attrs, "shape")?.is_none());
        Ok(())
    }
}
