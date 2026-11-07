//! A table resolved for the running build, and choosing that table.

use enshwick::code::Pattern;

use crate::reflect::Fingerprint;
use crate::{Body, Routed, Side, Table, TableError};

/// How a [`Resolved`] table came about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The table was made for the running build.
    Exact,
    /// Another build's table, its entries found by their patterns.
    Fallback,
}

/// A table as the running build uses it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub(crate) name: String,
    pub(crate) kind: Kind,
    pub(crate) checks: Vec<(String, u64)>,
    pub(crate) symbols: Vec<(String, u64)>,
    pub(crate) constants: Vec<(String, u64)>,
    pub(crate) missing: Vec<String>,
    pub(crate) uncarried: Vec<String>,
    pub(crate) attributes: Vec<(String, String)>,
    pub(crate) heads: Vec<(String, Pattern, u64)>,
    pub(crate) bodies: Vec<(String, Body)>,
    pub(crate) routed: Vec<String>,
}

/// What an engine route did to an entry of a [`Resolved`] table.
///
/// # Examples
///
/// See [`Resolved::route`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Routing {
    /// The route found what the table holds.
    Agrees,
    /// The route found an entry the table lacks or its patterns missed.
    Supplied,
    /// The route and the table disagree, so the entry is off.
    Conflict {
        /// What the table holds.
        table: u64,
        /// What the route found.
        engine: u64,
    },
    /// The route found nothing, so the table's entry, if any, stands.
    Unresolved,
}

impl Resolved {
    /// The whole code of symbol `id`'s function, for a symbol whose code
    /// the loader replaces or relies on as a whole: a hook checks the code
    /// there before it patches, and the code it searches stays inside it.
    #[must_use]
    pub fn body(&self, id: &str) -> Option<&Body> {
        self.bodies
            .iter()
            .find(|(body, _)| body == id)
            .map(|(_, body)| body)
    }

    /// The pattern of symbol `id` and how many bytes before the symbol it
    /// starts, for a symbol its pattern finds directly: a hook checks the
    /// code there before it patches. `None` for a symbol without one, or
    /// one its pattern finds through a displacement.
    #[must_use]
    pub fn head(&self, id: &str) -> Option<(&Pattern, u64)> {
        self.heads
            .iter()
            .find(|(head, ..)| head == id)
            .map(|(_, pattern, at)| (pattern, *at))
    }

    /// The name the log shows.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the table was made for this build.
    #[must_use]
    pub const fn kind(&self) -> Kind {
        self.kind
    }

    /// Systems whose reflected entry must equal the pinned address; none on
    /// a fallback.
    #[must_use]
    pub fn checks(&self) -> &[(String, u64)] {
        &self.checks
    }

    /// The symbol `id`, relative to the image base.
    #[must_use]
    pub fn symbol(&self, id: &str) -> Option<u64> {
        find(&self.symbols, id)
    }

    /// The constant `id`.
    #[must_use]
    pub fn constant(&self, id: &str) -> Option<u64> {
        find(&self.constants, id)
    }

    /// Every symbol, sorted by id.
    #[must_use]
    pub fn symbols(&self) -> &[(String, u64)] {
        &self.symbols
    }

    /// Every constant, sorted by id.
    #[must_use]
    pub fn constants(&self) -> &[(String, u64)] {
        &self.constants
    }

    /// The entries of the source table this build lacks, sorted.
    #[must_use]
    pub fn missing(&self) -> &[String] {
        &self.missing
    }

    /// The constants of the source table only its own build holds, left
    /// out on a build it is carried to and not [`Self::missing`] there
    /// ([`Constant::carried`](crate::Constant::carried)).
    #[must_use]
    pub fn uncarried(&self) -> &[String] {
        &self.uncarried
    }

    /// The constants that are slots of attributes, by id, with the name of
    /// their attribute ([`Constant::attribute`](crate::Constant::attribute)).
    #[must_use]
    pub fn attributes(&self) -> &[(String, String)] {
        &self.attributes
    }

    /// Sets the constant `id` to `value`, read from the game's data, and
    /// takes it off [`Self::missing`]; the value it had, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::Table;
    ///
    /// let table = Table::parse(
    ///     r#"
    ///     name = "1.0 server (1)"
    ///     side = "server"
    ///     guid = "00000000000000000000000000000001"
    ///     age = 2
    ///     [constants]
    ///     "attribute.health_max_slot" = { value = 7, attribute = "Health_Max" }
    ///     "#,
    /// )?;
    /// let mut resolved = table.fall_back(&[], 0, |_| None);
    /// assert_eq!(resolved.missing(), ["attribute.health_max_slot"]);
    /// assert_eq!(resolved.supply("attribute.health_max_slot", 7), None);
    /// assert_eq!(resolved.constant("attribute.health_max_slot"), Some(7));
    /// assert!(resolved.missing().is_empty());
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn supply(&mut self, id: &str, value: u64) -> Option<u64> {
        self.missing.retain(|have| have != id);
        match self
            .constants
            .binary_search_by(|(have, _)| have.as_str().cmp(id))
        {
            Ok(i) => Some(core::mem::replace(&mut self.constants[i].1, value)),
            Err(i) => {
                self.constants.insert(i, (id.to_owned(), value));
                None
            }
        }
    }
}

impl Resolved {
    /// Takes what an engine route found for entry `id`: an entry the
    /// table lacks or missed is supplied, one it holds must agree or goes
    /// off as [`Self::missing`], and nothing found leaves the table's
    /// entry. A [`Routed::Symbol`] or constant is [`Self::routed`], and a
    /// symbol also stands for its `<id>.bytes` check, which the route's own
    /// match replaces; a [`Routed::Reached`] symbol is not, and keeps its
    /// checks. A symbol the route supplies loses the table's head pattern,
    /// body and `<id>.bytes`, which were made for code the table did not
    /// find, so a hook on a supplied [`Routed::Reached`] symbol has no check.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{Routed, Routing, Table};
    ///
    /// let table = Table::parse(
    ///     r#"
    ///     name = "1.0 server (1)"
    ///     side = "server"
    ///     guid = "00000000000000000000000000000001"
    ///     age = 2
    ///     [symbols]
    ///     "f.held" = 0x1000
    ///     "#,
    /// )?;
    /// let mut resolved = table.resolve();
    /// assert_eq!(resolved.route("f.held", Some(Routed::Symbol(0x1000))), Routing::Agrees);
    /// assert_eq!(resolved.route("f.new", Some(Routed::Constant(8))), Routing::Supplied);
    /// assert_eq!(resolved.constant("f.new"), Some(8));
    /// assert!(resolved.routed("f.new"));
    /// assert_eq!(
    ///     resolved.route("f.held", Some(Routed::Symbol(0x2000))),
    ///     Routing::Conflict { table: 0x1000, engine: 0x2000 }
    /// );
    /// assert_eq!(resolved.symbol("f.held"), None);
    /// assert_eq!(resolved.route("f.call", Some(Routed::Reached(0x3000))), Routing::Supplied);
    /// assert_eq!(resolved.symbol("f.call"), Some(0x3000));
    /// assert!(!resolved.routed("f.call"));
    /// assert_eq!(resolved.route("f.none", None), Routing::Unresolved);
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn route(&mut self, id: &str, found: Option<Routed>) -> Routing {
        let Some(found) = found else {
            return Routing::Unresolved;
        };
        let (entries, engine) = match found {
            Routed::Symbol(rva) | Routed::Reached(rva) => (&mut self.symbols, rva),
            Routed::Constant(value) => (&mut self.constants, value),
        };
        let routing = match entries.binary_search_by(|(have, _)| have.as_str().cmp(id)) {
            Ok(i) if entries[i].1 == engine => Routing::Agrees,
            Ok(i) => {
                let table = entries.remove(i).1;
                self.routed.retain(|have| have != id);
                if let Err(at) = self.missing.binary_search_by(|have| have.as_str().cmp(id)) {
                    self.missing.insert(at, id.to_owned());
                }
                return Routing::Conflict { table, engine };
            }
            Err(i) => {
                entries.insert(i, (id.to_owned(), engine));
                Routing::Supplied
            }
        };
        self.missing.retain(|have| have != id);
        let symbol = !matches!(found, Routed::Constant(_));
        if symbol && routing == Routing::Supplied {
            // The table's checks of this symbol were made for code it did not
            // find, so none of them stands for the code at the route's answer.
            let check = format!("{id}.bytes");
            self.heads.retain(|(have, ..)| have != id);
            self.bodies.retain(|(have, _)| have != id);
            self.constants.retain(|(have, _)| *have != check);
        }
        if matches!(found, Routed::Reached(_)) {
            return routing;
        }
        let check = format!("{id}.bytes");
        self.missing.retain(|have| *have != check);
        if let Err(at) = self.routed.binary_search_by(|have| have.as_str().cmp(id)) {
            self.routed.insert(at, id.to_owned());
        }
        routing
    }

    /// Counts entry `id` as [`Self::missing`] when the table has no row for
    /// it, such as an entry whose engine route found nothing on a build its
    /// anchors do not cover; whether it was added.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::{Side, anchors};
    ///
    /// let mut resolved = anchors(Side::Server)?.fall_back(&[], 0, |_| None);
    /// assert!(resolved.lack("f.unrouted"));
    /// assert!(!resolved.lack("f.unrouted"));
    /// assert!(resolved.missing().iter().any(|id| id == "f.unrouted"));
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn lack(&mut self, id: &str) -> bool {
        if self.symbol(id).is_some() || self.constant(id).is_some() {
            return false;
        }
        match self.missing.binary_search_by(|have| have.as_str().cmp(id)) {
            Ok(_) => false,
            Err(at) => {
                self.missing.insert(at, id.to_owned());
                true
            }
        }
    }

    /// Whether an engine route found entry `id`, whose code the route's own
    /// match then checks.
    ///
    /// # Examples
    ///
    /// See [`Resolved::route`].
    #[must_use]
    pub fn routed(&self, id: &str) -> bool {
        self.routed
            .binary_search_by(|have| have.as_str().cmp(id))
            .is_ok()
    }
}

fn find(entries: &[(String, u64)], id: &str) -> Option<u64> {
    entries
        .binary_search_by(|(have, _)| have.as_str().cmp(id))
        .ok()
        .map(|i| entries[i].1)
}

/// The patterns the loader carries to every build of `side` after the
/// engine routes: a table without a build whose rows find, on any build,
/// the entries no route finds ([`Table::fall_back`] resolves it).
///
/// # Errors
///
/// [`TableError`] when the embedded anchors do not read; a test reads them
/// on every build of the crate.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Side, anchors};
///
/// let server = anchors(Side::Server)?;
/// assert!(server.symbols().iter().all(|s| s.pattern().is_some()));
/// # Ok::<(), enshwick_gamedata::TableError>(())
/// ```
pub fn anchors(side: Side) -> Result<Table, TableError> {
    Table::anchors(include_str!("../anchors/patterns.toml"), side)
}

/// Whether a compatibility report of this crate covers the build of `side`
/// with `fingerprint`: a build the tests ran the anchors and routes on.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::{Fingerprint, Side, reported};
///
/// assert!(!reported(Side::Server, Fingerprint::new([0xee; 16], 1)));
/// ```
#[must_use]
pub fn reported(side: Side, fingerprint: Fingerprint) -> bool {
    let builds: &[(&str, &str, u32)] = include!(concat!(env!("OUT_DIR"), "/reported.rs"));
    builds.iter().any(|&(s, guid, age)| {
        s == side.as_str()
            && age == fingerprint.age()
            && crate::table::parse_guid(guid) == Some(fingerprint.guid())
    })
}

/// The environment variable naming the folder of the kept builds' game
/// data, read from a developer's own game files and kept outside this
/// repository: their tables in `tables/` and their typed catalogues in
/// `typed/`. The tests that compare with them skip without it.
pub const DATA_VAR: &str = "ENSH_GAMEDATA";

/// The folder [`DATA_VAR`] names; `None` when it is unset.
///
/// # Examples
///
/// ```
/// let tables = enshwick_gamedata::data_dir().map(|dir| dir.join("tables"));
/// # let _ = tables;
/// ```
#[must_use]
pub fn data_dir() -> Option<std::path::PathBuf> {
    std::env::var_os(DATA_VAR).map(std::path::PathBuf::from)
}

/// Every table in `dir`'s `tables/`, as `(file name, text)` sorted by file
/// name; empty when `dir` has no `tables/`.
///
/// # Errors
///
/// When the folder or a table in it does not read.
///
/// # Examples
///
/// ```
/// let tables = enshwick_gamedata::tables_in(std::path::Path::new("no such folder"))?;
/// assert!(tables.is_empty());
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn tables_in(dir: &std::path::Path) -> std::io::Result<Vec<(String, String)>> {
    let dir = dir.join("tables");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut tables = Vec::new();
    for entry in entries {
        let path = entry?.path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        if let Some(name) = name.filter(|n| n.ends_with(".toml")) {
            tables.push((name, std::fs::read_to_string(&path)?));
        }
    }
    tables.sort();
    Ok(tables)
}

/// The kept builds' tables in the folder [`DATA_VAR`] names, which the
/// tests compare the routes and anchors with; empty without it.
///
/// # Errors
///
/// When the folder or a table in it does not read.
///
/// # Examples
///
/// ```
/// let tables = enshwick_gamedata::kept()?;
/// assert!(tables.iter().all(|(file, _)| file.ends_with(".toml")));
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn kept() -> std::io::Result<Vec<(String, String)>> {
    data_dir().map_or_else(|| Ok(Vec::new()), |dir| tables_in(&dir))
}

/// The table for a build among `tables`: the highest revision of those made
/// for it.
#[must_use]
pub fn exact(tables: &[Table], side: Side, fingerprint: Fingerprint) -> Option<&Table> {
    tables
        .iter()
        .filter(|t| t.side == side && t.fingerprint == fingerprint)
        .max_by_key(|t| t.revision)
}

/// The table of the newest build of `side` among `tables`: the base for a
/// build without a table.
///
/// Tables rank by Steam build id, then the game's version, then revision. A
/// table without a Steam build id ranks below every table with one.
#[must_use]
pub fn newest(tables: &[Table], side: Side) -> Option<&Table> {
    tables
        .iter()
        .filter(|t| t.side == side)
        .max_by_key(|t| (t.steam_build, t.game_version, t.revision))
}
