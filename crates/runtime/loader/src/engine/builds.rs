//! The engine functions and constants that reflection does not describe,
//! found on the running build by symbolic id as SKSE's Address Library pins
//! them.
//!
//! Every build is found the same way, with no table made for it: the
//! anchors of `enshwick-gamedata` carried by their code patterns, then the
//! engine routes. What neither finds stays missing: the features and mods
//! that need it turn off, the rest runs.

use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_core::abi::Side;
use enshwick_gamedata::reflect::Fingerprint;
use enshwick_gamedata::{self as tables, Part, PartState, Resolved, Routing, SteamBuild};

/// Whether a build no compatibility report covers runs; set from the
/// settings.
static PATTERNS: AtomicBool = AtomicBool::new(true);

/// Sets whether a build no compatibility report covers runs on its anchors
/// and routes.
pub fn configure(patterns: bool) {
    PATTERNS.store(patterns, Ordering::Release);
}

/// What the running build has of the entries the loader knows.
#[derive(Debug)]
pub struct KnownBuild {
    /// For the log, such as `server, Steam build 26430253, game 1287369,
    /// executable <guid>/<age>`.
    pub name: String,
    table: Resolved,
    /// The entries no engine route found that have no anchor either.
    unanchored: Vec<String>,
    /// The entries whose engine route and anchor disagree.
    conflicts: Vec<String>,
    /// The constants whose engine route took a value outside its range.
    outside: Vec<String>,
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
    pub fn head(&self, id: &str) -> Option<(&enshwick_core::code::Pattern, u64)> {
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

    /// `reason` for entry `id` as this build tells it: an [`ABSENT`] entry
    /// is one whose engine route and anchor disagree, an attribute slot the
    /// game's data did not give, one its anchor's pattern did not find, or
    /// one with no anchor that no engine route found.
    #[must_use]
    pub fn reason<'a>(&self, id: &str, reason: &'a str) -> &'a str {
        let has = |ids: &[String]| ids.iter().any(|m| m == id);
        if reason != ABSENT {
            reason
        } else if has(&self.conflicts) {
            "is found elsewhere by its engine route than by its anchor"
        } else if has(&self.outside) {
            "is outside its range"
        } else if self.attributes().iter().any(|(slot, _)| slot == id) {
            "is not read from the game's data"
        } else if has(self.missing()) && !has(&self.unanchored) {
            "is not found by its anchor's pattern"
        } else {
            "is found by no engine route or anchor"
        }
    }

    /// Counts every entry a route of `running`'s side finds as one no route
    /// found, for a run on the anchors alone, so what only a route finds is
    /// missing.
    pub fn unrouted(&mut self, routes: &tables::Routes, running: Side) {
        let Some(side) = side(running) else {
            return;
        };
        for id in routes.ids().filter(|id| routes.routes(id, side)) {
            self.unfound(id);
        }
    }

    /// Notes that no engine route found entry `id`: missing unless an
    /// anchor found it; whether it is missing.
    fn unfound(&mut self, id: &str) -> bool {
        let lacks = self.table.lack(id);
        if lacks {
            self.unanchored.push(id.to_owned());
        }
        lacks
    }

    /// Notes that entry `id`'s engine route and anchor disagree, which
    /// took it off.
    fn conflict(&mut self, id: &str) {
        self.conflicts.push(id.to_owned());
    }

    /// Whether the build has every entry the loader knows: nothing missing.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.missing().is_empty()
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

    /// Takes into the build what `routes` find in `image` for the running
    /// `side`: each entry a route finds is the engine's, and one an anchor
    /// finds elsewhere goes off with a warning; an entry no route finds
    /// keeps its anchor's answer, and one without an anchor is missing. The
    /// number of entries routed.
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
                Routing::Conflict { table, engine } => {
                    self.conflict(id);
                    crate::log!(
                        WARN,
                        "{id} off on {}: its engine route finds {engine:#x}, its anchor {table:#x} ({})",
                        self.name,
                        crate::engine::errors::TABLE_MISMATCH
                    );
                }
                _ => {
                    let lacks = self.unfound(id);
                    if let Some(enshwick_gamedata::Routed::Outside(value)) = found
                        && lacks
                    {
                        self.outside.push(id.to_owned());
                        crate::log!(
                            WARN,
                            "{id} is outside its range on {}: its engine route takes {value:#x} ({})",
                            self.name,
                            crate::engine::errors::FEATURES_OFF
                        );
                    }
                    unfound.push(id);
                }
            }
        }
        if !unfound.is_empty() {
            crate::log!(
                DEBUG,
                "not found by their engine route: {}",
                unfound.join(", ")
            );
        }
        count
    }

    /// The start's line about this build, once the engine routes ran: at
    /// info when nothing is missing, else a warning with the count.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_core::abi::{Level, Side};
    ///
    /// use enshwick_loader::engine::builds::by_patterns;
    ///
    /// let code = [0u8; 64];
    /// let build = by_patterns(Side::SERVER, "server".into(), (&code, 0x1000), |_| None)
    ///     .ok_or("no server anchors")?;
    /// let (level, line) = build.start_line();
    /// assert_eq!(level, Level::WARN, "{line}");
    /// # Ok::<(), &str>(())
    /// ```
    #[must_use]
    pub fn start_line(&self) -> (enshwick_core::abi::Level, String) {
        use enshwick_core::abi::Level;
        if self.complete() {
            return (
                Level::INFO,
                format!(
                    "game: {}, {} entries found by engine routes and anchors",
                    self.name,
                    self.entries()
                ),
            );
        }
        (
            Level::WARN,
            format!(
                "game: {}, {} entries found by engine routes and anchors, {} missing; features and mods that need a missing one stay off until a loader release finds it ({})",
                self.name,
                self.entries(),
                self.missing().len(),
                crate::engine::errors::FEATURES_OFF
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

/// Every kept build's table, for tests that compare with what a build pins;
/// none without the game data folder.
///
/// # Panics
///
/// When a kept table does not read.
#[cfg(test)]
#[must_use]
pub fn load() -> Vec<tables::Table> {
    tables::kept()
        .unwrap()
        .iter()
        .map(|(_, text)| tables::Table::parse(text).unwrap())
        .collect()
}

/// The running build as the loader finds it: unless the settings turned
/// off builds no compatibility report covers, [`by_patterns`]; the engine
/// routes come after ([`KnownBuild::route`]).
#[must_use]
pub fn select(
    running: Side,
    fingerprint: Fingerprint,
    name: String,
    code: Option<(&[u8], u64)>,
    cell: impl Fn(u64) -> Option<u32> + Sync,
) -> Option<KnownBuild> {
    let reported = tables::reported(side(running)?, fingerprint);
    if !reported && !PATTERNS.load(Ordering::Acquire) {
        return None;
    }
    by_patterns(running, name, code?, cell)
}

/// The anchors of `running`'s side carried to the build by their patterns.
///
/// `code` is the code section and its RVA, and `cell` reads the `u32` the
/// game's image holds at an RVA for the constants the game keeps in data
/// cells; `None` when the anchors do not read, which is logged.
#[must_use]
pub fn by_patterns(
    running: Side,
    name: String,
    code: (&[u8], u64),
    cell: impl Fn(u64) -> Option<u32> + Sync,
) -> Option<KnownBuild> {
    let anchors = match tables::anchors(side(running)?) {
        Ok(anchors) => anchors,
        Err(e) => {
            crate::log!(ERROR, "embedded anchors: {e}");
            return None;
        }
    };
    Some(KnownBuild {
        name,
        table: anchors.fall_back(code.0, code.1, cell),
        unanchored: Vec::new(),
        conflicts: Vec::new(),
        outside: Vec::new(),
        lacking: OnceLock::new(),
    })
}

/// A build as its fingerprint names it, `<guid>/<age>`.
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

/// The Steam app ids of the client and the dedicated server.
const APPS: [(Side, u32); 2] = [(Side::CLIENT, 1_203_620), (Side::SERVER, 2_278_520)];

/// The Steam build of the game in `game_dir` on `running`'s side, from the
/// appmanifest Steam keeps beside its library (`steamapps/`, two folders
/// up) or `SteamCMD` inside the install (`steamapps/` in it).
#[must_use]
pub fn steam_build(game_dir: &Path, running: Side) -> Option<SteamBuild> {
    let (_, app) = APPS.iter().find(|(side, _)| *side == running)?;
    let file = format!("appmanifest_{app}.acf");
    let text = std::fs::read_to_string(game_dir.join("steamapps").join(&file))
        .ok()
        .or_else(|| {
            let library = game_dir.parent()?.parent()?.join(&file);
            std::fs::read_to_string(library).ok()
        })?;
    manifest_build(&text)
}

/// The `buildid` of an appmanifest's text.
fn manifest_build(text: &str) -> Option<SteamBuild> {
    text.lines().find_map(|line| {
        let mut quoted = line.split('"').skip(1).step_by(2);
        (quoted.next()? == "buildid")
            .then(|| quoted.next()?.parse().ok().map(SteamBuild::new))
            .flatten()
    })
}

/// The running build's name for the log and the status report: its side,
/// then the Steam build, the game version its archive states and the
/// executable's fingerprint, each where it reads.
#[must_use]
pub fn build_name(
    running: Side,
    steam: Option<SteamBuild>,
    version: Option<&str>,
    fingerprint: Fingerprint,
) -> String {
    let side = side(running).map_or("game", tables::Side::as_str);
    let steam = steam.map(|b| format!("Steam build {b}, "));
    // The archive's version string goes on after `|` with its branch and date.
    let version = version
        .and_then(|v| v.split('|').next())
        .map(|v| format!("game {v}, "));
    format!(
        "{side}, {}{}executable {}",
        steam.unwrap_or_default(),
        version.unwrap_or_default(),
        fingerprint_name(fingerprint)
    )
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
            Self::Moved => "does not read as its anchor says",
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

/// What the newest kept build of `side` pins, for tests; `None` without
/// the kept tables, and the test skips.
///
/// # Panics
///
/// When `running` is no side.
#[cfg(test)]
#[must_use]
pub fn kept(running: Side) -> Option<KnownBuild> {
    let tables = load();
    let side = side(running).unwrap();
    let table = tables::newest(&tables, side)?;
    Some(KnownBuild {
        name: table.name().to_owned(),
        table: table.resolve(),
        unanchored: Vec::new(),
        conflicts: Vec::new(),
        outside: Vec::new(),
        lacking: OnceLock::new(),
    })
}

/// A build that knows no entry, for tests that run the loader without a
/// game or a kept table.
///
/// # Panics
///
/// When `tables::Table::parse` stops taking its fixed text.
#[cfg(test)]
#[must_use]
pub fn empty() -> KnownBuild {
    let table = tables::Table::parse(
        "name = \"empty\"\nside = \"server\"\nguid = \"00000000000000000000000000000000\"\nage = 0",
    )
    .unwrap();
    KnownBuild {
        name: table.name().to_owned(),
        table: table.resolve(),
        unanchored: Vec::new(),
        conflicts: Vec::new(),
        outside: Vec::new(),
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

/// The executable variables this test run names whose build has a kept
/// table; the tests that compare with it skip the others.
#[cfg(test)]
pub(crate) fn named_executables() -> Vec<&'static str> {
    let tables = load();
    [
        ("ENSH_SERVER_EXE", tables::Side::Server),
        ("ENSH_CLIENT_EXE", tables::Side::Client),
    ]
    .into_iter()
    .filter(|&(var, side)| {
        std::env::var_os(var).is_some_and(|path| {
            let bytes = std::fs::read(path).unwrap();
            let image = enshwick_gamedata::reflect::Image::parse(
                &bytes,
                enshwick_gamedata::reflect::Layout::File,
                None,
            )
            .unwrap();
            tables::exact(&tables, side, image.fingerprint().unwrap()).is_some()
        })
    })
    .map(|(var, _)| var)
    .collect()
}

#[cfg(test)]
fn constants_on_executables_by(
    ids: &[&str],
    keep: impl Fn(&tables::Table) -> bool,
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
        let Some(own) = tables::exact(&tables, side, image.fingerprint().unwrap()) else {
            continue;
        };
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
