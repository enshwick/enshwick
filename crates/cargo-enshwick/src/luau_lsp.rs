//! The luau-lsp that `cargo enshwick check` and the tests run: one named by
//! `LUAU_LSP` or on the path, else the pinned release, downloaded once into
//! the user's cache and checked against its SHA-256.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest as _, Sha256};

/// The variable naming a luau-lsp to run instead of looking for one.
pub const VAR: &str = "LUAU_LSP";

/// The pinned release per host, as tables named `<os>-<arch>`.
const PIN: &str = include_str!("../luau-lsp.toml");

/// Where the pinned releases are downloaded from.
const RELEASES: &str = "https://github.com/JohnnyMorganz/luau-lsp/releases/download";

/// The tool's file name in a release archive and in the cache.
const EXE: &str = if cfg!(windows) {
    "luau-lsp.exe"
} else {
    "luau-lsp"
};

/// The largest release archive taken, in bytes; luau-lsp's are under 20 MB.
const LIMIT: usize = 64 << 20;

/// How long a download may take, in seconds.
const SECONDS: u32 = 300;

/// Why no luau-lsp is at hand.
///
/// # Examples
///
/// ```no_run
/// use cargo_enshwick::luau_lsp::{Error, find};
///
/// let downloaded = !matches!(find(true), Err(Error::Offline(_)));
/// # let _ = downloaded;
/// ```
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// It is not installed and the download was not allowed.
    #[error("luau-lsp is not on the path or in the cache, and --offline skips the download of {0}")]
    Offline(String),
    /// `LUAU_LSP` names a tool that does not run.
    #[error("{VAR} names {}, which does not run `--version`", .0.display())]
    Named(PathBuf),
    /// The pin has no release for this host.
    #[error("luau-lsp has no release for {0}; put luau-lsp on the path or name it in {VAR}")]
    NoRelease(String),
    /// Neither the platform's cache folder nor a home folder is known.
    #[error("no cache folder for luau-lsp: set {VAR} to a luau-lsp to run")]
    NoCache,
    /// The download failed.
    #[error(transparent)]
    Download(#[from] crate::net::Error),
    /// The download is not the file the pin names.
    #[error("{url}: SHA-256 {found}, but the pin says {pinned}")]
    Mismatch {
        /// What was downloaded.
        url: String,
        /// The download's digest.
        found: String,
        /// The pinned digest.
        pinned: String,
    },
    /// The archive does not read or holds no tool.
    #[error("{url}: {why}")]
    Archive {
        /// What was downloaded.
        url: String,
        /// What is wrong with it.
        why: String,
    },
    /// A file in the cache could not be written.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file.
        path: PathBuf,
        /// The failure.
        source: std::io::Error,
    },
}

/// One pinned download.
#[derive(Debug, serde::Deserialize)]
struct Release {
    version: String,
    asset: String,
    sha256: String,
}

/// The luau-lsp to run: `LUAU_LSP`, else `luau-lsp` on the path, else the
/// pinned release from the cache, downloaded into it first unless `offline`.
///
/// # Errors
///
/// [`Error`] when `LUAU_LSP` names a tool that does not run, or none is
/// found and none can be downloaded.
///
/// # Examples
///
/// ```no_run
/// let tool = cargo_enshwick::luau_lsp::find(false)?;
/// std::process::Command::new(tool).arg("--version").status()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn find(offline: bool) -> Result<PathBuf, Error> {
    find_from(std::env::var_os(VAR), offline)
}

/// [`find`] with `named` as the value of `LUAU_LSP`.
fn find_from(named: Option<std::ffi::OsString>, offline: bool) -> Result<PathBuf, Error> {
    if let Some(named) = named.filter(|named| !named.is_empty()) {
        let named = PathBuf::from(named);
        return if runs(&named) {
            Ok(named)
        } else {
            Err(Error::Named(named))
        };
    }
    if runs(Path::new("luau-lsp")) {
        return Ok("luau-lsp".into());
    }
    let host = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let release = release(PIN, &host)?;
    let dir = cache_dir()
        .ok_or(Error::NoCache)?
        .join(format!("luau-lsp-{}", release.version));
    let exe = dir.join(EXE);
    if exe.is_file() {
        return Ok(exe);
    }
    if offline {
        return Err(Error::Offline(release.version));
    }
    download(&release, &dir)?;
    Ok(exe)
}

/// The pinned release of `pin` for `host`.
fn release(pin: &str, host: &str) -> Result<Release, Error> {
    toml::from_str::<std::collections::BTreeMap<String, Release>>(pin)
        .ok()
        .and_then(|mut hosts| hosts.remove(host))
        .ok_or_else(|| Error::NoRelease(host.to_owned()))
}

/// Whether `tool --version` runs.
fn runs(tool: &Path) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .is_ok_and(|out| out.status.success())
}

/// The user's cache folder for enshwick's tools.
fn cache_dir() -> Option<PathBuf> {
    let var = |name| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let base = if cfg!(windows) {
        var("LOCALAPPDATA")
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Caches"))
    } else {
        var("XDG_CACHE_HOME").or_else(|| var("HOME").map(|home| home.join(".cache")))
    };
    base.map(|base| base.join("enshwick"))
}

/// Refuses `bytes`, downloaded from `url`, whose SHA-256 is not the one
/// `release` pins.
fn pinned(bytes: &[u8], release: &Release, url: &str) -> Result<(), Error> {
    let found = hex(&Sha256::digest(bytes));
    if found.eq_ignore_ascii_case(&release.sha256) {
        Ok(())
    } else {
        Err(Error::Mismatch {
            url: url.to_owned(),
            found,
            pinned: release.sha256.clone(),
        })
    }
}

/// Downloads `release`, checks its SHA-256 before anything is written and
/// unpacks the tool into `dir`.
fn download(release: &Release, dir: &Path) -> Result<(), Error> {
    let url = format!("{RELEASES}/{}/{}", release.version, release.asset);
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::Io { path, source }
    };
    let bytes = crate::net::get(&url, &[], LIMIT, SECONDS)?;
    pinned(&bytes, release, &url)?;
    let exe = unpack(&bytes, &url)?;
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    // Per process and thread, as parallel checks or tests may download at once.
    let part = dir.join(format!(
        "{EXE}.{}-{:?}.part",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&part, exe).map_err(io(&part))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))
            .map_err(io(&part))?;
    }
    let target = dir.join(EXE);
    match std::fs::rename(&part, &target) {
        // Another process put it there first, and Windows keeps a running
        // executable from being replaced.
        Err(_) if target.is_file() => {
            let _gone = std::fs::remove_file(&part);
            Ok(())
        }
        moved => moved.map_err(io(&target)),
    }
}

/// The tool's bytes in the zip archive `bytes`, downloaded from `url`.
fn unpack(bytes: &[u8], url: &str) -> Result<Vec<u8>, Error> {
    use std::io::Read as _;
    let failed = |why: String| Error::Archive {
        url: url.to_owned(),
        why,
    };
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| failed(e.to_string()))?;
    let mut file = archive
        .by_name(EXE)
        .map_err(|_| failed(format!("the archive holds no {EXE}")))?;
    let mut exe = Vec::new();
    file.read_to_end(&mut exe)
        .map_err(|e| failed(e.to_string()))?;
    Ok(exe)
}

/// `bytes` in lowercase hex.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _written = write!(out, "{b:02x}");
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_is_taken_only_with_the_pinned_digest() {
        let release = Release {
            version: "1".to_owned(),
            asset: "a.zip".to_owned(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned(),
        };
        assert!(pinned(b"", &release, "u").is_ok());
        let why = pinned(b"other", &release, "u").unwrap_err().to_string();
        assert!(
            why.ends_with(&format!("the pin says {}", release.sha256)),
            "{why}"
        );
    }

    #[test]
    fn a_named_tool_that_does_not_run_is_refused_by_its_name() {
        let named = std::env::temp_dir().join("no-such-luau-lsp.exe");
        let why = find_from(Some(named.clone().into()), true).unwrap_err();
        assert!(
            matches!(&why, Error::Named(path) if *path == named),
            "{why}"
        );
        assert!(why.to_string().starts_with("LUAU_LSP names "), "{why}");
    }

    #[test]
    fn a_named_tool_that_runs_is_taken_as_named() {
        // Cargo runs the tests with CARGO set to a program that answers --version.
        let cargo = std::env::var_os("CARGO").unwrap();
        assert_eq!(
            find_from(Some(cargo.clone()), true).unwrap(),
            PathBuf::from(cargo)
        );
    }

    #[test]
    fn an_empty_name_counts_as_none() {
        assert!(!matches!(
            find_from(Some("".into()), true),
            Err(Error::Named(_))
        ));
    }

    #[test]
    fn every_host_of_the_pin_names_one_version_and_a_digest() {
        let hosts: std::collections::BTreeMap<String, Release> = toml::from_str(PIN).unwrap();
        assert!(hosts.contains_key("windows-x86_64"));
        assert!(hosts.contains_key("linux-x86_64"));
        let versions: std::collections::BTreeSet<&str> =
            hosts.values().map(|r| r.version.as_str()).collect();
        assert_eq!(
            versions.len(),
            1,
            "one luau-lsp for every host: {versions:?}"
        );
        for (host, release) in &hosts {
            assert!(
                release.sha256.len() == 64 && release.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                "{host}"
            );
        }
    }

    #[test]
    fn a_host_without_a_release_is_named() {
        let why = release(PIN, "plan9-mips").unwrap_err().to_string();
        assert!(why.contains("plan9-mips"), "{why}");
    }

    #[test]
    fn a_digest_is_lowercase_hex() {
        assert_eq!(
            hex(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn an_archive_without_the_tool_is_refused() {
        let mut bytes = Vec::new();
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut bytes));
        zip.start_file("README.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        let why = unpack(&bytes, "u").unwrap_err().to_string();
        assert!(why.contains(EXE), "{why}");
    }
}
