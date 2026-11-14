//! The engine functions and constants that reflection does not describe,
//! by symbolic id, found on any game build without a table made for it.
//!
//! The [`Routes`] find entries in the game's own code through what the
//! engine names, the same way on every build ([`Resolved::route`]); the
//! [`anchors`] find the rest by their code patterns ([`Table::fall_back`]),
//! and what neither finds stays missing. The kept builds' tables, each a
//! TOML file named after a build's fingerprint ([`file_name`]), are a record
//! the tests check the routes and anchors against, read from the folder
//! [`DATA_VAR`] names ([`kept`]) and never part of this crate; the loader
//! never reads them, and [`reported`] says which
//! builds a compatibility report covers. The game's own archive reads with
//! [`archive::Archive`], and the engine's reflection data with
//! [`reflect::Reflection`].
//! The [`catalogue`] maps the ids the API asks engine facts by to the
//! game's reflected types and fields.

#![forbid(unsafe_code)]

/// Fails when rustdoc builds this crate's examples one by one, which it does
/// without a word when one of them does not merge.
///
/// ```
/// assert_ne!(module_path!(), "rust_out", "an example does not merge: \
///     RUSTC_BOOTSTRAP=1 RUSTDOCFLAGS='-Zunstable-options --merge-doctests=yes' \
///     cargo test --doc names it");
/// ```
#[cfg(doctest)]
struct MergedExamples;

pub mod archive;
pub mod catalogue;
#[cfg(test)]
mod classes;
mod entries;
#[cfg(test)]
mod game_names;
mod parts;
pub mod reflect;
pub mod releases;
mod resolve;
mod routes;
mod table;
pub mod typed;
mod version;

pub use entries::{Body, Constant, Locate, Operand, Symbol, Witness};
pub use parts::{Part, PartState};
pub use reflect::Fingerprint;
pub use resolve::{
    DATA_VAR, Kind, Resolved, Routing, anchors, data_dir, exact, kept, newest, reported, tables_in,
};
pub use routes::{Routed, Routes, ShapeNames, check_holds, make_check, shape_checks};
pub use table::{MAX_PATTERN, Table, TableError, file_name};
pub use version::{GameVersion, SteamBuild};

/// The binary a table is for.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::Side;
///
/// assert_eq!(Side::Server.as_str(), "server");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// The dedicated server.
    Server,
    /// The game client.
    Client,
}

impl Side {
    /// The side as table files spell it.
    ///
    /// # Examples
    ///
    /// See [`Side`].
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Client => "client",
        }
    }
}

impl std::fmt::Display for SteamBuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Display for GameVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests;
