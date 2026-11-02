//! `mods.lock`, the world's record of what ran with it: the loader's version,
//! the game build and each running mod's id, version, folder and SHA-256, so
//! a server can be set up again exactly and two servers compared.
//!
//! The record is taken once at start, since the mod set never changes in a
//! process, and written at every world load into that world's data; what
//! changed since the world last loaded goes to the log. Mods that a present
//! mod replaces are listed apart, with the mod that replaces them, since
//! their DLLs never ran.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The record's file name, in the loader's data of a world.
pub const RECORD_FILE: &str = "mods.lock";

/// The comment the record starts with.
const RECORD_HEADER: &str = "\
# Written by Enshwick at every load of this world: the mods that ran with it.
# Copy their folders with it to set up the same server again; do not edit it.
";

/// The mods a present mod replaced at start.
static REPLACED: OnceLock<Vec<Replaced>> = OnceLock::new();

/// Records the mods a present mod replaced at start.
pub fn set_replaced(replaced: Vec<Replaced>) {
    let _first = REPLACED.set(replaced);
}

/// The mods a present mod replaced at start.
#[must_use]
pub fn replaced() -> &'static [Replaced] {
    REPLACED.get().map_or(&[], Vec::as_slice)
}

/// The record taken at start, once its hashes are read.
static TAKEN: OnceLock<Record> = OnceLock::new();

/// Keeps the record taken at start for the worlds that load.
pub fn set_taken(record: Record) {
    let _first = TAKEN.set(record);
}

/// The record taken at start; `None` while its hashes are still read.
#[must_use]
pub fn taken() -> Option<&'static Record> {
    TAKEN.get()
}

/// One running mod in the record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ran {
    /// The mod id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// Its folder in `mods/`.
    pub folder: String,
    /// The SHA-256 of its DLL, or of a pack's files, in lowercase hex.
    pub sha256: String,
}

/// A mod in `mods/` that a present mod replaces, never started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replaced {
    /// The old mod id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// Its folder in `mods/`.
    pub folder: String,
    /// The id of the mod that replaces it.
    pub by: String,
}

impl Replaced {
    /// The line the log, the status and `enshwick` on the console show.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{} is replaced by {}; delete mods/{}/",
            self.id, self.by, self.folder
        )
    }
}

/// The record: the loader, the game build and the mods that ran.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The loader's version.
    pub loader: String,
    /// The SHA-256 of the loader's DLL.
    pub loader_sha256: String,
    /// The game build's name.
    pub game: String,
    /// The mods that ran, in load order.
    #[serde(default, rename = "mod")]
    pub mods: Vec<Ran>,
    /// The mods present mods replaced, never started.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced: Vec<Replaced>,
}

impl Record {
    /// Reads a record; `None` for text that is not one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        toml::from_str(text).ok()
    }

    /// The record as its file holds it.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{RECORD_HEADER}\n{}",
            toml::to_string(self).unwrap_or_default()
        )
    }

    /// What changed from `old` to this record, one line each.
    #[must_use]
    pub fn changes(&self, old: &Self) -> Vec<String> {
        let mut lines = Vec::new();
        if self.loader != old.loader {
            lines.push(format!("loader {} -> {}", old.loader, self.loader));
        } else if self.loader_sha256 != old.loader_sha256 {
            lines.push(format!("loader {}: another build", self.loader));
        }
        if self.game != old.game {
            lines.push(format!("game {} -> {}", old.game, self.game));
        }
        for new in &self.mods {
            match old.mods.iter().find(|m| m.id == new.id) {
                None => lines.push(format!("{} {} added", new.id, new.version)),
                Some(was) if was.version != new.version => {
                    lines.push(format!("{} {} -> {}", new.id, was.version, new.version));
                }
                Some(was) if was.sha256 != new.sha256 => {
                    lines.push(format!("{} {}: another build", new.id, new.version));
                }
                Some(_) => {}
            }
        }
        for was in &old.mods {
            if self.mods.iter().all(|m| m.id != was.id) {
                lines.push(format!("{} {} no longer runs", was.id, was.version));
            }
        }
        lines
    }
}

/// The SHA-256 of `bytes` in lowercase hex.
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            let _written = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(mods: &[(&str, &str, &str)]) -> Record {
        Record {
            loader: "0.4.0".into(),
            loader_sha256: "aa".into(),
            game: "1.0 server (1269579)".into(),
            mods: mods
                .iter()
                .map(|&(id, version, sha)| Ran {
                    id: id.into(),
                    version: version.into(),
                    folder: id.into(),
                    sha256: sha.into(),
                })
                .collect(),
            replaced: Vec::new(),
        }
    }

    #[test]
    fn replaced_mods_are_kept_apart_in_the_record() {
        let mut new = record(&[("you.market-stalls", "0.2.0", "01")]);
        new.replaced.push(Replaced {
            id: "you.stalls".into(),
            version: "0.1.0".into(),
            folder: "stalls".into(),
            by: "you.market-stalls".into(),
        });
        let text = new.render();
        assert!(text.contains(
            "[[replaced]]
id = \"you.stalls\""
        ));
        assert_eq!(Record::parse(&text), Some(new.clone()));
        assert!(!record(&[]).render().contains("replaced"));
        assert_eq!(
            new.replaced[0].line(),
            "you.stalls is replaced by you.market-stalls; delete mods/stalls/"
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "the snapshots are held on the host; under Wine their paths do not resolve"
    )]
    fn the_record_reads_as_its_snapshot() {
        let record = record(&[
            ("you.trading-post", "0.3.0", "01"),
            ("you.lockers", "1.0.0", "02"),
        ]);
        insta::assert_snapshot!("record", record.render());
    }

    #[test]
    fn the_record_reads_back_and_names_what_changed() {
        let old = record(&[("you.trading-post", "0.3.0", "01"), ("gone", "1.0.0", "02")]);
        let text = old.render();
        assert!(text.starts_with("# Written by Enshwick"));
        assert!(text.contains("[[mod]]\nid = \"you.trading-post\""));
        assert_eq!(Record::parse(&text), Some(old.clone()));
        let mut new = record(&[
            ("you.trading-post", "0.3.1", "03"),
            ("count", "0.2.0", "04"),
        ]);
        new.loader = "0.5.0".into();
        assert_eq!(
            new.changes(&old),
            [
                "loader 0.4.0 -> 0.5.0",
                "you.trading-post 0.3.0 -> 0.3.1",
                "count 0.2.0 added",
                "gone 1.0.0 no longer runs"
            ]
        );
        let rebuilt = record(&[("you.trading-post", "0.3.0", "ff"), ("gone", "1.0.0", "02")]);
        assert_eq!(
            rebuilt.changes(&old),
            ["you.trading-post 0.3.0: another build"]
        );
        assert!(old.changes(&old).is_empty());
        assert_eq!(Record::parse("not a record"), None);
    }

    #[test]
    fn hashes_are_sha256_hex() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
