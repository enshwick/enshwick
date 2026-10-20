//! Whose signature an update needs: trust on first install. A mod's
//! installed version names the keys its author signs releases with
//! (`signed_by:` in its manifest), and every later release must be signed by
//! one of them. A mod the game lacks comes from a server the player chose to
//! join, so the key that server names for it is the one that must have
//! signed the download.

use enshwick_loader_api::mods::{ModEntry, Updates};

use crate::verify::ENSHWICK_KEY;

/// Enshwick's own repository; releases of its mods older than author keys
/// declare none but were always signed with [`ENSHWICK_KEY`].
const ENSHWICK_REPOSITORY: &str = "themaiby/enshwick";

/// Why an update is not installed automatically.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The installed version names no key: its author does not sign updates.
    Unsigned,
    /// The mod is not installed and the server names no key for it.
    NoServerKey,
}

impl Refusal {
    /// What the player reads about it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::trust::Refusal;
    ///
    /// assert!(Refusal::Unsigned.note().contains("by hand"));
    /// ```
    #[must_use]
    pub const fn note(&self) -> &'static str {
        match self {
            Self::Unsigned => "the author does not sign updates \u{2014} update it by hand",
            Self::NoServerKey => {
                "the server names no author key for it \u{2014} install it by hand"
            }
        }
    }
}

/// Where the keys of an update come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trust {
    /// The installed version's keys and the player's `trusted_keys`.
    Installed(Vec<String>),
    /// The keys the server advertises for a mod the game lacks.
    Server(Vec<String>),
}

impl Trust {
    /// The keys one of which must have signed the update.
    #[must_use]
    pub fn keys(&self) -> &[String] {
        match self {
            Self::Installed(keys) | Self::Server(keys) => keys,
        }
    }

    /// Why a download none of [`Trust::keys`] signed is refused.
    #[must_use]
    pub const fn refusal(&self) -> &'static str {
        match self {
            Self::Installed(_) => {
                "it is signed by another key than the installed version names (possible tampering, or the author changed keys); update it by hand if you trust it"
            }
            Self::Server(_) => "it is not signed by the author key the server names",
        }
    }
}

/// The keys a mod's installed version names; Enshwick's own for a release
/// of Enshwick's repository older than author keys.
fn declared(entry: &ModEntry) -> Vec<String> {
    if entry.signed_by.is_empty() && is_enshwick(entry.updates.as_ref()) {
        vec![
            ENSHWICK_KEY
                .lines()
                .last()
                .unwrap_or_default()
                .trim()
                .to_owned(),
        ]
    } else {
        entry.signed_by.clone()
    }
}

fn is_enshwick(updates: Option<&Updates>) -> bool {
    matches!(updates, Some(Updates::GitHub { repository, .. }) if repository == ENSHWICK_REPOSITORY)
}

/// Whose signature an update of `installed`, the mod as this game has it,
/// needs: one of its keys or of `trusted`, the player's `trusted_keys`.
///
/// # Errors
///
/// [`Refusal::Unsigned`] when the installed version names no key.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Side;
/// use enshwick_loader_api::mods::ModEntry;
/// use enshwick_updater::trust::{Refusal, for_installed};
///
/// let mut entry = ModEntry {
///     id: "their-mod".into(),
///     version: "1.2.0".into(),
///     side: Side::BOTH,
///     join: false,
///     running: true,
///     file: "mods/their_mod.dll".into(),
///     updates: None,
///     signed_by: Vec::new(),
///     ..ModEntry::default()
/// };
/// assert_eq!(for_installed(&entry, &[]), Err(Refusal::Unsigned));
/// entry.signed_by = vec!["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF".into()];
/// assert_eq!(for_installed(&entry, &[]).unwrap().keys().len(), 1);
/// ```
pub fn for_installed(installed: &ModEntry, trusted: &[String]) -> Result<Trust, Refusal> {
    let mut keys = declared(installed);
    if keys.is_empty() {
        return Err(Refusal::Unsigned);
    }
    keys.extend(trusted.iter().cloned());
    Ok(Trust::Installed(keys))
}

/// Whose signature the server's version of `missing`, a mod the join check
/// found missing, needs: the installed version's when the game has the mod
/// in another version, else the key the server advertises.
///
/// # Errors
///
/// [`Refusal::Unsigned`] when the installed version names no key,
/// [`Refusal::NoServerKey`] when the game lacks the mod and the server names
/// no key.
pub fn for_missing(
    missing: &ModEntry,
    installed: Option<&ModEntry>,
    trusted: &[String],
) -> Result<Trust, Refusal> {
    if let Some(installed) = installed {
        return for_installed(installed, trusted);
    }
    let keys = declared(missing);
    if keys.is_empty() {
        Err(Refusal::NoServerKey)
    } else {
        Ok(Trust::Server(keys))
    }
}

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::Side;

    use super::*;
    use crate::verify::{check, key};

    /// The throwaway key of `testdata/signed.minisig`.
    const TEST_KEY: &str = "RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r";
    const OTHER_KEY: &str = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";
    const SIGNATURE: &str = include_str!("../testdata/signed.minisig");

    fn entry(signed_by: &[&str], updates: Option<Updates>) -> ModEntry {
        ModEntry {
            id: "safes".into(),
            version: "0.1.0".into(),
            side: Side::BOTH,
            join: true,
            running: true,
            file: "mods/enshwick_safes.dll".into(),
            updates,
            signed_by: signed_by.iter().map(|&k| k.to_owned()).collect(),
            ..ModEntry::default()
        }
    }

    fn github(repository: &str) -> Updates {
        Updates::GitHub {
            repository: repository.into(),
            component: "enshwick-safes".into(),
        }
    }

    /// Whether the test archive, signed with [`TEST_KEY`], passes `trust`.
    fn accepted(trust: &Trust) -> bool {
        let keys: Vec<_> = trust.keys().iter().filter_map(|k| key(k).ok()).collect();
        check(
            &keys,
            b"test",
            SIGNATURE,
            "enshwick-safes-v0.2.0",
            "enshwick-safes.zip",
        )
        .is_ok()
    }

    #[test]
    fn an_update_signed_by_the_installed_key_is_accepted() {
        let trust = for_installed(&entry(&[TEST_KEY], None), &[]).unwrap();
        assert!(accepted(&trust));
    }

    #[test]
    fn an_update_signed_by_a_trusted_key_is_accepted() {
        let trust = for_installed(&entry(&[OTHER_KEY], None), &[TEST_KEY.into()]).unwrap();
        assert!(accepted(&trust));
    }

    #[test]
    fn an_update_signed_by_another_key_is_refused() {
        let trust = for_installed(&entry(&[OTHER_KEY], None), &[]).unwrap();
        assert!(!accepted(&trust));
        assert!(trust.refusal().contains("another key"));
    }

    #[test]
    fn a_mod_without_a_key_is_updated_by_hand() {
        let unsigned = entry(&[], Some(github("someone/their-mods")));
        assert_eq!(for_installed(&unsigned, &[]), Err(Refusal::Unsigned));
        // Trusted keys do not stand in for a key the author never named.
        assert_eq!(
            for_installed(&unsigned, &[TEST_KEY.into()]),
            Err(Refusal::Unsigned)
        );
    }

    #[test]
    fn an_older_enshwick_release_needs_the_enshwick_key() {
        let trust = for_installed(&entry(&[], Some(github(ENSHWICK_REPOSITORY))), &[]).unwrap();
        assert_eq!(trust.keys(), [OTHER_KEY]);
        assert!(!accepted(&trust));
    }

    #[test]
    fn a_missing_mod_needs_the_key_the_server_names() {
        let advertised = entry(&[TEST_KEY], Some(github("someone/their-mods")));
        let trust = for_missing(&advertised, None, &[]).unwrap();
        assert!(matches!(trust, Trust::Server(_)));
        assert!(accepted(&trust));
        let other = entry(&[OTHER_KEY], Some(github("someone/their-mods")));
        let trust = for_missing(&other, None, &[TEST_KEY.into()]).unwrap();
        assert!(
            !accepted(&trust),
            "trusted keys do not override the server's"
        );
        assert!(trust.refusal().contains("server"));
        let none = entry(&[], Some(github("someone/their-mods")));
        assert_eq!(for_missing(&none, None, &[]), Err(Refusal::NoServerKey));
    }

    #[test]
    fn a_mod_in_another_version_keeps_the_installed_key() {
        let advertised = entry(&[TEST_KEY], None);
        let installed = entry(&[OTHER_KEY], None);
        let trust = for_missing(&advertised, Some(&installed), &[]).unwrap();
        assert!(!accepted(&trust), "the server cannot swap the author's key");
        let unsigned = entry(&[], None);
        assert_eq!(
            for_missing(&advertised, Some(&unsigned), &[]),
            Err(Refusal::Unsigned)
        );
    }
}
