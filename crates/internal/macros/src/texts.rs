//! `texts!()` and the texts `declare_mod!` embeds: a mod's `src/texts.toml`,
//! read and checked when the mod builds.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, LitStr};

/// Where a mod keeps its texts, from the crate's root.
pub(crate) const DEFAULT: &str = "src/texts.toml";

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

    /// The file `path` holding `text`, checked: no text is empty, and every
    /// text of a key fills in the same values. Which languages a key needs is the API's to say,
    /// in the checks of [`File::language_checks`].
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
        for key in self.keys() {
            if let Some((code, _)) = self.texts(key).find(|(_, text)| text.is_empty()) {
                return Err(refuse(format!("{path}: `{key}` in [{code}] is empty")));
            }
            let filled: Vec<(&str, BTreeSet<&str>)> = self
                .texts(key)
                .map(|(code, text)| (code, values(text)))
                .collect();
            if filled.windows(2).any(|pair| pair[0].1 != pair[1].1) {
                let each: Vec<String> = filled
                    .iter()
                    .map(|(code, names)| format!("[{code}] {}", named(names)))
                    .collect();
                return Err(refuse(format!(
                    "{path}: the texts of `{key}` fill in different values: {}",
                    each.join("; ")
                )));
            }
        }
        Ok(())
    }

    /// The keys of every table, each once, in order.
    fn keys(&self) -> impl Iterator<Item = &str> {
        self.tables
            .values()
            .flat_map(BTreeMap::keys)
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
    }

    /// The texts of `key`, by language code in order.
    fn texts<'a>(&'a self, key: &'a str) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.tables.iter().filter_map(move |(code, entries)| {
            entries.get(key).map(|text| (code.as_str(), text.as_str()))
        })
    }

    /// A constant check per language code that the API knows it, and one
    /// per key that it has a text in every language the API requires, so
    /// both lists stay in the API.
    fn language_checks(&self) -> TokenStream {
        let path = &self.path;
        let checks = self.tables.keys().map(|code| {
            let message = format!(
                "{path}: [{code}] is no language code the game knows; the codes are those of `enshwick::i18n::Language::ALL`, such as uk or pt-br"
            );
            quote! {
                const _: () = ::core::assert!(::enshwick::__private::is_language_code(#code), #message);
            }
        });
        let required = self.keys().map(|key| {
            let codes: Vec<&str> = self.texts(key).map(|(code, _)| code).collect();
            let before = format!("{path}: `{key}` has no text in [");
            let after =
                "], which every key needs: the languages of `enshwick::i18n::Language::REQUIRED`";
            quote! {
                const _: () = {
                    const MISSING: ::enshwick::__private::Codes =
                        ::enshwick::__private::missing_languages(&[#(#codes),*]);
                    const NAMES: &str = ::enshwick::__private::Codes::as_str(&MISSING);
                    ::core::assert!(
                        NAMES.is_empty(),
                        "{}",
                        ::enshwick::__private::concatcp!(#before, NAMES, #after)
                    );
                };
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
    for key in file.keys() {
        let name = constant(key).map_err(|e| refuse(format!("{}: {e}", file.path)))?;
        if let Some(other) = seen.insert(name.clone(), key) {
            return Err(refuse(format!(
                "{}: `{other}` and `{key}` both make the constant `{name}`; rename one",
                file.path
            )));
        }
        let fills: BTreeSet<&str> = file.texts(key).flat_map(|(_, text)| values(text)).collect();
        let filled = if fills.is_empty() {
            String::new()
        } else {
            let list: Vec<String> = fills.iter().map(|v| format!("`{v}`")).collect();
            format!(", which fills in {}", list.join(", "))
        };
        let texts: Vec<String> = file
            .texts(key)
            .map(|(code, text)| format!("- `{code}`: \"{}\"", literal_markdown(text)))
            .collect();
        let doc = format!("`{key}`{filled}.\n\n{}", texts.join("\n"));
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
    fn a_translation_filling_in_other_values_is_refused() {
        let other = parse("[en]\na = \"{name} at {at}\"\n[de]\na = \"{name} bei {wo}\"\n")
            .err()
            .unwrap_or_default();
        assert!(
            other.contains(
                "the texts of `a` fill in different values: [de] {name}, {wo}; [en] {at}, {name}"
            ),
            "{other}"
        );
        let fewer = parse("[en]\na = \"{name}\"\n[de]\na = \"Name\"\n")
            .err()
            .unwrap_or_default();
        assert!(fewer.contains("[de] no values; [en] {name}"), "{fewer}");
    }

    #[test]
    fn every_key_is_checked_against_the_api_s_required_languages() {
        let file = parse("[en]\na = \"A\"\n[uk]\na = \"А\"\nb = \"Б\"\n").unwrap();
        let checks = file.language_checks().to_string();
        assert!(
            checks.contains("missing_languages (& [\"en\" , \"uk\"])"),
            "{checks}"
        );
        assert!(
            checks.contains("missing_languages (& [\"uk\"])"),
            "{checks}"
        );
        assert!(checks.contains("`b` has no text in ["), "{checks}");
    }

    #[test]
    fn an_empty_text_in_any_language_is_refused() {
        for file in [
            "[en]\na = \"A\"\n[uk]\na = \"\"\n",
            "[en]\na = \"{name}\"\n[uk]\na = \"{name}\"\n[de]\na = \"\"\n",
        ] {
            let refused = parse(file).err().unwrap_or_default();
            assert!(refused.contains("is empty"), "{refused}");
        }
        let refused = parse("[en]\na = \"A\"\n[de]\na = \"\"\n")
            .err()
            .unwrap_or_default();
        assert!(
            refused.contains("texts.toml: `a` in [de] is empty"),
            "{refused}"
        );
    }

    #[test]
    fn a_key_s_doc_shows_its_text_in_every_language() {
        let file = parse("[en]\na = \"{x} A\"\n[uk]\na = \"{x} А\"\n").unwrap();
        let items = items(&file).unwrap().to_string();
        assert!(items.contains("- `en`: \\\"{x} A\\\""), "{items}");
        assert!(items.contains("- `uk`: \\\"{x} А\\\""), "{items}");
        assert!(items.contains("which fills in `x`"), "{items}");
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
        let keys: Vec<&str> = file.keys().collect();
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
