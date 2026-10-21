//! Build tables: when the loader runs a game build that has no table yet, on
//! the code patterns of an older build's table, the updater fetches the
//! table for this build from the release the project publishes them in. The
//! loader uses it from the next start, and only when Enshwick's key signed
//! it, so the updater checks the signature before it writes anything.

use std::path::Path;

use enshwick_loader_api::abi::{Fingerprint, Side};

/// The release whose files are the build tables, each beside its
/// signature.
pub const RELEASE: &str = "build-tables";

/// The first word of a table signature's trusted comment; the second is the
/// table's file name.
pub const SIGNATURE_TAG: &str = "build-table";

/// Where the loader keeps fetched tables, relative to the game's folder.
pub const DIR: &str = "mods/enshwick/builds";

/// The file name of the table for a build, as the loader looks for it.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::{Fingerprint, Side};
/// use enshwick_updater::tables::file_name;
///
/// let build = Fingerprint { guid: [0xab; 16], age: 7 };
/// assert_eq!(
///     file_name(Side::SERVER, build),
///     Some(format!("server-{}-7.toml", "ab".repeat(16)))
/// );
/// assert_eq!(file_name(Side::BOTH, build), None);
/// ```
#[must_use]
pub fn file_name(side: Side, fingerprint: Fingerprint) -> Option<String> {
    let side = match side {
        Side::SERVER => "server",
        Side::CLIENT => "client",
        _ => return None,
    };
    let guid = fingerprint
        .guid
        .iter()
        .fold(String::new(), |mut out, byte| {
            use core::fmt::Write as _;
            let _infallible = write!(out, "{byte:02x}");
            out
        });
    Some(format!("{side}-{guid}-{}.toml", fingerprint.age))
}

/// The address of the table `file` in `repository`'s table release.
///
/// # Examples
///
/// ```
/// use enshwick_updater::tables::url;
///
/// assert_eq!(
///     url("themaiby/enshwick", "server-x.toml"),
///     "https://github.com/themaiby/enshwick/releases/download/build-tables/server-x.toml"
/// );
/// ```
#[must_use]
pub fn url(repository: &str, file: &str) -> String {
    format!("https://github.com/{repository}/releases/download/{RELEASE}/{file}")
}

/// Checks `table` against `signature` with Enshwick's key and writes both
/// into `game`'s table folder, the signature first, so the loader never sees
/// a table without one.
///
/// # Errors
///
/// Why the table is refused or could not be written.
pub fn store(game: &Path, file: &str, table: &[u8], signature: &str) -> Result<(), String> {
    let keys = [crate::verify::key(crate::verify::ENSHWICK_KEY)?];
    crate::verify::check(&keys, table, signature, SIGNATURE_TAG, file)
        .map_err(|why| format!("refused: {why}"))?;
    let dir = game.join(DIR);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let write = |name: String, bytes: &[u8]| {
        let path = dir.join(&name);
        let partial = dir.join(format!("{name}.partial"));
        std::fs::write(&partial, bytes)
            .and_then(|()| std::fs::rename(&partial, &path))
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    write(format!("{file}.minisig"), signature.as_bytes())?;
    write(file.to_owned(), table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsigned_table_is_never_written() {
        let game = std::env::temp_dir().join(format!("enshwick-tables-{}", std::process::id()));
        let refused = store(&game, "server-x.toml", b"table", "garbage");
        assert!(refused.unwrap_err().contains("unreadable signature"));
        let signature = include_str!("../testdata/signed.minisig");
        let refused = store(&game, "server-x.toml", b"test", signature);
        assert!(refused.unwrap_err().starts_with("refused:"));
        assert!(!game.join(DIR).join("server-x.toml").exists());
        let _gone = std::fs::remove_dir_all(&game);
    }
}
