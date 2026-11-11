//! The rules a crate's manifest and sources are held to, read from
//! `cargo metadata` without compiling: a crate whose package has no unsafe
//! code forbids it at its root, the `test` feature is never on by default or
//! through a normal or build dependency, only where a target asks for it,
//! only the API crate's `test` feature reaches the fake loader, whose crate
//! alone turns on the core's `fake` and core alone that of each crate below
//! it, a platform crate depends on the API
//! crate only for its tests, a package that builds a DLL, such as a mod, names its version, features
//! name what they add, `anyhow` stays in binaries, `rust-version` is the
//! pinned toolchain's, and a published crate's path dependencies carry
//! versions.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::affected::Package;

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

/// The problems of the workspace's crates, one line each; `packages` says
/// where each one lies.
///
/// # Errors
///
/// When `cargo metadata` does not run or its output does not read.
pub(crate) fn check(packages: &[Package]) -> Result<Vec<String>, String> {
    let output = Command::new(crate::cargo())
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let toolchain = toolchain(&read(&crate::workspace_root().join("rust-toolchain.toml"))?)?;
    let mut problems = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or_default();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap_or_default());
        let dir = manifest.parent().unwrap_or(Path::new("."));
        problems.extend(test_feature(name, package));
        problems.extend(fake_reach(name, package));
        if packages
            .iter()
            .any(|p| p.name == name && in_platform_dir(&p.dir))
        {
            problems.extend(facade_use(name, package));
        }
        problems.extend(manifest_facts(name, package, &toolchain));
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

/// The fake loader's crate, which only [`FACADE`] depends on, through its
/// [`TEST`] feature alone, so the fake's features are always the facade's.
const FAKE_CRATE: &str = "enshwick-fake";
/// The API crate mods depend on.
const FACADE: &str = "enshwick";
/// The crate whose test-only feature [`FAKE_FEATURE`] only [`FAKE_CRATE`]
/// turns on.
const CORE: &str = "enshwick-core";
/// What [`CORE`] builds for the fake: its hooks and a mod life per install.
const FAKE_FEATURE: &str = "fake";

/// The problems of `package` with the fake loader: a crate other than the
/// facade depending on it, the facade reaching it past its `test` feature,
/// or a crate turning on a `fake` feature it may not ([`fake_enabler`]).
fn fake_reach(name: &str, package: &serde_json::Value) -> Vec<String> {
    let mut problems = Vec::new();
    for (feature, enabled) in package["features"].as_object().into_iter().flatten() {
        for enabled in enabled
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            let target = enabled.strip_prefix("dep:").unwrap_or(enabled);
            let (dependency, wanted) = target.split_once('/').unwrap_or((target, ""));
            let weak = dependency.ends_with('?');
            let dependency = dependency.trim_end_matches('?');
            if dependency == FAKE_CRATE && !weak && feature != TEST {
                problems.push(format!(
                    "{name}: its feature `{feature}` turns on {FAKE_CRATE}; only `{TEST}` may"
                ));
            }
            if let Some(enabler) = fake_enabler(dependency)
                && wanted == FAKE_FEATURE
                && name != enabler
            {
                problems.push(format!(
                    "{name}: its feature `{feature}` turns on {dependency}/{FAKE_FEATURE}; only {enabler} may"
                ));
            }
        }
    }
    for dependency in package["dependencies"].as_array().into_iter().flatten() {
        let kind = dependency["kind"].as_str().unwrap_or("normal");
        match dependency["name"].as_str().unwrap_or_default() {
            FAKE_CRATE if name != FACADE => problems.push(format!(
                "{name}: it depends on {FAKE_CRATE}; a test reaches the fake through {FACADE}'s `{TEST}`"
            )),
            FAKE_CRATE if dependency["optional"] != true => problems.push(format!(
                "{name}: its {kind} dependency {FAKE_CRATE} is not optional; only `{TEST}` may turn it on"
            )),
            other => {
                if let Some(enabler) = fake_enabler(other)
                    && name != enabler
                    && dependency["features"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|feature| feature == FAKE_FEATURE)
                {
                    problems.push(format!(
                        "{name}: its {kind} dependency {other} turns on `{FAKE_FEATURE}`; only {enabler} may"
                    ));
                }
            }
        }
    }
    problems
}

/// The one crate that may turn on `dependency`'s test-only `fake` feature:
/// the fake loader for core, core for every internal crate below it.
fn fake_enabler(dependency: &str) -> Option<&'static str> {
    match dependency {
        CORE => Some(FAKE_CRATE),
        FAKE_CRATE | FACADE => None,
        below if below.starts_with("enshwick-") => Some(CORE),
        _ => None,
    }
}

/// The workspace folders of the platform's own crates, which name the
/// internal crate they need; the mods in `examples/` depend on [`FACADE`].
const PLATFORM_DIRS: [&str; 2] = ["crates", "tools"];

/// Whether the package folder `dir` (relative to the workspace, with `/`)
/// lies in one of the [`PLATFORM_DIRS`].
fn in_platform_dir(dir: &str) -> bool {
    dir.split('/')
        .next()
        .is_some_and(|first| PLATFORM_DIRS.contains(&first))
}

/// The problems of `package`, a platform crate, depending on [`FACADE`]
/// other than for its tests: it names the layer it needs, so the facade
/// stays what only mods build on.
fn facade_use(name: &str, package: &serde_json::Value) -> Vec<String> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|dependency| dependency["name"] == FACADE && dependency["kind"] != "dev")
        .map(|dependency| {
            format!(
                "{name}: its {} dependency {FACADE} is the mods' crate; name the internal crate it needs, such as {CORE}, and keep {FACADE} for tests",
                dependency["kind"].as_str().unwrap_or("normal")
            )
        })
        .collect()
}

/// The feature prefixes that say how a feature adds instead of what it adds.
const FEATURE_PREFIXES: [&str; 4] = ["use-", "use_", "with-", "with_"];

/// The problems of `package`'s manifest facts: a feature named for how it
/// adds, `anyhow` in a package without a binary or in the API crate, a
/// missing `rust-version` or one other than the `toolchain`'s major and
/// minor, and a path dependency without a version in a crate that publishes.
///
/// `anyhow` belongs to binaries, and a package with both a library and a
/// binary may use it in its library half too.
fn manifest_facts(name: &str, package: &serde_json::Value, toolchain: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for feature in package["features"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(f, _)| f)
    {
        if FEATURE_PREFIXES
            .iter()
            .any(|prefix| feature.starts_with(prefix))
        {
            problems.push(format!(
                "{name}: the feature `{feature}` is named for how it adds; name what it adds"
            ));
        }
    }
    let has_binary = package["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|target| {
            target["kind"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "bin"))
        });
    let publishes = package["publish"]
        .as_array()
        .is_none_or(|to| !to.is_empty());
    for dependency in package["dependencies"].as_array().into_iter().flatten() {
        let kind = dependency["kind"].as_str().unwrap_or("normal");
        let dependency_name = dependency["name"].as_str().unwrap_or_default();
        if dependency_name == "anyhow" && kind == "normal" && (!has_binary || name == "enshwick") {
            problems.push(format!(
                "{name}: anyhow is for binaries; a library returns a thiserror type"
            ));
        }
        if publishes
            && kind != "dev"
            && dependency["path"].is_string()
            && dependency["req"].as_str() == Some("*")
        {
            problems.push(format!(
                "{name}: its path dependency {dependency_name} carries no version, which publishing needs"
            ));
        }
    }
    match package["rust_version"].as_str() {
        None => problems.push(format!(
            "{name}: no rust-version; it names the pinned toolchain {toolchain}"
        )),
        Some(declared) if major_minor(declared) != toolchain => problems.push(format!(
            "{name}: rust-version {declared} is not the pinned toolchain {toolchain}"
        )),
        Some(_) => {}
    }
    problems
}

/// The major and minor of the channel in the `rust-toolchain.toml` `text`.
fn toolchain(text: &str) -> Result<String, String> {
    let file: toml::Table =
        toml::from_str(text).map_err(|e| format!("rust-toolchain.toml: {}", e.message()))?;
    file.get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .map(major_minor)
        .ok_or_else(|| "rust-toolchain.toml names no channel".to_owned())
}

/// `version`'s major and minor: `1.99` of `1.99.1`.
fn major_minor(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
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
pub(crate) fn code(text: &str) -> String {
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
    fn the_facade_reaching_the_fake_through_test_and_the_fake_asking_core_for_fake_pass() {
        let facade = serde_json::json!({
            "features": {
                "test": ["dep:enshwick-fake"],
                "unstable": ["enshwick-core/unstable", "enshwick-fake?/unstable"],
            },
            "dependencies": [{ "name": "enshwick-fake", "kind": null, "optional": true }],
        });
        let fake = serde_json::json!({
            "features": { "unstable": ["enshwick-core/unstable"] },
            "dependencies": [{ "name": "enshwick-core", "kind": null, "features": ["fake"] }],
        });
        assert!(fake_reach("enshwick", &facade).is_empty());
        assert!(fake_reach("enshwick-fake", &fake).is_empty());
    }

    #[test]
    fn only_core_turns_on_the_fake_of_a_crate_below_it() {
        let asks = |by: &str| {
            fake_reach(
                by,
                &serde_json::json!({
                    "features": { "fake": ["enshwick-base/fake"] },
                    "dependencies": [{ "name": "enshwick-base", "kind": null, "features": ["fake"] }],
                }),
            )
        };
        assert!(asks("enshwick-core").is_empty());
        assert_eq!(fake_enabler("enshwick-base"), Some(CORE));
        assert_eq!(fake_enabler(CORE), Some(FAKE_CRATE));
        for none in [FAKE_CRATE, FACADE, "serde"] {
            assert_eq!(fake_enabler(none), None, "{none}");
        }
        assert_eq!(
            asks("enshwick-loader"),
            [
                "enshwick-loader: its feature `fake` turns on enshwick-base/fake; only enshwick-core may",
                "enshwick-loader: its normal dependency enshwick-base turns on `fake`; only enshwick-core may",
            ]
        );
    }

    #[test]
    fn the_fake_reached_past_the_facade_s_test_feature_is_a_problem() {
        let facade = serde_json::json!({
            "features": { "test": [], "unstable": ["enshwick-fake/unstable"] },
            "dependencies": [{ "name": "enshwick-fake", "kind": null, "optional": false }],
        });
        assert_eq!(
            fake_reach("enshwick", &facade),
            [
                "enshwick: its feature `unstable` turns on enshwick-fake; only `test` may",
                "enshwick: its normal dependency enshwick-fake is not optional; only `test` may turn it on",
            ]
        );
        let other = serde_json::json!({
            "features": { "extra": ["enshwick-core?/fake"] },
            "dependencies": [
                { "name": "enshwick-fake", "kind": "dev" },
                { "name": "enshwick-core", "kind": "dev", "features": ["fake"] },
            ],
        });
        assert_eq!(
            fake_reach("a", &other),
            [
                "a: its feature `extra` turns on enshwick-core/fake; only enshwick-fake may",
                "a: it depends on enshwick-fake; a test reaches the fake through enshwick's `test`",
                "a: its dev dependency enshwick-core turns on `fake`; only enshwick-fake may",
            ]
        );
    }

    #[test]
    fn the_crates_and_tools_folders_hold_the_platform_and_examples_do_not() {
        assert!(in_platform_dir("crates/internal/loader"));
        assert!(in_platform_dir("tools/xtask"));
        assert!(!in_platform_dir("examples/horn"));
        assert!(!in_platform_dir(""));
        assert!(!in_platform_dir("crates-old/a"));
    }

    #[test]
    fn a_platform_crate_depends_on_the_facade_only_for_its_tests() {
        let loader = serde_json::json!({
            "dependencies": [
                { "name": "enshwick", "kind": null },
                { "name": "enshwick", "kind": "build" },
                { "name": "enshwick", "kind": "dev", "features": ["test"] },
                { "name": "enshwick-core", "kind": null },
                { "name": "enshwick", "kind": null, "rename": "api" },
                { "name": "enshwick", "kind": null, "target": "cfg(windows)" },
                { "name": "enshwick", "kind": null, "optional": true },
            ],
        });
        let normal = "loader: its normal dependency enshwick is the mods' crate; name the internal crate it needs, such as enshwick-core, and keep enshwick for tests";
        assert_eq!(
            facade_use("loader", &loader),
            [
                normal,
                "loader: its build dependency enshwick is the mods' crate; name the internal crate it needs, such as enshwick-core, and keep enshwick for tests",
                normal,
                normal,
                normal,
            ]
        );
    }

    #[test]
    fn features_named_for_how_anyhow_in_a_library_and_an_old_rust_version_are_problems() {
        let library = serde_json::json!({
            "features": { "use-serde": [], "with_std": [], "serde": [] },
            "targets": [{ "kind": ["lib"] }],
            "publish": null,
            "rust_version": "1.98",
            "dependencies": [
                { "name": "anyhow", "kind": null, "req": "^1" },
                { "name": "enshwick-gamedata", "kind": null, "req": "*", "path": "../gamedata" },
                { "name": "helper", "kind": "dev", "req": "*", "path": "../helper" },
            ],
        });
        assert_eq!(
            manifest_facts("a", &library, "1.99"),
            [
                "a: the feature `use-serde` is named for how it adds; name what it adds",
                "a: the feature `with_std` is named for how it adds; name what it adds",
                "a: anyhow is for binaries; a library returns a thiserror type",
                "a: its path dependency enshwick-gamedata carries no version, which publishing needs",
                "a: rust-version 1.98 is not the pinned toolchain 1.99",
            ]
        );
    }

    #[test]
    fn anyhow_in_a_binary_and_an_unpublished_path_dependency_pass() {
        let binary = serde_json::json!({
            "features": { "serde": [] },
            "targets": [{ "kind": ["lib"] }, { "kind": ["bin"] }],
            "publish": [],
            "rust_version": "1.99.1",
            "dependencies": [
                { "name": "anyhow", "kind": null, "req": "^1" },
                { "name": "enshwick", "kind": null, "req": "*", "path": "../enshwick" },
            ],
        });
        assert!(manifest_facts("cargo-enshwick", &binary, "1.99").is_empty());
        assert_eq!(
            manifest_facts("enshwick", &binary, "1.99"),
            ["enshwick: anyhow is for binaries; a library returns a thiserror type"]
        );
    }

    #[test]
    fn a_missing_rust_version_is_a_problem() {
        let package = serde_json::json!({
            "features": {},
            "targets": [{ "kind": ["lib"] }],
            "publish": [],
            "dependencies": [],
        });
        assert_eq!(
            manifest_facts("a", &package, "1.99"),
            ["a: no rust-version; it names the pinned toolchain 1.99"]
        );
    }

    #[test]
    fn the_toolchain_is_its_channel_s_major_and_minor() {
        assert_eq!(
            toolchain("[toolchain]\nchannel = \"1.99.1\"\n").unwrap(),
            "1.99"
        );
        assert!(toolchain("[toolchain]\n").is_err());
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
