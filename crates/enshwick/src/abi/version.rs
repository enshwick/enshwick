//! The ABI version, a major and a minor, and the check a loader makes on a
//! mod's.

/// The major version of this interface. A loader refuses a mod built for
/// another major.
///
/// Any change to the layout other than an append is a new major, with
/// [`ABI_MINOR`] back at 0; the layout tests of this crate hold the pair to
/// the committed layout.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, Header, Manifest};
///
/// assert_eq!(Header::of::<Manifest>().abi_major, ABI_MAJOR);
/// ```
pub const ABI_MAJOR: u16 = 6;

/// The minor version of this interface: one more for each append to the
/// layout of this [`ABI_MAJOR`], a field at the end of a struct that starts
/// with a [`Header`](super::Header) or a new type.
///
/// A loader loads mods built for its minor or an older one, and reads a
/// field appended after the mod's minor only where
/// [`Header::covers`](super::Header::covers) it.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MINOR, Header, Manifest};
///
/// assert_eq!(Header::of::<Manifest>().abi_minor, ABI_MINOR);
/// ```
pub const ABI_MINOR: u16 = 0;

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
    /// The mod was built for an older major: it needs a rebuild.
    #[error(
        "built for ABI major {major}, and Enshwick {} runs ABI {ABI_MAJOR}.{ABI_MINOR}: \
         rebuild it for Enshwick {}",
        crate::VERSION,
        crate::VERSION
    )]
    OtherMajor {
        /// The major the mod was built for.
        major: u16,
    },
    /// The mod was built for an Enshwick older than ABI majors, whose
    /// `enshwick_mod_abi` returned a layout hash: it needs a rebuild.
    #[error(
        "built for an older Enshwick, and Enshwick {} runs ABI {ABI_MAJOR}.{ABI_MINOR}: \
         rebuild it for Enshwick {}",
        crate::VERSION,
        crate::VERSION
    )]
    BeforeMajors,
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
/// [`CheckAbiError`] for another major, and
/// [`BeforeMajors`](CheckAbiError::BeforeMajors) for a value past `u16`, the
/// layout hash mods returned before majors.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, CheckAbiError, check_major};
///
/// assert!(check_major(u32::from(ABI_MAJOR)).is_ok());
/// assert_eq!(check_major(0x0dc8_5f01), Err(CheckAbiError::BeforeMajors));
/// ```
pub fn check_major(major: u32) -> Result<(), CheckAbiError> {
    match u16::try_from(major) {
        Ok(ours) if ours == ABI_MAJOR => Ok(()),
        Ok(newer) if newer > ABI_MAJOR => Err(CheckAbiError::NewerMajor { major: newer }),
        Ok(older) => Err(CheckAbiError::OtherMajor { major: older }),
        Err(_) => Err(CheckAbiError::BeforeMajors),
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
    check_major(u32::from(major))?;
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
        assert_eq!(check_abi(ABI_MAJOR, ABI_MINOR), Ok(()));
        assert_eq!(
            check_abi(ABI_MAJOR, ABI_MINOR + 1),
            Err(CheckAbiError::NewerMinor {
                minor: ABI_MINOR + 1
            })
        );
    }

    #[test]
    fn a_newer_major_asks_for_an_update_and_an_older_one_for_a_rebuild() {
        let major = u32::from(ABI_MAJOR);
        assert_eq!(check_major(major), Ok(()));
        let newer = check_major(major + 1).unwrap_err();
        assert_eq!(
            newer,
            CheckAbiError::NewerMajor {
                major: ABI_MAJOR + 1
            }
        );
        assert!(newer.to_string().ends_with("update Enshwick"), "{newer}");
        for older in [ABI_MAJOR - 1, 1] {
            assert_eq!(
                check_major(u32::from(older)),
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
    fn a_layout_hash_of_an_enshwick_before_majors_asks_for_a_rebuild() {
        for hash in [0x0dc8_5f01, 0x1aea_51a4, u32::MAX] {
            let why = check_major(hash).unwrap_err();
            assert_eq!(why, CheckAbiError::BeforeMajors);
            assert!(
                why.to_string().starts_with("built for an older Enshwick"),
                "{why}"
            );
            assert!(!why.to_string().contains(&hash.to_string()), "{why}");
        }
    }
}
