//! Every `requires:` range of a mod in this repository takes the current
//! version of the mod it names, so a release bump never makes the loader
//! skip a mod that needs another. It reads every mod's sources, not only this
//! one's; it lives here because this mod is the one that requires another.

use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};

/// The repository's `mods` folder.
fn mods() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the mods folder")
        .to_path_buf()
}

/// Each mod's id, from `id: "..."` in its manifest, with its crate version.
fn versions() -> Vec<(String, Version)> {
    let mut found = Vec::new();
    for dir in std::fs::read_dir(mods()).expect("mods").flatten() {
        let source = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap_or_default();
        let Some(id) = source
            .lines()
            .find_map(|line| line.trim().strip_prefix("id: \""))
            .and_then(|rest| rest.split('"').next())
        else {
            continue;
        };
        let cargo = std::fs::read_to_string(dir.path().join("Cargo.toml")).expect("Cargo.toml");
        let cargo: toml::Table = toml::from_str(&cargo).expect("a Cargo.toml");
        let version = cargo["package"]["version"].as_str().expect("a version");
        found.push((id.to_owned(), Version::parse(version).expect("semver")));
    }
    found
}

/// Every `(mod dir, required id, range)` written as `requires: [..]`.
fn requirements() -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for dir in std::fs::read_dir(mods()).expect("mods").flatten() {
        let source = std::fs::read_to_string(dir.path().join("src/lib.rs")).unwrap_or_default();
        for line in source.lines() {
            let Some(list) = line.trim().strip_prefix("requires: [") else {
                continue;
            };
            let list = list.trim_end_matches(',').trim_end_matches(']');
            for pair in list.split("\",").filter(|pair| pair.contains("=>")) {
                let (id, range) = pair.split_once("=>").expect("id => range");
                let clean = |text: &str| text.trim().trim_matches('"').to_owned();
                found.push((
                    dir.file_name().to_string_lossy().into_owned(),
                    clean(id),
                    clean(range),
                ));
            }
        }
    }
    found
}

#[test]
fn every_requires_range_takes_the_current_version() {
    let versions = versions();
    let requirements = requirements();
    assert!(
        requirements
            .iter()
            .any(|(_, id, _)| id == "flame-altar-zones"),
        "this mod's own requirement is found"
    );
    for (dir, id, range) in requirements {
        let req = VersionReq::parse(&range).unwrap_or_else(|e| panic!("{dir}: {range}: {e}"));
        let (_, version) = versions
            .iter()
            .find(|(known, _)| *known == id)
            .unwrap_or_else(|| panic!("{dir} requires {id}, which is not in this repository"));
        assert!(
            req.matches(version),
            "{dir} requires {id} {range}, but {id} is {version}"
        );
    }
}

#[test]
fn a_minor_bump_before_1_0_keeps_the_range() {
    // `^0.1` would refuse 0.2.0: pre-1.0 ranges name the whole 0.x series.
    let req = VersionReq::parse(">=0.1, <1.0").expect("a range");
    for version in ["0.1.0", "0.2.0", "0.9.3"] {
        assert!(req.matches(&Version::parse(version).expect("semver")));
    }
    assert!(!req.matches(&Version::new(1, 0, 0)));
}
