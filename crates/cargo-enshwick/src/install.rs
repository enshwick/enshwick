//! `cargo enshwick install`: build the mod in release and copy it into the game.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, anyhow, bail};

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
pub(crate) fn install(game: &Path, package: Option<&str>) -> anyhow::Result<String> {
    if !["enshrouded.exe", "enshrouded_server.exe"]
        .iter()
        .any(|exe| game.join(exe).is_file())
    {
        bail!(
            "{} holds neither enshrouded.exe nor enshrouded_server.exe",
            game.display()
        );
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("cargo metadata")?;
    if !output.status.success() {
        bail!("{}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout).context("cargo metadata")?;
    let here = std::env::current_dir()?;
    let (package, library) = choose(&metadata.packages, package, &here)?;
    let built = Command::new(&cargo)
        .args([
            "build",
            "--release",
            "--message-format=json-render-diagnostics",
            "--package",
            &package.name,
        ])
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("cargo build")?;
    if !built.status.success() {
        bail!("cargo build failed for {}", package.name);
    }
    // Cargo names the file it built, wherever a build target puts it.
    let from = built_dll(&String::from_utf8_lossy(&built.stdout), library).unwrap_or_else(|| {
        metadata
            .target_directory
            .join("release")
            .join(format!("{library}.dll"))
    });
    let mods = game.join("mods");
    std::fs::create_dir_all(&mods).with_context(|| mods.display().to_string())?;
    let to = mods.join(format!("{}.dll", package.name));
    std::fs::copy(&from, &to).with_context(|| format!("{} -> {}", from.display(), to.display()))?;
    let mut done = format!("installed {}", to.display());
    if !mods.join("enshwick").join("enshwick.dll").is_file() {
        done.push_str(
            "; the Enshwick loader is not installed there yet (enshwick.zip from the releases)",
        );
    }
    Ok(done)
}

/// The DLL cargo's JSON messages say it built for the cdylib `library`.
fn built_dll(messages: &str, library: &str) -> Option<PathBuf> {
    #[derive(serde::Deserialize)]
    struct Artifact {
        reason: String,
        target: Option<Target>,
        #[serde(default)]
        filenames: Vec<PathBuf>,
    }
    messages
        .lines()
        .filter_map(|line| serde_json::from_str::<Artifact>(line).ok())
        .filter(|a| a.reason == "compiler-artifact")
        .filter(|a| {
            a.target
                .as_ref()
                .is_some_and(|t| t.name == library && t.crate_types.iter().any(|c| c == "cdylib"))
        })
        .flat_map(|a| a.filenames)
        .find(|path| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dll"))
        })
}

/// The package to install and its library's file name: `wanted`, or else the
/// package in `here` or the nearest folder above it, or else the
/// workspace's only mod.
fn choose<'a>(
    packages: &'a [Package],
    wanted: Option<&str>,
    here: &Path,
) -> anyhow::Result<(&'a Package, &'a str)> {
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
            .with_context(|| format!("no mod package named {wanted}"));
    }
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let here = canonical(here);
    if let Some((_, found)) = mods
        .iter()
        .filter_map(|found| {
            let dir = canonical(found.0.manifest_path.parent()?);
            here.starts_with(&dir)
                .then(|| (dir.components().count(), *found))
        })
        .max_by_key(|(depth, _)| *depth)
    {
        return Ok(found);
    }
    match mods.as_slice() {
        [only] => Ok(*only),
        [] => Err(anyhow!("no package here builds a cdylib")),
        _ => Err(anyhow!("several mods here; name one with --package")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dll_is_where_cargo_says_it_built_it() {
        let messages = [
            r#"{"reason":"compiler-artifact","target":{"name":"serde","crate_types":["lib"]},"filenames":["C:/t/libserde.rlib"]}"#,
            r#"{"reason":"compiler-artifact","target":{"name":"market_stalls","crate_types":["cdylib"]},"filenames":["C:/t/x86_64-pc-windows-msvc/release/market_stalls.dll","C:/t/x86_64-pc-windows-msvc/release/market_stalls.dll.lib"]}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ]
        .join("\n");
        assert_eq!(
            built_dll(&messages, "market_stalls"),
            Some(PathBuf::from(
                "C:/t/x86_64-pc-windows-msvc/release/market_stalls.dll"
            ))
        );
        assert_eq!(built_dll(&messages, "other"), None);
    }

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
            package("market-stalls", "/w/market-stalls", &["cdylib"]),
            package("trading-post", "/w/trading-post", &["cdylib", "rlib"]),
            package("geometry", "/w/geometry", &["lib"]),
        ];
        let named = choose(&packages, Some("trading-post"), Path::new("/w")).unwrap();
        assert_eq!(
            (named.0.name.as_str(), named.1),
            ("trading-post", "trading_post")
        );
        let here = choose(&packages, None, Path::new("/w/market-stalls")).unwrap();
        let below = choose(&packages, None, Path::new("/w/market-stalls/src")).unwrap();
        assert_eq!(below.0.name, here.0.name);
        assert_eq!(here.1, "market_stalls");
        assert!(choose(&packages, None, Path::new("/w")).is_err());
        assert!(choose(&packages, Some("geometry"), Path::new("/w")).is_err());
        assert_eq!(
            choose(&packages[2..], None, Path::new("/w"))
                .unwrap_err()
                .to_string(),
            "no package here builds a cdylib"
        );
        assert_eq!(
            choose(&packages[1..], None, Path::new("/w"))
                .unwrap()
                .0
                .name,
            "trading-post"
        );
    }

    #[test]
    fn metadata_parses() {
        let json = r#"{"packages":[{"name":"trading-post","manifest_path":"/w/trading-post/Cargo.toml",
            "targets":[{"name":"trading_post","crate_types":["cdylib"],"kind":["cdylib"]}],"version":"0.1.0"}],
            "target_directory":"/w/target","workspace_root":"/w"}"#;
        let metadata: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.packages[0].targets[0].crate_types, ["cdylib"]);
    }
}
