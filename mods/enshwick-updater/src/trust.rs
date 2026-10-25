//! Whose signature an update needs: trust on first install. A mod whose
//! installed version names the keys its author signs releases with
//! (`signed_by:` in its manifest) gets only releases signed by one of them.
//! A mod that names no key is updated unsigned, over `https://` from the
//! source its own manifest names.

use enshwick_loader_api::mods::{ModEntry, Updates};

use crate::verify::ENSHWICK_KEY;

/// Enshwick's own repository; releases of its mods older than author keys
/// declare none but were always signed with [`ENSHWICK_KEY`].
const ENSHWICK_REPOSITORY: &str = "themaiby/enshwick";

/// The key of [`REFUSAL`]'s text in the updater's texts.
pub const REFUSAL_KEY: &str = "refusal.other-key";
/// Why a signed mod's download none of [`Trust::keys`] signed is refused, as
/// the log says it.
pub const REFUSAL: &str = "it is signed by another key than the installed version names (possible tampering, or the author changed keys); update it by hand if you trust it";

/// Where the keys of an update come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trust {
    /// The installed version's keys and the player's `trusted_keys`.
    Installed(Vec<String>),
    /// No key: the mod's author does not sign releases.
    Unsigned,
}

impl Trust {
    /// The keys one of which must have signed the update; none for
    /// [`Trust::Unsigned`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::trust::Trust;
    ///
    /// assert!(Trust::Unsigned.keys().is_empty());
    /// ```
    #[must_use]
    pub fn keys(&self) -> &[String] {
        match self {
            Self::Installed(keys) => keys,
            Self::Unsigned => &[],
        }
    }

    /// Whether the update must carry a signature.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_updater::trust::Trust;
    ///
    /// assert!(!Trust::Unsigned.signed());
    /// assert!(Trust::Installed(vec!["RW...".into()]).signed());
    /// ```
    #[must_use]
    pub const fn signed(&self) -> bool {
        !matches!(self, Self::Unsigned)
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
/// needs: one of its keys or of `trusted`, the player's `trusted_keys`;
/// none when it names no key.
///
/// # Examples
///
/// ```
/// use enshwick_loader_api::abi::Side;
/// use enshwick_loader_api::mods::ModEntry;
/// use enshwick_updater::trust::{Trust, for_installed};
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
/// assert_eq!(for_installed(&entry, &[]), Trust::Unsigned);
/// entry.signed_by = vec!["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF".into()];
/// assert_eq!(for_installed(&entry, &[]).keys().len(), 1);
/// ```
#[must_use]
pub fn for_installed(installed: &ModEntry, trusted: &[String]) -> Trust {
    let mut keys = declared(installed);
    if keys.is_empty() {
        return Trust::Unsigned;
    }
    keys.extend(trusted.iter().cloned());
    Trust::Installed(keys)
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
        let trust = for_installed(&entry(&[TEST_KEY], None), &[]);
        assert!(trust.signed() && accepted(&trust));
    }

    #[test]
    fn an_update_signed_by_a_trusted_key_is_accepted() {
        let trust = for_installed(&entry(&[OTHER_KEY], None), &[TEST_KEY.into()]);
        assert!(accepted(&trust));
    }

    #[test]
    fn an_update_signed_by_another_key_is_refused() {
        let trust = for_installed(&entry(&[OTHER_KEY], None), &[]);
        assert!(!accepted(&trust));
        assert!(REFUSAL.contains("another key"));
    }

    #[test]
    fn a_mod_without_a_key_updates_unsigned() {
        let unsigned = entry(&[], Some(github("someone/their-mods")));
        assert_eq!(for_installed(&unsigned, &[]), Trust::Unsigned);
        // Trusted keys do not make a mod signed that never named a key.
        assert_eq!(
            for_installed(&unsigned, &[TEST_KEY.into()]),
            Trust::Unsigned
        );
    }

    #[test]
    fn an_older_enshwick_release_needs_the_enshwick_key() {
        let trust = for_installed(&entry(&[], Some(github(ENSHWICK_REPOSITORY))), &[]);
        assert_eq!(trust.keys(), [OTHER_KEY]);
        assert!(!accepted(&trust));
    }
}
