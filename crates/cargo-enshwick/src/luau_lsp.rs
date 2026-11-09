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

/// Why no luau-lsp is at hand.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// It is not installed and the download was not allowed.
    #[error("luau-lsp is not on the path or in the cache, and --offline skips the download of {0}")]
    Offline(String),
    /// The pin has no release for this host.
    #[error("luau-lsp has no release for {0}; put luau-lsp on the path or name it in {VAR}")]
    NoRelease(String),
    /// Neither the platform's cache folder nor a home folder is known.
    #[error("no cache folder for luau-lsp: set {VAR} to a luau-lsp to run")]
    NoCache,
    /// The download failed or does not match its pin.
    #[error("{url}: {why}")]
    Download {
        /// What was downloaded.
        url: String,
        /// What went wrong.
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
/// [`Error`] when none is found and none can be downloaded.
pub fn find(offline: bool) -> Result<PathBuf, Error> {
    if let Some(named) = std::env::var_os(VAR) {
        return Ok(named.into());
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

/// Downloads `release` with curl, which Windows, macOS and Linux ship,
/// checks its SHA-256 and unpacks the tool into `dir`.
fn download(release: &Release, dir: &Path) -> Result<(), Error> {
    let url = format!("{RELEASES}/{}/{}", release.version, release.asset);
    let failed = |why: String| Error::Download {
        url: url.clone(),
        why,
    };
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::Io { path, source }
    };
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    // Per process, as parallel checks or tests may download at once.
    let pid = std::process::id();
    let archive = dir.join(format!("{}.{pid}", release.asset));
    let out = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-sSfL"])
        .args(["--retry", "6", "--retry-all-errors", "-o"])
        .arg(&archive)
        .arg(&url)
        .output()
        .map_err(|e| failed(format!("curl does not run: {e}")))?;
    if !out.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        ));
    }
    let bytes = std::fs::read(&archive).map_err(io(&archive))?;
    let _gone = std::fs::remove_file(&archive);
    let found = hex(&Sha256::digest(&bytes));
    if found != release.sha256 {
        return Err(failed(format!(
            "SHA-256 {found}, but the pin says {}",
            release.sha256
        )));
    }
    let exe = unpack(&bytes).map_err(failed)?;
    let part = dir.join(format!("{EXE}.{pid}.part"));
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

/// The tool's bytes in the zip archive `bytes`.
fn unpack(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let mut file = archive
        .by_name(EXE)
        .map_err(|_| format!("the archive holds no {EXE}"))?;
    let mut exe = Vec::new();
    file.read_to_end(&mut exe).map_err(|e| e.to_string())?;
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
        let why = unpack(&bytes).unwrap_err();
        assert!(why.contains(EXE), "{why}");
    }
}
