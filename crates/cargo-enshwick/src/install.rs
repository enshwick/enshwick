//! `cargo enshwick install`: build a mod crate in release, or check a pack
//! folder, and copy it into the game.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, anyhow, bail};
use enshwick_core::packs::{MANIFEST_FILE, PackFiles};

/// The game's executables, one per side, as a game folder holds them.
pub(crate) const GAME_EXECUTABLES: [&str; 2] = [SERVER_EXE, "enshrouded.exe"];
/// The dedicated server's executable, which `cargo enshwick run` starts.
pub(crate) const SERVER_EXE: &str = "enshrouded_server.exe";

/// What `cargo metadata` says about the workspace, as far as installing goes.
#[derive(Debug, serde::Deserialize)]
struct Metadata {
    packages: Vec<Package>,
}

#[derive(Debug, serde::Deserialize)]
struct Package {
    name: String,
    #[serde(default)]
    version: String,
    manifest_path: PathBuf,
    targets: Vec<Target>,
    #[serde(default)]
    metadata: Option<PackageMetadata>,
}

impl Package {
    /// `[package.metadata.enshwick]`, when the package has it.
    fn enshwick(&self) -> Option<&ModMetadata> {
        self.metadata.as_ref().and_then(|m| m.enshwick.as_ref())
    }

    /// The mod's folder in `mods/`: its id from `[package.metadata.enshwick]`,
    /// else the package name, since the loader reads any folder name.
    fn folder(&self) -> &str {
        self.enshwick().map_or(&self.name, |e| &e.id)
    }
}

/// `[package.metadata]`, as far as installing goes.
#[derive(Debug, serde::Deserialize)]
struct PackageMetadata {
    enshwick: Option<ModMetadata>,
}

/// `[package.metadata.enshwick]`, as far as installing goes.
#[derive(Debug, serde::Deserialize)]
struct ModMetadata {
    id: String,
    side: Option<String>,
}

/// A mod built or checked, with the files of its folder in `mods/`.
#[derive(Debug)]
pub(crate) struct Built {
    pub(crate) id: String,
    pub(crate) version: String,
    /// What the build or check found, a line or more.
    pub(crate) summary: String,
    /// The folder's files by their path in it, sorted.
    pub(crate) files: Vec<(String, Vec<u8>)>,
    /// The DLL of a mod crate, with the side `Cargo.toml` names.
    pub(crate) dll: Option<(PathBuf, Option<String>)>,
}

impl Built {
    /// Writes the files into `<game>/mods/<id>/`; for a pack, also removes
    /// the pack files an earlier install left there that it no longer has,
    /// as the loader would read them, with the folders that leaves empty.
    /// Other files, such as a hold the loader keeps there, stay.
    ///
    /// # Errors
    ///
    /// When a file cannot be written or removed, or the files an earlier
    /// install left do not read as a pack.
    pub(crate) fn place(&self, game: &Path) -> anyhow::Result<PathBuf> {
        let folder = game.join("mods").join(&self.id);
        if self.dll.is_none() && folder.is_dir() {
            let old = PackFiles::read_dir(&folder).with_context(|| {
                format!(
                    "{}: the files an earlier install left do not read as a pack; remove the folder and install again",
                    folder.display()
                )
            })?;
            for stale in old
                .names()
                .filter(|name| self.files.iter().all(|(file, _)| file != name))
            {
                let path = folder.join(stale);
                std::fs::remove_file(&path).with_context(|| path.display().to_string())?;
                let mut parent = path.parent();
                while let Some(dir) = parent
                    && dir != folder
                    && std::fs::remove_dir(dir).is_ok()
                {
                    parent = dir.parent();
                }
            }
        }
        for (name, bytes) in &self.files {
            let path = folder.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).with_context(|| parent.display().to_string())?;
            }
            std::fs::write(&path, bytes).with_context(|| path.display().to_string())?;
        }
        Ok(folder)
    }
}

/// Builds the mod crate in `dir`, `package` in a workspace, in release for
/// Windows; or, when `dir` holds a `pack.toml`, checks the data pack or Luau
/// mod there as the loader reads it.
pub(crate) fn build(dir: &Path, package: Option<&str>) -> anyhow::Result<Built> {
    if dir.join(MANIFEST_FILE).is_file() {
        if package.is_some() {
            bail!(
                "{} is a pack folder: --package names a crate",
                dir.display()
            );
        }
        let (summary, manifest, pack) = crate::pack::check(dir, None)?;
        let mut files: Vec<(String, Vec<u8>)> = pack
            .names()
            .map(|name| (name.to_owned(), pack.get(name).unwrap_or_default().to_vec()))
            .collect();
        files.sort();
        return Ok(Built {
            id: manifest.id,
            version: manifest.version,
            summary,
            files,
            dll: None,
        });
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(&cargo)
        .current_dir(dir)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .context("cargo metadata")?;
    if !output.status.success() {
        bail!(
            "{} holds neither {MANIFEST_FILE} nor a mod crate: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let metadata: Metadata = serde_json::from_slice(&output.stdout).context("cargo metadata")?;
    let here = std::path::absolute(dir).context("the mod's folder")?;
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
        || windows_target_installed(&here),
    )?;
    let built = Command::new(&cargo)
        .current_dir(dir)
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
    let bytes = std::fs::read(&from).with_context(|| from.display().to_string())?;
    Ok(Built {
        id: package.folder().to_owned(),
        version: package.version.clone(),
        summary: format!("built {}", from.display()),
        files: vec![(format!("{}.dll", package.name), bytes)],
        dll: Some((from, package.enshwick().and_then(|e| e.side.clone()))),
    })
}

#[derive(Debug, serde::Deserialize)]
struct Target {
    name: String,
    crate_types: Vec<String>,
}

/// Builds the mod in the current folder, or checks its pack, and copies it
/// to `<game>/mods/<id>/`, `game` found through Steam when not given; with
/// `with_loader`, then puts the loader of this tool's release in.
pub(crate) fn install(
    game: Option<&Path>,
    package: Option<&str>,
    with_loader: bool,
) -> anyhow::Result<String> {
    let (game, chosen) = game_folder(game, || crate::steam::find(&crate::steam::steam_folders()))?;
    let game = game.as_path();
    if !GAME_EXECUTABLES.iter().any(|exe| game.join(exe).is_file()) {
        bail!(
            "{} holds none of {}",
            game.display(),
            GAME_EXECUTABLES.join(", ")
        );
    }
    let mut said: Vec<String> = chosen.into_iter().collect();
    let built = build(Path::new("."), package)?;
    let folder = built.place(game)?;
    said.push(format!(
        "installed {} {} in {}",
        built.id,
        built.version,
        folder.display()
    ));
    said.extend(loader_line(game, with_loader)?);
    Ok(said.join("\n"))
}

/// With `with_loader`, puts the loader of this tool's release in `game` and
/// says so; else a note when `game` has none.
pub(crate) fn loader_line(game: &Path, with_loader: bool) -> anyhow::Result<Option<String>> {
    Ok(if with_loader {
        Some(crate::loader::put_in(game)?)
    } else {
        (!crate::loader::is_installed(game)).then(|| crate::loader::missing_note(game))
    })
}

/// The game folder, `given` or else the first install `find` reports, with
/// a line naming the one taken and the others.
pub(crate) fn game_folder(
    given: Option<&Path>,
    find: impl FnOnce() -> Vec<crate::steam::Found>,
) -> anyhow::Result<(PathBuf, Option<String>)> {
    if let Some(given) = given {
        return Ok((given.to_owned(), None));
    }
    let found = find();
    let Some(first) = found.first() else {
        bail!(
            "Steam has neither the Enshrouded dedicated server nor the game installed here; name the game folder with --game or ENSHROUDED_DIR"
        );
    };
    let others: Vec<String> = found
        .iter()
        .skip(1)
        .map(|other| format!("{} ({})", other.folder.display(), other.what()))
        .collect();
    let others = if others.is_empty() {
        String::new()
    } else {
        format!("; --game picks another: {}", others.join(", "))
    };
    let said = format!(
        "the game folder is {}, {} as Steam installed it{others}",
        first.folder.display(),
        first.what()
    );
    Ok((first.folder.clone(), Some(said)))
}

/// The target every mod DLL is built for, whatever the host.
pub(crate) const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

/// The cargo arguments that build `package` as a Windows DLL: plain cargo on
/// Windows, `cargo xwin` with the Windows SDK it fetches elsewhere. The
/// library builds as a cdylib alone, since cargo runs the release profile's
/// LTO only on a unit whose every crate type can take it, and a mod's rlib
/// for its tests cannot.
fn build_args(windows_host: bool, package: &str) -> Vec<&str> {
    let xwin: &[&str] = if windows_host { &[] } else { &["xwin"] };
    xwin.iter()
        .copied()
        .chain([
            "rustc",
            "--release",
            "--target",
            WINDOWS_TARGET,
            "--message-format=json-render-diagnostics",
            "--package",
            package,
            "--lib",
            "--crate-type",
            "cdylib",
        ])
        .collect()
}

/// Fails with the install command of a tool the Windows build lacks;
/// `target_installed` is `None` when rustup cannot tell.
pub(crate) fn preflight(
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

/// Whether rustup has the Windows target for the toolchain of the folder
/// `dir`; `None` without rustup.
pub(crate) fn windows_target_installed(dir: &Path) -> Option<bool> {
    let out = Command::new("rustup")
        .current_dir(dir)
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
        assert_eq!(on_windows[0], "rustc");
        assert_eq!(&elsewhere[..2], ["xwin", "rustc"]);
        for args in [&on_windows, &elsewhere] {
            assert!(
                args.windows(3)
                    .any(|args| args == ["--lib", "--crate-type", "cdylib"]),
                "fat LTO needs the cdylib alone: {args:?}"
            );
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
            version: "0.1.0".into(),
            manifest_path: Path::new(dir).join("Cargo.toml"),
            targets: vec![Target {
                name: name.replace('-', "_"),
                crate_types: kinds.iter().map(|&k| k.to_owned()).collect(),
            }],
            metadata: None,
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
    fn a_given_game_folder_wins_and_else_steam_s_first_install_is_named() {
        use crate::steam::{Found, GAME_APP, SERVER_APP};
        let given = game_folder(Some(Path::new("/g")), || unreachable!()).unwrap();
        assert_eq!(given, (PathBuf::from("/g"), None));
        let found = || {
            vec![
                Found {
                    folder: PathBuf::from("/s"),
                    app: SERVER_APP,
                },
                Found {
                    folder: PathBuf::from("/c"),
                    app: GAME_APP,
                },
            ]
        };
        let (folder, said) = game_folder(None, found).unwrap();
        assert_eq!(folder, Path::new("/s"));
        let said = said.unwrap();
        assert!(said.contains("the dedicated server"), "{said}");
        assert!(said.contains("--game picks another"), "{said}");
        let (_, alone) = game_folder(None, || found().split_off(1)).unwrap();
        assert!(!alone.unwrap().contains("another"));
        let none = game_folder(None, Vec::new).unwrap_err().to_string();
        assert!(none.contains("--game"), "{none}");
    }

    #[test]
    fn metadata_reads_the_crate_types() {
        let json = r#"{"packages":[{"name":"trading-post","manifest_path":"/w/trading-post/Cargo.toml",
            "targets":[{"name":"trading_post","crate_types":["cdylib"],"kind":["cdylib"]}],"version":"0.1.0"}],
            "target_directory":"/w/target","workspace_root":"/w"}"#;
        let metadata: Metadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.packages[0].targets[0].crate_types, ["cdylib"]);
    }

    #[test]
    fn a_mod_installs_into_the_folder_of_its_id_else_of_its_package() {
        let json = r#"{"packages":[
            {"name":"trading-post","manifest_path":"/w/a/Cargo.toml","targets":[],
             "metadata":{"enshwick":{"id":"you.trading-post","side":"both"}}},
            {"name":"stalls","manifest_path":"/w/b/Cargo.toml","targets":[],"metadata":null},
            {"name":"other","manifest_path":"/w/c/Cargo.toml","targets":[],"metadata":{"docs":{}}}]}"#;
        let metadata: Metadata = serde_json::from_str(json).unwrap();
        let folders: Vec<&str> = metadata.packages.iter().map(Package::folder).collect();
        assert_eq!(folders, ["you.trading-post", "stalls", "other"]);
    }

    #[test]
    fn a_pack_install_drops_old_files_with_their_folders_and_refuses_old_files_that_do_not_read() {
        let game = crate::scratch("place");
        let built = Built {
            id: "you.pack".to_owned(),
            version: "1.0.0".to_owned(),
            summary: String::new(),
            files: vec![("main.luau".to_owned(), b"print()".to_vec())],
            dll: None,
        };
        let folder = game.join("mods/you.pack");
        assert_eq!(built.place(&game).unwrap(), folder, "a first install");
        std::fs::create_dir_all(folder.join("lib")).unwrap();
        std::fs::write(folder.join("lib/old.luau"), "").unwrap();
        std::fs::write(folder.join("notes.txt"), "kept").unwrap();
        built.place(&game).unwrap();
        assert!(!folder.join("lib").exists());
        assert!(folder.join("notes.txt").is_file());
        assert!(folder.join("main.luau").is_file());
        let too_large = usize::try_from(enshwick_core::packs::MAX_FILE).unwrap() + 1;
        std::fs::write(folder.join("items.toml"), vec![b' '; too_large]).unwrap();
        let refused = built.place(&game).unwrap_err();
        assert!(
            refused.to_string().contains("do not read as a pack"),
            "{refused:#}"
        );
        assert!(format!("{refused:#}").contains("items.toml"), "{refused:#}");
        let _gone = std::fs::remove_dir_all(&game);
    }
}
