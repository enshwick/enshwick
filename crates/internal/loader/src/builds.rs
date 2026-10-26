//! The running build's table of engine functions and constants that
//! reflection does not describe, pinned by symbolic id as SKSE's Address
//! Library does.
//!
//! The tables come as data (`enshwick-gamedata`), embedded in the
//! loader. A build with its own table
//! uses it whole, after its systems are checked against reflection. On a
//! build without one, the newest table of the same side finds each entry by
//! its code pattern, and what it cannot find stays missing: the features and
//! mods that need it turn off, the rest runs.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::Side;
use enshwick_gamedata::reflect::Fingerprint;
use enshwick_gamedata::{self as tables, Part, PartState, Resolved, Table};

pub use enshwick_gamedata::Kind;

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
    /// The names of the parts [`Self::lacking`] found, once asked.
    lacking: OnceLock<String>,
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

    /// The pattern of symbol `id` and how many bytes before the symbol it
    /// starts; see [`Resolved::head`].
    #[must_use]
    pub fn head(&self, id: &str) -> Option<(&enshwick::code::Pattern, u64)> {
        self.table.head(id)
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

    /// The constants that are slots of attributes, with the name of their
    /// attribute.
    #[must_use]
    pub fn attributes(&self) -> &[(String, String)] {
        self.table.attributes()
    }

    /// Sets the constant `id` to `value`, read from the game's data, and
    /// takes it off the missing entries; the value it had, if any.
    pub fn supply(&mut self, id: &str, value: u64) -> Option<u64> {
        self.table.supply(id, value)
    }

    /// Each of `parts` with the entries it names that this build lacks.
    #[must_use]
    pub fn parts(&self, parts: &[(&'static str, &[&'static str])]) -> Vec<Part<'static>> {
        self.table.parts(parts)
    }

    /// The names of `parts` this build serves only partly or not at all,
    /// comma separated; worked out on the first call, which names the parts
    /// for every later one.
    pub fn lacking(&self, parts: &[(&'static str, &[&'static str])]) -> &str {
        self.lacking.get_or_init(|| {
            self.parts(parts)
                .iter()
                .filter(|part| part.state() != PartState::Works)
                .map(|part| part.name)
                .collect::<Vec<_>>()
                .join(", ")
        })
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

/// Every table the loader embeds; one that does not read is logged and
/// skipped.
#[must_use]
pub fn load() -> Vec<Table> {
    tables::embedded()
        .iter()
        .filter_map(|(file, text)| match Table::parse(text) {
            Ok(table) => Some(table),
            Err(e) => {
                crate::log!(ERROR, "embedded build table {file}: {e}");
                None
            }
        })
        .collect()
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
            lacking: OnceLock::new(),
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
        lacking: OnceLock::new(),
    })
}

/// The newest table of `running`'s side carried over by patterns in `code`
/// even when the build has its own, to see what the patterns alone carry.
#[must_use]
pub fn by_patterns(tables: &[Table], running: Side, code: (&[u8], u64)) -> Option<KnownBuild> {
    let base = tables::newest(tables, side(running)?)?;
    let table = base.fall_back(code.0, code.1);
    Some(KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Fallback,
        table,
        lacking: OnceLock::new(),
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
    let tables = load();
    let side = side(running).unwrap();
    let table = tables::newest(&tables, side).unwrap();
    KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Exact,
        table: table.resolve(),
        lacking: OnceLock::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_build_names_the_loader_features_it_lacks_once() {
        assert_eq!(embedded(Side::SERVER).lacking(crate::TABLE_IDS), "");
        let tables = load();
        let unknown = Fingerprint::new([0xee; 16], 1);
        let code = [0u8; 64];
        let carried = select(&tables, Side::SERVER, unknown, Some((&code, 0x1000))).unwrap();
        let lacking = carried.lacking(crate::TABLE_IDS);
        assert!(
            lacking.split(", ").any(|name| name == "altars"),
            "{lacking}"
        );
        assert_eq!(
            carried.lacking(&[]),
            lacking,
            "the first call names the parts"
        );
    }

    #[test]
    fn a_known_build_resolves_its_symbols() {
        let tables = load();
        let server = embedded(Side::SERVER);
        let fingerprint = tables
            .iter()
            .find(|t| t.name() == server.name)
            .unwrap()
            .fingerprint();
        let found = select(&tables, Side::SERVER, fingerprint, None).unwrap();
        assert_eq!(found.kind, Kind::Exact);
        assert_eq!(found.symbol("npc.lookup"), Some(0x20_3d30));
        assert_eq!(found.symbol("npc.nope"), None);
        assert!(!found.checks().is_empty());
        assert!(select(&tables, Side::CLIENT, fingerprint, None).is_none());
    }

    #[test]
    fn an_unknown_build_falls_back_only_when_allowed_and_given_code() {
        let tables = load();
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

    /// Every symbol with a `.bytes` constant reads those bytes in the client
    /// `ENSH_CLIENT_EXE` names; skipped without it.
    #[test]
    fn checked_symbols_read_their_bytes_in_the_client() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let tables = load();
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
        let reflection = enshwick_gamedata::reflect::Reflection::scan(&image);
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

    /// On the server `ENSH_SERVER_EXE` names (skipped without it), finding
    /// every entry by its pattern alone takes a small part of the 10 s the
    /// stub holds the server's code for the loader.
    #[test]
    fn the_pattern_fallback_fits_the_server_hold() {
        let Ok(path) = std::env::var("ENSH_SERVER_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let text = image.section(".text").unwrap();
        let range = image.range(text);
        let code = image.read(range.start, range.end - range.start).unwrap();
        let tables = load();
        let started = std::time::Instant::now();
        let found = by_patterns(&tables, Side::SERVER, (code, range.start - image.base())).unwrap();
        let took = started.elapsed();
        assert_eq!(found.kind, Kind::Fallback);
        // Unoptimised code scans about ten times slower than the release build.
        let budget = if cfg!(debug_assertions) { 15 } else { 2 };
        assert!(took < std::time::Duration::from_secs(budget), "{took:?}");
    }
}
