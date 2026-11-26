//! The ABI version, a major and a minor, and the check a loader makes on a
//! mod's.

use core::ops::RangeInclusive;

/// The major version of this interface. A loader refuses a mod built for
/// another major.
///
/// The interface is stable from major 1: a mod built for major 1 loads on
/// every loader of major 1 at its minor or a newer one. Any change to the layout other than an append is
/// a new major, with [`ABI_MINOR`] back at 0; the layout tests of this crate
/// hold the pair to the committed layout.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, Header, Manifest};
///
/// assert_eq!(Header::of::<Manifest>().abi_major, ABI_MAJOR);
/// ```
pub const ABI_MAJOR: u16 = 1;

/// The minor version of this interface: one more for each append to the
/// layout of this [`ABI_MAJOR`], a field at the end of a struct that starts
/// with a [`Header`](super::Header) or a new type.
///
/// A loader loads mods built for its minor or an older one, and reads a
/// field appended after the mod's minor only where
/// the header's `size` covers it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MINOR, Header, Manifest};
///
/// assert_eq!(Header::of::<Manifest>().abi_minor, ABI_MINOR);
/// ```
pub const ABI_MINOR: u16 = 0;

/// The majors of the Enshwick alphas, before the interface was called stable
/// at major 1: a mod built for one of them needs a rebuild, not a newer
/// Enshwick.
///
/// A stable major never takes one of them, so a loader never mistakes an
/// alpha's mod for one of its own major: the major after 4 is 13.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ALPHA_MAJORS, CheckAbiError, check_major};
///
/// let alpha = *ALPHA_MAJORS.end();
/// assert_eq!(check_major(alpha), Err(CheckAbiError::OtherMajor { major: alpha }));
/// ```
pub const ALPHA_MAJORS: RangeInclusive<u16> = 5..=12;

const _: () = assert!(
    ABI_MAJOR < *ALPHA_MAJORS.start() || ABI_MAJOR > *ALPHA_MAJORS.end(),
    "a stable ABI major never takes one of ALPHA_MAJORS"
);

/// Why a loader or its tools refuse a mod built for another ABI, naming the
/// mod's version, this build's and what to do.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, CheckAbiError, check_abi};
///
/// let why = check_abi(ABI_MAJOR - 1, 0).unwrap_err();
/// assert!(matches!(why, CheckAbiError::OtherMajor { .. }));
/// assert!(why.to_string().ends_with(enshwick::VERSION));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CheckAbiError {
    /// The mod was built for a newer major: the loader needs an update.
    #[error(
        "built for ABI major {major}, newer than ABI {ABI_MAJOR}.{ABI_MINOR} of \
         Enshwick {}: update Enshwick",
        crate::VERSION
    )]
    NewerMajor {
        /// The major the mod was built for.
        major: u16,
    },
    /// The mod was built for an older major or one of the
    /// [`ALPHA_MAJORS`]: it needs a rebuild.
    #[error(
        "built for ABI major {major}, and Enshwick {version} runs ABI {ABI_MAJOR}.{ABI_MINOR}: \
         rebuild it for Enshwick {version}",
        version = crate::VERSION
    )]
    OtherMajor {
        /// The major the mod was built for.
        major: u16,
    },
    /// The mod was built for a newer minor of this major: the loader needs
    /// an update.
    #[error(
        "built for ABI {ABI_MAJOR}.{minor}, newer than ABI {ABI_MAJOR}.{ABI_MINOR} of \
         Enshwick {}: update Enshwick",
        crate::VERSION
    )]
    NewerMinor {
        /// The minor the mod was built for.
        minor: u16,
    },
}

/// Whether this build loads a mod whose `enshwick_mod_abi`
/// ([`exports::ABI`](super::exports::ABI)) returned `major`: this
/// [`ABI_MAJOR`]. The loader asks this before it reads the manifest, whose
/// layout the major decides.
///
/// # Errors
///
/// [`CheckAbiError`] for another major: [`CheckAbiError::OtherMajor`] for an
/// older one or one of the [`ALPHA_MAJORS`], else
/// [`CheckAbiError::NewerMajor`].
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, CheckAbiError, check_major};
///
/// assert!(check_major(ABI_MAJOR).is_ok());
/// assert_eq!(check_major(ABI_MAJOR + 1), Err(CheckAbiError::NewerMajor { major: ABI_MAJOR + 1 }));
/// ```
pub fn check_major(major: u16) -> Result<(), CheckAbiError> {
    let alpha = ALPHA_MAJORS.contains(&major);
    match major {
        ABI_MAJOR => Ok(()),
        newer if newer > ABI_MAJOR && !alpha => Err(CheckAbiError::NewerMajor { major: newer }),
        older => Err(CheckAbiError::OtherMajor { major: older }),
    }
}

/// Whether this build loads a mod whose manifest's header states ABI
/// `major`.`minor`: this [`ABI_MAJOR`] ([`check_major`]), and [`ABI_MINOR`]
/// or an older minor.
///
/// # Errors
///
/// [`CheckAbiError`] for another major or a newer minor.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, ABI_MINOR, check_abi};
///
/// assert!(check_abi(ABI_MAJOR, ABI_MINOR).is_ok());
/// assert!(check_abi(ABI_MAJOR, ABI_MINOR + 1).is_err());
/// ```
pub fn check_abi(major: u16, minor: u16) -> Result<(), CheckAbiError> {
    check_major(major)?;
    if minor > ABI_MINOR {
        return Err(CheckAbiError::NewerMinor { minor });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_major_loads_up_to_this_minor() {
        for minor in [0, ABI_MINOR.saturating_sub(1), ABI_MINOR] {
            assert_eq!(check_abi(ABI_MAJOR, minor), Ok(()), "{minor}");
        }
        assert_eq!(
            check_abi(ABI_MAJOR, ABI_MINOR + 1),
            Err(CheckAbiError::NewerMinor {
                minor: ABI_MINOR + 1
            })
        );
    }

    #[test]
    fn a_newer_major_asks_for_an_update_and_an_older_one_for_a_rebuild() {
        let major = ABI_MAJOR;
        assert_eq!(check_major(major), Ok(()));
        let newer = check_major(major + 1).unwrap_err();
        assert_eq!(
            newer,
            CheckAbiError::NewerMajor {
                major: ABI_MAJOR + 1
            }
        );
        assert!(newer.to_string().ends_with("update Enshwick"), "{newer}");
        for older in 0..ABI_MAJOR {
            assert_eq!(
                check_major(older),
                Err(CheckAbiError::OtherMajor { major: older })
            );
        }
        assert_eq!(
            check_abi(ABI_MAJOR + 1, 0),
            Err(CheckAbiError::NewerMajor {
                major: ABI_MAJOR + 1
            })
        );
    }

    #[test]
    fn a_mod_built_for_an_alpha_major_asks_for_a_rebuild() {
        for alpha in ALPHA_MAJORS {
            let why = check_abi(alpha, 3).unwrap_err();
            assert_eq!(why, CheckAbiError::OtherMajor { major: alpha });
            assert!(why.to_string().contains("rebuild it for Enshwick"), "{why}");
        }
        let past = *ALPHA_MAJORS.end() + 1;
        assert_eq!(
            check_major(past),
            Err(CheckAbiError::NewerMajor { major: past })
        );
    }
}
