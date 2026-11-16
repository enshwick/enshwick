//! What a package's sources name, for the scope of a check: the name its
//! library gives itself, which shadows a dev-dependency of that name
//! ([`super::Package::doc_deps`]), and whether its code names a type.

use std::path::Path;

use proc_macro2::{TokenStream, TokenTree};

/// Cargo's build output, which a package's folder may hold.
const TARGET: &str = "target";

/// The words before the name a crate gives itself.
const SELF_ALIAS: [&str; 4] = ["extern", "crate", "self", "as"];

/// The name `lib`, the text of a library's root, gives the crate itself
/// (`extern crate self as enshwick;`, so its macros' paths resolve inside
/// it): its own code cannot reach a dependency of that name, macro
/// expansions included. `None` when it gives none or does not lex.
pub(crate) fn self_alias(lib: &str) -> Option<String> {
    let tokens: Vec<TokenTree> = lib.parse::<TokenStream>().ok()?.into_iter().collect();
    tokens.windows(SELF_ALIAS.len() + 1).find_map(|window| {
        let (name, words) = window.split_last()?;
        let alias = words
            .iter()
            .zip(SELF_ALIAS)
            .all(|(token, word)| matches!(token, TokenTree::Ident(ident) if ident == word));
        match name {
            TokenTree::Ident(name) if alias => Some(name.to_string()),
            _ => None,
        }
    })
}

/// Whether a Rust file under `dir` names `word` as an identifier, in code
/// or a macro body (a doc comment or a string names nothing); a file or
/// folder that does not read or lex counts as one that does, so a doubt
/// keeps what depends on the answer.
pub(crate) fn names(dir: &Path, word: &str) -> bool {
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return true;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                if path.file_name().is_none_or(|name| name != TARGET) {
                    dirs.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && std::fs::read_to_string(&path).map_or(true, |text| names_in(&text, word))
            {
                return true;
            }
        }
    }
    false
}

/// Whether `text` names `word` as an identifier; `true` when it does not
/// lex.
fn names_in(text: &str, word: &str) -> bool {
    let Ok(tokens) = text.parse::<TokenStream>() else {
        return true;
    };
    let mut streams = vec![tokens];
    while let Some(stream) = streams.pop() {
        for token in stream {
            match token {
                TokenTree::Ident(ident) if ident == word => return true,
                TokenTree::Group(group) => streams.push(group.stream()),
                _ => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_library_that_names_itself_gives_that_name() {
        let lib = "//! Docs.\n#![forbid(unsafe_code)]\n#[cfg(any())]\nextern crate self as enshwick;\npub mod ui;";
        assert_eq!(self_alias(lib).as_deref(), Some("enshwick"));
    }

    #[test]
    fn a_library_that_does_not_name_itself_or_does_not_lex_gives_none() {
        for lib in [
            "extern crate enshwick as api;\nfn f() { enshwick::x(); }",
            "/// extern crate self as enshwick;\nfn f() {}",
            "mod m { extern crate self as enshwick; }",
            "extern crate self as enshwick; fn f() { \"open",
        ] {
            assert_eq!(self_alias(lib), None, "{lib}");
        }
    }

    #[test]
    fn code_or_a_macro_body_names_a_word_and_a_doc_or_a_string_does_not() {
        for text in [
            "fn f() { let host = FakeHost::new(); }",
            "macro_rules! m { () => { FakeHost::new() }; }",
            "fn f() { \"open",
        ] {
            assert!(names_in(text, "FakeHost"), "{text}");
        }
        for text in [
            "/// Runs on a [`FakeHost`].\nfn f() {}",
            "const FAKE_HOST: &str = \"FakeHost\";",
        ] {
            assert!(!names_in(text, "FakeHost"), "{text}");
        }
    }

    #[test]
    fn a_name_anywhere_under_the_folder_counts_and_its_target_does_not() {
        let dir = std::env::temp_dir().join(format!("xtask-names-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/ui")).unwrap();
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub mod ui;").unwrap();
        std::fs::write(dir.join("target/debug/out.rs"), "use x::FakeHost;").unwrap();
        let before = names(&dir, "FakeHost");
        std::fs::write(dir.join("src/ui/tests.rs"), "use x::FakeHost;").unwrap();
        let after = names(&dir, "FakeHost");
        let missing = names(&dir.join("missing"), "FakeHost");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!before && after && missing);
    }
}
