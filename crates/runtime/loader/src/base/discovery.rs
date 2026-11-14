//! What the start decides about each mod folder in `mods/`, and the line the
//! log gives each decision.

use crate::base::manifest::ManifestError;
use crate::engine::errors;

/// Why a DLL in a mod folder does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// Its folder holds another DLL with the Enshwick exports, and a folder
    /// holds one mod.
    SecondMod {
        /// The other DLL, as `<folder>/<file>`.
        other: String,
    },
    /// It exports the interface's version but not all four functions.
    NotAMod,
    /// It was built for another ABI major, as its `enshwick_mod_abi` says.
    OtherAbi(enshwick_core::abi::CheckAbiError),
    /// Its manifest does not read or parse.
    Manifest(ManifestError),
    /// Windows did not load it, with the system's error code and text.
    Unloadable {
        /// The system's error code, when the error has one.
        code: Option<i32>,
        /// The system's text.
        error: String,
    },
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
    /// The log line for the DLL `file`, with the error code a problem
    /// carries.
    #[must_use]
    pub fn line(&self, file: &str) -> String {
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
            Self::Unloadable { code, error } => errors::load_failure(file, *code, error),
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

    /// Why the file does not start, in the player's language, as the Mods
    /// screen and the chat line at world load show it.
    #[must_use]
    pub fn shown(&self) -> String {
        use crate::engine::texts::fill;
        match self {
            Self::SecondMod { other } => fill("mods.skipped.second", &[("other", other)]),
            Self::OtherAbi(_) | Self::Manifest(ManifestError::OtherAbi(_)) => {
                fill("mods.skipped.abi", &[])
            }
            Self::NotAMod | Self::Manifest(_) => fill("mods.skipped.broken", &[]),
            Self::Unloadable { code, error } => match errors::load_hint(*code) {
                Some(errors::LoadHint::NotWindows64) => fill("mods.skipped.not-64-bit", &[]),
                Some(errors::LoadHint::MissingDll) => fill("mods.skipped.needs-dll", &[]),
                None => fill("mods.skipped.unloadable", &[("error", error)]),
            },
            Self::Duplicate { id, .. } => fill("mods.skipped.copy", &[("id", id)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_file_tells_the_player_why_in_their_language() {
        let unloadable = |code| Skip::Unloadable {
            code,
            error: "denied".into(),
        };
        assert!(
            unloadable(Some(193))
                .shown()
                .contains("not a 64-bit Windows DLL")
        );
        assert!(unloadable(Some(126)).shown().contains("Visual C++"));
        assert!(unloadable(Some(5)).shown().ends_with("denied"));
        let why = enshwick_core::abi::check_abi(1, 0).unwrap_err();
        let abi = Skip::OtherAbi(why).shown();
        assert!(abi.contains("another version of Enshwick"), "{abi}");
        assert_eq!(Skip::Manifest(ManifestError::OtherAbi(why)).shown(), abi);
        assert_ne!(Skip::Manifest(ManifestError::Missing).shown(), abi);
        assert_eq!(
            Skip::NotAMod.shown(),
            Skip::Manifest(ManifestError::Missing).shown()
        );
        let second = Skip::SecondMod {
            other: "lanterns/lanterns.dll".into(),
        };
        assert!(second.shown().contains("lanterns/lanterns.dll"));
        let uk = Some(enshwick_core::i18n::Language::UK_UA);
        let shown = crate::engine::texts::for_reader(uk, || unloadable(Some(193)).shown());
        assert!(shown.contains("64-бітна"), "{shown}");
    }

    #[test]
    fn a_second_mod_in_a_folder_names_both_files() {
        let skip = Skip::SecondMod {
            other: "lanterns/lanterns.dll".into(),
        };
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
        assert_eq!(
            Skip::NotAMod.line("half.dll"),
            format!("half.dll is not an Enshwick mod ({})", errors::BROKEN_DLL)
        );
    }

    #[test]
    fn a_mod_of_another_abi_names_what_to_do_with_its_code() {
        let why = enshwick_core::abi::check_abi(1, 0).unwrap_err();
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
