//! `mods/enshwick/mods.toml`, which turns mods on and off without deleting
//! their DLLs, and `mods/enshwick/enshwick.lock`, the mod set that loaded.
//!
//! The list names every mod the loader has found, by id, with its file and
//! `enabled`. The loader adds a mod on its first start, turned on, and keeps
//! everything else the player wrote, comments included. A file whose entry is
//! off is never loaded, so its code never runs.
//!
//! The lock is written at every start: the loader's version, the game build
//! and each loaded mod's id, version, file and SHA-256, so a server can be
//! set up again exactly and two servers compared. What changed since the last
//! start goes to the log. Mods that a present mod replaces are listed apart,
//! with the mod that replaces them, since their DLLs never ran.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, Item, Table, value};

/// The list's file name, in the loader's folder.
pub const LIST_FILE: &str = "mods.toml";
/// The lock's file name, in the loader's folder.
pub const LOCK_FILE: &str = "enshwick.lock";
/// How messages name the list, from the game's folder.
pub const LIST_PATH: &str = "mods/enshwick/mods.toml";

/// The comment a new list starts with.
const LIST_HEADER: &str = "\
# The mods Enshwick found in mods/. Set enabled = false to turn one off
# without deleting its DLL; the loader then never loads that file. Mods found
# later are added here, turned on. Changes take effect at the next start.

";

/// The comment the lock starts with.
const LOCK_HEADER: &str = "\
# Written by Enshwick at every start: the mods that loaded. Copy it with the
# DLLs to set up the same server again; do not edit it.
";

/// The mods turned off in the list at start, by id.
static DISABLED: OnceLock<Vec<String>> = OnceLock::new();

/// Records the mods turned off at start.
pub fn set_disabled(ids: Vec<String>) {
    let _first = DISABLED.set(ids);
}

/// The mods turned off in the list at start, by id.
#[must_use]
pub fn disabled() -> &'static [String] {
    DISABLED.get().map_or(&[], Vec::as_slice)
}

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

/// `mods.toml`, parsed with its comments and layout kept.
#[derive(Debug, Clone)]
pub struct ModList {
    doc: DocumentMut,
}

impl Default for ModList {
    fn default() -> Self {
        let mut doc = DocumentMut::new();
        doc.decor_mut().set_prefix(LIST_HEADER);
        Self { doc }
    }
}

impl ModList {
    /// Reads a list.
    ///
    /// # Errors
    ///
    /// A reason for the log when the text is not TOML or an entry is not a
    /// table.
    pub fn parse(text: &str) -> Result<Self, String> {
        let doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| {
            e.to_string().lines().next().unwrap_or_default().to_owned()
        })?;
        if let Some((id, _)) = doc.iter().find(|(_, item)| !item.is_table_like()) {
            return Err(format!("{id} is not a table such as [{id}]"));
        }
        Ok(Self { doc })
    }

    /// Whether the entry of `id` says `enabled = false`.
    #[must_use]
    pub fn is_off(&self, id: &str) -> bool {
        self.doc
            .get(id)
            .and_then(|entry| entry.get("enabled"))
            .and_then(Item::as_bool)
            == Some(false)
    }

    /// Whether an entry that is off names `file`, compared without case.
    #[must_use]
    pub fn is_file_off(&self, file: &str) -> bool {
        self.doc.iter().any(|(id, entry)| {
            entry
                .get("file")
                .and_then(Item::as_str)
                .is_some_and(|f| f.eq_ignore_ascii_case(file))
                && self.is_off(id)
        })
    }

    /// The ids of the entries that are off.
    #[must_use]
    pub fn off(&self) -> Vec<String> {
        self.doc
            .iter()
            .map(|(id, _)| id)
            .filter(|id| self.is_off(id))
            .map(str::to_owned)
            .collect()
    }

    /// Adds mod `id` from `file`, turned on, or corrects the file of its
    /// entry; whether the list changed.
    pub fn note(&mut self, id: &str, file: &str) -> bool {
        if let Some(entry) = self.doc.get_mut(id).and_then(Item::as_table_like_mut) {
            if entry.get("file").and_then(Item::as_str) == Some(file) {
                return false;
            }
            entry.insert("file", value(file));
        } else {
            let mut entry = Table::new();
            entry.insert("enabled", value(true));
            entry.insert("file", value(file));
            self.doc.insert(id, Item::Table(entry));
        }
        true
    }

    /// The list as its file holds it.
    #[must_use]
    pub fn render(&self) -> String {
        self.doc.to_string()
    }
}

/// One loaded mod in the lock.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locked {
    /// The mod id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// Its DLL in `mods/`.
    pub file: String,
    /// The SHA-256 of the DLL, in lowercase hex.
    pub sha256: String,
}

/// A mod in `mods/` that a present mod replaces, never started.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replaced {
    /// The old mod id.
    pub id: String,
    /// Its version.
    pub version: String,
    /// Its DLL in `mods/`.
    pub file: String,
    /// The id of the mod that replaces it.
    pub by: String,
}

impl Replaced {
    /// The line the log, the status and `/enshwick` show.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{} is replaced by {}; delete mods/{}",
            self.id, self.by, self.file
        )
    }
}

/// The lock: the loader, the game build and the mods that loaded.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// The loader's version.
    pub loader: String,
    /// The SHA-256 of the loader's DLL.
    pub loader_sha256: String,
    /// The game build's name.
    pub game: String,
    /// The mods that loaded, in load order.
    #[serde(default, rename = "mod")]
    pub mods: Vec<Locked>,
    /// The mods present mods replaced, never started.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaced: Vec<Replaced>,
}

impl Lock {
    /// Reads a lock; `None` for text that is not one.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        toml::from_str(text).ok()
    }

    /// The lock as its file holds it.
    #[must_use]
    pub fn render(&self) -> String {
        format!(
            "{LOCK_HEADER}\n{}",
            toml::to_string(self).unwrap_or_default()
        )
    }

    /// What changed from `old` to this lock, one line each.
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
                lines.push(format!("{} {} gone", was.id, was.version));
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

    #[test]
    fn a_new_list_gets_every_mod_turned_on() {
        let mut list = ModList::default();
        assert!(list.note("safes", "enshwick-safes.dll"));
        assert!(list.note("area-loot", "enshwick-area-loot.dll"));
        assert!(!list.note("safes", "enshwick-safes.dll"));
        let text = list.render();
        assert!(text.starts_with("# The mods Enshwick found"));
        assert!(text.contains("[safes]\nenabled = true\nfile = \"enshwick-safes.dll\"\n"));
        assert!(list.off().is_empty());
        let read = ModList::parse(&text).unwrap();
        assert_eq!(read.render(), text);
    }

    #[test]
    fn mods_turned_off_stay_off_with_the_players_comments() {
        let text = "# my server\n\n[safes]\nenabled = false # broken for now\nfile = \"enshwick-safes.dll\"\n";
        let mut list = ModList::parse(text).unwrap();
        assert!(list.is_off("safes"));
        assert!(list.is_file_off("ENSHWICK-SAFES.DLL"));
        assert!(!list.is_file_off("enshwick-area-loot.dll"));
        assert_eq!(list.off(), ["safes"]);
        assert!(list.note("area-loot", "enshwick-area-loot.dll"));
        assert!(list.note("safes", "safes-renamed.dll"));
        let text = list.render();
        assert!(text.contains("# my server"));
        assert!(text.contains("enabled = false # broken for now"));
        assert!(text.contains("file = \"safes-renamed.dll\""));
        assert!(text.contains("[area-loot]\nenabled = true"));
    }

    #[test]
    fn a_broken_list_says_why() {
        assert!(ModList::parse("[safes\n").is_err());
        assert!(
            ModList::parse("safes = true\n")
                .unwrap_err()
                .contains("not a table")
        );
    }

    fn lock(mods: &[(&str, &str, &str)]) -> Lock {
        Lock {
            loader: "0.4.0".into(),
            loader_sha256: "aa".into(),
            game: "1.0 server (1269579)".into(),
            mods: mods
                .iter()
                .map(|&(id, version, sha)| Locked {
                    id: id.into(),
                    version: version.into(),
                    file: format!("enshwick-{id}.dll"),
                    sha256: sha.into(),
                })
                .collect(),
            replaced: Vec::new(),
        }
    }

    #[test]
    fn replaced_mods_are_kept_apart_in_the_lock() {
        let mut new = lock(&[("optimizer", "0.2.0", "01")]);
        new.replaced.push(Replaced {
            id: "proton-optimizer".into(),
            version: "0.1.0".into(),
            file: "enshwick_proton_optimizer.dll".into(),
            by: "optimizer".into(),
        });
        let text = new.render();
        assert!(text.contains(
            "[[replaced]]
id = \"proton-optimizer\""
        ));
        assert_eq!(Lock::parse(&text), Some(new.clone()));
        assert!(!lock(&[]).render().contains("replaced"));
        assert_eq!(
            new.replaced[0].line(),
            "proton-optimizer is replaced by optimizer; delete mods/enshwick_proton_optimizer.dll"
        );
    }

    #[test]
    fn the_lock_reads_back_and_names_what_changed() {
        let old = lock(&[("safes", "0.3.0", "01"), ("gone", "1.0.0", "02")]);
        let text = old.render();
        assert!(text.starts_with("# Written by Enshwick"));
        assert!(text.contains("[[mod]]\nid = \"safes\""));
        assert_eq!(Lock::parse(&text), Some(old.clone()));
        let mut new = lock(&[("safes", "0.3.1", "03"), ("count", "0.2.0", "04")]);
        new.loader = "0.5.0".into();
        assert_eq!(
            new.changes(&old),
            [
                "loader 0.4.0 -> 0.5.0",
                "safes 0.3.0 -> 0.3.1",
                "count 0.2.0 added",
                "gone 1.0.0 gone"
            ]
        );
        let rebuilt = lock(&[("safes", "0.3.0", "ff"), ("gone", "1.0.0", "02")]);
        assert_eq!(rebuilt.changes(&old), ["safes 0.3.0: another build"]);
        assert!(old.changes(&old).is_empty());
        assert_eq!(Lock::parse("not a lock"), None);
    }

    #[test]
    fn hashes_are_sha256_hex() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
