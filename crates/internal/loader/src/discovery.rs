//! What the start decides about each DLL in `mods/` and the mod list, and
//! the line the log gives each decision.

use enshwick::abi::Level;

use crate::errors;
use crate::manifest::ManifestError;
use crate::modlist::{LIST_PATH, ModList};

/// Why a DLL in `mods/` does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    /// An entry of the mod list that is off names the file.
    FileOff,
    /// The mod list turns off the mod's id.
    IdOff(String),
    /// It exports the interface's version but not all four functions.
    NotAMod,
    /// It was built for another ABI major, as its `enshwick_mod_abi` says.
    OtherAbi(enshwick::abi::CheckAbiError),
    /// Its manifest does not read or parse.
    Manifest(ManifestError),
    /// Another file in `mods/` holds the same mod and runs instead.
    Duplicate {
        /// The mod's id.
        id: String,
        /// The version this file holds.
        version: String,
        /// The file that runs.
        runs: String,
        /// The version that runs.
        runs_version: String,
    },
}

impl Skip {
    /// The level of the log line: a choice of the player's is news, the
    /// rest a problem.
    pub(crate) fn level(&self) -> Level {
        match self {
            Self::FileOff | Self::IdOff(_) => Level::INFO,
            _ => Level::WARN,
        }
    }

    /// The log line for the DLL `file`, with the error code a problem
    /// carries.
    pub(crate) fn line(&self, file: &str) -> String {
        match self {
            Self::FileOff => format!("{file} skipped: off in {LIST_PATH}"),
            Self::IdOff(id) => format!("{file} skipped: {id} is off in {LIST_PATH}"),
            Self::NotAMod => format!("{file} is not an Enshwick mod ({})", errors::BROKEN_DLL),
            Self::OtherAbi(why) | Self::Manifest(ManifestError::OtherAbi(why)) => {
                format!("{file} skipped: {why} ({})", errors::OTHER_ABI)
            }
            Self::Manifest(reason) => {
                format!("{file} skipped: {reason} ({})", errors::BROKEN_DLL)
            }
            Self::Duplicate {
                id,
                version,
                runs,
                runs_version,
            } => format!(
                "{file} skipped: {id} {version} is also in {runs}, which runs {runs_version}; \
                 {file} can be deleted ({})",
                errors::DUPLICATE_MOD
            ),
        }
    }
}

/// The mod list as the start reads it.
#[derive(Debug)]
pub(crate) struct ListRead {
    /// The list; empty, every mod on, when the file is missing or broken.
    pub(crate) list: ModList,
    /// Whether the start may write the list back: not over a broken file
    /// the player still has to fix.
    pub(crate) writable: bool,
    /// The warning for a broken file.
    pub(crate) problem: Option<String>,
}

/// Reads the mod list from what reading its file gave.
pub(crate) fn read_list(read: std::io::Result<String>) -> ListRead {
    let Ok(text) = read else {
        return ListRead {
            list: ModList::default(),
            writable: true,
            problem: None,
        };
    };
    match ModList::parse(&text) {
        Ok(list) => ListRead {
            list,
            writable: true,
            problem: None,
        },
        Err(reason) => ListRead {
            list: ModList::default(),
            writable: false,
            problem: Some(format!(
                "{LIST_PATH} is not read, every mod is on: {reason} ({})",
                errors::BAD_MOD_LIST
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_the_player_turned_off_is_news_without_an_error_code() {
        assert_eq!(Skip::FileOff.level(), Level::INFO);
        assert_eq!(
            Skip::FileOff.line("lanterns.dll"),
            "lanterns.dll skipped: off in mods/enshwick/mods.toml"
        );
        let id = Skip::IdOff("you.lanterns".into());
        assert_eq!(id.level(), Level::INFO);
        assert_eq!(
            id.line("lanterns-2.dll"),
            "lanterns-2.dll skipped: you.lanterns is off in mods/enshwick/mods.toml"
        );
    }

    #[test]
    fn a_dll_without_all_four_exports_is_a_broken_dll() {
        assert_eq!(Skip::NotAMod.level(), Level::WARN);
        assert_eq!(
            Skip::NotAMod.line("half.dll"),
            format!("half.dll is not an Enshwick mod ({})", errors::BROKEN_DLL)
        );
    }

    #[test]
    fn a_mod_of_another_abi_names_what_to_do_with_its_code() {
        let why = enshwick::abi::check_abi(1, 0).unwrap_err();
        let line = Skip::OtherAbi(why).line("old.dll");
        assert_eq!(
            line,
            format!("old.dll skipped: {why} ({})", errors::OTHER_ABI)
        );
        assert!(line.contains("rebuild it for Enshwick"), "{line}");
        assert_eq!(
            Skip::Manifest(ManifestError::OtherAbi(why)).line("old.dll"),
            line
        );
    }

    #[test]
    fn a_manifest_that_does_not_read_says_why_with_the_broken_dll_code() {
        let skip = Skip::Manifest(ManifestError::Missing);
        assert_eq!(skip.level(), Level::WARN);
        assert_eq!(
            skip.line("empty.dll"),
            format!(
                "empty.dll skipped: the manifest is missing ({})",
                errors::BROKEN_DLL
            )
        );
        let bad = Skip::Manifest(ManifestError::BadId("Lanterns".into())).line("bad.dll");
        assert!(bad.contains("\"Lanterns\""), "{bad}");
        assert!(bad.ends_with(&format!("({})", errors::BROKEN_DLL)), "{bad}");
    }

    #[test]
    fn a_duplicate_names_the_file_that_runs_and_the_one_to_delete() {
        let skip = Skip::Duplicate {
            id: "you.lanterns".into(),
            version: "0.1.0".into(),
            runs: "lanterns-new.dll".into(),
            runs_version: "0.2.0".into(),
        };
        assert_eq!(skip.level(), Level::WARN);
        assert_eq!(
            skip.line("lanterns.dll"),
            format!(
                "lanterns.dll skipped: you.lanterns 0.1.0 is also in lanterns-new.dll, which \
                 runs 0.2.0; lanterns.dll can be deleted ({})",
                errors::DUPLICATE_MOD
            )
        );
    }

    #[test]
    fn a_missing_mod_list_turns_every_mod_on_and_is_written() {
        let read = read_list(Err(std::io::ErrorKind::NotFound.into()));
        assert!(read.writable);
        assert!(read.problem.is_none());
        assert!(read.list.off().is_empty());
    }

    #[test]
    fn a_mod_list_that_reads_keeps_its_mods_off() {
        let read = read_list(Ok("[\"you.lanterns\"]\nenabled = false\n".into()));
        assert!(read.writable);
        assert!(read.problem.is_none());
        assert!(read.list.is_off("you.lanterns"));
    }

    #[test]
    fn a_broken_mod_list_turns_every_mod_on_and_is_never_overwritten() {
        let read = read_list(Ok("[you.lanterns\nenabled = false\n".into()));
        assert!(!read.writable);
        assert!(!read.list.is_off("you.lanterns"));
        let problem = read.problem.unwrap();
        assert!(
            problem.starts_with("mods/enshwick/mods.toml is not read, every mod is on: "),
            "{problem}"
        );
        assert!(
            problem.ends_with(&format!("({})", errors::BAD_MOD_LIST)),
            "{problem}"
        );
    }
}
