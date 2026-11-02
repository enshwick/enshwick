//! What the start decides about each mod folder in `mods/`, and the line the
//! log gives each decision.

use enshwick::abi::Level;

use crate::errors;
use crate::manifest::ManifestError;

/// Why a DLL in a mod folder does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Skip {
    /// Its folder holds another DLL with the Enshwick exports, and a folder
    /// holds one mod.
    SecondMod {
        /// The other DLL, as `<folder>/<file>`.
        other: String,
    },
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
    /// The level of the log line: every skip is a problem.
    pub(crate) const fn level(&self) -> Level {
        Level::WARN
    }

    /// The log line for the DLL `file`, with the error code a problem
    /// carries.
    pub(crate) fn line(&self, file: &str) -> String {
        match self {
            Self::SecondMod { other } => format!(
                "{file} skipped: its folder also holds the mod {other}, and a folder in mods/ \
                 holds one mod; move one to a folder of its own ({})",
                errors::DUPLICATE_MOD
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_mod_in_a_folder_names_both_files() {
        let skip = Skip::SecondMod {
            other: "lanterns/lanterns.dll".into(),
        };
        assert_eq!(skip.level(), Level::WARN);
        let line = skip.line("lanterns/lanterns-old.dll");
        assert!(
            line.starts_with("lanterns/lanterns-old.dll skipped:"),
            "{line}"
        );
        assert!(line.contains("lanterns/lanterns.dll"), "{line}");
        assert!(line.contains(&errors::DUPLICATE_MOD.to_string()), "{line}");
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
}
