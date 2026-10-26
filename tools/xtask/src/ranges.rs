//! The version ranges in the workspace's mod manifests, held to the crates
//! they name: a `requires` or `recommends` range the named mod's version
//! does not meet, a `breaks` range it does meet, or a range that does not
//! parse is a problem, so a version bump cannot quietly turn a mod off.

use std::path::Path;

use semver::{Version, VersionReq};

use crate::affected::Package;

/// A mod a workspace package declares.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Declared {
    /// The package that declares it.
    package: String,
    /// The mod id.
    id: String,
    /// Its version: the manifest's own or the package's.
    version: String,
    /// `(relation, mod id, range)` of each dependency.
    dependencies: Vec<(String, String, String)>,
}

/// The relations a manifest names other mods in.
const RELATIONS: [&str; 3] = ["requires", "recommends", "breaks"];

/// The problems with the ranges of the manifests of the mods under `root`,
/// one line each: the packages that build on the API crate, outside the
/// platform's own crates and tools.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let declared: Vec<Declared> = packages
        .iter()
        .filter(|package| {
            package.deps.iter().any(|dep| dep == "enshwick")
                && !package.dir.starts_with("crates/")
                && !package.dir.starts_with("tools/")
        })
        .flat_map(|package| {
            sources(&root.join(&package.dir).join("src"))
                .into_iter()
                .flat_map(|text| manifests(&text, &package.name, &package.version))
        })
        .collect();
    problems(&declared)
}

/// The text of every `.rs` file below `dir`.
fn sources(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                sources(&path)
            } else if path.extension().is_some_and(|e| e == "rs") {
                std::fs::read_to_string(&path).into_iter().collect()
            } else {
                Vec::new()
            }
        })
        .collect()
}

/// The mods the `manifest!` calls in `text` declare.
fn manifests(text: &str, package: &str, version: &str) -> Vec<Declared> {
    text.match_indices("manifest!(")
        .filter_map(|(at, call)| {
            let body = &text[at + call.len()..];
            let body = &body[..closing(body)?];
            Some(Declared {
                package: package.to_owned(),
                id: literal_after(body, "id:")?,
                version: literal_after(body, "version:").unwrap_or_else(|| version.to_owned()),
                dependencies: RELATIONS
                    .iter()
                    .flat_map(|relation| {
                        pairs(body, relation)
                            .into_iter()
                            .map(|(id, range)| ((*relation).to_owned(), id, range))
                    })
                    .collect(),
            })
        })
        .collect()
}

/// Where the parenthesis opened before `text` closes.
fn closing(text: &str) -> Option<usize> {
    let mut depth = 1usize;
    let mut quoted = false;
    for (at, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// The string literal after `key` in `body`.
fn literal_after(body: &str, key: &str) -> Option<String> {
    let rest = body[body.find(key)? + key.len()..].trim_start();
    let rest = rest.strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_owned())
}

/// The `"id" => "range"` pairs of `relation: [...]` in `body`.
fn pairs(body: &str, relation: &str) -> Vec<(String, String)> {
    let Some(start) = body.find(&format!("{relation}:")) else {
        return Vec::new();
    };
    let rest = &body[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let list = &rest[open + 1..];
    let list = &list[..list.find(']').unwrap_or(list.len())];
    let literals: Vec<&str> = list.split('"').skip(1).step_by(2).collect();
    literals
        .as_chunks::<2>()
        .0
        .iter()
        .map(|[id, range]| ((*id).to_owned(), (*range).to_owned()))
        .collect()
}

/// Whether `version` is a dev build, which meets every range, as the
/// loader has it.
fn dev(version: &Version) -> bool {
    (version.major, version.minor, version.patch) == (0, 0, 0) && version.pre.as_str() == "dev"
}

/// The problems with the dependencies of `declared` on each other.
fn problems(declared: &[Declared]) -> Vec<String> {
    let mut problems = Vec::new();
    for named in declared {
        for (relation, id, range) in &named.dependencies {
            let at = format!("{} ({}) {relation} {id} {range:?}", named.id, named.package);
            let Ok(req) = VersionReq::parse(if range.is_empty() { "*" } else { range }) else {
                problems.push(format!("{at}: not a version range"));
                continue;
            };
            let Some(other) = declared.iter().find(|other| other.id == *id) else {
                continue;
            };
            let Ok(version) = Version::parse(&other.version) else {
                problems.push(format!("{at}: {id} has no version, {:?}", other.version));
                continue;
            };
            let meets = dev(&version) || req.matches(&version);
            match (relation.as_str(), meets) {
                ("breaks", true) => problems.push(format!("{at}: {id} {version} meets it")),
                ("breaks", false) | (_, true) => {}
                (_, false) => problems.push(format!("{at}: {id} {version} does not meet it")),
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared(text: &str, version: &str) -> Vec<Declared> {
        manifests(text, "crate", version)
    }

    #[test]
    fn a_manifest_s_ranges_are_read_with_its_version() {
        let text = r#"enshwick::manifest!(
            id: "market-stalls",
            side: SERVER,
            requires: ["trading-post" => ">=0.1, <1.0"],
            breaks: ["old-market" => "<0.2", "other" => ""],
        )"#;
        assert_eq!(
            declared(text, "0.3.0"),
            [Declared {
                package: "crate".to_owned(),
                id: "market-stalls".to_owned(),
                version: "0.3.0".to_owned(),
                dependencies: vec![
                    (
                        "requires".to_owned(),
                        "trading-post".to_owned(),
                        ">=0.1, <1.0".to_owned()
                    ),
                    (
                        "breaks".to_owned(),
                        "old-market".to_owned(),
                        "<0.2".to_owned()
                    ),
                    ("breaks".to_owned(), "other".to_owned(), String::new()),
                ],
            }]
        );
        let pinned = r#"manifest!(id: "safes", side: SERVER, version: "0.3.0")"#;
        assert_eq!(declared(pinned, "9.9.9")[0].version, "0.3.0");
    }

    #[test]
    fn a_range_the_named_version_misses_is_a_problem() {
        let mut all = declared(
            r#"manifest!(id: "a", requires: ["b" => ">=0.2"], breaks: ["c" => "<1.0"])"#,
            "1.0.0",
        );
        all.extend(declared(r#"manifest!(id: "b")"#, "0.1.0"));
        all.extend(declared(r#"manifest!(id: "c")"#, "0.5.0"));
        assert_eq!(
            problems(&all),
            [
                r#"a (crate) requires b ">=0.2": b 0.1.0 does not meet it"#,
                r#"a (crate) breaks c "<1.0": c 0.5.0 meets it"#,
            ]
        );
    }

    #[test]
    fn dev_builds_meet_every_range_and_unknown_mods_are_skipped() {
        let mut all = declared(
            r#"manifest!(id: "a", requires: ["b" => ">=0.2"], recommends: ["outside" => ">=9"])"#,
            "0.0.0-dev",
        );
        all.extend(declared(r#"manifest!(id: "b")"#, "0.0.0-dev"));
        assert!(problems(&all).is_empty());
    }

    #[test]
    fn a_range_that_does_not_parse_is_a_problem() {
        let all = declared(r#"manifest!(id: "a", requires: ["b" => "soon"])"#, "1.0.0");
        assert_eq!(
            problems(&all),
            [r#"a (crate) requires b "soon": not a version range"#]
        );
    }
}
