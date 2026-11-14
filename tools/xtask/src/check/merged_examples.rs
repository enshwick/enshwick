//! Every library whose examples rustdoc merges into one binary holds the
//! sentinel example ([`SENTINEL`]), which fails when they are built one by
//! one instead. rustdoc falls back to that silently when one example does
//! not merge, and a crate's doctests then take minutes instead of seconds.

use std::path::{Path, PathBuf};

use crate::affected::Package;

/// The sentinel's assertion: a merged example compiles in a module of
/// rustdoc's bundle crate, one built alone is the crate `rust_out`.
const SENTINEL: &str = r#"assert_ne!(module_path!(), "rust_out", "#;

/// The item the sentinel documents, which its test's name carries.
pub(crate) const SENTINEL_NAME: &str = "MergedExamples";

/// The item a crate root adds, as the problem shows it.
const SENTINEL_ITEM: &str = r#"/// Fails when rustdoc builds this crate's examples one by one, which it does
/// without a word when one of them does not merge.
///
/// ```
/// assert_ne!(module_path!(), "rust_out", "an example does not merge: \
///     RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='-Zunstable-options --merge-doctests=yes' \
///     cargo test --doc names it");
/// ```
#[cfg(doctest)]
struct MergedExamples;"#;

/// The fence tokens rustdoc reads as Rust attributes, besides `rust`,
/// `ignore`, the `ignore-<target>`, `edition` and error code ones.
const RUST_TOKENS: [&str; 5] = [
    "should_panic",
    "no_run",
    "compile_fail",
    "standalone_crate",
    "test_harness",
];

/// The tokens of examples rustdoc always builds alone.
const ALONE: [&str; 3] = ["compile_fail", "standalone_crate", "test_harness"];

/// The edition whose examples rustdoc merges.
const MERGED_EDITION: &str = "edition2024";

/// The problems of the libraries among `packages` under `root`, one line
/// each.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let mut problems = Vec::new();
    for package in packages.iter().filter(|package| package.lib.is_some()) {
        let src = root.join(&package.dir).join("src");
        let mut sources = Vec::new();
        if let Err(error) = rust_files(&src, &mut sources) {
            problems.push(error);
            continue;
        }
        let mut merged = 0;
        for source in &sources {
            match std::fs::read_to_string(source) {
                Ok(text) => merged += merged_count(&text),
                Err(error) => problems.push(format!("{}: {error}", source.display())),
            }
        }
        let lib = format!("{}/src/lib.rs", package.dir);
        let guarded = std::fs::read_to_string(root.join(&lib))
            .is_ok_and(|text| text.lines().any(|line| line.contains(SENTINEL)));
        if merged > 0 && !guarded {
            problems.push(format!(
                "{lib}: {merged} examples build as one binary, and nothing fails when rustdoc builds them one by one; add\n{SENTINEL_ITEM}"
            ));
        }
    }
    problems
}

/// How many examples of the doc comments of the Rust `text` rustdoc merges.
fn merged_count(text: &str) -> usize {
    fences(text).iter().filter(|info| merges(info)).count()
}

/// Whether rustdoc compiles the example of fence `info` into the merged
/// binary. By rustdoc's rule a fence is Rust without a token of another
/// language, or with `rust` or a Rust attribute before the first such
/// token; `ignore` compiles nothing.
fn merges(info: &str) -> bool {
    let (mut other, mut rust, mut ignored, mut alone) = (false, false, false, false);
    for token in info.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
        match token {
            "rust" => rust = true,
            "ignore" => ignored = true,
            _ if token.starts_with("edition") => alone |= token != MERGED_EDITION,
            _ if RUST_TOKENS.contains(&token)
                || token.starts_with("ignore-")
                || is_error_code(token) =>
            {
                rust |= !other;
                alone |= ALONE.contains(&token);
            }
            _ => other = true,
        }
    }
    !ignored && !alone && (rust || !other)
}

/// What follows the opening fence of each code block of the doc comments
/// (`///`, `//!`) of the Rust `text`, trimmed.
fn fences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let Some(code) = doc_line(line).map(str::trim_start) else {
            open = false;
            continue;
        };
        if code.starts_with("```") {
            if !open {
                found.push(code.trim_start_matches('`').trim().to_owned());
            }
            open = !open;
        }
    }
    found
}

/// The text of a doc comment `line` after its marker.
fn doc_line(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let marked = ["///", "//!"]
        .iter()
        .find(|marker| rest.starts_with(*marker) && !rest.starts_with("////"))?;
    Some(&rest[marked.len()..])
}

/// Whether `token` is an error code a `compile_fail` example expects, such
/// as `E0428`.
fn is_error_code(token: &str) -> bool {
    token.len() == 5 && token.starts_with('E') && token.bytes().skip(1).all(|b| b.is_ascii_digit())
}

/// Adds the Rust files under `dir` to `out`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_rust_and_runnable_attributes_merge_and_alone_or_foreign_ones_do_not() {
        for info in ["", "rust", "no_run", "should_panic", "rust,edition2024"] {
            assert!(merges(info), "{info:?}");
        }
        for info in [
            "compile_fail",
            "rust,standalone_crate",
            "standalone_crate",
            "test_harness",
            "compile_fail,E0428",
            "edition2021",
            "ignore",
            "toml",
            "text",
        ] {
            assert!(!merges(info), "{info:?}");
        }
    }

    #[test]
    fn only_the_opening_fences_of_doc_comment_blocks_count() {
        let text = "/// ```\n/// let a = 1;\n/// ```\n//! ```compile_fail\n//! x\n//! ```\n// ```\nfn a() {}\n/// ```toml\n/// a = 1\n/// ```\n";
        assert_eq!(fences(text), ["", "compile_fail", "toml"]);
        assert_eq!(merged_count(text), 1);
    }

    #[test]
    fn a_block_left_open_ends_with_its_comment() {
        let text = "/// ```\n/// let a = 1;\nfn a() {}\n/// ```\n/// let b = 2;\n/// ```\n";
        assert_eq!(fences(text), ["", ""]);
    }

    #[test]
    fn the_item_a_problem_shows_holds_the_sentinel_under_its_name() {
        assert!(SENTINEL_ITEM.lines().any(|line| line.contains(SENTINEL)));
        assert!(SENTINEL_ITEM.ends_with(&format!("struct {SENTINEL_NAME};")));
    }

    fn package(dir: &str) -> Package {
        Package {
            name: dir.to_owned(),
            version: "0.0.0-dev".to_owned(),
            dir: dir.to_owned(),
            deps: Vec::new(),
            lib: Some(dir.to_owned()),
            windows_only: false,
            windows_tests: false,
            unstable: false,
        }
    }

    #[test]
    fn a_library_with_merged_examples_and_no_sentinel_is_a_problem() {
        let root = std::env::temp_dir().join(format!("xtask-merged-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "bare/src/lib.rs",
            "/// ```\n/// let a = 1;\n/// ```\nfn a() {}\n",
        );
        write(
            "alone/src/lib.rs",
            "/// ```compile_fail\n/// x\n/// ```\nfn a() {}\n",
        );
        write("guarded/src/lib.rs", &format!("{SENTINEL_ITEM}\n"));
        write(
            "guarded/src/deep/a.rs",
            "/// ```\n/// let a = 1;\n/// ```\nfn a() {}\n",
        );
        let problems = check(
            &root,
            &[package("bare"), package("alone"), package("guarded")],
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].starts_with("bare/src/lib.rs: 1 examples build as one binary"));
    }
}
