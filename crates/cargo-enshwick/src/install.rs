//! `cargo enshwick install`: build the mod in release and copy it into the game.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What `cargo metadata` says about the workspace, as far as installing goes.
#[derive(Debug, serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    target_directory: PathBuf,
}

#[derive(Debug, serde::Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
}

#[derive(Debug, serde::Deserialize)]
struct Target {
    name: String,
    crate_types: Vec<String>,
}

/// Builds the chosen mod and copies it to `<game>/mods/<package>.dll`.
pub(crate) fn install(game: &Path, package: Option<&str>) -> Result<String, String> {
    if !["enshrouded.exe", "enshrouded_server.exe"]
        .iter()
        .any(|exe| game.join(exe).is_file())
    {
        return Err(format!(
            "{} holds neither enshrouded.exe nor enshrouded_server.exe",
            game.display()
        ));
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    let metadata: Metadata =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let here = std::env::current_dir().map_err(|e| e.to_string())?;
    let (package, library) = choose(&metadata.packages, package, &here)?;
    let built = Command::new(&cargo)
        .args(["build", "--release", "--package", &package.name])
        .status()
        .map_err(|e| format!("cargo build: {e}"))?;
    if !built.success() {
        return Err(format!("cargo build failed for {}", package.name));
    }
    let from = metadata
        .target_directory
        .join("release")
        .join(format!("{library}.dll"));
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).map_err(|e| format!("{}: {e}", mods.display()))?;
    let to = mods.join(format!("{}.dll", package.name));
    std::fs::copy(&from, &to)
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))?;
    let mut done = format!("installed {}", to.display());
    if !mods.join("enshwick").join("enshwick.dll").is_file() {
        done.push_str(
            "; the Enshwick loader is not installed there yet (enshwick.zip from the releases)",
        );
    }
    Ok(done)
}

/// The package to install and its library's file name: `wanted`, or else the
/// package in `here`, or else the workspace's only mod.
fn choose<'a>(
    packages: &'a [Package],
    wanted: Option<&str>,
    here: &Path,
) -> Result<(&'a Package, &'a str), String> {
    let mods: Vec<(&Package, &str)> = packages
        .iter()
        .filter_map(|p| {
            p.targets
                .iter()
                .find(|t| t.crate_types.iter().any(|c| c == "cdylib"))
                .map(|t| (p, t.name.as_str()))
        })
        .collect();
    if let Some(wanted) = wanted {
        return mods
            .into_iter()
            .find(|(p, _)| p.name == wanted)
            .ok_or_else(|| format!("no mod package named {wanted}"));
    }
    if let Some(found) = mods
        .iter()
        .find(|(p, _)| p.manifest_path.parent() == Some(here))
    {
        return Ok(*found);
    }
    match mods.as_slice() {
        [only] => Ok(*only),
        [] => Err("no package here builds a cdylib".into()),
        _ => Err("several mods here; name one with --package".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(name: &str, dir: &str, kinds: &[&str]) -> Package {
        Package {
            name: name.into(),
            manifest_path: Path::new(dir).join("Cargo.toml"),
            targets: vec![Target {
                name: name.replace('-', "_"),
                crate_types: kinds.iter().map(|&k| k.to_owned()).collect(),
            }],
        }
    }

    #[test]
    fn the_package_is_named_found_here_or_the_only_mod() {
        let packages = [
            package("area-loot", "/w/area-loot", &["cdylib"]),
            package("safes", "/w/safes", &["cdylib", "rlib"]),
            package("geometry", "/w/geometry", &["lib"]),
        ];
        let named = choose(&packages, Some("safes"), Path::new("/w")).unwrap();
        assert_eq!((named.0.name.as_str(), named.1), ("safes", "safes"));
        let here = choose(&packages, None, Path::new("/w/area-loot")).unwrap();
        assert_eq!(here.1, "area_loot");
        assert!(choose(&packages, None, Path::new("/w")).is_err());
        assert!(choose(&packages, Some("geometry"), Path::new("/w")).is_err());
        assert_eq!(
            choose(&packages[2..], None, Path::new("/w")).unwrap_err(),
            "no package here builds a cdylib"
        );
        assert_eq!(
            choose(&packages[1..], None, Path::new("/w"))
                .unwrap()
                .0
                .name,
            "safes"
        );
    }

    #[test]
    fn metadata_parses() {
        let json = r#"{"packages":[{"name":"safes","manifest_path":"/w/safes/Cargo.toml",
            "targets":[{"name":"safes","crate_types":["cdylib"],"kind":["cdylib"]}],"version":"0.1.0"}],
            "target_directory":"/w/target","workspace_root":"/w"}"#;
        let metadata: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.packages[0].targets[0].crate_types, ["cdylib"]);
    }
}
