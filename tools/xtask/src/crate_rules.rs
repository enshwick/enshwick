//! The rules a crate's manifest and sources are held to, read from
//! `cargo metadata` without compiling: a crate whose package has no unsafe
//! code forbids it at its root, the `test` feature is never on by default or
//! through a normal or build dependency, only where a target asks for it,
//! and a package that builds a DLL, such as a mod, names its version.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The test-only feature of the API and of any crate that has one.
const TEST: &str = "test";
/// The crate kinds whose root is a crate the rules apply to; tests,
/// benches, examples and build scripts are not.
const CRATE_KINDS: [&str; 7] = [
    "lib",
    "rlib",
    "dylib",
    "cdylib",
    "staticlib",
    "proc-macro",
    "bin",
];

/// The problems of the workspace's crates, one line each.
///
/// # Errors
///
/// When `cargo metadata` does not run or its output does not read.
pub(crate) fn check() -> Result<Vec<String>, String> {
    let output = Command::new(crate::cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let mut problems = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or_default();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap_or_default());
        let dir = manifest.parent().unwrap_or(Path::new("."));
        problems.extend(test_feature(name, package));
        let builds_a_dll = package["targets"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|target| {
                target["kind"]
                    .as_array()
                    .is_some_and(|k| k.iter().any(|k| k == "cdylib"))
            });
        if builds_a_dll && !names_version(&read(manifest)?) {
            problems.push(format!(
                "{name}: it builds a DLL, so [package] names the version the loader shows"
            ));
        }
        if !has_unsafe(dir)? {
            problems.extend(
                roots(package)
                    .into_iter()
                    .filter(|root| !forbids_unsafe(root))
                    .map(|root| {
                        format!(
                            "{name}: no unsafe code, so {} declares #![forbid(unsafe_code)]",
                            shown(&root, dir)
                        )
                    }),
            );
        }
    }
    Ok(problems)
}

/// The problems of `package` with the `test` feature: on by default, or
/// asked of a normal or build dependency.
fn test_feature(name: &str, package: &serde_json::Value) -> Vec<String> {
    let mut problems = Vec::new();
    let features = &package["features"];
    let mut on: Vec<&str> = vec!["default"];
    let mut seen = 0;
    while seen < on.len() {
        let feature = on[seen];
        seen += 1;
        for enabled in features[feature]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            if enabled == TEST || enabled.rsplit_once('/').is_some_and(|(_, f)| f == TEST) {
                problems.push(format!(
                    "{name}: the default features turn on {enabled}; a target that needs it sets it"
                ));
            } else if !enabled.contains(['/', ':']) && !on.contains(&enabled) {
                on.push(enabled);
            }
        }
    }
    for dependency in package["dependencies"].as_array().into_iter().flatten() {
        let kind = dependency["kind"].as_str().unwrap_or("normal");
        let asks = dependency["features"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|feature| feature == TEST);
        if asks && kind != "dev" {
            problems.push(format!(
                "{name}: its {kind} dependency {} turns on `test`; only a dev-dependency may",
                dependency["name"].as_str().unwrap_or_default()
            ));
        }
    }
    problems
}

/// Whether the `Cargo.toml` `text` gives `[package]` a version of its own or
/// the workspace's.
fn names_version(text: &str) -> bool {
    let mut in_package = false;
    text.lines().map(str::trim).any(|line| {
        if line.starts_with('[') {
            in_package = line == "[package]";
            return false;
        }
        in_package
            && line
                .strip_prefix("version")
                .is_some_and(|rest| rest.trim_start().starts_with(['=', '.']))
    })
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The roots of the crates of `package` the rules apply to.
fn roots(package: &serde_json::Value) -> Vec<PathBuf> {
    package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|target| {
            target["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|kind| CRATE_KINDS.iter().any(|k| kind == k))
        })
        .filter_map(|target| target["src_path"].as_str().map(PathBuf::from))
        .collect()
}

/// Whether a Rust file under `dir` uses `unsafe` as a keyword. A folder with
/// a `Cargo.toml` of its own, and `target`, belong to no package here.
fn has_unsafe(dir: &Path) -> Result<bool, String> {
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries {
            let path = entry
                .map_err(|e| format!("{}: {e}", folder.display()))?
                .path();
            if path.is_dir() {
                let own = path.join("Cargo.toml").is_file() || path.ends_with("target");
                if !own {
                    folders.push(path);
                }
            } else if path.extension().is_some_and(|ext| ext == "rs") && uses_unsafe(&read(&path)?)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Whether the Rust `text` has the `unsafe` keyword outside comments,
/// strings and character literals.
fn uses_unsafe(text: &str) -> bool {
    code(text)
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|word| word == "unsafe")
}

/// The Rust `text` with each comment, string and character literal turned
/// into one space.
fn code(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                at = text[at..].find('\n').map_or(bytes.len(), |end| at + end);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let mut depth = 0_usize;
                while at < bytes.len() {
                    if bytes[at..].starts_with(b"/*") {
                        depth += 1;
                        at += 2;
                    } else if bytes[at..].starts_with(b"*/") {
                        depth -= 1;
                        at += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        at += 1;
                    }
                }
            }
            b'"' => at = string_end(bytes, at + 1, None),
            // A character literal, `'"'` or `'\''`; the quote of a lifetime
            // stays.
            b'\'' => {
                let rest = &text[at + 1..];
                let len = match rest.chars().next() {
                    Some('\\') => rest
                        .get(2..)
                        .and_then(|escaped| escaped.find('\''))
                        .map(|end| end + 4),
                    Some(c) if rest[c.len_utf8()..].starts_with('\'') => Some(c.len_utf8() + 2),
                    _ => None,
                };
                if let Some(len) = len {
                    at += len;
                } else {
                    at += 1;
                    out.push(b'\'');
                    continue;
                }
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                    at += 1;
                }
                out.extend_from_slice(&bytes[start..at]);
                // A raw string: `r"..."`, `r#"..."#`, `br"..."`.
                if matches!(&text[start..at], "r" | "br" | "cr") {
                    let hashes = bytes[at..].iter().take_while(|&&b| b == b'#').count();
                    if bytes.get(at + hashes) == Some(&b'"') {
                        at = string_end(bytes, at + hashes + 1, Some(hashes));
                        out.push(b' ');
                    }
                }
                continue;
            }
            byte => {
                at += 1;
                out.push(byte);
                continue;
            }
        }
        out.push(b' ');
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Where the string literal whose text starts at `at` ends: past its closing
/// quote, and for a raw one (`Some` of its hash count) its hashes; escapes
/// count only in a string that is not raw.
fn string_end(bytes: &[u8], mut at: usize, raw: Option<usize>) -> usize {
    let hashes = raw.unwrap_or(0);
    while at < bytes.len() {
        match bytes[at] {
            b'\\' if raw.is_none() => at += 2,
            b'"' if bytes.len() > at + hashes
                && bytes[at + 1..=at + hashes].iter().all(|&b| b == b'#') =>
            {
                return at + 1 + hashes;
            }
            _ => at += 1,
        }
    }
    bytes.len()
}

/// Whether the crate root at `path` forbids unsafe code.
fn forbids_unsafe(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|text| forbids(&text))
}

/// Whether an inner attribute of the Rust `text` forbids `unsafe_code`,
/// alone or among other lints, on one line or several. Under `cfg_attr` it
/// counts only with the predicate `not(test)`, which leaves the crate's
/// own code forbidden.
fn forbids(text: &str) -> bool {
    let code: String = code(text).chars().filter(|c| !c.is_whitespace()).collect();
    code.match_indices("#![").any(|(start, _)| {
        let mut depth = 0_usize;
        let end = code[start + 2..]
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map_or(code.len(), |(at, _)| start + 2 + at);
        let attribute = &code[start + 3..end];
        let attributes = match attribute.strip_prefix("cfg_attr(not(test),") {
            Some(rest) => top_level(rest.strip_suffix(')').unwrap_or(rest)),
            None => vec![attribute],
        };
        attributes.iter().any(|attribute| {
            attribute
                .strip_prefix("forbid(")
                .and_then(|lints| lints.strip_suffix(')'))
                .is_some_and(|lints| lints.split(',').any(|lint| lint == "unsafe_code"))
        })
    })
}

/// The items of the comma-separated `list`, split outside parentheses.
fn top_level(list: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let (mut depth, mut start) = (0_usize, 0);
    for (at, c) in list.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                items.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    items.push(&list[start..]);
    items
}

/// `path` as the package folder `dir` names it.
fn shown(path: &Path, dir: &Path) -> String {
    path.strip_prefix(dir).unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_counts_as_a_keyword_outside_comments_and_literals() {
        assert!(uses_unsafe("let x = unsafe { read(p) };"));
        assert!(uses_unsafe("#[unsafe(no_mangle)]"));
        assert!(uses_unsafe("unsafe impl Send for T {}"));
        assert!(!uses_unsafe("// unsafe { read(p) }"));
        assert!(!uses_unsafe("/// Needs no unsafe code."));
        assert!(!uses_unsafe("let unsafe_count = 0;"));
        assert!(!uses_unsafe("/* unsafe { /* nested */ read(p) } */"));
        assert!(!uses_unsafe(r#"let s = "unsafe \" unsafe";"#));
        assert!(!uses_unsafe(r##"let s = r#"unsafe " unsafe"#;"##));
        assert!(!uses_unsafe("let q = b'\"'; let s = \"unsafe\";"));
        assert!(uses_unsafe(
            "fn f<'a>(x: &'a u8) { let q = '\\''; unsafe {} }"
        ));
    }

    #[test]
    fn a_default_test_feature_and_a_normal_dependency_asking_for_it_are_problems() {
        let package = serde_json::json!({
            "features": {
                "default": ["extra"],
                "extra": ["enshwick/test"],
                "test": [],
            },
            "dependencies": [
                { "name": "enshwick", "kind": null, "features": ["test"] },
                { "name": "enshwick", "kind": "dev", "features": ["test"] },
                { "name": "builder", "kind": "build", "features": ["test"] },
            ],
        });
        assert_eq!(
            test_feature("a", &package),
            [
                "a: the default features turn on enshwick/test; a target that needs it sets it",
                "a: its normal dependency enshwick turns on `test`; only a dev-dependency may",
                "a: its build dependency builder turns on `test`; only a dev-dependency may",
            ]
        );
    }

    #[test]
    fn a_test_feature_off_by_default_and_asked_by_dev_dependencies_passes() {
        let package = serde_json::json!({
            "features": { "default": ["std"], "std": [], "test": ["std"] },
            "dependencies": [
                { "name": "enshwick", "kind": "dev", "features": ["test", "unstable"] },
                { "name": "serde", "kind": null, "features": ["derive"] },
            ],
        });
        assert!(test_feature("a", &package).is_empty());
    }

    #[test]
    fn a_version_counts_only_in_the_package_table() {
        assert!(names_version(
            "[package]\nname = \"a\"\nversion = \"0.1.0\"\n"
        ));
        assert!(names_version("[package]\nversion.workspace = true\n"));
        assert!(!names_version(
            "[package]\nname = \"a\"\n\n[dependencies]\nversion = \"1\"\n"
        ));
    }

    #[test]
    fn a_crate_root_forbids_unsafe_by_an_inner_attribute() {
        assert!(forbids("//! A crate.\n\n#![forbid(unsafe_code)]\n"));
        assert!(forbids("#![forbid(missing_docs, unsafe_code)]\n"));
        assert!(forbids(
            "#![forbid(\n    unsafe_code,\n    missing_docs,\n)]\n"
        ));
        assert!(forbids("#![cfg_attr(not(test), forbid(unsafe_code))]\n"));
        assert!(!forbids("//! #![forbid(unsafe_code)]\n"));
        assert!(!forbids(
            "#![deny(unsafe_code)]\n#![forbid(missing_docs)]\n"
        ));
        assert!(!forbids("#![forbid(unsafe_code_extra)]\n"));
        assert!(!forbids("const S: &str = \"#![forbid(unsafe_code)]\";\n"));
        assert!(!forbids("#![cfg_attr(test, forbid(unsafe_code))]\n"));
        assert!(!forbids(
            "#![cfg_attr(feature = \"x\", forbid(unsafe_code))]\n"
        ));
        assert!(forbids(
            "#![cfg_attr(not(test), deny(missing_docs), forbid(unsafe_code))]\n"
        ));
    }
}
