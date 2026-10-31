//! `cargo xtask dist`: `enshwick.zip`, the archive of the `dev` prerelease,
//! in `target/dist`. A player unpacks it into the game folder: the stub, the
//! loader and the license. Each mod ships in its own archive from its own
//! repository.

use std::collections::BTreeMap;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{WINDOWS_TARGET, cargo, say};

const USAGE: &str = "usage: cargo xtask dist [--out <dir>]";
/// The archive that goes into the game folder.
const ARCHIVE: &str = "enshwick.zip";
/// The stub's package and where it goes: the game loads it as a system DLL.
const STUB: (&str, &str) = ("enshwick-loader-stub", "powrprof.dll");
/// The loader's package and where it goes: the stub loads it from there,
/// whatever its library is called.
const LOADER: (&str, &str) = ("enshwick-loader", "mods/enshwick/enshwick.dll");

pub(crate) fn run(args: &[String]) -> Result<bool, String> {
    let mut out = PathBuf::from("target/dist");
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(rest.next().ok_or(USAGE)?),
            _ => return Err(USAGE.to_owned()),
        }
    }
    let libraries = workspace_libraries()?;
    let release_dir = build()?;
    let built_as = |package: &str| {
        libraries
            .get(package)
            .map(|lib| release_dir.join(format!("{lib}.dll")))
            .ok_or_else(|| format!("{package} builds no library"))
    };
    let entries = contents(
        read(&built_as(STUB.0)?)?,
        read(&built_as(LOADER.0)?)?,
        read(Path::new("LICENSE"))?,
    );
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(ARCHIVE);
    std::fs::write(&path, zip(&entries)?).map_err(|e| format!("{}: {e}", path.display()))?;
    say(&format!(
        "wrote {} ({} files)",
        path.display(),
        entries.len()
    ));
    Ok(true)
}

/// [`ARCHIVE`] as path in the archive to content, in the game folder's
/// layout.
fn contents(stub: Vec<u8>, loader: Vec<u8>, license: Vec<u8>) -> BTreeMap<String, Vec<u8>> {
    [
        (STUB.1.to_owned(), stub),
        (LOADER.1.to_owned(), loader),
        ("LICENSE".to_owned(), license),
    ]
    .into_iter()
    .collect()
}

/// The packages of this workspace with a cdylib, as `package -> library`.
fn workspace_libraries() -> Result<BTreeMap<String, String>, String> {
    let output = Command::new(cargo())
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .output()
        .map_err(|error| format!("running cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("reading cargo metadata: {error}"))?;
    let mut libraries = BTreeMap::new();
    for p in metadata["packages"].as_array().into_iter().flatten() {
        let Some(lib) = p["targets"].as_array().into_iter().flatten().find(|t| {
            t["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|k| k == "cdylib"))
        }) else {
            continue;
        };
        if let (Some(name), Some(lib)) = (p["name"].as_str(), lib["name"].as_str()) {
            libraries.insert(name.to_owned(), lib.to_owned());
        }
    }
    Ok(libraries)
}

/// Builds the stub and the loader in release for Windows; the folder that
/// holds the DLLs.
fn build() -> Result<PathBuf, String> {
    let mut args: Vec<String> = if cfg!(windows) {
        vec!["build".to_owned()]
    } else {
        vec!["xwin".to_owned(), "build".to_owned()]
    };
    // CI's dev builds embed the dependency tree in each DLL, for
    // `cargo audit bin` and other scanners.
    if std::env::var_os("ENSHWICK_AUDITABLE").is_some_and(|value| value == "1") {
        args.insert(0, "auditable".to_owned());
    }
    args.extend(
        [
            "--release",
            "--locked",
            "--target",
            WINDOWS_TARGET,
            "--package",
            STUB.0,
            "--package",
            LOADER.0,
        ]
        .iter()
        .map(|a| (*a).to_owned()),
    );
    say(&format!("> cargo {}", args.join(" ")));
    let status = Command::new(cargo())
        .args(&args)
        .status()
        .map_err(|error| format!("running cargo: {error}"))?;
    if !status.success() {
        return Err("the build failed".to_owned());
    }
    Ok(Path::new("target").join(WINDOWS_TARGET).join("release"))
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

    #[test]
    fn the_archive_holds_the_stub_the_loader_and_the_license_alone() {
        let entries = contents(b"stub".to_vec(), b"loader".to_vec(), b"MIT".to_vec());
        assert_eq!(
            entries.keys().map(String::as_str).collect::<Vec<_>>(),
            ["LICENSE", "mods/enshwick/enshwick.dll", "powrprof.dll"]
        );
    }

    #[test]
    fn the_same_files_give_the_same_archive_in_the_game_s_layout() {
        let entries = contents(b"stub".to_vec(), b"loader".to_vec(), b"MIT".to_vec());
        let first = zip(&entries).unwrap();
        assert_eq!(first, zip(&entries).unwrap());
        let archive = ::zip::ZipArchive::new(Cursor::new(first)).unwrap();
        let names: Vec<String> = archive
            .file_names()
            .map(|name| name.unwrap().into_owned())
            .collect();
        assert_eq!(
            names,
            ["LICENSE", "mods/enshwick/enshwick.dll", "powrprof.dll"]
        );
    }
}
