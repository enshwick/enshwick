//! `install --with-loader`: puts the Enshwick loader of the release this tool
//! is from into a game folder, from that release's `enshwick.zip` on GitHub,
//! checked against the release's `SHA256SUMS`. A loader already there that
//! differs is named and left as it is.

use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use sha2::{Digest as _, Sha256};

/// Where the release files of each tag are downloaded from.
const DOWNLOADS: &str = "https://github.com/enshwick/enshwick/releases/download";
/// The release file a player unpacks into the game folder.
const ARCHIVE: &str = "enshwick.zip";
/// The release file of each other file's SHA-256, as `sha256sum` writes it.
const SUMS: &str = "SHA256SUMS";
/// The files that make a loader install: the stub the game loads and the
/// loader the stub loads.
const FILES: [&str; 2] = ["powrprof.dll", "mods/enshwick/enshwick.dll"];
/// The most bytes of the archive read; it holds two DLLs and a license.
const ARCHIVE_LIMIT: usize = 64 << 20;
/// The most bytes of the checksum list read.
const SUMS_LIMIT: usize = 64 << 10;
/// The longest each download takes before it is given up.
const SECONDS: u32 = 300;

/// The release this tool is from, whose loader it puts in.
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Whether `game` holds both files of a loader install.
pub(crate) fn is_installed(game: &Path) -> bool {
    FILES.iter().all(|file| game.join(file).is_file())
}

/// What `install` says of a game folder without a loader.
pub(crate) fn missing_note(game: &Path) -> String {
    format!(
        "{} has no Enshwick loader; `--with-loader` puts in that of {}",
        game.display(),
        version()
    )
}

/// Downloads the release's archive, checks it and unpacks it into `game`;
/// what was done, or a warning that a different loader was left there.
pub(crate) fn put_in(game: &Path) -> anyhow::Result<String> {
    let version = version();
    let url = |file: &str| format!("{DOWNLOADS}/v{version}/{file}");
    let sums = crate::net::get(&url(SUMS), &[], SUMS_LIMIT, SECONDS)
        .with_context(|| format!("the checksums of Enshwick {version}"))?;
    let expected = checksum(&String::from_utf8_lossy(&sums), ARCHIVE)
        .with_context(|| format!("{SUMS} of Enshwick {version} lists no {ARCHIVE}"))?;
    let archive = crate::net::get(&url(ARCHIVE), &[], ARCHIVE_LIMIT, SECONDS)
        .with_context(|| format!("{ARCHIVE} of Enshwick {version}"))?;
    if hex(&Sha256::digest(&archive)) != expected {
        bail!("{ARCHIVE} of Enshwick {version} does not match its checksum; nothing was put in");
    }
    unpack(&archive, game, version)
}

/// The SHA-256 that `sums`, in `sha256sum`'s format, gives `file`, in lower
/// case hex.
fn checksum(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (sum, name) = line.split_once(char::is_whitespace)?;
        let name = name.trim_start();
        let name = name.strip_prefix('*').unwrap_or(name);
        (name == file && sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _infallible = write!(text, "{byte:02x}");
        text
    })
}

/// Unpacks `archive` into `game`, writing only files that are missing; when
/// a file there differs from the archive's, nothing is written.
fn unpack(archive: &[u8], game: &Path, version: &str) -> anyhow::Result<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).context(ARCHIVE)?;
    let mut entries: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for index in 0..zip.len() {
        let mut file = zip.by_index(index).context(ARCHIVE)?;
        if file.is_dir() {
            continue;
        }
        let name = file.enclosed_name().with_context(|| {
            format!(
                "{ARCHIVE} names {}, outside the game folder",
                String::from_utf8_lossy(file.name_raw())
            )
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).context(ARCHIVE)?;
        entries.push((name, bytes));
    }
    let differ: Vec<&PathBuf> = entries
        .iter()
        .filter(|(name, bytes)| std::fs::read(game.join(name)).is_ok_and(|have| have != *bytes))
        .map(|(name, _)| name)
        .collect();
    if !differ.is_empty() {
        return Ok(format!(
            "warning: {} in {} differ from those of Enshwick {version}, another loader; left as they are: remove them to put {version} in",
            listed(differ),
            game.display()
        ));
    }
    let missing: Vec<&(PathBuf, Vec<u8>)> = entries
        .iter()
        .filter(|(name, _)| !game.join(name).exists())
        .collect();
    if missing.is_empty() {
        return Ok(format!(
            "the Enshwick loader {version} is in {} already",
            game.display()
        ));
    }
    for (name, bytes) in &missing {
        let to = game.join(name);
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).with_context(|| dir.display().to_string())?;
        }
        std::fs::write(&to, bytes).with_context(|| to.display().to_string())?;
    }
    Ok(format!(
        "put in the Enshwick loader {version}: {}",
        listed(missing.iter().map(|(name, _)| name))
    ))
}

/// `names` as one line, comma-separated.
fn listed<'a>(names: impl IntoIterator<Item = &'a PathBuf>) -> String {
    names
        .into_iter()
        .map(|name| name.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    /// A fresh folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-loader-{name}-{}", std::process::id()));
        let _old = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn release() -> Vec<u8> {
        archive(&[
            (FILES[0], b"stub"),
            (FILES[1], b"loader"),
            ("LICENSE", b"MIT"),
        ])
    }

    #[test]
    fn the_loader_goes_into_a_folder_without_one() {
        let game = scratch("fresh");
        assert!(!is_installed(&game));
        let done = unpack(&release(), &game, "1.0.0-alpha.14").unwrap();
        assert!(
            done.starts_with("put in the Enshwick loader 1.0.0-alpha.14:"),
            "{done}"
        );
        assert!(is_installed(&game));
        assert_eq!(std::fs::read(game.join(FILES[1])).unwrap(), b"loader");
        let again = unpack(&release(), &game, "1.0.0-alpha.14").unwrap();
        assert!(again.contains("is in"), "{again}");
    }

    #[test]
    fn another_loader_is_named_and_left_as_it_is() {
        let game = scratch("other");
        std::fs::create_dir_all(game.join("mods/enshwick")).unwrap();
        std::fs::write(game.join(FILES[1]), b"older loader").unwrap();
        let warned = unpack(&release(), &game, "1.0.0-alpha.14").unwrap();
        assert!(warned.starts_with("warning:"), "{warned}");
        assert!(warned.contains("enshwick.dll"), "{warned}");
        assert_eq!(std::fs::read(game.join(FILES[1])).unwrap(), b"older loader");
        assert!(!game.join(FILES[0]).exists(), "nothing is written");
    }

    #[test]
    fn an_archive_reaching_outside_the_game_folder_is_refused() {
        let game = scratch("outside");
        let evil = archive(&[("../evil.dll", b"x")]);
        let error = unpack(&evil, &game, "1.0.0-alpha.14").unwrap_err();
        assert!(
            error.to_string().contains("outside the game folder"),
            "{error}"
        );
        assert!(!game.parent().unwrap().join("evil.dll").exists());
    }

    #[test]
    fn the_checksum_list_names_the_archive_s_sum() {
        let sum = "AB".repeat(32);
        let sums = format!(
            "{}  cargo-enshwick.exe\n{sum} *enshwick.zip\n",
            "0".repeat(64)
        );
        assert_eq!(checksum(&sums, ARCHIVE), Some(sum.to_ascii_lowercase()));
        assert_eq!(checksum("abc  enshwick.zip", ARCHIVE), None);
        assert_eq!(checksum(&sums, "other.zip"), None);
        assert_eq!(
            hex(&Sha256::digest(b"")),
            format!(
                "e3b0c442{}",
                "98fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            )
        );
    }
}
