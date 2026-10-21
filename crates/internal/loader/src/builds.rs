//! The running build's table of engine functions and constants that
//! reflection does not describe, pinned by symbolic id as SKSE's Address
//! Library does.
//!
//! The tables come as data (`enshwick-loader-builds`): the ones the loader
//! embeds, and the ones the updater fetched into `mods/enshwick/builds/`,
//! each used only when the project key signed it. A build with its own table
//! uses it whole, after its systems are checked against reflection. On a
//! build without one, the newest table of the same side finds each entry by
//! its code pattern, and what it cannot find stays missing: the features and
//! mods that need it turn off, the rest runs.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_loader_api::abi::Side;
use enshwick_loader_builds::{self as tables, Resolved, Table};
use enshwick_loader_reflect::Fingerprint;

pub use enshwick_loader_builds::Kind;

/// The folder under `mods/enshwick/` that holds the tables the updater
/// fetched, each beside its signature.
pub const FETCHED_DIR: &str = "builds";

/// Whether a build without a table runs on patterns; set from the settings.
static PATTERNS: AtomicBool = AtomicBool::new(true);

/// Sets whether a build without a table runs on its patterns.
pub fn configure(patterns: bool) {
    PATTERNS.store(patterns, Ordering::Release);
}

/// The table the running build uses.
#[derive(Debug)]
pub struct KnownBuild {
    /// For the log, such as `1.0 server (1269579)`.
    pub name: String,
    /// Whether the table was made for this build.
    pub kind: Kind,
    table: Resolved,
}

impl KnownBuild {
    /// The engine constant `name` on this build.
    #[must_use]
    pub fn constant(&self, name: &str) -> Option<u64> {
        self.table.constant(name)
    }

    /// The address of `name` on this build, relative to the image base.
    #[must_use]
    pub fn symbol(&self, name: &str) -> Option<u64> {
        self.table.symbol(name)
    }

    /// Systems whose reflected entry must equal the pinned address.
    #[must_use]
    pub fn checks(&self) -> &[(String, u64)] {
        self.table.checks()
    }

    /// The entries of the source table this build lacks.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        self.table.missing()
    }

    /// Every symbol, sorted by id.
    #[must_use]
    pub fn symbols(&self) -> &[(String, u64)] {
        self.table.symbols()
    }

    /// Every constant, sorted by id.
    #[must_use]
    pub fn constants(&self) -> &[(String, u64)] {
        self.table.constants()
    }

    /// Whether an entry whose id starts with `prefix` is missing, for
    /// entries that only work as a whole set.
    #[must_use]
    pub fn lacks_any(&self, prefix: &str) -> bool {
        self.table.missing().iter().any(|id| id.starts_with(prefix))
    }

    /// How many symbols and constants this build has.
    #[must_use]
    pub fn entries(&self) -> usize {
        self.table.symbols().len() + self.table.constants().len()
    }
}

fn side(side: Side) -> Option<tables::Side> {
    match side {
        Side::SERVER => Some(tables::Side::Server),
        Side::CLIENT => Some(tables::Side::Client),
        _ => None,
    }
}

/// Every table the loader may use: the embedded ones and those in `fetched`
/// the project key signed; a refused file is logged and skipped.
#[must_use]
pub fn load(fetched: &Path) -> Vec<Table> {
    let mut out: Vec<Table> = tables::embedded()
        .iter()
        .filter_map(|(file, text)| match Table::parse(text) {
            Ok(table) => Some(table),
            Err(e) => {
                crate::log!(ERROR, "embedded build table {file}: {e}");
                None
            }
        })
        .collect();
    let Ok(entries) = std::fs::read_dir(fetched) else {
        return out;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Some(file) = path.file_name().and_then(|n| n.to_str()).filter(|_| {
            path.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
        }) else {
            continue;
        };
        match fetched_table(&path, file) {
            Ok(table) => out.push(table),
            Err(reason) => crate::log!(
                WARN,
                "build table mods/{}/{FETCHED_DIR}/{file} not used: {reason}",
                crate::settings::LOADER_ID
            ),
        }
    }
    out
}

fn fetched_table(path: &Path, file: &str) -> Result<Table, String> {
    let text = std::fs::read(path).map_err(|e| e.to_string())?;
    let signature_path = path.with_file_name(format!("{file}{}", tables::SIGNATURE_SUFFIX));
    let signature = std::fs::read_to_string(&signature_path)
        .map_err(|_| "it has no signature beside it".to_owned())?;
    tables::verify(&text, &signature, file).map_err(|e| e.to_string())?;
    let text = core::str::from_utf8(&text).map_err(|_| "it is not UTF-8".to_owned())?;
    let table = Table::parse(text).map_err(|e| e.to_string())?;
    if table.file_name() != file {
        return Err(format!("it is the table of {}", table.file_name()));
    }
    Ok(table)
}

/// The table for the running build among `tables`: its own when there is
/// one; else, unless the settings turned it off, the newest table of its side
/// carried over by patterns in `code` (the code section and its RVA).
#[must_use]
pub fn select(
    tables: &[Table],
    running: Side,
    fingerprint: Fingerprint,
    code: Option<(&[u8], u64)>,
) -> Option<KnownBuild> {
    let side = side(running)?;
    if let Some(table) = tables::exact(tables, side, fingerprint) {
        return Some(KnownBuild {
            name: table.name().to_owned(),
            kind: Kind::Exact,
            table: table.resolve(),
        });
    }
    if !PATTERNS.load(Ordering::Acquire) {
        return None;
    }
    let (code, code_rva) = code?;
    let base = tables::newest(tables, side)?;
    let table = base.fall_back(code, code_rva);
    Some(KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Fallback,
        table,
    })
}

/// The embedded table of `side`'s newest build, for tests.
///
/// # Panics
///
/// When no table of `running`'s side is embedded.
#[cfg(test)]
#[must_use]
pub fn embedded(running: Side) -> KnownBuild {
    let tables = load(Path::new("no such folder"));
    let side = side(running).unwrap();
    let table = tables::newest(&tables, side).unwrap();
    KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Exact,
        table: table.resolve(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_build_resolves_its_symbols() {
        let tables = load(Path::new("no such folder"));
        let server = embedded(Side::SERVER);
        let fingerprint = tables
            .iter()
            .find(|t| t.name() == server.name)
            .unwrap()
            .fingerprint();
        let found = select(&tables, Side::SERVER, fingerprint, None).unwrap();
        assert_eq!(found.kind, Kind::Exact);
        assert_eq!(found.symbol("npc.lookup"), Some(0x20_2e80));
        assert_eq!(found.symbol("npc.nope"), None);
        assert!(!found.checks().is_empty());
        assert!(select(&tables, Side::CLIENT, fingerprint, None).is_none());
    }

    #[test]
    fn an_unknown_build_falls_back_only_when_allowed_and_given_code() {
        let tables = load(Path::new("no such folder"));
        let unknown = Fingerprint::new([0xee; 16], 1);
        let code = [0u8; 64];
        let found = select(&tables, Side::SERVER, unknown, Some((&code, 0x1000))).unwrap();
        assert_eq!(found.kind, Kind::Fallback);
        assert_eq!(found.symbol("npc.lookup"), None);
        assert!(found.checks().is_empty());
        assert!(!found.missing().is_empty());
        assert!(select(&tables, Side::SERVER, unknown, None).is_none());
        assert!(select(&tables, Side::BOTH, unknown, Some((&code, 0x1000))).is_none());
    }

    #[test]
    fn a_fetched_table_needs_the_project_key_and_its_own_name() {
        let dir = std::env::temp_dir().join(format!("enshwick-builds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (file, text) = tables::embedded()[0];
        std::fs::write(dir.join(file), text).unwrap();
        assert!(
            fetched_table(&dir.join(file), file)
                .unwrap_err()
                .contains("no signature")
        );
        std::fs::write(
            dir.join(format!("{file}.minisig")),
            include_str!("../../builds/testdata/table.minisig"),
        )
        .unwrap();
        assert!(fetched_table(&dir.join(file), file).is_err());
        let embedded = tables::embedded().len();
        assert_eq!(load(&dir).len(), embedded);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Every symbol with a `.bytes` constant reads those bytes in the client
    /// `ENSH_CLIENT_EXE` names; skipped without it.
    #[test]
    fn checked_symbols_read_their_bytes_in_the_client() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_loader_reflect::Image::parse(
            &bytes,
            enshwick_loader_reflect::Layout::File,
            None,
        )
        .unwrap();
        let tables = load(Path::new("no such folder"));
        let client =
            tables::exact(&tables, tables::Side::Client, image.fingerprint().unwrap()).unwrap();
        let mut checked = 0;
        for symbol in client.symbols() {
            let resolved = client.resolve();
            if let Some(expected) = resolved.constant(&format!("{}.bytes", symbol.id())) {
                assert_eq!(
                    image.u64(image.base() + symbol.rva()),
                    Some(expected),
                    "{}",
                    symbol.id()
                );
                checked += 1;
            }
        }
        assert!(checked >= 42);
        let reflection = enshwick_loader_reflect::Reflection::scan(&image);
        for (ty, field, offset) in [
            ("keen::FbUiBundle", "namedStyles", 0x10a0c),
            ("keen::FbUiBundle", "sounds", 0x12060),
            ("keen::FbUiBundle", "locatags", 0x10),
            ("keen::FbUiBundle", "fontSizes", 0x109e0),
            ("keen::FbUiCharacterStyles", "menu", 0x1130),
            ("keen::FbUiCharacterStyles", "greenText", 0x210),
            ("keen::FbUiMenuCharacterStyles", "header1", 0x58),
            ("keen::FbUiSounds", "menu", 0x1e0),
            ("keen::FbUiSoundsMenuGeneric", "entryTrigger", 0xf0),
            ("keen::FbUiLocaLinks", "menuStrings", 0x54b0),
            ("keen::FbUiLocaMenus", "settings", 0x1c10),
            ("keen::FbUiLocaMenuSettings", "settingOff", 0xb0),
            ("keen::FbUiFontSizes", "caps_large", 0x24),
        ] {
            assert_eq!(
                reflection.field_offset(ty, field),
                Ok(offset),
                "{ty}.{field}"
            );
        }
    }
}
