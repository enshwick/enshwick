//! `cargo enshwick install`: build the mod in release and copy it into the game.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, anyhow, bail};

/// The game's executables, one per side, as a game folder holds them.
pub(crate) const GAME_EXECUTABLES: [&str; 2] = ["enshrouded_server.exe", "enshrouded.exe"];

/// What `cargo metadata` says about the workspace, as far as installing goes.
#[derive(Debug, serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
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
    if !GAME_EXECUTABLES.iter().any(|exe| game.join(exe).is_file()) {
        bail!(
            "{} holds none of {}",
            game.display(),
            GAME_EXECUTABLES.join(", ")
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
    let here = std::env::current_dir().context("the current folder")?;
    let (package, library) = choose(&metadata.packages, package, &here)?;
    let windows = cfg!(windows);
    preflight(
        windows,
        || {
            Command::new(&cargo)
                .args(["xwin", "--version"])
                .output()
                .is_ok_and(|out| out.status.success())
        },
        windows_target_installed,
    )?;
    let built = Command::new(&cargo)
        .args(build_args(windows, &package.name))
        .stderr(std::process::Stdio::inherit())
        .output()
        .context("cargo build")?;
    if !built.status.success() {
        bail!(
            "cargo build failed for {}{}",
            package.name,
            if windows {
                "; a Windows build links with the MSVC tools (Visual Studio Build Tools with the C++ workload)"
            } else {
                ""
            }
        );
    }
    let from = windows_dll(
        &String::from_utf8_lossy(&built.stdout),
        library,
        &package.name,
    )?;
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

/// The target every mod DLL is built for, whatever the host.
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

/// The cargo arguments that build `package` as a Windows DLL: plain cargo on
/// Windows, `cargo xwin` with the Windows SDK it fetches elsewhere.
fn build_args(windows_host: bool, package: &str) -> Vec<&str> {
    let xwin: &[&str] = if windows_host { &[] } else { &["xwin"] };
    xwin.iter()
        .copied()
        .chain([
            "build",
            "--release",
            "--target",
            WINDOWS_TARGET,
            "--message-format=json-render-diagnostics",
            "--package",
            package,
        ])
        .collect()
}

/// Fails with the install command of a tool the Windows build lacks;
/// `target_installed` is `None` when rustup cannot tell.
fn preflight(
    windows_host: bool,
    has_xwin: impl FnOnce() -> bool,
    target_installed: impl FnOnce() -> Option<bool>,
) -> anyhow::Result<()> {
    if target_installed() == Some(false) {
        bail!(
            "mods are Windows DLLs and the Rust target {WINDOWS_TARGET} is not installed; `rustup target add {WINDOWS_TARGET}` adds it"
        );
    }
    if !windows_host && !has_xwin() {
        bail!(
            "a Windows DLL is built off Windows with cargo-xwin, which is not installed; `cargo install --locked cargo-xwin` adds it"
        );
    }
    Ok(())
}

/// Whether rustup has the Windows target for the toolchain of the current
/// folder; `None` without rustup.
fn windows_target_installed() -> Option<bool> {
    let out = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .filter(|out| out.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|line| line.trim() == WINDOWS_TARGET),
    )
}

/// The Windows DLL cargo's JSON messages say it built for `package`, whose
/// cdylib is `library`.
fn windows_dll(messages: &str, library: &str, package: &str) -> anyhow::Result<PathBuf> {
    built_dll(messages, library).with_context(|| format!("no Windows DLL was built for {package}"))
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

    #[test]
    fn a_build_without_a_windows_dll_says_so() {
        let messages = r#"{"reason":"compiler-artifact","target":{"name":"market_stalls","crate_types":["cdylib"]},"filenames":["/t/release/libmarket_stalls.so"]}"#;
        assert_eq!(
            windows_dll(messages, "market_stalls", "market-stalls")
                .unwrap_err()
                .to_string(),
            "no Windows DLL was built for market-stalls"
        );
    }

    #[test]
    fn the_build_targets_windows_on_every_host() {
        let on_windows = build_args(true, "market-stalls");
        let elsewhere = build_args(false, "market-stalls");
        assert_eq!(on_windows[0], "build");
        assert_eq!(&elsewhere[..2], ["xwin", "build"]);
        for args in [&on_windows, &elsewhere] {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--target", WINDOWS_TARGET])
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--package", "market-stalls"])
            );
        }
    }

    #[test]
    fn a_missing_tool_is_named_with_its_install_command() {
        let target = preflight(true, || true, || Some(false))
            .unwrap_err()
            .to_string();
        assert!(
            target.contains("rustup target add x86_64-pc-windows-msvc"),
            "{target}"
        );
        let xwin = preflight(false, || false, || Some(true))
            .unwrap_err()
            .to_string();
        assert!(xwin.contains("cargo install --locked cargo-xwin"), "{xwin}");
        assert!(preflight(true, || false, || Some(true)).is_ok());
        assert!(preflight(false, || true, || None).is_ok());
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
