//! Whether this is an official build: each file of the loader carries the
//! release signature of its bytes beside it, checked once per start on a
//! thread of its own. A build without it runs as it is, and the log and the
//! Mods screen name it unofficial.

use std::path::Path;

use enshwick_gamedata::signature;

/// Why each of `files`, relative to `game_dir`, lacks a valid release
/// signature, `file: why` a line each; empty for an official build.
pub(crate) fn unsigned(game_dir: &Path, files: &[&str]) -> Vec<String> {
    files
        .iter()
        .filter_map(|file| {
            let why = check(&game_dir.join(file)).err()?;
            Some(format!("{file}: {why}"))
        })
        .collect()
}

/// Why a loader file does not carry a valid release signature.
#[derive(Debug, thiserror::Error)]
enum CheckFileError {
    /// The file itself does not read.
    #[error("{0}")]
    Unread(std::io::Error),
    /// No signature file reads beside it.
    #[error("no signature ({0})")]
    Unsigned(std::io::Error),
    /// The signature does not hold.
    #[error(transparent)]
    Signature(#[from] signature::CheckSignatureError),
}

fn check(path: &Path) -> Result<(), CheckFileError> {
    let bytes = std::fs::read(path).map_err(CheckFileError::Unread)?;
    let mut signed = path.as_os_str().to_owned();
    signed.push(".");
    signed.push(signature::EXTENSION);
    let sig = std::fs::read(&signed).map_err(CheckFileError::Unsigned)?;
    Ok(signature::verify(&bytes, &sig)?)
}

/// Checks the loader's files on a thread of its own: an unofficial build is
/// logged once with its reasons and marked in `loader` for the Mods screen.
#[cfg(windows)]
pub(crate) fn start(loader: &'static crate::engine::loader::Loader) {
    use std::sync::atomic::Ordering;

    let Some(game_dir) = loader.mods_dir.parent().map(Path::to_path_buf) else {
        return;
    };
    let _started = crate::engine::off_thread::start("signature", move || {
        let unsigned = unsigned(&game_dir, &enshwick_base::mods::LOADER_FILES);
        if unsigned.is_empty() {
            crate::log!(
                INFO,
                "official build: every loader file carries the release signature"
            );
            return;
        }
        loader.unofficial.store(true, Ordering::Release);
        crate::log!(
            WARN,
            "unofficial build: {} ({})",
            unsigned.join("; "),
            crate::engine::errors::UNOFFICIAL
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-official-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("mods/enshwick")).unwrap();
        dir
    }

    #[test]
    fn a_file_without_its_signature_beside_it_is_unsigned() {
        let dir = folder("bare");
        std::fs::write(dir.join("powrprof.dll"), b"stub").unwrap();
        let lines = unsigned(&dir, &["powrprof.dll"]);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("powrprof.dll: no signature"),
            "{lines:?}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_signature_that_is_not_the_release_key_s_names_the_mismatch() {
        let dir = folder("forged");
        std::fs::write(dir.join("mods/enshwick/enshwick.dll"), b"loader").unwrap();
        std::fs::write(
            dir.join("mods/enshwick/enshwick.dll.sig"),
            [7; signature::LEN],
        )
        .unwrap();
        let lines = unsigned(&dir, &["mods/enshwick/enshwick.dll"]);
        assert_eq!(
            lines,
            [format!(
                "mods/enshwick/enshwick.dll: {}",
                signature::CheckSignatureError::Mismatch
            )]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn every_file_is_checked_and_each_named_on_its_own_line() {
        let dir = folder("both");
        let lines = unsigned(&dir, &enshwick_base::mods::LOADER_FILES);
        assert_eq!(lines.len(), enshwick_base::mods::LOADER_FILES.len());
        for (line, file) in lines.iter().zip(enshwick_base::mods::LOADER_FILES) {
            assert!(line.starts_with(file), "{line}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
