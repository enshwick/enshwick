//! `cargo xtask dist`: `enshwick.zip`, the archive of an alpha release, in
//! `target/dist`. A player unpacks it into the game folder: the stub, the
//! loader and the license, and with [`KEY_VAR`] set the release signature of
//! each DLL beside it, which the loader checks at start. Each mod ships in
//! its own archive from its own repository.

use std::collections::BTreeMap;
use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use enshwick_gamedata::signature;

use crate::commands::check::WINDOWS_TARGET;
use crate::{cargo, say};

/// The archive that goes into the game folder.
const ARCHIVE: &str = "enshwick.zip";
/// The stub's package and where it goes: the game loads it as a system DLL.
const STUB: (&str, &str) = ("enshwick-loader-stub", "powrprof.dll");
/// The loader's package and where it goes: the stub loads it from there,
/// whatever its library is called.
const LOADER: (&str, &str) = ("enshwick-loader", "mods/enshwick/enshwick.dll");
/// The variable release CI sets to the release key's 32-byte secret, as 64
/// hex digits; the value is never shown.
const KEY_VAR: &str = "ENSHWICK_SIGNING_KEY";

/// Signs one file's bytes.
type Sign<'a> = &'a dyn Fn(&[u8]) -> Result<[u8; signature::LEN], String>;

pub(crate) fn run(out: &Path, signed: bool) -> Result<bool, String> {
    let seed = seed(std::env::var(KEY_VAR).ok().as_deref())?;
    if signed && seed.is_none() {
        return Err(format!("--signed needs {KEY_VAR}"));
    }
    let sign = seed.map(|seed| {
        move |file: &[u8]| signature::sign(&seed, file).map_err(|e| format!("{KEY_VAR}: {e}"))
    });
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
        sign.as_ref().map(|sign| sign as Sign<'_>),
    )?;
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(ARCHIVE);
    std::fs::write(&path, zip(&entries)?).map_err(|e| format!("{}: {e}", path.display()))?;
    say(&format!(
        "wrote {} ({} files, {})",
        path.display(),
        entries.len(),
        if sign.is_some() {
            "signed"
        } else {
            "unsigned: the loader names it an unofficial build"
        }
    ));
    Ok(true)
}

/// The release key's secret from [`KEY_VAR`]'s value; none when it is unset
/// or empty.
fn seed(value: Option<&str>) -> Result<Option<[u8; 32]>, String> {
    let Some(hex) = value.map(str::trim).filter(|hex| !hex.is_empty()) else {
        return Ok(None);
    };
    let not_hex = || format!("{KEY_VAR} is not 64 hex digits");
    if hex.len() != 64 {
        return Err(not_hex());
    }
    let digit = |b: &u8| char::from(*b).to_digit(16).ok_or_else(not_hex);
    let mut seed = [0; 32];
    for (byte, [high, low]) in seed.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
        *byte = u8::try_from(digit(high)? << 4 | digit(low)?).map_err(|_| not_hex())?;
    }
    Ok(Some(seed))
}

/// [`ARCHIVE`] as path in the archive to content, in the game folder's
/// layout, with each DLL's signature beside it when `sign` is given.
fn contents(
    stub: Vec<u8>,
    loader: Vec<u8>,
    license: Vec<u8>,
    sign: Option<Sign<'_>>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut entries = BTreeMap::new();
    for (path, dll) in [(STUB.1, stub), (LOADER.1, loader)] {
        if let Some(sign) = sign {
            let signed = format!("{path}.{}", signature::EXTENSION);
            entries.insert(signed, sign(&dll)?.to_vec());
        }
        entries.insert(path.to_owned(), dll);
    }
    entries.insert("LICENSE".to_owned(), license);
    Ok(entries)
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
    // CI's builds embed the dependency tree in each DLL, for
    // `cargo audit bin` and other scanners.
    let auditable = std::env::var_os("ENSHWICK_AUDITABLE").is_some_and(|value| value == "1");
    for package in [STUB.0, LOADER.0] {
        let args = build_args(cfg!(windows), auditable, package);
        say(&format!("> cargo {}", args.join(" ")));
        let status = Command::new(cargo())
            .args(&args)
            .status()
            .map_err(|error| format!("running cargo: {error}"))?;
        if !status.success() {
            return Err(format!("the build of {package} failed"));
        }
    }
    Ok(Path::new("target").join(WINDOWS_TARGET).join("release"))
}

/// The cargo arguments that build `package`'s library in release as a
/// Windows DLL. It builds as a cdylib alone, since cargo runs the release
/// profile's LTO only on a unit whose every crate type can take it, and the
/// loader's rlib for its tests and benches cannot.
fn build_args(windows_host: bool, auditable: bool, package: &str) -> Vec<&str> {
    let auditable: &[&str] = if auditable { &["auditable"] } else { &[] };
    let xwin: &[&str] = if windows_host { &[] } else { &["xwin"] };
    [
        auditable,
        xwin,
        &[
            "rustc",
            "--release",
            "--locked",
            "--target",
            WINDOWS_TARGET,
            "--package",
            package,
            "--lib",
            "--crate-type",
            "cdylib",
        ],
    ]
    .concat()
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
    fn each_dll_builds_as_a_cdylib_alone_so_fat_lto_reaches_it() {
        assert_eq!(
            build_args(true, false, LOADER.0),
            [
                "rustc",
                "--release",
                "--locked",
                "--target",
                WINDOWS_TARGET,
                "--package",
                LOADER.0,
                "--lib",
                "--crate-type",
                "cdylib"
            ]
        );
        assert_eq!(
            build_args(false, true, STUB.0)[..3],
            ["auditable", "xwin", "rustc"]
        );
    }

    fn unsigned() -> BTreeMap<String, Vec<u8>> {
        contents(b"stub".to_vec(), b"loader".to_vec(), b"MIT".to_vec(), None).unwrap()
    }

    #[test]
    fn the_archive_holds_the_stub_the_loader_and_the_license_alone() {
        assert_eq!(
            unsigned().keys().map(String::as_str).collect::<Vec<_>>(),
            ["LICENSE", "mods/enshwick/enshwick.dll", "powrprof.dll"]
        );
    }

    #[test]
    fn a_signed_archive_holds_each_dll_s_signature_of_its_own_bytes_beside_it() {
        let sign = |file: &[u8]| Ok([file[0]; signature::LEN]);
        let entries = contents(
            b"stub".to_vec(),
            b"loader".to_vec(),
            b"MIT".to_vec(),
            Some(&sign),
        )
        .unwrap();
        assert_eq!(
            entries.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "LICENSE",
                "mods/enshwick/enshwick.dll",
                "mods/enshwick/enshwick.dll.sig",
                "powrprof.dll",
                "powrprof.dll.sig"
            ]
        );
        assert_eq!(entries["powrprof.dll.sig"], [b's'; signature::LEN]);
        assert_eq!(
            entries["mods/enshwick/enshwick.dll.sig"],
            [b'l'; signature::LEN]
        );
    }

    #[test]
    fn a_signing_failure_writes_no_archive() {
        let sign = |_: &[u8]| Err("no key".to_owned());
        let made = contents(
            b"stub".to_vec(),
            b"loader".to_vec(),
            b"MIT".to_vec(),
            Some(&sign),
        );
        assert_eq!(made, Err("no key".to_owned()));
    }

    #[test]
    fn the_key_reads_as_64_hex_digits_and_an_empty_one_as_none() {
        let hex = "00ff".repeat(16);
        let read = seed(Some(&hex)).unwrap().unwrap();
        assert_eq!((read[0], read[1], read[31]), (0x00, 0xff, 0xff));
        assert_eq!(seed(Some(" ")), Ok(None));
        assert_eq!(seed(None), Ok(None));
    }

    #[test]
    fn a_key_that_is_not_64_hex_digits_is_refused_without_showing_it() {
        for bad in [
            "abc".to_owned(),
            format!("+{}", "0".repeat(63)),
            "g".repeat(64),
        ] {
            let refused = seed(Some(&bad)).unwrap_err();
            assert!(!refused.contains(&bad), "{refused}");
        }
    }

    #[test]
    fn the_same_files_give_the_same_archive_in_the_game_s_layout() {
        let entries = unsigned();
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
