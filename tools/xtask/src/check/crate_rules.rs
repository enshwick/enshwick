//! The rules a crate's manifest and sources are held to, read from
//! `cargo metadata` without compiling: those of any mod workspace
//! ([`cargo_enshwick::rules`]), and the platform's own: only the API
//! crate's `test` feature reaches the fake loader, whose crate alone turns
//! on the core's `fake` and core alone that of each crate below it, a
//! platform crate depends on the API crate only for its tests, features
//! name what they add, `anyhow` stays in binaries, `rust-version` is the
//! pinned toolchain's, a published crate's path dependencies carry
//! versions, and release-plz versions the API crate and every crate of
//! `crates/internal` as one group.

use std::path::Path;
use std::process::Command;

use cargo_enshwick::rules::TEST;

use crate::FACADE;
use crate::affected::Package;

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
    let root = Path::new(metadata["workspace_root"].as_str().unwrap_or_default());
    let mut api_crates = Vec::new();
    let mut problems = Vec::new();
    for package in metadata["packages"].as_array().into_iter().flatten() {
        let name = package["name"].as_str().unwrap_or_default();
        let manifest = Path::new(package["manifest_path"].as_str().unwrap_or_default());
        let dir = manifest.parent().unwrap_or(Path::new("."));
        if is_api_crate(name, dir.strip_prefix(root).unwrap_or(dir)) {
            api_crates.push(name.to_owned());
        }
        problems.extend(cargo_enshwick::rules::check(package).map_err(|e| e.to_string())?);
        problems.extend(fake_reach(name, package));
        if packages
            .iter()
            .any(|p| p.name == name && in_platform_dir(&p.dir))
        {
            problems.extend(facade_use(name, package));
        }
        problems.extend(manifest_facts(name, package, &toolchain));
    }
    problems.extend(release_group(
        &api_crates,
        &read(&root.join("release-plz.toml"))?,
    )?);
    Ok(problems)
}

/// The fake loader's crate, which only [`FACADE`] depends on, through its
/// [`TEST`] feature alone, so the fake's features are always the facade's.
const FAKE_CRATE: &str = "enshwick-fake";
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

/// The release-plz version group of the API crate and its internal parts,
/// which pin each other exactly and so release at one version.
const API_GROUP: &str = "api";

/// Whether the package `name` in `dir`, relative to the workspace root, is a
/// crate of the API, which release-plz versions as one group and the stable
/// API build takes: the facade, or a crate in `crates/internal`.
pub(crate) fn is_api_crate(name: &str, dir: &Path) -> bool {
    name == FACADE || dir.starts_with("crates/internal")
}

/// The problems of `release_plz`, the text of `release-plz.toml`: a crate of
/// `api_crates` that is not in the [`API_GROUP`] version group.
///
/// # Errors
///
/// When the text does not read as TOML.
fn release_group(api_crates: &[String], release_plz: &str) -> Result<Vec<String>, String> {
    let file: toml::Table =
        toml::from_str(release_plz).map_err(|e| format!("release-plz.toml: {}", e.message()))?;
    let grouped: Vec<&str> = file
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|package| {
            package.get("version_group").and_then(toml::Value::as_str) == Some(API_GROUP)
        })
        .filter_map(|package| package.get("name").and_then(toml::Value::as_str))
        .collect();
    Ok(api_crates
        .iter()
        .filter(|name| !grouped.contains(&name.as_str()))
        .map(|name| {
            format!(
                "{name}: release-plz.toml puts it in the `{API_GROUP}` version group, since the API's crates pin each other exactly"
            )
        })
        .collect())
}

/// `version`'s major and minor: `1.99` of `1.99.1`.
fn major_minor(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(in_platform_dir("crates/runtime/loader"));
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
    fn the_rules_hold_for_this_workspace() {
        assert_eq!(
            check(&crate::packages().unwrap()).unwrap(),
            Vec::<String>::new()
        );
    }

    #[test]
    fn an_api_crate_outside_the_api_version_group_is_a_problem() {
        let release_plz = "[[package]]\nname = \"enshwick\"\nversion_group = \"api\"\n\n\
                           [[package]]\nname = \"enshwick-world\"\nrelease = false\n";
        let crates = ["enshwick".to_owned(), "enshwick-world".to_owned()];
        assert_eq!(
            release_group(&crates, release_plz).unwrap(),
            [
                "enshwick-world: release-plz.toml puts it in the `api` version group, since the API's crates pin each other exactly"
            ]
        );
        assert!(release_group(&crates[..1], release_plz).unwrap().is_empty());
        assert!(release_group(&crates, "[[package]\n").is_err());
    }

    #[test]
    fn the_api_crates_are_the_facade_and_those_of_crates_internal() {
        assert!(is_api_crate("enshwick", Path::new("crates/enshwick")));
        assert!(is_api_crate(
            "enshwick-world",
            Path::new("crates/internal/world")
        ));
        assert!(!is_api_crate(
            "enshwick-luau",
            Path::new("crates/runtime/luau")
        ));
        assert!(!is_api_crate("xtask", Path::new("tools/xtask")));
        // A folder whose name only begins like the internal one.
        assert!(!is_api_crate("other", Path::new("crates/internals/x")));
    }
}
