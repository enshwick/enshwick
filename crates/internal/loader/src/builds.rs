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
use enshwick_gamedata::{self as tables, Part, PartState, Resolved, Routing, Table};

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
    /// The name of the table a build without its own carries.
    carried: Option<String>,
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

    /// `reason` for entry `id` as this build tells it: on a build without a
    /// table, an [`ABSENT`] entry is one the carried table lacks or one its
    /// pattern did not find.
    #[must_use]
    pub fn reason<'a>(&self, id: &str, reason: &'a str) -> &'a str {
        if self.kind != Kind::Fallback || reason != ABSENT {
            reason
        } else if self.missing().iter().any(|m| m == id) {
            "is not found by the carried table's patterns"
        } else {
            "is not in the carried table"
        }
    }

    /// How many symbols and constants this build has.
    #[must_use]
    pub fn entries(&self) -> usize {
        self.table.symbols().len() + self.table.constants().len()
    }

    /// Whether an engine route found entry `id`, so the route's own match
    /// checked its code.
    #[must_use]
    pub fn routed(&self, id: &str) -> bool {
        self.table.routed(id)
    }

    /// Takes into the table what `routes` find in `image` for the running
    /// `side`: each entry a route finds is the engine's, and one the table
    /// pins elsewhere goes off with a warning; the entries no route finds
    /// keep their table row. The number of entries routed.
    pub fn route(
        &mut self,
        routes: &tables::Routes,
        image: &enshwick_gamedata::reflect::Image<'_>,
        reflection: &enshwick_gamedata::reflect::Reflection<'_>,
        running: Side,
    ) -> usize {
        let Some(side) = side(running) else {
            return 0;
        };
        let mut count = 0;
        let mut unfound = Vec::new();
        for (id, found) in routes.find(image, reflection, side, &self.table) {
            match self.table.route(id, found) {
                Routing::Agrees | Routing::Supplied => count += 1,
                Routing::Conflict { table, engine } => crate::log!(
                    WARN,
                    "{id} off on {}: its engine route finds {engine:#x}, the build table {table:#x} ({})",
                    self.name,
                    crate::errors::TABLE_MISMATCH
                ),
                _ => unfound.push(id),
            }
        }
        if !unfound.is_empty() {
            crate::log!(
                DEBUG,
                "not found by their engine route, so taken from the build table: {}",
                unfound.join(", ")
            );
        }
        count
    }

    /// The start's line about this build, run as `fingerprint`, once the
    /// engine routes ran: a build without its own table that misses nothing
    /// and keeps no row from carrying logs at info, and only one that lacks
    /// entries warns.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{Level, Side};
    /// use enshwick_gamedata::Fingerprint;
    ///
    /// use enshwick_loader::builds::{load, select};
    ///
    /// let unknown = Fingerprint::new([0xee; 16], 1);
    /// let code = [0u8; 64];
    /// let build = select(&load(), Side::SERVER, unknown, Some((&code, 0x1000)), |_| None)
    ///     .ok_or("no server table")?;
    /// let (level, line) = build.start_line(unknown);
    /// assert_eq!(level, Level::WARN, "{line}");
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn start_line(&self, fingerprint: Fingerprint) -> (enshwick::abi::Level, String) {
        use enshwick::abi::Level;
        if self.kind == Kind::Exact {
            return (Level::INFO, format!("game: {}", self.name));
        }
        if self.missing().is_empty() && self.table.uncarried().is_empty() {
            return (
                Level::INFO,
                format!(
                    "game build {} runs without its own table: {} entries found by engine routes and the patterns of {}",
                    fingerprint_name(fingerprint),
                    self.entries(),
                    self.carried.as_deref().unwrap_or(self.table.name())
                ),
            );
        }
        (
            Level::WARN,
            format!(
                "game build {} has no build table yet: {} entries found by engine routes or their code patterns, {} missing; features and mods that need a missing one stay off until a loader build carries its table ({})",
                fingerprint_name(fingerprint),
                self.entries(),
                self.missing().len(),
                crate::errors::FEATURES_OFF
            ),
        )
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
/// holds at an RVA for the constants the game keeps in data cells.
#[must_use]
pub fn select(
    tables: &[Table],
    running: Side,
    fingerprint: Fingerprint,
    code: Option<(&[u8], u64)>,
    cell: impl Fn(u64) -> Option<u32> + Sync,
) -> Option<KnownBuild> {
    let side = side(running)?;
    if let Some(table) = tables::exact(tables, side, fingerprint) {
        return Some(KnownBuild {
            name: table.name().to_owned(),
            kind: Kind::Exact,
            carried: None,
            table: table.resolve(),
            lacking: OnceLock::new(),
        });
    }
    if !PATTERNS.load(Ordering::Acquire) {
        return None;
    }
    let (code, code_rva) = code?;
    let base = tables::newest(tables, side)?;
    let table = base.fall_back(code, code_rva, cell);
    Some(KnownBuild {
        name: format!(
            "build {}, carried table {}",
            fingerprint_name(fingerprint),
            base.name()
        ),
        kind: Kind::Fallback,
        carried: Some(base.name().to_owned()),
        table,
        lacking: OnceLock::new(),
    })
}

/// A build as its fingerprint names it, `<guid>/<age>`, for a build no
/// table names.
#[must_use]
pub fn fingerprint_name(fingerprint: Fingerprint) -> String {
    use core::fmt::Write as _;
    let mut name = fingerprint
        .guid()
        .iter()
        .fold(String::with_capacity(40), |mut out, b| {
            let _written = write!(out, "{b:02x}");
            out
        });
    let _written = write!(name, "/{}", fingerprint.age());
    name
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
        carried: Some(base.name().to_owned()),
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

/// Why a table entry fails when the build's table has no row for it; on a
/// build without a table [`KnownBuild::reason`] tells it apart.
pub const ABSENT: &str = "is not in this build";

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
            Self::Absent => ABSENT,
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
        carried: None,
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
    constants_on_executables_by(ids, |_| true)
}

/// What `ids` read on the executables, as [`constants_on_executables`], by
/// only the tables that pin every one of them.
#[cfg(test)]
pub(crate) fn pinned_constants_on_executables(ids: &[&str]) -> Vec<(String, Vec<Option<u64>>)> {
    constants_on_executables_by(ids, |table| {
        let resolved = table.resolve();
        ids.iter().all(|id| resolved.constant(id).is_some())
    })
}

/// The executables named in `named` that no row of `rows` was read on, so a
/// test over rows only some tables pin still fails where nothing reads them.
#[cfg(test)]
pub(crate) fn executables_without_rows<'a>(
    rows: &[(String, Vec<Option<u64>>)],
    named: &[&'a str],
) -> Vec<&'a str> {
    named
        .iter()
        .copied()
        .filter(|var| !rows.iter().any(|(table, _)| table.starts_with(var)))
        .collect()
}

/// The executable variables this test run names.
#[cfg(test)]
pub(crate) fn named_executables() -> Vec<&'static str> {
    ["ENSH_SERVER_EXE", "ENSH_CLIENT_EXE"]
        .into_iter()
        .filter(|var| std::env::var(var).is_ok())
        .collect()
}

#[cfg(test)]
fn constants_on_executables_by(
    ids: &[&str],
    keep: impl Fn(&Table) -> bool,
) -> Vec<(String, Vec<Option<u64>>)> {
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
        let own = tables::exact(&tables, side, image.fingerprint().unwrap()).unwrap();
        let read = |resolved: &Resolved| ids.iter().map(|id| resolved.constant(id)).collect();
        if keep(own) {
            rows.push((format!("{var}: own"), read(&own.resolve())));
        }
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables.iter().filter(|t| t.side() == side && keep(t)) {
            let carried = other.fall_back(code, text.start - base, |rva| image.u32(base + rva));
            rows.push((format!("{var}: {}", other.name()), read(&carried)));
        }
    }
    rows
}

#[cfg(test)]
mod tests;
