//! Stack sizes the loader keeps: the largest stack size above the game's
//! that was in force when a world of this game data folder was saved, so a
//! stack saved that large is never cut when a mod lowers the size or goes.
//!
//! The game cuts every stack above an item's stack size when a world loads
//! (server 399 and client 426, the validator in the inventory setup pass),
//! so a kept size is served from the first read of the item, before any
//! world loads.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use enshwick::content::Guid;

/// The file the kept sizes live in, in the loader's data folder.
pub const KEPT_FILE: &str = "kept-stacks.toml";

/// The kept stack size of each item that has one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    sizes: BTreeMap<[u8; 16], u16>,
}

impl Kept {
    /// The kept size of `item`; `None` when it has none.
    #[must_use]
    pub fn of(&self, item: Guid) -> Option<u16> {
        self.sizes.get(&item.0).copied()
    }

    /// Every item with a kept size, in GUID order.
    pub fn items(&self) -> impl Iterator<Item = (Guid, u16)> + '_ {
        self.sizes.iter().map(|(item, size)| (Guid(*item), *size))
    }

    /// Whether no item has a kept size.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }

    /// Notes that a world was saved while `item` stacked to `in_force`,
    /// whose game size is `vanilla`: the kept size grows to it when it is
    /// above both. Whether it grew.
    pub fn record(&mut self, item: Guid, vanilla: u16, in_force: u16) -> bool {
        if in_force <= vanilla || self.of(item).is_some_and(|kept| kept >= in_force) {
            return false;
        }
        self.sizes.insert(item.0, in_force);
        true
    }

    /// The kept sizes in the file's text.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text =
            String::from("# Stack sizes Enshwick keeps so saved stacks are never cut.\n");
        text.push_str(
            "# Delete an item's line only once no stack of it is larger than its size.\n",
        );
        for (item, size) in self.items() {
            let _ = writeln!(text, "\"{item}\" = {size}");
        }
        text
    }

    /// The kept sizes `text` holds; a line that does not read is left out.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let Ok(table) = text.parse::<toml::Table>() else {
            return Self::default();
        };
        let sizes = table
            .iter()
            .filter_map(|(item, size)| {
                let item = Guid::try_parse(item)?;
                let size = u16::try_from(size.as_integer()?).ok()?;
                Some((item.0, size))
            })
            .collect();
        Self { sizes }
    }

    /// The kept sizes in the loader's data folder `dir`; none while no file
    /// is there.
    ///
    /// # Errors
    ///
    /// When the file is there and does not read: its sizes are not known, so
    /// nothing may write over it or release the hold it keeps.
    pub fn load(dir: &Path) -> std::io::Result<Self> {
        let text = enshwick::files::read_to_string_if_present(&path(dir))?;
        Ok(text.map_or_else(Self::default, |text| Self::from_text(&text)))
    }

    /// Writes the kept sizes to the loader's data folder `dir`.
    ///
    /// # Errors
    ///
    /// When the file cannot be written.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        enshwick::files::write_atomically(&path(dir), self.to_text())
    }
}

/// Where the kept sizes live in the loader's data folder `dir`.
fn path(dir: &Path) -> PathBuf {
    dir.join(KEPT_FILE)
}

/// The stack size the loader serves for an item whose changes give
/// `merged` and whose kept size is `kept`, with whether the kept size won.
#[must_use]
pub fn served(merged: u16, kept: Option<u16>) -> (u16, bool) {
    match kept {
        Some(kept) if kept > merged => (kept, true),
        _ => (merged, false),
    }
}

/// The reason of the loader's hold while sizes are kept.
#[must_use]
pub fn hold_reason(kept: &Kept) -> String {
    let count = kept.items().count();
    format!(
        "{count} items are kept above the game's stack size ({KEPT_FILE}), so saved stacks are never cut; \
         the game without Enshwick would cut them"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WOOD: Guid = Guid([1; 16]);

    #[test]
    fn a_save_keeps_the_size_above_the_game_s_and_never_lowers_it() {
        let mut kept = Kept::default();
        assert!(!kept.record(WOOD, 20, 20));
        assert!(kept.is_empty());
        assert!(kept.record(WOOD, 20, 999));
        assert!(!kept.record(WOOD, 20, 100));
        assert_eq!(kept.of(WOOD), Some(999));
    }

    #[test]
    fn a_kept_size_wins_only_above_the_merged_value() {
        assert_eq!(served(100, Some(999)), (999, true));
        assert_eq!(served(2000, Some(999)), (2000, false));
        assert_eq!(served(20, None), (20, false));
    }

    #[test]
    fn kept_sizes_go_through_their_file_and_bad_lines_are_left_out() {
        let mut kept = Kept::default();
        kept.record(WOOD, 20, 999);
        kept.record(Guid([2; 16]), 50, 200);
        assert_eq!(Kept::from_text(&kept.to_text()), kept);
        let text = format!(
            "\"{WOOD}\" = 999\n\"not a guid\" = 4\n\"{}\" = 70000\n",
            Guid([3; 16])
        );
        let read = Kept::from_text(&text);
        assert_eq!(read.items().collect::<Vec<_>>(), vec![(WOOD, 999)]);
        assert!(Kept::from_text("not toml [").is_empty());
    }

    #[test]
    fn kept_sizes_survive_a_save_and_a_load() {
        let dir = std::env::temp_dir().join(format!("enshwick-kept-{}", std::process::id()));
        let mut kept = Kept::default();
        kept.record(WOOD, 20, 500);
        kept.save(&dir).unwrap();
        assert_eq!(Kept::load(&dir).unwrap(), kept);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(Kept::load(&dir).unwrap().is_empty());
    }

    #[test]
    fn kept_sizes_that_do_not_read_are_an_error_never_empty() {
        let dir = std::env::temp_dir().join(format!("enshwick-kept-unread-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(path(&dir)).unwrap();
        assert!(Kept::load(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
