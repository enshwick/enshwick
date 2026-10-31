//! `texts!()` and the texts `declare_mod!` embeds: a mod's `src/texts.toml`,
//! read and checked when the mod builds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr};

/// Where a mod keeps its texts, from the crate's root.
pub(crate) const DEFAULT: &str = "src/texts.toml";

/// The language every key needs a text in, the loader's fallback.
const ENGLISH: &str = "en";

/// A texts file, read and checked.
pub(crate) struct File {
    /// The absolute path, for messages and build tracking.
    path: String,
    /// Keys to texts by lowercase language code.
    tables: BTreeMap<String, BTreeMap<String, String>>,
}

impl File {
    /// The file at `relative` from the crate's root; `None` when there is
    /// none.
    pub(crate) fn read(relative: &str) -> syn::Result<Option<Self>> {
        let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
            syn::Error::new(
                Span::call_site(),
                "CARGO_MANIFEST_DIR is not set: build the mod with cargo",
            )
        })?;
        let path = PathBuf::from(dir).join(relative);
        let shown = path.display().to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("{shown} does not read: {e}"),
                ));
            }
        };
        let file = Self::parse(shown, &text)?;
        Ok(Some(file))
    }

    /// The file `path` holding `text`, checked: every key has an `en` text,
    /// and every translation fills in the values its `en` text does.
    fn parse(path: String, text: &str) -> syn::Result<Self> {
        let read: BTreeMap<String, BTreeMap<String, String>> =
            toml::from_str(text).map_err(|e| refuse(format!("{path}: {}", e.message())))?;
        let mut tables: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (code, entries) in read {
            tables
                .entry(code.to_ascii_lowercase())
                .or_default()
                .extend(entries);
        }
        let file = Self { path, tables };
        file.check()?;
        Ok(file)
    }

    fn check(&self) -> syn::Result<()> {
        let path = &self.path;
        let english = self.tables.get(ENGLISH);
        for (code, entries) in &self.tables {
            for (key, text) in entries {
                let Some(base) = english.and_then(|en| en.get(key)) else {
                    return Err(refuse(format!(
                        "{path}: `{key}` in [{code}] has no text in [{ENGLISH}], which every key needs"
                    )));
                };
                let (wanted, given) = (values(base), values(text));
                if wanted != given {
                    return Err(refuse(format!(
                        "{path}: `{key}` in [{code}] fills in {} where its [{ENGLISH}] text fills in {}",
                        named(&given),
                        named(&wanted)
                    )));
                }
            }
        }
        Ok(())
    }

    /// The keys, each once, in order.
    fn keys(&self) -> impl Iterator<Item = (&str, &str)> {
        self.tables
            .get(ENGLISH)
            .into_iter()
            .flatten()
            .map(|(key, text)| (key.as_str(), text.as_str()))
    }

    /// A constant check per language code that the API knows it, and one
    /// per key that it has a text in every language the API requires, so
    /// both lists stay in one place.
    fn language_checks(&self) -> TokenStream {
        let checks = self.tables.keys().map(|code| {
            let message = format!(
                "{}: [{code}] is no language code the game knows; the codes are those of `enshwick::i18n::Language::ALL`, such as uk or pt-br",
                self.path
            );
            quote! {
                const _: () = ::core::assert!(::enshwick::__private::is_language_code(#code), #message);
            }
        });
        let required = self.keys().map(|(key, _)| {
            let codes: Vec<&str> = self
                .tables
                .iter()
                .filter(|(_, entries)| entries.get(key).is_some_and(|text| !text.is_empty()))
                .map(|(code, _)| code.as_str())
                .collect();
            let message = format!(
                "{}: `{key}` has a text in [{}] only; every key needs one in each language of `enshwick::i18n::Language::REQUIRED`",
                self.path,
                codes.join("], [")
            );
            quote! {
                const _: () = ::core::assert!(::enshwick::__private::has_required_languages(&[#(#codes),*]), #message);
            }
        });
        quote!(#(#checks)* #(#required)*)
    }

    /// The file of `test::declare_mod!`: at `path` from the crate's root,
    /// which must hold one, else at [`DEFAULT`] if there is one.
    pub(crate) fn for_test(path: Option<&LitStr>) -> syn::Result<Option<Self>> {
        let Some(path) = path else {
            return Self::read(DEFAULT);
        };
        let file = Self::read(&path.value())?.ok_or_else(|| {
            syn::Error::new(path.span(), format!("the crate has no {}", path.value()))
        })?;
        Ok(Some(file))
    }

    /// `file` as an `Option<&'static str>` expression, for `declare_mod!`.
    pub(crate) fn option(file: Option<&Self>) -> TokenStream {
        match file {
            None => quote!(::core::option::Option::None),
            Some(file) => {
                let embedded = file.embedded();
                quote!(::core::option::Option::Some(#embedded))
            }
        }
    }

    /// The file as a `&'static str` expression whose build tracks it, with
    /// its checks.
    pub(crate) fn embedded(&self) -> TokenStream {
        let path = &self.path;
        let checks = self.language_checks();
        quote! {{
            #checks
            ::core::include_str!(#path)
        }}
    }
}

/// The names of the values `text` fills in: each `{name}` of `[A-Za-z0-9_]`,
/// as `Texts::fill` replaces them.
fn values(text: &str) -> BTreeSet<&str> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        if let Some(close) = rest.find('}') {
            let name = &rest[..close];
            if is_value_name(name) {
                names.insert(name);
                rest = &rest[close + 1..];
            }
        }
    }
    names
}

fn is_value_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// `names` for a message: `{a}, {b}` or "no values".
fn named(names: &BTreeSet<&str>) -> String {
    if names.is_empty() {
        return "no values".to_owned();
    }
    let all: Vec<String> = names.iter().map(|n| format!("{{{n}}}")).collect();
    all.join(", ")
}

/// The constant name of a key or value: upper case, `.` and `-` as `_`.
fn constant(name: &str) -> syn::Result<String> {
    let ok = name.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
    if !ok {
        return Err(refuse(format!(
            "`{name}` makes no Rust name: a key starts with a letter and holds [A-Za-z0-9_.-]"
        )));
    }
    Ok(name.to_ascii_uppercase().replace(['.', '-'], "_"))
}

/// A refusal of the texts file, shown at the macro call.
fn refuse(message: impl core::fmt::Display) -> syn::Error {
    syn::Error::new(Span::call_site(), message)
}

/// `text` for a doc comment, its Markdown taken literally.
fn literal_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The items of the `texts` module: the file and a constant per key and
/// per value name.
fn items(file: &File) -> syn::Result<TokenStream> {
    let mut seen: BTreeMap<String, &str> = BTreeMap::new();
    let mut keys = Vec::new();
    let mut constants = Vec::new();
    let mut all_values = BTreeSet::new();
    for (key, text) in file.keys() {
        let name = constant(key).map_err(|e| refuse(format!("{}: {e}", file.path)))?;
        if let Some(other) = seen.insert(name.clone(), key) {
            return Err(refuse(format!(
                "{}: `{other}` and `{key}` both make the constant `{name}`; rename one",
                file.path
            )));
        }
        let fills = values(text);
        let filled = if fills.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = fills.iter().map(|v| format!("`{v}`")).collect();
            format!(", which fills in {}", list.join(", "))
        };
        let doc = format!("`{key}`: \"{}\"{filled}.", literal_markdown(text));
        all_values.extend(fills);
        let ident = Ident::new(&name, Span::call_site());
        constants.push(quote! {
            #[doc = #doc]
            pub const #ident: &str = #key;
        });
        keys.push(ident);
    }
    let mut seen_values: BTreeMap<String, &str> = BTreeMap::new();
    let mut value_constants = Vec::new();
    for value in all_values {
        let name = constant(value).map_err(|e| refuse(format!("{}: {e}", file.path)))?;
        if let Some(other) = seen_values.insert(name.clone(), value) {
            return Err(refuse(format!(
                "{}: the values `{other}` and `{value}` both make the constant `value::{name}`; rename one",
                file.path
            )));
        }
        let ident = Ident::new(&name, Span::call_site());
        let doc = format!("`{value}`, a value the texts fill in.");
        value_constants.push(quote! {
            #[doc = #doc]
            pub const #ident: &str = #value;
        });
    }
    let embedded = file.embedded();
    Ok(quote! {
        /// The file itself, as the loader reads it.
        pub const TOML: &str = #embedded;

        /// Every key, in order.
        pub const ALL: &[&str] = &[#(#keys),*];

        #(#constants)*

        /// The names of the values the texts fill in, for `Texts::fill`.
        pub mod value {
            #(#value_constants)*
        }
    })
}

/// Expands `texts!()` or `texts!("<path>")`: `pub mod texts` with the file
/// at the path from the crate's root, `src/texts.toml` by default.
pub(crate) fn expand(path: Option<&LitStr>) -> syn::Result<TokenStream> {
    let span = path.map_or_else(Span::call_site, LitStr::span);
    let relative = path.map_or_else(|| DEFAULT.to_owned(), LitStr::value);
    let Some(file) = File::read(&relative)? else {
        return Err(syn::Error::new(
            span,
            format!("the crate has no {relative}, the mod's texts a table per language"),
        ));
    };
    let items = items(&file).map_err(|e| syn::Error::new(span, e))?;
    Ok(quote! {
        /// The mod's texts: a constant per key, checked when the mod builds.
        pub mod texts {
            #items
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<File, String> {
        File::parse("texts.toml".to_owned(), text).map_err(|e| e.to_string())
    }

    #[test]
    fn a_key_without_an_english_text_is_refused() {
        let refused = parse("[en]\na = \"A\"\n[uk]\na = \"А\"\nb = \"Б\"\n")
            .err()
            .unwrap_or_default();
        assert!(
            refused.contains("`b` in [uk] has no text in [en]"),
            "{refused}"
        );
        assert!(parse("[uk]\na = \"А\"\n").is_err());
    }

    #[test]
    fn a_translation_filling_in_other_values_is_refused() {
        let other = parse("[en]\na = \"{name} at {at}\"\n[de]\na = \"{name} bei {wo}\"\n")
            .err()
            .unwrap_or_default();
        assert!(
            other.contains(
                "`a` in [de] fills in {name}, {wo} where its [en] text fills in {at}, {name}"
            ),
            "{other}"
        );
        let fewer = parse("[en]\na = \"{name}\"\n[de]\na = \"Name\"\n")
            .err()
            .unwrap_or_default();
        assert!(fewer.contains("fills in no values"), "{fewer}");
    }

    #[test]
    fn a_key_without_a_text_in_a_required_language_fails_the_build() {
        let file = parse("[en]\na = \"A\"\nb = \"B\"\n[uk]\na = \"А\"\n").unwrap();
        let checks = file.language_checks().to_string();
        assert!(
            checks.contains("has_required_languages (& [\"en\"])"),
            "{checks}"
        );
        assert!(checks.contains("`b` has a text in [en] only"), "{checks}");
        assert!(
            checks.contains("has_required_languages (& [\"en\" , \"uk\"])"),
            "{checks}"
        );
    }

    #[test]
    fn an_empty_text_counts_as_missing() {
        let file = parse("[en]\na = \"A\"\n[uk]\na = \"\"\n").unwrap();
        let checks = file.language_checks().to_string();
        assert!(
            checks.contains("has_required_languages (& [\"en\"])"),
            "{checks}"
        );
        assert!(checks.contains("`a` has a text in [en] only"), "{checks}");
    }

    #[test]
    fn values_reorder_repeat_and_braces_without_a_name_stay_text() {
        assert!(parse("[en]\na = \"{x} {y} {\"\n[uk]\na = \"{y}, {x}, {x} { }\"\n").is_ok());
        let names: Vec<&str> = values("{a} {b c} {} {_d}} {e").into_iter().collect();
        assert_eq!(names, ["_d", "a"]);
    }

    #[test]
    fn language_codes_merge_whatever_their_case() {
        let file = parse("[EN]\na = \"A\"\n[en]\nb = \"B\"\n").unwrap();
        let keys: Vec<&str> = file.keys().map(|(key, _)| key).collect();
        assert_eq!(keys, ["a", "b"]);
    }

    #[test]
    fn keys_become_constants_and_two_alike_are_refused() {
        assert_eq!(
            constant("config.label_language.unknown").unwrap(),
            "CONFIG_LABEL_LANGUAGE_UNKNOWN"
        );
        assert_eq!(constant("mod-name").unwrap(), "MOD_NAME");
        assert!(constant("1st").is_err());
        assert!(constant("a key").is_err());
        let file = parse("[en]\n\"a.b\" = \"A\"\n\"a_b\" = \"B\"\n").unwrap();
        let refused = items(&file)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert!(
            refused.contains("both make the constant `A_B`"),
            "{refused}"
        );
    }

    #[test]
    fn markdown_in_a_text_stays_literal_in_its_doc() {
        assert_eq!(literal_markdown("[x] `y` *z*"), "\\[x\\] \\`y\\` \\*z\\*");
    }
}
