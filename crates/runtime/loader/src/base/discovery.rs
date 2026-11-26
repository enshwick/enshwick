//! What the start decides about each mod folder in `mods/`, and the line the
//! log gives each decision.

use crate::base::manifest::ManifestError;
use crate::engine::errors;

/// Why a DLL in a mod folder does not start.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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

/// Why, without the file, with the error code a problem carries.
impl core::fmt::Display for Skip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SecondMod { other } => write!(
                f,
                "its folder also holds the mod {other}, and a folder in mods/ holds one mod; \
                 move one to a folder of its own ({})",
                errors::DUPLICATE_MOD
            ),
            Self::NotAMod => write!(f, "not an Enshwick mod ({})", errors::BROKEN_DLL),
            Self::OtherAbi(why) | Self::Manifest(ManifestError::OtherAbi(why)) => {
                write!(f, "{why} ({})", errors::OTHER_ABI)
            }
            Self::Manifest(reason) => write!(f, "{reason} ({})", errors::BROKEN_DLL),
            Self::Unloadable { code, error } => f.write_str(&errors::load_reason(*code, error)),
            Self::Duplicate {
                id,
                version,
                runs,
                runs_version,
            } => write!(
                f,
                "{id} {version} is also in {runs}, which runs {runs_version}; this file can \
                 be deleted ({})",
                errors::DUPLICATE_MOD
            ),
        }
    }
}

impl Skip {
    /// The log line for the DLL `file`.
    #[must_use]
    pub fn line(&self, file: &str) -> String {
        match self {
            Self::NotAMod => format!("{file} is {self}"),
            Self::Unloadable { .. } => format!("{file} {self}"),
            _ => format!("{file} skipped: {self}"),
        }
    }

    /// Whether the folder's mod still runs beside the file, so the file
    /// does not count as a mod that is off.
    #[must_use]
    pub const fn beside_a_running_mod(&self) -> bool {
        matches!(self, Self::SecondMod { .. })
    }

    /// Why the file does not start, in the player's language, as the Mods
    /// screen and the chat line at world load show it.
    #[must_use]
    pub fn shown(&self) -> String {
        use crate::engine::texts::fill;
        use enshwick_core::abi::CheckAbiError;
        match self {
            Self::SecondMod { other } => fill("mods.skipped.second", &[("other", other)]),
            Self::OtherAbi(why) | Self::Manifest(ManifestError::OtherAbi(why)) => match why {
                CheckAbiError::NewerMajor { .. } | CheckAbiError::NewerMinor { .. } => {
                    fill("mods.skipped.abi-newer", &[])
                }
                _ => fill("mods.skipped.abi-older", &[]),
            },
            Self::NotAMod
            | Self::Manifest(
                ManifestError::Missing | ManifestError::Truncated | ManifestError::NotUtf8(_),
            ) => fill("mods.skipped.broken", &[]),
            Self::Manifest(reason) => {
                fill("mods.skipped.manifest", &[("reason", &reason.to_string())])
            }
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
        let why =
            enshwick_core::abi::check_abi(*enshwick_core::abi::ALPHA_MAJORS.end(), 0).unwrap_err();
        let abi = Skip::OtherAbi(why).shown();
        assert!(abi.contains("older Enshwick. Update the mod"), "{abi}");
        assert_eq!(Skip::Manifest(ManifestError::OtherAbi(why)).shown(), abi);
        let newer = enshwick_core::abi::check_abi(enshwick_core::abi::ABI_MAJOR, u16::MAX);
        let newer = Skip::OtherAbi(newer.unwrap_err()).shown();
        assert!(newer.ends_with("Update Enshwick."), "{newer}");
        assert_ne!(Skip::Manifest(ManifestError::Missing).shown(), abi);
        let bad_id = Skip::Manifest(ManifestError::BadId("Lanterns".into())).shown();
        assert!(
            bad_id.starts_with("Its author made a mistake in it: ") && bad_id.contains("Lanterns"),
            "a manifest error a download does not fix: {bad_id}"
        );
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
        let why =
            enshwick_core::abi::check_abi(*enshwick_core::abi::ALPHA_MAJORS.end(), 0).unwrap_err();
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
                 runs 0.2.0; this file can be deleted ({})",
                errors::DUPLICATE_MOD
            )
        );
    }
}
