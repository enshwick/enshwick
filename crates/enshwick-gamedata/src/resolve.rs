//! A table resolved for the running build, and choosing that table.

use crate::reflect::Fingerprint;
use crate::{Side, Table};

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
}

impl Resolved {
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
    /// ([`Constant::carried`]).
    #[must_use]
    pub fn uncarried(&self) -> &[String] {
        &self.uncarried
    }

    /// The constants that are slots of attributes, by id, with the name of
    /// their attribute ([`Constant::attribute`]).
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
    /// let mut resolved = table.fall_back(&[], 0);
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
