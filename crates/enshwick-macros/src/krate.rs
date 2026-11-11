//! `crate = "path"`, the hidden argument that names the API crate in an
//! expansion, as serde's `crate = "..."`: `::enshwick` unless an internal
//! crate of the API, which mods never see, names itself.

use proc_macro2::{Group, Spacing, TokenStream, TokenTree};
use quote::ToTokens as _;
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

/// `tokens` with every `::enshwick` path start written as `krate`; the same
/// tokens without one. A `::enshwick` that continues a path, as in
/// `crate::enshwick`, stays.
///
/// shortcut: the item a mod wrote is rewritten with the expansion, which only
/// the internal crates that name themselves see; split them if one ever
/// needs a literal `::enshwick` path.
pub(crate) fn rename(tokens: TokenStream, krate: Option<&Path>) -> TokenStream {
    match krate {
        Some(krate) => rewrite(tokens, &krate.to_token_stream()),
        None => tokens,
    }
}

fn rewrite(tokens: TokenStream, krate: &TokenStream) -> TokenStream {
    let trees: Vec<TokenTree> = tokens.into_iter().collect();
    let mut out = TokenStream::new();
    let mut at = 0;
    while at < trees.len() {
        if let [
            TokenTree::Punct(a),
            TokenTree::Punct(b),
            TokenTree::Ident(name),
            ..,
        ] = &trees[at..]
            && a.as_char() == ':'
            && a.spacing() == Spacing::Joint
            && b.as_char() == ':'
            && name == "enshwick"
            && !(at > 0 && continues(&trees[at - 1]))
        {
            out.extend(krate.clone());
            at += 3;
            continue;
        }
        match &trees[at] {
            TokenTree::Group(group) => {
                let mut inner = Group::new(group.delimiter(), rewrite(group.stream(), krate));
                inner.set_span(group.span());
                out.extend([TokenTree::Group(inner)]);
            }
            other => out.extend([other.clone()]),
        }
        at += 1;
    }
    out
}

/// Whether a `::` after `tree` continues a path: after a name or `crate`,
/// `self`, `super` or `Self`, not after a keyword such as `impl` or `use`.
fn continues(tree: &TokenTree) -> bool {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "static", "struct", "trait", "type", "unsafe", "use", "where", "while",
        "yield",
    ];
    matches!(tree, TokenTree::Ident(ident) if !KEYWORDS.iter().any(|k| ident == k))
}

/// `path` as the text serde's `crate = "..."` takes.
pub(crate) fn text(path: &Path) -> String {
    path.to_token_stream()
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
    fn rename_writes_every_enshwick_path_start_as_the_given_crate() {
        let krate: Path = syn::parse_quote!(::enshwick_core);
        let out = rename(
            quote!(impl ::enshwick::Shape for X { fn f() -> ::enshwick::abi::Str { [::enshwick::a] } }),
            Some(&krate),
        );
        assert_eq!(
            out.to_string(),
            quote!(impl ::enshwick_core::Shape for X { fn f() -> ::enshwick_core::abi::Str { [::enshwick_core::a] } })
                .to_string()
        );
    }

    #[test]
    fn rename_leaves_tokens_alone_without_a_crate_or_other_paths() {
        let tokens = quote!(::enshwick::Shape + enshwick::Shape + ::enshwick_core::X);
        assert_eq!(rename(tokens.clone(), None).to_string(), tokens.to_string());
        let krate: Path = syn::parse_quote!(crate);
        assert_eq!(
            rename(tokens, Some(&krate)).to_string(),
            quote!(crate::Shape + enshwick::Shape + ::enshwick_core::X).to_string()
        );
    }

    #[test]
    fn rename_leaves_an_enshwick_segment_inside_a_path_but_rewrites_one_after_a_keyword() {
        let krate: Path = syn::parse_quote!(::enshwick_core);
        let out = rename(
            quote!(
                use crate::enshwick::A;
                impl<T> ::enshwick::B for super::enshwick::C<dyn ::enshwick::D> {}
            ),
            Some(&krate),
        );
        assert_eq!(
            out.to_string(),
            quote!(
                use crate::enshwick::A;
                impl<T> ::enshwick_core::B for super::enshwick::C<dyn ::enshwick_core::D> {}
            )
            .to_string()
        );
    }

    #[test]
    fn in_attrs_finds_the_crate_beside_other_arguments() -> syn::Result<()> {
        let attrs: Vec<Attribute> = vec![
            syn::parse_quote!(#[config(validate = check, crate = "::enshwick_core")]),
            syn::parse_quote!(#[shape(values(A, B))]),
            syn::parse_quote!(#[config]),
        ];
        let found = in_attrs(&attrs, "config")?.map(|p| text(&p));
        assert_eq!(found.as_deref(), Some("::enshwick_core"));
        assert!(in_attrs(&attrs, "shape")?.is_none());
        Ok(())
    }
}
