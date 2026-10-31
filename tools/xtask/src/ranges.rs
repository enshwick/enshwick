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
    /// Its version, the package's.
    version: String,
    /// `(relation, mod id, range)` of each dependency.
    dependencies: Vec<(String, String, String)>,
}

/// The relations a manifest names other mods in, by alias.
const RELATIONS: [&str; 3] = ["requires", "recommends", "breaks"];

/// The problems with the ranges of the manifests of the mods under `root`,
/// one line each: the packages with a `[package.metadata.enshwick]` table,
/// outside the platform's own crates and tools.
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let declared: Vec<Declared> = packages
        .iter()
        .filter(|package| !package.dir.starts_with("crates/") && !package.dir.starts_with("tools/"))
        .filter_map(|package| {
            let text = std::fs::read_to_string(root.join(&package.dir).join("Cargo.toml")).ok()?;
            manifest(&text, &package.name, &package.version)
        })
        .collect();
    problems(&declared)
}

/// The mod the `[package.metadata.enshwick]` table of the `Cargo.toml`
/// `text` declares, if any.
fn manifest(text: &str, package: &str, version: &str) -> Option<Declared> {
    let cargo: toml::Table = toml::from_str(text).ok()?;
    let table = cargo.get("package")?.get("metadata")?.get("enshwick")?;
    Some(Declared {
        package: package.to_owned(),
        id: table.get("id")?.as_str()?.to_owned(),
        version: version.to_owned(),
        dependencies: RELATIONS
            .iter()
            .flat_map(|relation| {
                table
                    .get(*relation)
                    .and_then(toml::Value::as_table)
                    .into_iter()
                    .flatten()
                    .filter_map(|(_, entry)| {
                        let (id, range) = match entry {
                            toml::Value::String(id) => (id.clone(), String::new()),
                            toml::Value::Table(fields) => (
                                fields.get("id")?.as_str()?.to_owned(),
                                fields
                                    .get("version")
                                    .and_then(toml::Value::as_str)
                                    .unwrap_or_default()
                                    .to_owned(),
                            ),
                            _ => return None,
                        };
                        Some(((*relation).to_owned(), id, range))
                    })
            })
            .collect(),
    })
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

    /// The mod a package `crate` of `version` declares with the table `text`.
    fn declared(text: &str, version: &str) -> Vec<Declared> {
        let cargo = format!("[package.metadata.enshwick]\n{text}");
        manifest(&cargo, "crate", version).into_iter().collect()
    }

    #[test]
    fn a_manifest_s_ranges_are_read_with_its_version() {
        let text = r#"
            id = "you.market-stalls"
            side = "server"

            [package.metadata.enshwick.requires]
            post = { id = "you.trading-post", version = ">=0.1, <1.0" }

            [package.metadata.enshwick.breaks]
            market = { id = "old-market", version = "<0.2" }
            other = "other"
        "#;
        assert_eq!(
            declared(text, "0.3.0"),
            [Declared {
                package: "crate".to_owned(),
                id: "you.market-stalls".to_owned(),
                version: "0.3.0".to_owned(),
                dependencies: vec![
                    (
                        "requires".to_owned(),
                        "you.trading-post".to_owned(),
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
        assert!(manifest("[package]\nname = \"plain\"\n", "plain", "1.0.0").is_none());
    }

    #[test]
    fn a_range_the_named_version_misses_is_a_problem() {
        let mut all = declared(
            "id = \"a\"\n[package.metadata.enshwick.requires]\nb = { id = \"b\", version = \">=0.2\" }\n\
             [package.metadata.enshwick.breaks]\nc = { id = \"c\", version = \"<1.0\" }",
            "1.0.0",
        );
        all.extend(declared("id = \"b\"", "0.1.0"));
        all.extend(declared("id = \"c\"", "0.5.0"));
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
            "id = \"a\"\n[package.metadata.enshwick.requires]\nb = { id = \"b\", version = \">=0.2\" }\n\
             [package.metadata.enshwick.recommends]\nout = { id = \"outside\", version = \">=9\" }",
            "0.0.0-dev",
        );
        all.extend(declared("id = \"b\"", "0.0.0-dev"));
        assert!(problems(&all).is_empty());
    }

    #[test]
    fn a_range_that_does_not_parse_is_a_problem() {
        let all = declared(
            "id = \"a\"\n[package.metadata.enshwick.requires]\nb = { id = \"b\", version = \"soon\" }",
            "1.0.0",
        );
        assert_eq!(
            problems(&all),
            [r#"a (crate) requires b "soon": not a version range"#]
        );
    }
}
