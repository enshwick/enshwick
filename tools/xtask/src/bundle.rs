//! `cargo xtask bundle`: the archives of the `dev` prerelease, in
//! `target/bundle`. `enshwick.zip`, which a tester unpacks into the game
//! folder, holds the stub, the loader and every mod of `tools/mods.toml`,
//! each from the `dev` prerelease of its repository when that repository has
//! one and built from this workspace otherwise.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{WINDOWS_TARGET, cargo, say};

const USAGE: &str = "usage: cargo xtask bundle [--out <dir>] [--local]";
/// The archive with everything that goes into the game folder.
const BUNDLE: &str = "enshwick.zip";
/// The archive with the stub and the loader alone.
const LOADER_ONLY: &str = "enshwick-loader.zip";
/// The list of mods, relative to the workspace.
const MODS: &str = "tools/mods.toml";
/// The stub's package and where it goes: the game loads it as a system DLL.
const STUB: (&str, &str) = ("enshwick-loader-stub", "powrprof.dll");
/// The loader's package and where it goes: the stub loads it from there,
/// whatever its library is called.
const LOADER: (&str, &str) = ("enshwick-loader", "mods/enshwick/enshwick.dll");

/// One mod of `tools/mods.toml`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Listed {
    /// The package, which names the DLL in the bundle.
    package: String,
    /// `owner/name` on GitHub.
    repository: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ModsFile {
    #[serde(rename = "mod", default)]
    mods: Vec<Listed>,
}

/// Where one mod of the bundle comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    /// The `dev` prerelease of its repository.
    Released { package: String, repository: String },
    /// This workspace, built as the library `lib`.
    Built { package: String, lib: String },
}

impl Source {
    fn package(&self) -> &str {
        match self {
            Self::Released { package, .. } | Self::Built { package, .. } => package,
        }
    }
}

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let mut out = PathBuf::from("target/bundle");
    let mut local = false;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(rest.next().ok_or(USAGE)?),
            "--local" => local = true,
            _ => return Err(USAGE.to_owned()),
        }
    }
    let listed = read_list(&std::fs::read_to_string(MODS).map_err(|e| format!("{MODS}: {e}"))?)?;
    let (libraries, workspace) = workspace_libraries()?;
    let built_as = |package: &str| {
        libraries
            .get(package)
            .map(|lib| release_dir_of().join(format!("{lib}.dll")))
            .ok_or_else(|| format!("{package} builds no library"))
    };
    let sources = plan(&listed, &workspace, |repository| {
        if local {
            Ok(false)
        } else {
            has_dev_release(repository)
        }
    })?;
    for source in &sources {
        match source {
            Source::Released {
                package,
                repository,
            } => say(&format!("{package}: the dev prerelease of {repository}")),
            Source::Built { package, .. } => say(&format!("{package}: built here")),
        }
    }
    let release_dir = build(&sources)?;
    let staging = out.join("downloads");
    let mut mods = Vec::with_capacity(sources.len());
    for source in &sources {
        let (dll, built) = match source {
            Source::Built { lib, .. } => (read(&release_dir.join(format!("{lib}.dll")))?, true),
            Source::Released {
                package,
                repository,
            } => (download(repository, package, &staging)?, false),
        };
        mods.push(Mod {
            package: source.package().to_owned(),
            dll,
            built,
        });
    }
    let _gone = std::fs::remove_dir_all(&staging);
    let parts = Parts {
        stub: read(&built_as(STUB.0)?)?,
        loader: read(&built_as(LOADER.0)?)?,
        license: read(Path::new("LICENSE"))?,
        mods,
    };
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    // Archives of an earlier run, such as that of a mod now taken from its
    // own repository, would be published with these.
    for entry in std::fs::read_dir(&out).map_err(|e| format!("{}: {e}", out.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "zip") {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    for (name, entries) in archives(&parts) {
        let path = out.join(&name);
        std::fs::write(&path, zip(&entries)?).map_err(|e| format!("{}: {e}", path.display()))?;
        say(&format!(
            "wrote {} ({} files)",
            path.display(),
            entries.len()
        ));
    }
    Ok(true)
}

/// One mod's DLL, and whether it was built here.
struct Mod {
    package: String,
    dll: Vec<u8>,
    built: bool,
}

/// What the archives are made of.
struct Parts {
    stub: Vec<u8>,
    loader: Vec<u8>,
    license: Vec<u8>,
    mods: Vec<Mod>,
}

/// The archives by name, each as path in the archive to content:
/// [`BUNDLE`] with everything that goes into the game folder, [`LOADER_ONLY`]
/// with the stub and the loader, and `<package>.zip` for each mod built
/// here; a mod from its own repository has its own archive there.
fn archives(parts: &Parts) -> BTreeMap<String, BTreeMap<String, Vec<u8>>> {
    let license = ("LICENSE".to_owned(), parts.license.clone());
    let loader: BTreeMap<String, Vec<u8>> = [
        (STUB.1.to_owned(), parts.stub.clone()),
        (LOADER.1.to_owned(), parts.loader.clone()),
        license.clone(),
    ]
    .into_iter()
    .collect();
    let mut bundle = loader.clone();
    let mut out = BTreeMap::new();
    for m in &parts.mods {
        let dll = (format!("mods/{}.dll", m.package), m.dll.clone());
        if m.built {
            out.insert(
                format!("{}.zip", m.package),
                [dll.clone(), license.clone()].into_iter().collect(),
            );
        }
        bundle.insert(dll.0, dll.1);
    }
    out.insert(LOADER_ONLY.to_owned(), loader);
    out.insert(BUNDLE.to_owned(), bundle);
    out
}

fn read_list(text: &str) -> Result<Vec<Listed>, String> {
    let file: ModsFile = toml::from_str(text).map_err(|e| format!("{MODS}: {e}"))?;
    Ok(file.mods)
}

/// The packages of this workspace with a cdylib, as `package -> library`,
/// and of them the mods that release: those under `mods/`, without
/// `[package.metadata.enshwick] release = false`.
type Libraries = BTreeMap<String, String>;

fn workspace_libraries() -> Result<(Libraries, Libraries), String> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let root = metadata["workspace_root"]
        .as_str()
        .unwrap_or_default()
        .replace('\\', "/");
    let (mut libraries, mut mods) = (Libraries::new(), Libraries::new());
    for p in metadata["packages"].as_array().into_iter().flatten() {
        let Some(lib) = p["targets"].as_array().into_iter().flatten().find(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|k| k == "cdylib"))
        }) else {
            continue;
        };
        let (Some(name), Some(lib)) = (p["name"].as_str(), lib["name"].as_str()) else {
            continue;
        };
        libraries.insert(name.to_owned(), lib.to_owned());
        let is_mod = p["manifest_path"]
            .as_str()
            .unwrap_or_default()
            .replace('\\', "/")
            .starts_with(&format!("{root}/mods/"));
        if is_mod && p["metadata"]["enshwick"]["release"].as_bool() != Some(false) {
            mods.insert(name.to_owned(), lib.to_owned());
        }
    }
    Ok((libraries, mods))
}

/// Where the release build for Windows puts its DLLs.
fn release_dir_of() -> PathBuf {
    Path::new("target").join(WINDOWS_TARGET).join("release")
}

/// Where each listed mod comes from: its repository's `dev` prerelease
/// where `has_dev` says there is one, else this workspace. A listed mod
/// with neither, a workspace mod the list lacks, or a failed lookup is an
/// error.
fn plan(
    listed: &[Listed],
    workspace: &BTreeMap<String, String>,
    has_dev: impl Fn(&str) -> Result<bool, String>,
) -> Result<Vec<Source>, String> {
    let names: BTreeSet<&str> = listed.iter().map(|l| l.package.as_str()).collect();
    let unlisted: Vec<&str> = workspace
        .keys()
        .map(String::as_str)
        .filter(|package| !names.contains(package))
        .collect();
    if !unlisted.is_empty() {
        return Err(format!("{MODS} lacks {}", unlisted.join(", ")));
    }
    listed
        .iter()
        .map(|l| {
            if has_dev(&l.repository)? {
                Ok(Source::Released {
                    package: l.package.clone(),
                    repository: l.repository.clone(),
                })
            } else if let Some(lib) = workspace.get(&l.package) {
                Ok(Source::Built {
                    package: l.package.clone(),
                    lib: lib.clone(),
                })
            } else {
                Err(format!(
                    "{}: {} has no dev prerelease and the workspace no such mod",
                    l.package, l.repository
                ))
            }
        })
        .collect()
}

/// Builds the stub, the loader and the mods built here, in release for
/// Windows; the folder that holds the DLLs.
fn build(sources: &[Source]) -> Result<PathBuf, String> {
    let mut args: Vec<String> = if cfg!(windows) {
        vec!["build".to_owned()]
    } else {
        vec!["xwin".to_owned(), "build".to_owned()]
    };
    args.extend(
        ["--release", "--locked", "--target", WINDOWS_TARGET]
            .iter()
            .map(|a| (*a).to_owned()),
    );
    let built = sources.iter().filter_map(|s| match s {
        Source::Built { package, .. } => Some(package.as_str()),
        Source::Released { .. } => None,
    });
    for package in [STUB.0, LOADER.0].into_iter().chain(built) {
        args.extend(["--package".to_owned(), package.to_owned()]);
    }
    say(&format!("> cargo {}", args.join(" ")));
    let status = Command::new(cargo())
        .args(&args)
        .status()
        .map_err(|error| format!("running cargo: {error}"))?;
    if !status.success() {
        return Err("the build failed".to_owned());
    }
    Ok(release_dir_of())
}

/// Whether `repository` has a `dev` prerelease; an error when gh cannot
/// tell, as when it is missing or not logged in, rather than a mod built
/// here in place of its released one.
fn has_dev_release(repository: &str) -> Result<bool, String> {
    /// gh's exit code when it needs a login.
    const NOT_LOGGED_IN: i32 = 4;
    let output = Command::new("gh")
        .args([
            "release", "view", "dev", "--repo", repository, "--json", "tagName",
        ])
        .output()
        .map_err(|error| {
            format!("running gh, which looks for the mods' dev prereleases: {error}; --local builds every mod here")
        })?;
    if output.status.code() == Some(NOT_LOGGED_IN) {
        return Err(format!(
            "gh needs a login (gh auth login, or GH_TOKEN) to look at {repository}"
        ));
    }
    Ok(output.status.success())
}

/// The DLL of `package` from the `dev` prerelease of `repository`: its
/// archive `<package>.zip`, downloaded into `dir`, holds it as
/// `mods/<package>.dll`, as the single-mod archive players download does.
fn download(repository: &str, package: &str, dir: &Path) -> Result<Vec<u8>, String> {
    let file = format!("{package}.zip");
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let status = Command::new("gh")
        .args([
            "release",
            "download",
            "dev",
            "--repo",
            repository,
            "--clobber",
        ])
        .args(["--pattern", file.as_str(), "--dir"])
        .arg(dir)
        .status()
        .map_err(|error| format!("running gh: {error}"))?;
    if !status.success() {
        return Err(format!("{repository}: no {file} on its dev prerelease"));
    }
    dll_in(&read(&dir.join(&file))?, package).map_err(|e| format!("{repository} {file}: {e}"))
}

/// The DLL `mods/<package>.dll` in the single-mod `archive`.
fn dll_in(archive: &[u8], package: &str) -> Result<Vec<u8>, String> {
    let mut archive = ::zip::ZipArchive::new(Cursor::new(archive)).map_err(|e| e.to_string())?;
    let path = format!("mods/{package}.dll");
    let mut entry = archive.by_name(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut dll = Vec::new();
    std::io::Read::read_to_end(&mut entry, &mut dll).map_err(|e| format!("{path}: {e}"))?;
    Ok(dll)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The archive of `entries`, path in the archive to content, in path order
/// and with the fixed time of the format's epoch, so the same files give
/// the same bytes.
fn zip(entries: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut archive = ::zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = ::zip::write::SimpleFileOptions::default()
        .compression_method(::zip::CompressionMethod::Deflated)
        .last_modified_time(::zip::DateTime::default());
    for (path, content) in entries {
        archive
            .start_file(path.as_str(), options)
            .and_then(|()| archive.write_all(content).map_err(Into::into))
            .map_err(|e| format!("{path}: {e}"))?;
    }
    archive
        .finish()
        .map(Cursor::into_inner)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(package: &str) -> Listed {
        Listed {
            package: package.to_owned(),
            repository: format!("enshwick/mod-{package}"),
        }
    }

    fn workspace(packages: &[&str]) -> BTreeMap<String, String> {
        packages
            .iter()
            .map(|p| ((*p).to_owned(), p.replace('-', "_")))
            .collect()
    }

    #[test]
    fn a_mod_comes_from_its_dev_prerelease_once_its_repository_has_one() {
        let list = [listed("safes"), listed("zones")];
        let sources = plan(&list, &workspace(&["safes", "zones"]), |repository| {
            Ok(repository == "enshwick/mod-safes")
        })
        .unwrap();
        assert_eq!(
            sources,
            [
                Source::Released {
                    package: "safes".into(),
                    repository: "enshwick/mod-safes".into()
                },
                Source::Built {
                    package: "zones".into(),
                    lib: "zones".into()
                },
            ]
        );
    }

    #[test]
    fn a_mod_that_left_the_workspace_needs_its_dev_prerelease() {
        let list = [listed("safes")];
        assert!(plan(&list, &workspace(&[]), |_| Ok(false)).is_err());
        assert!(plan(&list, &workspace(&[]), |_| Ok(true)).is_ok());
    }

    #[test]
    fn a_lookup_that_cannot_run_stops_the_plan() {
        let failed = plan(&[listed("safes")], &workspace(&["safes"]), |_| {
            Err("gh: not found".to_owned())
        });
        assert_eq!(failed, Err("gh: not found".to_owned()));
    }

    #[test]
    fn a_workspace_mod_missing_from_the_list_is_an_error() {
        let error = plan(
            &[listed("safes")],
            &workspace(&["safes", "new-mod"]),
            |_| Ok(false),
        );
        assert_eq!(error, Err(format!("{MODS} lacks new-mod")));
    }

    #[test]
    fn the_list_in_the_repository_reads() {
        let text =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../mods.toml"))
                .unwrap();
        let list = read_list(&text).unwrap();
        assert!(
            list.iter()
                .all(|l| l.repository.starts_with("enshwick/mod-"))
        );
        assert!(read_list("[[mod]]\npackage = \"a\"\n").is_err());
    }

    #[test]
    fn the_bundle_holds_every_mod_and_only_built_mods_get_their_own_archive() {
        let parts = Parts {
            stub: b"stub".to_vec(),
            loader: b"loader".to_vec(),
            license: b"MIT".to_vec(),
            mods: vec![
                Mod {
                    package: "safes".into(),
                    dll: b"s".to_vec(),
                    built: true,
                },
                Mod {
                    package: "zones".into(),
                    dll: b"z".to_vec(),
                    built: false,
                },
            ],
        };
        let archives = archives(&parts);
        let names =
            |archive: &str| -> Vec<&str> { archives[archive].keys().map(String::as_str).collect() };
        assert_eq!(
            archives.keys().map(String::as_str).collect::<Vec<_>>(),
            ["enshwick-loader.zip", "enshwick.zip", "safes.zip"]
        );
        assert_eq!(
            names("enshwick.zip"),
            [
                "LICENSE",
                "mods/enshwick/enshwick.dll",
                "mods/safes.dll",
                "mods/zones.dll",
                "powrprof.dll"
            ]
        );
        assert_eq!(
            names("enshwick-loader.zip"),
            ["LICENSE", "mods/enshwick/enshwick.dll", "powrprof.dll"]
        );
        assert_eq!(names("safes.zip"), ["LICENSE", "mods/safes.dll"]);
    }

    #[test]
    fn a_released_mod_s_dll_comes_out_of_its_single_mod_archive() {
        let archive = zip(&[
            ("LICENSE".to_owned(), b"MIT".to_vec()),
            ("mods/safes.dll".to_owned(), b"dll".to_vec()),
        ]
        .into_iter()
        .collect())
        .unwrap();
        assert_eq!(dll_in(&archive, "safes").unwrap(), b"dll");
        assert!(dll_in(&archive, "zones").is_err());
    }

    #[test]
    fn the_same_files_give_the_same_archive_in_the_game_s_layout() {
        let entries: BTreeMap<String, Vec<u8>> = [
            ("powrprof.dll", b"stub".as_slice()),
            ("mods/enshwick/enshwick.dll", b"loader"),
            ("mods/safes.dll", b"mod"),
        ]
        .into_iter()
        .map(|(path, content)| (path.to_owned(), content.to_vec()))
        .collect();
        let first = zip(&entries).unwrap();
        assert_eq!(first, zip(&entries).unwrap());
        let archive = ::zip::ZipArchive::new(Cursor::new(first)).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        assert_eq!(
            names,
            [
                "mods/enshwick/enshwick.dll",
                "mods/safes.dll",
                "powrprof.dll"
            ]
        );
    }
}
