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

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::Side;
use enshwick_gamedata::reflect::Fingerprint;
use enshwick_gamedata::{self as tables, Part, PartState, Resolved, Table};

pub use enshwick_gamedata::Kind;

use crate::settings::LOADER_ID;

/// The name a carried table is saved under, before its side.
const CARRIED_FILE: &str = "carried";

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

    /// The whole code of symbol `id`'s function; see [`Resolved::body`].
    #[must_use]
    pub fn body(&self, id: &str) -> Option<&enshwick_gamedata::Body> {
        self.table.body(id)
    }

    /// The resolved table, for code that reads entries the same way on
    /// this build and in a test on a game executable.
    #[must_use]
    pub fn resolved(&self) -> &Resolved {
        &self.table
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

/// The table for the running build among `tables`.
///
/// Its own when there is one; else, unless the settings turned it off, the
/// newest table of its side carried over by patterns in `code` (the code
/// section and its RVA), with `cell` reading the `u32` the game's image
/// holds at an RVA for the constants the game keeps in data cells. A carried table is read from its file under
/// `data_dir` while the code and the table stay the same, and saved there
/// otherwise.
#[must_use]
pub fn select(
    tables: &[Table],
    running: Side,
    fingerprint: Fingerprint,
    code: Option<(&[u8], u64)>,
    cell: impl Fn(u64) -> Option<u32> + Sync,
    data_dir: Option<&Path>,
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
    let table = match data_dir.and_then(|dir| carried_file(dir, running)) {
        Some(path) => carry(base, code, code_rva, cell, &path),
        None => base.fall_back(code, code_rva, cell),
    };
    Some(KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Fallback,
        table,
        lacking: OnceLock::new(),
    })
}

/// The file under the game's `mod-data` a build of `running`'s side without
/// a table keeps its carried table in.
fn carried_file(data_dir: &Path, running: Side) -> Option<PathBuf> {
    let side = side(running)?;
    Some(
        data_dir
            .join(LOADER_ID)
            .join(format!("{CARRIED_FILE}-{}.txt", side.as_str())),
    )
}

/// `base` carried to `code`: read from `path` when it was saved for this
/// code and table, else found by patterns and saved there.
fn carry(
    base: &Table,
    code: &[u8],
    code_rva: u64,
    cell: impl Fn(u64) -> Option<u32> + Sync,
    path: &Path,
) -> Resolved {
    let key = base.carry_key(code, code_rva);
    if let Some(table) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| base.restore(&text, key))
    {
        return table;
    }
    let table = base.fall_back(code, code_rva, cell);
    if let Some(text) = table.to_saved(key)
        && let Err(e) = save(path, &text)
    {
        crate::log!(
            WARN,
            "the carried build table could not be saved to {}, so the next start searches again: {e}",
            path.display()
        );
    }
    table
}

fn save(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let staged = path.with_extension("tmp");
    std::fs::write(&staged, text)?;
    std::fs::rename(&staged, path)
}

/// The newest table of `running`'s side carried over by patterns in `code`
/// even when the build has its own, to see what the patterns alone carry;
/// `cell` reads as for [`select`].
#[must_use]
pub fn by_patterns(
    tables: &[Table],
    running: Side,
    code: (&[u8], u64),
    cell: impl Fn(u64) -> Option<u32> + Sync,
) -> Option<KnownBuild> {
    let base = tables::newest(tables, side(running)?)?;
    let table = base.fall_back(code.0, code.1, cell);
    Some(KnownBuild {
        name: table.name().to_owned(),
        kind: Kind::Fallback,
        table,
        lacking: OnceLock::new(),
    })
}

/// Whether the code a hook relies on holds, from the checks its table offers.
///
/// Each check is `None` when the table does not offer it: whether the
/// symbol's pattern matches where its code starts, whether its first eight
/// bytes are the table's `<id>.bytes`, and whether its whole function holds
/// the table's body. A symbol with none is refused, and every check offered
/// must hold.
#[must_use]
pub fn code_holds(checks: &[Option<bool>]) -> bool {
    checks.iter().any(Option::is_some) && !checks.contains(&Some(false))
}

/// Why a hook stays off before anything is patched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unhooked {
    /// The build has no address for the target.
    Absent,
    /// The code at the target is not what the table expects.
    Moved,
}

impl core::fmt::Display for Unhooked {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Absent => "is not in this build",
            Self::Moved => "does not read as the build table says",
        })
    }
}

/// The address a hook may patch: present, not zero, and with code that
/// `holds` confirms, asked only once there is an address to read.
///
/// # Errors
///
/// [`Unhooked`] says why the hook stays off.
pub fn hook_target(address: Option<u64>, holds: impl FnOnce() -> bool) -> Result<u64, Unhooked> {
    let address = address.filter(|&a| a != 0).ok_or(Unhooked::Absent)?;
    if holds() {
        Ok(address)
    } else {
        Err(Unhooked::Moved)
    }
}

/// The line that says why `feature` is off on the build named `build`: the
/// table entry `id` and the `reason` it failed.
#[must_use]
pub fn off_line(feature: &str, build: &str, id: &str, reason: &str) -> String {
    format!("{feature} off on {build}: {id} {reason}")
}

/// Whether the reflection scan read the game: a build whose scan finds no
/// type or no system has a reflection format the loader does not know, so
/// it runs vanilla as an unknown build.
#[must_use]
pub const fn reflection_read(types: usize, systems: usize) -> bool {
    types > 0 && systems > 0
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

/// What `ids` read on the executables `ENSH_SERVER_EXE` and
/// `ENSH_CLIENT_EXE` name, each skipped without its variable: by the
/// executable's own table, then by every table of its side carried to it by
/// patterns, each row named after its table.
#[cfg(test)]
pub(crate) fn constants_on_executables(ids: &[&str]) -> Vec<(String, Vec<Option<u64>>)> {
    let tables = load();
    let mut rows = Vec::new();
    for (var, side) in [
        ("ENSH_SERVER_EXE", tables::Side::Server),
        ("ENSH_CLIENT_EXE", tables::Side::Client),
    ] {
        let Ok(path) = std::env::var(var) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let own = tables::exact(&tables, side, image.fingerprint().unwrap())
            .unwrap()
            .resolve();
        let read = |resolved: &Resolved| ids.iter().map(|id| resolved.constant(id)).collect();
        rows.push((format!("{var}: own"), read(&own)));
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables.iter().filter(|t| t.side() == side) {
            let carried = other.fall_back(code, text.start - base, |rva| image.u32(base + rva));
            rows.push((format!("{var}: {}", other.name()), read(&carried)));
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On each client executable, every client table reads the size of a
    /// texture ring's completion record and where it holds its context.
    #[test]
    fn the_texture_ring_record_reads_on_each_client() {
        let ids = ["textures.ring_record_size", "textures.ring_record_context"];
        let rows = constants_on_executables(&ids);
        let clients: Vec<_> = rows
            .iter()
            .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
            .collect();
        if std::env::var("ENSH_CLIENT_EXE").is_ok() {
            assert!(clients.len() >= 2, "{rows:?}");
        }
        for (table, values) in clients {
            assert_eq!(values, &[Some(0x18), Some(0x8)], "{table}");
        }
    }

    /// On each client executable, every client table reads where the
    /// slider's config fields end, before its stop list.
    #[test]
    fn the_slider_extent_reads_on_each_client() {
        let rows = constants_on_executables(&["ui.slider_extent"]);
        let clients: Vec<_> = rows
            .iter()
            .filter(|(table, _)| table.starts_with("ENSH_CLIENT_EXE"))
            .collect();
        if std::env::var("ENSH_CLIENT_EXE").is_ok() {
            assert!(clients.len() >= 2, "{rows:?}");
        }
        for (table, values) in clients {
            assert_eq!(values, &[Some(0xa0)], "{table}");
        }
    }

    /// On each executable, every table reads where the zone size's answer
    /// holds its valid byte.
    #[test]
    fn the_zone_size_valid_byte_reads_on_each_executable() {
        let rows = constants_on_executables(&["balancing.zone_size_valid"]);
        let named = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
            .iter()
            .filter(|var| std::env::var(var).is_ok())
            .count();
        assert!(rows.len() >= named * 2, "{rows:?}");
        for (table, values) in rows {
            assert_eq!(values, [Some(0xc)], "{table}");
        }
    }

    /// On each executable, every table reads the words of the offer test's
    /// out record a refused offer zeroes.
    #[test]
    fn the_offer_out_record_reads_on_each_executable() {
        let ids = [
            "access.offer_out_u32",
            "access.offer_out_u64",
            "access.offer_out_tail",
        ];
        let rows = constants_on_executables(&ids);
        let named = ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
            .iter()
            .filter(|var| std::env::var(var).is_ok())
            .count();
        assert!(rows.len() >= named * 2, "{rows:?}");
        for (table, values) in rows {
            assert_eq!(values, [Some(0x4), Some(0x8), Some(0x10)], "{table}");
        }
    }

    #[test]
    fn code_with_no_check_to_offer_never_holds() {
        assert!(!code_holds(&[None, None, None]));
    }

    #[test]
    fn every_check_offered_must_hold() {
        assert!(code_holds(&[Some(true), Some(true), None]));
        assert!(!code_holds(&[Some(false), Some(true), None]));
        assert!(!code_holds(&[Some(true), Some(false), None]));
    }

    #[test]
    fn a_single_check_decides_alone() {
        assert!(code_holds(&[Some(true), None, None]));
        assert!(!code_holds(&[Some(false), None, None]));
        assert!(code_holds(&[None, Some(true), None]));
        assert!(!code_holds(&[None, Some(false), None]));
    }

    /// A function whose whole code changed fails its check even when its
    /// first bytes still match.
    #[test]
    fn a_changed_body_fails_the_code_whose_head_holds() {
        assert!(code_holds(&[Some(true), None, Some(true)]));
        assert!(!code_holds(&[Some(true), Some(true), Some(false)]));
        assert!(code_holds(&[None, None, Some(true)]));
    }

    #[test]
    fn a_hook_goes_only_where_the_code_is_found_and_holds() {
        assert_eq!(hook_target(Some(0x10), || true), Ok(0x10));
        assert_eq!(hook_target(None, || true), Err(Unhooked::Absent));
        assert_eq!(hook_target(Some(0), || true), Err(Unhooked::Absent));
        assert_eq!(hook_target(Some(0x10), || false), Err(Unhooked::Moved));
    }

    #[test]
    fn the_code_of_an_absent_hook_is_never_read() {
        assert_eq!(
            hook_target(None, || unreachable!("read")),
            Err(Unhooked::Absent)
        );
    }

    #[test]
    fn an_off_line_names_the_feature_the_build_the_entry_and_why() {
        assert_eq!(
            off_line(
                "fog of war",
                "1.0 client (26082781)",
                "map.fog_reveal",
                &Unhooked::Moved.to_string()
            ),
            "fog of war off on 1.0 client (26082781): map.fog_reveal does not read as the build table says"
        );
        assert_eq!(
            off_line(
                "chat lines",
                "b",
                "chat.post",
                &Unhooked::Absent.to_string()
            ),
            "chat lines off on b: chat.post is not in this build"
        );
    }

    #[test]
    fn a_scan_that_reflects_no_type_or_no_system_is_an_unknown_build() {
        assert!(reflection_read(4000, 300));
        assert!(!reflection_read(0, 300));
        assert!(!reflection_read(4000, 0));
        assert!(!reflection_read(0, 0));
    }

    /// Every symbol the loader hooks offers a check of its code in every
    /// table that pins it, so a table pinned wrong turns one feature off
    /// instead of patching another function.
    #[test]
    fn every_hooked_symbol_offers_a_check_of_its_code_in_every_table() {
        let mut unchecked = Vec::new();
        for table in load() {
            let resolved = table.resolve();
            for symbol in crate::pinned::HOOKED {
                let id = symbol.id();
                if resolved.symbol(id).is_none() {
                    continue;
                }
                let head = resolved.head(id).is_some();
                let bytes = resolved.constant(&format!("{id}.bytes")).is_some();
                let body = resolved.body(id).is_some();
                if !head && !bytes && !body {
                    unchecked.push(format!("{}: {id}", table.name()));
                }
            }
        }
        assert!(unchecked.is_empty(), "no check offered: {unchecked:#?}");
    }

    /// Every symbol whose code the loader replaces or calls as a whole
    /// carries a body over its whole function in every table that pins it,
    /// so a change to the game's logic there fails the code check and
    /// turns its feature off.
    #[test]
    fn every_replaced_function_carries_a_body_in_every_table() {
        let mut wrong = Vec::new();
        for table in load() {
            for symbol in crate::pinned::BODIES {
                let id = symbol.id();
                if table
                    .symbols()
                    .iter()
                    .any(|s| s.id() == id && s.body().is_none())
                {
                    wrong.push(format!("{}: {id}", table.name()));
                }
            }
        }
        assert!(wrong.is_empty(), "{wrong:#?}");
    }

    #[cfg(windows)]
    #[test]
    fn a_build_names_the_loader_features_it_lacks_once() {
        assert_eq!(embedded(Side::SERVER).lacking(crate::TABLE_IDS), "");
        let tables = load();
        let unknown = Fingerprint::new([0xee; 16], 1);
        let code = [0u8; 64];
        let carried = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            None,
        )
        .unwrap();
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
        let found = select(&tables, Side::SERVER, fingerprint, None, |_| None, None).unwrap();
        assert_eq!(found.kind, Kind::Exact);
        assert_eq!(found.symbol("npc.lookup"), Some(0x20_3d30));
        assert_eq!(found.symbol("npc.nope"), None);
        assert!(!found.checks().is_empty());
        assert!(select(&tables, Side::CLIENT, fingerprint, None, |_| None, None).is_none());
    }

    #[test]
    fn an_unknown_build_falls_back_only_when_allowed_and_given_code() {
        let tables = load();
        let unknown = Fingerprint::new([0xee; 16], 1);
        let code = [0u8; 64];
        let found = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            None,
        )
        .unwrap();
        assert_eq!(found.kind, Kind::Fallback);
        assert_eq!(found.symbol("npc.lookup"), None);
        assert!(found.checks().is_empty());
        assert!(!found.missing().is_empty());
        assert!(select(&tables, Side::SERVER, unknown, None, |_| None, None).is_none());
        assert!(
            select(
                &tables,
                Side::BOTH,
                unknown,
                Some((&code, 0x1000)),
                |_| None,
                None
            )
            .is_none()
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("enshwick-carried-{name}-{}", std::process::id()));
        let _fresh = std::fs::remove_dir_all(&dir);
        dir
    }

    fn patterned_code() -> Vec<u8> {
        let tables = load();
        let base = tables::newest(&tables, enshwick_gamedata::Side::Server).unwrap();
        let mut code = vec![0u8; 4096];
        let (pattern, _) = base.symbols().iter().find_map(|s| s.pattern()).unwrap();
        for i in 0..pattern.len() {
            code[16 + i] = if pattern.is_fixed(i) == Some(true) {
                0
            } else {
                0x5a
            };
        }
        code
    }

    #[test]
    fn a_carried_table_is_saved_and_read_back_while_the_code_stays() {
        let tables = load();
        let unknown = Fingerprint::new([0xee; 16], 1);
        let dir = scratch("same");
        let saved = carried_file(&dir, Side::SERVER).unwrap();
        let code = patterned_code();
        let first = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        let text = std::fs::read_to_string(&saved).unwrap();
        assert!(text.contains("missing npc.lookup\n"), "{text}");
        let again = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        assert_eq!(again.table, first.table);
        std::fs::write(
            &saved,
            text.replace("missing npc.lookup\n", "symbol npc.lookup 1234\n"),
        )
        .unwrap();
        let edited = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        assert_eq!(
            edited.symbol("npc.lookup"),
            Some(0x1234),
            "read from the save, not searched"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_save_for_other_code_or_a_damaged_one_is_replaced() {
        let tables = load();
        let unknown = Fingerprint::new([0xee; 16], 1);
        let dir = scratch("other");
        let saved = carried_file(&dir, Side::SERVER).unwrap();
        let code = patterned_code();
        let base = tables::newest(&tables, enshwick_gamedata::Side::Server).unwrap();
        std::fs::create_dir_all(saved.parent().unwrap()).unwrap();
        std::fs::write(&saved, "not a carried table").unwrap();
        let found = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&code, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        assert_eq!(found.table, base.fall_back(&code, 0x1000, |_| None));
        let key = base.carry_key(&code, 0x1000);
        assert!(
            std::fs::read_to_string(&saved)
                .unwrap()
                .contains(&key.to_string())
        );
        let other = vec![0u8; 4096];
        let found = select(
            &tables,
            Side::SERVER,
            unknown,
            Some((&other, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        assert_eq!(found.table, base.fall_back(&other, 0x1000, |_| None));
        let key = base.carry_key(&other, 0x1000);
        assert!(
            std::fs::read_to_string(&saved)
                .unwrap()
                .contains(&key.to_string())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_build_with_its_own_table_saves_nothing() {
        let tables = load();
        let server = embedded(Side::SERVER);
        let fingerprint = tables
            .iter()
            .find(|t| t.name() == server.name)
            .unwrap()
            .fingerprint();
        let dir = scratch("exact");
        let saved = carried_file(&dir, Side::SERVER).unwrap();
        let code = patterned_code();
        let found = select(
            &tables,
            Side::SERVER,
            fingerprint,
            Some((&code, 0x1000)),
            |_| None,
            Some(&dir),
        )
        .unwrap();
        assert_eq!(found.kind, Kind::Exact);
        assert!(!saved.exists());
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
        let found = by_patterns(
            &tables,
            Side::SERVER,
            (code, range.start - image.base()),
            |rva| image.u32(image.base() + rva),
        )
        .unwrap();
        let took = started.elapsed();
        assert_eq!(found.kind, Kind::Fallback);
        // Unoptimised code scans about ten times slower than the release build.
        let budget = if cfg!(debug_assertions) { 15 } else { 2 };
        assert!(took < std::time::Duration::from_secs(budget), "{took:?}");
    }
}
