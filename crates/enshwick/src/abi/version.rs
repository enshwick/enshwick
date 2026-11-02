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
pub const ABI_MAJOR: u16 = 5;

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
/// let why = check_abi(u32::from(ABI_MAJOR) - 1, 0).unwrap_err();
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
    /// The mod was built for an older major, or states no major this
    /// loader knows: it needs a rebuild.
    #[error(
        "built for ABI major {major}, and Enshwick {} runs ABI {ABI_MAJOR}.{ABI_MINOR}: \
         rebuild it for Enshwick {}",
        crate::VERSION,
        crate::VERSION
    )]
    OtherMajor {
        /// The major the mod states, as its `enshwick_mod_abi` returns it.
        major: u32,
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

/// Whether this build loads a mod built for ABI `major`.`minor`: the same
/// [`ABI_MAJOR`], and [`ABI_MINOR`] or an older minor.
///
/// `major` is as wide as the `enshwick_mod_abi` export
/// ([`exports::ABI`](super::exports::ABI)).
///
/// # Errors
///
/// [`CheckAbiError`] for another major or a newer minor; a `major` past
/// `u16::MAX` counts as an older one.
///
/// # Examples
///
/// ```
/// use enshwick::abi::{ABI_MAJOR, ABI_MINOR, check_abi};
///
/// assert!(check_abi(u32::from(ABI_MAJOR), ABI_MINOR).is_ok());
/// assert!(check_abi(u32::from(ABI_MAJOR), ABI_MINOR + 1).is_err());
/// ```
pub fn check_abi(major: u32, minor: u16) -> Result<(), CheckAbiError> {
    let ours = u32::from(ABI_MAJOR);
    // A value past u16 is no major: the layout hash mods returned before majors.
    match u16::try_from(major) {
        _ if major == ours && minor > ABI_MINOR => Err(CheckAbiError::NewerMinor { minor }),
        _ if major == ours => Ok(()),
        Ok(newer) if newer > ABI_MAJOR => Err(CheckAbiError::NewerMajor { major: newer }),
        _ => Err(CheckAbiError::OtherMajor { major }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_major_loads_up_to_this_minor() {
        let major = u32::from(ABI_MAJOR);
        assert_eq!(check_abi(major, ABI_MINOR), Ok(()));
        assert_eq!(
            check_abi(major, ABI_MINOR + 1),
            Err(CheckAbiError::NewerMinor {
                minor: ABI_MINOR + 1
            })
        );
    }

    #[test]
    fn a_newer_major_asks_for_an_update_and_an_older_one_or_a_hash_for_a_rebuild() {
        let major = u32::from(ABI_MAJOR);
        assert_eq!(
            check_abi(major + 1, 0),
            Err(CheckAbiError::NewerMajor {
                major: ABI_MAJOR + 1
            })
        );
        for other in [major - 1, 1, 0x0dc8_5f01] {
            assert_eq!(
                check_abi(other, 0),
                Err(CheckAbiError::OtherMajor { major: other })
            );
        }
        assert!(
            check_abi(major + 1, 0)
                .unwrap_err()
                .to_string()
                .ends_with("update Enshwick")
        );
    }
}
