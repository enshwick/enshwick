use std::collections::BTreeMap;

use serde::Deserialize;

use super::MANIFEST_FILE;
use super::files::PackError;

/// Where a pack runs, in `pack.toml`.
///
/// # Examples
///
/// See [`PackManifest`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PackSide {
    /// The server and the clients: the default, as both read content.
    #[default]
    Both,
    /// The dedicated server and hosting clients only, such as a pack of
    /// loot alone.
    Server,
    /// Clients only.
    Client,
}

/// A pack folder's `pack.toml`: what the loader reads before it runs the
/// pack, as a mod's [`manifest!`](crate::manifest) says it for a DLL.
///
/// # Examples
///
/// ```
/// use enshwick::packs::{PackManifest, PackSide};
///
/// let manifest = PackManifest::parse(r#"
/// id = "you.lanterns"
/// version = "1.0.0"
/// name = "Lanterns"
///
/// [requires]
/// "you.trading-post" = ">=0.5, <1.0"
/// "#)?;
/// assert_eq!(manifest.id, "you.lanterns");
/// assert_eq!(manifest.side, PackSide::Both);
/// assert!(manifest.join);
/// assert_eq!(manifest.requires["you.trading-post"], ">=0.5, <1.0");
/// # Ok::<(), enshwick::packs::PackError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackManifest {
    /// The pack id, `<author>.<name>` like a mod's; it names the folder, the
    /// namespace of the pack's content and its texts override file.
    pub id: String,
    /// Its SemVer version.
    pub version: String,
    /// The name players read; the id when empty.
    #[serde(default)]
    pub name: String,
    /// One sentence on what it adds.
    #[serde(default)]
    pub description: String,
    /// The authors' names.
    #[serde(default)]
    pub authors: Vec<String>,
    /// An SPDX licence expression.
    #[serde(default)]
    pub license: String,
    /// Where players read about it, an `https://` address.
    #[serde(default)]
    pub homepage: String,
    /// Where it runs.
    #[serde(default)]
    pub side: PackSide,
    /// Whether server and clients must run the same pack, files and all, to
    /// play together; on unless the pack says otherwise.
    #[serde(default = "yes")]
    pub join: bool,
    /// Mods and packs it needs, by id, each with a Cargo-style version
    /// requirement (empty for any).
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
}

pub(super) const fn yes() -> bool {
    true
}

impl PackManifest {
    /// Reads a `pack.toml`.
    ///
    /// # Errors
    ///
    /// The first problem, with its line and column.
    ///
    /// # Examples
    ///
    /// See [`PackManifest`].
    pub fn parse(text: &str) -> Result<Self, PackError> {
        let manifest: Self =
            toml::from_str(text).map_err(|e| PackError::of(MANIFEST_FILE, text, &e))?;
        if !crate::abi::is_valid_id(&manifest.id) {
            return Err(PackError::at(
                MANIFEST_FILE,
                text,
                find(text, "id"),
                "id is <author>.<name>, each of [a-z0-9-]",
            ));
        }
        Ok(manifest)
    }
}

/// The byte offset of the line that sets `key`, for an error about it.
pub(super) fn find(text: &str, key: &str) -> usize {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with(key) {
            return at + line.len() - line.trim_start().len();
        }
        at += line.len();
    }
    0
}
