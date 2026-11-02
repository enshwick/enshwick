//! A table resolved for the running build, and choosing that table.

use enshwick::code::Pattern;

use crate::reflect::Fingerprint;
use crate::{Body, Routed, Side, Table};

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
    /// entry. A found symbol also stands for its `<id>.bytes` check, which
    /// the route's own match replaces.
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
    /// assert_eq!(resolved.route("f.none", None), Routing::Unresolved);
    /// # Ok::<(), enshwick_gamedata::TableError>(())
    /// ```
    pub fn route(&mut self, id: &str, found: Option<Routed>) -> Routing {
        let Some(found) = found else {
            return Routing::Unresolved;
        };
        let (entries, engine) = match found {
            Routed::Symbol(rva) => (&mut self.symbols, rva),
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
        let check = format!("{id}.bytes");
        self.missing.retain(|have| have != id && *have != check);
        if let Err(at) = self.routed.binary_search_by(|have| have.as_str().cmp(id)) {
            self.routed.insert(at, id.to_owned());
        }
        routing
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

/// Every table this crate embeds, as `(file name, text)`.
///
/// # Examples
///
/// ```
/// let tables = enshwick_gamedata::embedded();
/// assert!(tables.iter().all(|(file, _)| file.ends_with(".toml")));
/// ```
#[must_use]
pub fn embedded() -> &'static [(&'static str, &'static str)] {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"))
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
